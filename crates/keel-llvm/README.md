# Diagnostic Keel → LLVM text exporter

`keel-llvm` reads textual Keel IR and emits a complete LLVM `.ll` module. It is
an experiment for comparing frontend and optimizer output through LLVM, not a
production LLVM backend. There is no LLVM library dependency. The API is
`keel_llvm::export(&keel_ir::Module, TargetProfile::AppleArm64) -> Result<String, String>`.

Build and use:

```sh
cargo build -p keel -p keel-llvm
cargo test -p keel-llvm
./target/debug/keel-llvm --target arm64-apple-macosx11.0.0 input.keel -o output.ll
clang -Wno-override-module -c output.ll -o output.o
```

Use `-` for stdin/stdout. The CLI requires an explicit target profile; it never
infers an ABI from the host. The initial profile is Apple arm64, LLVM triple
`arm64-apple-macosx11.0.0`, data layout
`e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32` (little endian, 64-bit pointers).
This layout, including Clang's address spaces 270–272, is checked against local
Clang output for the explicit triple by the validation script. The exporter
only emits ordinary address-space-zero pointers.
Custom triples/layouts and other ABIs are rejected. Narrow `i8`/`i16` signature
extensions deliberately map Keel `sext`/`uext` to LLVM `signext`/`zeroext` on
parameters, returns and calls; the native C fixture exercises both signed and
unsigned narrow parameters and a signed narrow return. Aggregate ABI lowering
requires a separate verified implementation; a pointer with ByVal/Sret purpose
is rejected rather than mapped naively to LLVM attributes.

The exporter verifies Keel IR first and buffers the entire translation. Any
unsupported feature fails before stdout or the destination file is written.
The CLI publishes files by a same-directory temporary file and rename, so an
existing output survives translation or write failure. Errors identify the
function, block and instruction for unsupported operations.

## Supported subset

* Integer scalar values (`i8` through `i128` internally), `f32`, `f64`, opaque
  pointers; ordinary C scalar signatures with zero or one return. `i128` ABI
  values are rejected even though internal arithmetic is supported.
* Wrapping add/subtract/multiply, bit operations, shifts, integer and floating
  comparisons, selects, scalar conversions and scalar overflow operations.
  Comparison results remain Keel `i8` 0/1; LLVM `i1` is only an intermediate.
* FP arithmetic, negate, abs/sqrt, ceil/floor/trunc/nearest-even, FMA,
  minimum/maximum/copysign; bit-exact integer-to-FP constant materialization.
  Float-to-integer conversion uses LLVM saturating intrinsics, not `fptosi` or
  `fptoui` instructions (whose invalid inputs yield poison).
* Leading/trailing-zero count (zero gives bit width), population count and
  byte-swap. No invented `nsw`, `nuw`, `exact` or fast-math flags.
* Arbitrary CFGs with block parameters, loops, switches, and parallel edges.
  Each outgoing edge is split into a unique block before PHI construction, so
  two branches to one block can carry different values.
* Fixed stack slots, dynamic allocation, stack save/restore; byte pointer
  arithmetic without `inbounds`; loads/stores retaining alignment and volatile.
  `notrap` is conservatively omitted. Memory remains untyped bytes.
* Direct and scalar indirect calls, function and data addresses, ordinary function attributes,
  internal function/global linkage, constant and writable globals, packed data
  initializers, byte arrays, zero fill, symbol relocations and byte addends.
  LLVM performs the Mach-O underscore prefixing. Symbol bytes are quoted and
  escaped; the exporter does not prepend an underscore itself.
* Existing memcpy/memmove/memset intrinsics and debugtrap/trap. No alias,
  nonnull, alignment or fault-freedom contract is inferred from pointer values.
  Definition-only restrict parameter facts map to LLVM parameter `noalias`
  on definitions only. They do not create call-site or allocation-return facts.

