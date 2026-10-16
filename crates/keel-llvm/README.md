# Diagnostic Keel → LLVM text exporter

`keel-llvm` reads textual Keel IR and emits a complete LLVM `.ll` module. It is
an experiment for comparing frontend and optimizer output through LLVM, not a
production LLVM backend. There is no LLVM library dependency. The API is
`keel_llvm::export(&keel_ir::Module, TargetProfile::AppleArm64) -> Result<String, String>`.

Build and use (coordinate the build/native lease when working in the parallel
compiler campaign):

```sh
cargo build -j2 -p keel -p keel-llvm
cargo test -j2 -p keel-llvm
./target/debug/keel-llvm --target arm64-apple-macosx11.0.0 input.keel -o output.ll
clang -Wno-override-module -c output.ll -o output.o
```

Use `-` for stdin/stdout. The CLI requires an explicit target profile; it never
infers an ABI from the host. The initial profile is Apple arm64, LLVM triple
`arm64-apple-macosx11.0.0`, data layout
`e-m:o-i64:64-i128:128-n32:64-S128-Fn32` (little endian, 64-bit pointers).
This layout is checked against local Clang output by the validation script.
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
* Direct calls, function and data addresses, ordinary function attributes,
  internal function/global linkage, constant and writable globals, packed data
  initializers, byte arrays, zero fill, symbol relocations and byte addends.
  LLVM performs the Mach-O underscore prefixing. Symbol bytes are quoted and
  escaped; the exporter does not prepend an underscore itself.
* Existing memcpy/memmove/memset intrinsics and debugtrap/trap. No alias,
  nonnull, alignment or fault-freedom contract is inferred from pointer values.
  Definition-only restrict parameter facts added after the initial base may
  be conservatively omitted; they are not translated to LLVM `noalias`.

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
features include vectors, atomics/fences, inline assembly, indirect/tail calls,
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

Initial local inventory: Apple Clang 21.0.0 (clang-2100.3.34.2), target
arm64-apple-darwin27.0.0; rustc 1.99.0 (b940084d7 2026-09-28); cargo 1.99.0
(5f94df478 2026-08-27). No `opt`, `llvm-as` or `llc` was found on PATH;
`opt` was also absent from `/opt/homebrew/opt/llvm/bin`,
`/usr/local/opt/llvm/bin`, the Command Line Tools bin and the Xcode default
toolchain bin. Thus Clang acceptance/native correctness can be checked here,
but no standalone LLVM pass trace has been obtained. No installation is needed.

Run the committed local validation harness after building both executables:

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
`target/keel-llvm-validation/manifest.json` records every command/output,
compiler and exporter binary hashes, source hashes, source revision/tree,
working-tree status and tool versions. Binary/report artifacts remain ignored.
This is a correctness exercise, not a performance campaign or speed claim.