Ordinary user function declarations, definitions and direct calls carry
`nobuiltin`: Keel does not assign libc contracts from function names. Explicit
`llvm.*` intrinsics keep their specified semantics and do not get `nobuiltin`.
LLVM 21.1 documents this attribute as valid on declarations, definitions and
call sites; [InferFunctionAttrs](https://github.com/llvm/llvm-project/blob/release/21.x/llvm/lib/Transforms/IPO/InferFunctionAttrs.cpp)
checks the declaration itself before library-name attribute inference, while
[CallBase/SimplifyLibCalls](https://github.com/llvm/llvm-project/blob/release/21.x/llvm/lib/Transforms/Utils/SimplifyLibCalls.cpp)
checks call protection. Declaration protection is needed in addition to call
protection. No `readonly`, `willreturn` or other libc promise is invented. See
[LLVM 21.1 function attributes](https://releases.llvm.org/21.1.0/docs/LangRef.html#function-attributes).
This syntax and behavior are validated with the installed Apple Clang 21;
compatibility with older LLVM versions is not claimed.

Shifts mask the amount modulo the *left operand's* width, including oversized
or differently typed amounts. The amount is first zero-extended or truncated
to the left operand type; this preserves the low bits needed by the mask.
FP comparisons use ordered predicates except Keel `ne` → LLVM `une` and
`uno` → `uno`; NaNs and signed zeros retain their specified comparison behavior.
Ordinary FP operations assume Keel's default round-to-nearest-even environment;
strict/constrained FP, dynamic rounding and exception observability are outside
this diagnostic subset.

Semantics are grounded in `keel-ir/src/inst.rs`, `types.rs`, `memory.rs`,
`intrinsic.rs`, `function.rs`, and the shift masking in
`keel-opt/src/const_fold.rs`, plus the authoritative
[LLVM language reference](https://llvm.org/docs/LangRef.html) sections for PHIs,
shifts, saturating FP conversion, overflow, memory and mathematical intrinsics.
Keel's FP conversion contract saturates infinities/out-of-range and converts NaN
to zero for signed/unsigned 32/64-bit destinations, from both FP source widths.
The fixture covers all eight source/destination/signedness combinations.

## Explicit limitations

Integer division/remainder are rejected: Keel requires a trap for a zero
divisor and signed division overflow, whereas naive LLVM instructions introduce
undefined behavior/poison. Signed remainder MIN/-1 also needs deliberate
handling. Rotates and high-half multiply are currently rejected. Other rejected
features include vectors, atomics/fences, inline assembly, tail calls,
exception handling, debug metadata/locations, TLS, varargs/va_arg/va_start,
returns-twice, frame-address, multiple returns, aggregate parameters/returns,
and user symbols in LLVM's reserved `llvm.` namespace. Invalid IR is rejected
by the existing verifier. This is not a full-application compatibility claim.

The next useful expansion is guarded integer division/remainder in reduced
hot functions, with explicit trap CFGs and MIN/-1 tests. Aggregate ABI support
and full application linking need a separate design and validation effort.

## Compare before/after Keel optimization and Clang

Start with a reduced aggregate-free C function, such as `tests/fixtures/hot.c`.
Generate IR without `-g`. The full Keel pipeline can introduce vectors, which
this exporter rejects; the explicit scalar pipeline below omits vectorization.

```sh
mkdir -p target/llvm-compare
./target/debug/keel cc --emit-ir --target aarch64-apple-darwin tests/fixtures/hot.c -o target/llvm-compare/raw.keel
./target/debug/keel opt --target aarch64-apple-darwin -p inline,simplify-cfg,canonicalize,instcombine,call-contracts,simplify-cfg,gvn,stack-forward,canonicalize,instcombine,dce,gcm target/llvm-compare/raw.keel -o target/llvm-compare/optimized.keel
./target/debug/keel-llvm --target arm64-apple-macosx11.0.0 target/llvm-compare/raw.keel -o target/llvm-compare/raw.ll
./target/debug/keel-llvm --target arm64-apple-macosx11.0.0 target/llvm-compare/optimized.keel -o target/llvm-compare/optimized.ll
clang -target arm64-apple-macosx11.0.0 -S -emit-llvm -O0 tests/fixtures/hot.c -o target/llvm-compare/clang-O0.ll
clang -target arm64-apple-macosx11.0.0 -S -emit-llvm -O2 tests/fixtures/hot.c -o target/llvm-compare/clang-O2.ll
clang -O2 -S target/llvm-compare/raw.ll -o target/llvm-compare/raw-llvm.s
clang -O2 -S target/llvm-compare/optimized.ll -o target/llvm-compare/optimized-llvm.s
```

If standalone LLVM `opt` is installed, this produces verifier checks and an
actual pass-by-pass trace, for either exported input:

```sh
opt -passes='verify,default<O2>,verify' -print-before-all -print-after-all -S target/llvm-compare/raw.ll -o target/llvm-compare/raw-llvm-opt.ll 2> target/llvm-compare/raw-passes.txt
```

Apple's Command Line Tools and Xcode do not ship `opt`, `llvm-as` or `llc`;
without a separate LLVM installation, Clang acceptance and native correctness
can still be checked, but no pass trace is produced.

## Validation

Run the validation harness after building both executables:

```sh
python3 crates/keel-llvm/tests/validate.py
```

It checks LLVM parsing/code generation and compares native output: handwritten
Keel versus LLVM at O0/O2 (loops, duplicate branch/switch edges, wrapping and
overflow arithmetic, signed/unsigned/oversized/mixed-width shifts, stack,
volatile store, memory copy, direct call, global relocations, FP NaN comparisons
and saturating conversion boundaries), then Clang C versus unoptimized and
scalar-optimized Keel through both native Keel and LLVM. It checks generated
profile layout against Clang and runs `opt` pass traces if available.
The extended fixtures cover nested stack save/restore inside a loop (including
16-byte alignment and fixed-slot survival), volatile and unaligned memory,
overlapping memmove, memset, external data, relocation addends and local
function addresses. FP checks include exact f32/f64 constant-bit round trips,
signed-zero minimum/maximum in both operand orders, sign manipulation, rounding
ties and fused cancellation. Arithmetic checks cover all six overflow ops and
zero-input bit counts through i128, plus multiple CFG backedges and mixed PHIs.
The native Keel backend currently rejects i128 multiply-overflow; those two
exported ops are compared to a Clang C `__builtin_mul_overflow` oracle instead.
NaN arithmetic payloads with unspecified results are classified, not compared
as exact bit patterns.

The ordinary-libcall reducer links an external custom `strlen` provider,
compiled separately with `-fno-builtin`. Keel and exported LLVM at O0/O2 must
preserve an unused call's side effect and a constant-string call's nonstandard
return. Optimized IR must retain both calls and avoid inferred library attrs;
a generated pre-fix negative control must fail. Trap is exercised in subprocesses
at O0/O2 and must terminate abnormally. Rust tests cover unsupported feature
families, whole-module buffering, quoted symbols, CLI errors, output preservation
and temporary-file cleanup on write failure.

Each harness invocation needs a fresh `--out` directory (default
`target/keel-llvm-validation`); existing manifests are never overwritten. The
`manifest.json` records every command and output, compiler and exporter hashes,
source revision and tool versions, and every produced artifact hash.
This is a correctness exercise, not a performance campaign or speed claim.

## Native LLVM import

`keel_llvm::import::Importer` loads a trusted LLVM 23 shared library once and
imports LLVM text in-process. Each translation creates its own LLVM context,
parses and verifies the original module through LLVM's C API, then parses and
verifies the complete Keel output. It has no build-time LLVM installation or
linker requirement. No Python process or external Keel executable is required.

```sh
cargo build -p keel-llvm --bins
./target/debug/keel-llvm-import \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" input.ll -o output.keel
```

The supported operations and rejection policies match the diagnostic Python
importer described in [the import contract](../../suite/README.md).
The Rust importer also retains scalar load/store alignment and function or
declaration `inlinehint`, `alwaysinline`, `noinline`, `noreturn`, `nounwind`,
`cold`, `hot` and `optnone` attributes. It keeps LLVM's ABI-default access
alignment when source IR omits an explicit alignment. Alignment never implies
`notrap`. Call-site-only attributes remain omitted because Keel has no matching
per-call representation; they are not promoted to callee or caller promises.
`optnone` preserves the request to suppress optional optimization, together with
LLVM's required `noinline` companion. It does not promise identical optimizer
pipelines or instruction selection across compilers.
LLVM permits simultaneous `cold` and `hot` hints; both are omitted when they
conflict, because Keel requires mutually exclusive frequency hints.
`--localize-odr-functions` retains its explicit no-outside-TU-reference
assertion. The CLI buffers output and atomically replaces destinations only
after verification succeeds. An optional `--keel PATH` adds the old external
verification step for compatibility and failure-injection checks.

The three-input harness uses the Rust API directly. The Python implementation
remains a reference for port parity tests. See [porting notes](PORTING.md) for
the mapping, deliberate differences and validation commands.

## Memory and pointer contract transport

LLVM import retains typed function/declaration contracts and independent direct
or indirect call-site contracts. Memory summaries distinguish argument-reachable,
other accessible and inaccessible memory, with separate `nofree`, `nosync` and
contextual `nocallback` promises. The instruction scheduler still treats calls
as barriers. LLVM's `nocallback` means no reentry into the caller's module;
import conservatively omits it for local definitions and unknown indirect sites.

The LLVM 23 C API stores memory effects as two read/write bits per location,
as defined by `llvm/Support/ModRef.h`. Parser-based tests check every mapped
location. Errno effects widen the `other` bucket; target-state effects widen the
`inaccessible` bucket. Unknown encoded classes produce an unknown summary.
Export stays compatible with Apple Clang 21, which cannot name target-state
locations. Its default access set is the union of `other` and `inaccessible`,
with explicit argument/inaccessible overrides. This may lose precision but
cannot create an unsupported no-access promise.

Pointer alignment and non-null facts require `noundef` or a supported positive
`dereferenceable` extent establishing definedness. Readable extents are bounded
to i64. Address/nullness and read/write provenance capture permissions are
retained separately for returned copies and other escapes. A poisonable `nonnull`
argument alone supplies no concrete pointer fact. Export emits `noundef` for
concrete pointer facts, keeps capture independent and uses
`dereferenceable_or_null` when a native Keel readable extent lacks a separate
non-null promise.

With definedness established, `dereferenceable_or_null` also has its own
conditional extent field. It implies neither a non-null address nor unconditional
readability. Allocation-like return `noalias` survives at declaration, definition
and call scopes independently of parameter restrict facts. It supplies no size,
non-null or writable-byte promise.

Integer range attributes paired with noundef at the same boundary survive for
i8/i16/i32/i64 parameters and results, including direct and indirect call sites.
Ranges without that definedness guarantee are omitted. Multiple simultaneous
ranges are retained as a conjunction in Keel; LLVM export diagnoses those it
cannot express with a single range attribute. No new optimizer consumes these
facts. Assume/guard facts, access alias scopes and TBAA remain deferred.

Scalar load-result metadata now maps to sparse instruction `ValueFacts`. Explicit
`!noundef` is required before retaining one integer range, pointer alignment,
non-nullness or readable/nullable-readable extents. Metadata without that boundary,
multi-interval ranges and i1 load facts remain omitted. Export attaches the facts
to the same scalar load and diagnoses facts on other instruction kinds. It does
not convert result-pointer facts into memory access flags or function contracts.
The [instruction-local contract](../../docs/frontends/llvm.md#llvm-contracts-instruction-local-result-facts)
documents scope, mutation invalidation and remaining precision loss.

LLVM parameter `noalias` maps to definition-only `restrict_params` for normal
pointer parameters; declarations, calls, returns and aggregate ABI positions do
not acquire that contract. The mapping preserves the modified-object access
rule and permits legal read-only aliases and equal pointer values.

Recognized direct fixed-slot lifetime markers survive as pinned `lifetime_start`
and `lifetime_end` intrinsics. Export uses LLVM 21's size-plus-pointer form with
size -1; LLVM 23 upgrades it to its one-pointer form. They emit no native
instructions and remain barriers to memory facts. Derived/dynamic markers stay
unsupported. These changes retain no overflow, poison or inbounds facts.

The Rust importer accepts supported scalar `undef` and `poison` operands by
choosing zero (positive zero for floating point, null for pointers). Scalar
`freeze` keeps the concretized operand, preserving concrete input bits and
consistent uses of the frozen result. This discards nondeterminism without
introducing traps or unreachable assumptions. It does not implement full LLVM
poison semantics or extend aggregate/global support. Differential tests must
check permitted outcomes rather than require LLVM to make the same arbitrary
choice. See [the refinement policy](PORTING.md#scalar-deferred-value-refinement).

The diagnostic exporter still has an observed memory-fault loss: LLVM may delete
an unused ordinary load that Keel retains for its possible fault. Defined-memory
execution tests do not establish memory-fault parity. See the
[reproducer and boundary](../../docs/frontends/llvm.md#llvm-contracts-known-llvm-export-loss-ordinary-memory-faults).

Keel's `preserves_access` call contract is not inferred from LLVM attributes or
emitted as an LLVM attribute. It separately protects the native optimizer from
assuming that no-freeing also excludes permission/mapping changes.
