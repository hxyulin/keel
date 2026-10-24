# WebAssembly output feasibility

This is the original design evaluation for WebAssembly output, written as a source
audit of commit `c1a9ef8f95640b5ad428990627ad3b6d0d85f577`. Source links below are
pinned to that commit. The recommendations were later implemented as the
[`keel-wasm` backend](../crates/keel-wasm/README.md), the
[wasm32 layout design](wasm32-layout-plan.md), the
[freestanding C profile](wasm-c-freestanding.md) and the [CLI route](wasm-cli.md);
those pages describe current behavior.

## Recommendation

Proceed only as a bounded new backend experiment. Start with a pointer-free
scalar Keel IR module emitted as a complete core `.wasm`, then make
`wasm32-unknown-unknown` freestanding C the first useful target. Use a separate
`keel-wasm` crate taking `&keel_ir::Module` and explicit options and returning
complete bytes or a diagnostic. Do not extend native register allocation or
route Wasm through native assembly IR. Do not resume x86 optimization for this.

The principal integration risk is wasm32 data layout, followed by CFG/phi
correctness and the runtime contract. Encoding arithmetic opcodes is the smaller
part. wasm64 avoids some width changes but does not establish a C ABI, runtime,
or linker and should not be the initial public target.

## Repository seams and risks

All repository links below pin the audited commit. These are examples of concrete
dependencies, not an exhaustive list of every match that implementation must audit.

### Pointer width and layout: high risk

`Type::Ptr` reports no byte size, which appears target-neutral, but
[`data.rs:71–95`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/data.rs#L71)
defines pointer initializers as eight bytes and `PTR_BYTES = 8`.
[`layout.rs:29–67,88–94`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/layout.rs#L29)
computes aggregate sizes, alignment, and leaf offsets without a target.
[`inst.rs:453–454,560–563`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/inst.rs#L453)
specifies `ptradd(ptr,i64)` and pointer/i64 conversions; the
[`verifier.rs:1060–1066,1935–1936`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/verifier.rs#L1060)
enforces those contracts. A wasm32 emitter cannot silently reinterpret native
pointer-containing layout or truncate all pointer values and call it compatible.

Optimizer assumptions matter even with a correct emitter:
[`stack_promote.rs:419–420`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-opt/src/stack_promote.rs#L419)
and [`stack_forward.rs:192–198,230,307–310`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-opt/src/stack_forward.rs#L192)
default unsized types to eight bytes. Keep target-sensitive memory optimization
disabled for the first wasm32 memory checkpoint until audited. Do not assume
the portable vector target also supplies a portable data layout.

Smallest coordinated change: explicit data-layout queries for pointer size and
alignment, target-aware aggregate/data size queries, and frontend integer/layout
queries. Keep existing native defaults intact. Decide explicitly whether to
retain i64 IR pointer offsets/conversions with documented wasm32 normalization,
or version these operations to target pointer integers. Either choice needs
verifier, frontend, optimizer, serialization, and backend agreement; neither
requires redesigning the whole SSA IR. A Module currently has no target field
([`module.rs:9–24`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/module.rs#L9));
pass the layout explicitly rather than infer it from the host.

### C types, ABI, and preprocessing: high risk

[`types.rs:31–38,589–601,641–689`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/types.rs#L31)
makes `long` eight bytes and pointer size/alignment eight bytes. This affects
literal type selection, promotions, usual arithmetic conversions, struct/union
layout, pointer difference, initializers, bitfields, and typedef identity, not
just `sizeof`. [`irgen.rs:2591–2615,3228–3245`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/irgen.rs#L2591)
uses i64 pointer arithmetic/conversions. GNU `mode(DI)`, `word`, and `pointer`
currently share the eight-byte selection
([`parse.rs:2325`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/parse.rs#L2325));
DI remains 64 bits on wasm32 while pointer mode must follow the target.

[`predefined.rs:45–58,99–107,360–403`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/predefined.rs#L45)
advertises LP64 and native architecture/OS macros.
[`preprocess.rs:674–690`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/preprocess.rs#L674)
selects native system include paths; [`parse.rs:600–640`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-c/src/parse.rs#L600)
constructs native va_list representations. Add a Wasm macro branch and explicit
freestanding include policy; never feed host libc headers to a Wasm target.
Audit bundled headers, builtin recognition, `size_t`, `ptrdiff_t`, `uintptr_t`,
long-double declarations, and atomic capability macros together.

The [Basic C ABI](https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md)
uses ILP32 for wasm32, with binary128 `long double`; address-taken objects live
on a separate linear-memory stack, conventionally aligned to 16 bytes. Scalars
smaller than i32 travel as i32. Its vector ABI is explicitly unstable. Keel's
native aggregate classifications cannot be reused. Existing `Purpose::ByVal`,
`Sret`, and operand ABI descriptors provide useful source contracts
([`function.rs:44–119`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/function.rs#L44)),
but need new Wasm lowering and cross-compiler ABI tests before acceptance.
Initially reject variadics, aggregate call boundaries, extended FP, and
`returns_twice`; permit their ordinary memory layouts only when separately tested.

### Backend architecture and CFG: medium/high risk

[`target.rs:7–21`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/target.rs#L7)
enumerates only three native targets. Adding Wasm must audit exhaustive matches
across CLI, frontend, section validation, optimization context, and output routing;
selecting it must never fall through to a native default.

[`codegen/lib.rs:320–391`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-codegen/src/lib.rs#L320)
dispatches to native text/typed-program/chunk emitters.
[`select.rs:12–15`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-codegen/src/select.rs#L12)
has AArch64/X86 instruction selection; [`regalloc.rs:102–136`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-codegen/src/regalloc.rs#L102)
models physical registers, call preservation, and frame spills. A Wasm local
index is none of these. Assign one typed local per resolved SSA value initially;
let the eventual Wasm engine allocate physical registers.

Keel branches carry block arguments, including separate arguments on each
conditional/switch edge ([`inst.rs:232–246,426–430`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/inst.rs#L232)).
Do not reconstruct source C nesting: goto, optimization, and imported IR can
produce arbitrary or irreducible CFGs. Wasm branches address enclosing structured
labels ([control instructions](https://webassembly.github.io/spec/core/syntax/instructions.html#control-instructions)).
Use a structured outer loop plus program-counter dispatch for the first backend;
an if-chain is sufficient before a nested-block/br_table dispatcher. This handles
arbitrary ordinary CFGs without a general structurizer. Leave operand stacks
empty at dispatch boundaries. Execute only the selected edge's phi copies,
evaluate all copy sources into temporaries before assigning destinations, then
set the PC and continue. This preserves cycles, self edges, and critical edges.
Resolve aliases and skip removed/unreachable definitions through Function APIs.
Improve reducible-CFG structuring and local reuse only after correctness gates;
the dispatcher is a feasibility baseline, not a performance claim.

Cranelift is not a Wasm-output shortcut: Keel uses native ISA lookup and
ObjectModule ([`keel-cranelift/lib.rs:86–104,144`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-cranelift/src/lib.rs#L86)).
Its [official description](https://cranelift.dev/) specifies machine-code output
and native targets; Wasmtime consumes Wasm and uses Cranelift in that direction.
The LLVM exporter is another possible experiment, but currently has only an
AppleArm64 profile and rejects vectors/aggregate ABI
([`keel-llvm/lib.rs:9–44`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-llvm/src/lib.rs#L9)).
Adding LLVM Wasm output still requires frontend/layout/ABI work and external LLVM;
it is not an existing supported output path.

### Modules, data, calls, and linking: high risk if combined too early

Module declarations/definitions and DataObject/DataItem are a good input seam
([`module.rs:9–14`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/module.rs#L9),
[`data.rs:22–80`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/data.rs#L22)).
Preassign types and import/defined function indices, validate symbol kinds and
signatures, allocate aligned data offsets, then encode bodies and data fixups.
Wasm has distinct index spaces and explicit imports/exports/data/element segments
([module specification](https://webassembly.github.io/spec/core/syntax/modules.html)).
An unresolved C function is not automatically an authorized host import: require
an explicit name-to-module/field/signature manifest and reject all other undefined
symbols. No underscore prefixing or implicit libc lookup. Export an explicit list.

Function pointers require a function table, reserved null slot, element entries,
typed indirect calls, and table-index data fixups. They are not linear-memory
code addresses. Resolve `DataItem::Ptr` by symbol kind; reject function-pointer
addends and unknown symbol kinds. Native RTTI tagging, external data, TLS,
weak/coalesced definitions, custom sections, retain, constructors/destructors,
EH, tail-call guarantees, and frame-address/debugtrap semantics need explicit
diagnostics until independently implemented. A writable linear-memory data
segment does not enforce native read-only-page protection.

A complete single-module emitter needs no native linker. Existing
[`keel-link/lib.rs:1`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-link/src/lib.rs#L1)
is Mach-O/ELF. A Wasm relocatable object is a separate deliverable requiring
symbol/linking/relocation and target-feature metadata, not merely a `.wasm`
extension ([object-linking conventions](https://github.com/WebAssembly/tool-conventions/blob/main/Linking.md)).
Use wasm-ld interoperability for that later checkpoint rather than build a Wasm
linker now. Existing target-tagged Keel IR units also are not Wasm objects
([`artifact.rs:1–15,35–49`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-toolchain/src/artifact.rs#L1)).

### Integer, FP, stack, and vectors: semantic gates

Keel specifies modulo-width shifts/rotates, trapping division, nearest-even FP,
NaN-propagating min/max, and saturating FP-to-integer conversions
([`inst.rs:433–464,516–578,625–631`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/inst.rs#L433)).
Wasm i32/i64 shifts and division largely fit; i8/i16 need normalization,
signed extension before signed operations, width-specific shift masks, zero-count
clz/ctz correction, and narrow signed-division overflow guards. Signed remainder
at MIN/-1 is zero in Wasm; do not accidentally implement it through trapping
signed division. Use saturating `trunc_sat`, not trapping `trunc`, for Keel casts.
Regular min/max fit the documented NaN/zero rule, with no exact arithmetic NaN
payload guarantee. Check comparisons separately for ordered/unordered predicates.
These mappings follow the [numeric semantics](https://webassembly.github.io/spec/core/exec/numerics.html).

Initially reject i128, F80, fused FMA, high/overflow multiplication not explicitly
lowered, and target-specific intrinsics. Do not split fused FMA into multiply/add
or assume native libgcc/libm helpers exist in a Wasm host. Subsequent helper
support must define signatures and supply Wasm code or explicit imports. Preserve
volatile access order and exact access widths
([`memory.rs:5–15`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/memory.rs#L5));
unaligned ordinary accesses and out-of-bounds traps are covered by
[memory execution rules](https://webassembly.github.io/spec/core/exec/instructions.html#memory-instructions).
Linear-memory bounds do not protect individual objects or a software stack.

Static slots and dynamic stack save/alloc/restore exist
([`function.rs:356–362`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/function.rs#L356),
[`intrinsic.rs:236–267`](https://github.com/hxyulin/keel/blob/c1a9ef8f95640b5ad428990627ad3b6d0d85f577/crates/keel-ir/src/intrinsic.rs#L236)).
Wasm locals cannot supply their addresses. Use a checked software stack global,
restore the incoming SP on every normal return, and support recursion. Initially
reject dynamic allocation and stack introspection. Memory builtins currently
use i64 lengths (`intrinsic.rs:1693`); a wasm32 lowering must range-check before
narrowing, or reject unsupported lengths. Provide byte-exact loops initially
for bounded memcpy/memmove/memset; memmove must handle overlap. Do not lower an
ordinary external symbol named memcpy as a builtin without its IR contract.

Read [userspace ISA status](userspace-isa-status.md),
[C x86 intrinsics](c-x86-intrinsics.md), and [GNU vector modes](gnu-vector-mode.md)
as native contracts, not portable Wasm promises. In particular MXCSR-observable
conversions and oversized SSE shifts differ from ordinary portable operations.
Reject x86/NEON architectural operations, inline assembly, syscalls, and physical
machine-state operations. Start scalar-only with vectorization disabled.
Later map a bounded generic 128-bit subset to `v128`; Wasm has no core `v256`
numeric type ([value types](https://webassembly.github.io/spec/core/syntax/types.html)).
64-bit vectors need exact-width loads/stores and defined unused lanes; 256-bit
vectors require splitting or scalarization. Test shuffle indices, mask lanes,
min/max, saturation, and shift counts per operation; do not equate equal storage
widths with identical semantics. Keep vector external ABI out of the first release.

## Initial target and runtime contract

The scalar IR smoke test is pointer-free and does not claim a C ABI. The first
useful C checkpoint targets wasm32 ILP32 and one complete core module from one
translation unit: one owned/exported unshared memory, little-endian data, reserved
low null region, aligned static data, fixed bounded downward software stack,
and no heap or memory growth. Check static-data/stack overlap and stack arithmetic
overflow; trap on exhaustion. Reserve low memory but do not claim hardware-style
null-page protection. Explicit scalar exports and direct calls only at first;
optional imports come from a typed manifest and are synchronous, non-reentrant,
and must not unwind through Wasm. A trap invalidates this prototype instance:
the host must discard it because SP restoration is not guaranteed on traps.
No start function, libc, filesystem, process exit, threads, atomics, TLS, EH,
setjmp/longjmp, debugger integration, or dynamic linking is implied.

Allow i32/i64/f32/f64 and tested narrow integer lowering; accept only normal
scalar call ABI parameters/results initially. Require non-trapping FP-to-int
support explicitly if those conversions are used; reject unenabled features.
Choose the binary encoder and validator as implementation dependencies after
repository dependency-policy review; a binary encoder alone is not validation.

wasm64 is a later memory64 profile with i64 memory addresses, not simply a new
triple. The [core type specification](https://webassembly.github.io/spec/core/syntax/types.html#address-types)
allows i32/i64 address types; this does not fix C long-double, function-pointer,
aggregate, or import ABI choices. The Basic C ABI describes LP64 as its wasm64
expectation; verify actual chosen toolchain interoperability independently.
The existing u32 data-pointer addend and stack-slot sizes also remain bounded
representations even when pointer width is 64 bits; widening memory addresses
does not automatically enable arbitrarily large objects or frames.

WASI is a separate host/API/runtime choice, not another spelling for Wasm.
Pin a version if requested later: [WASI documentation](https://github.com/WebAssembly/WASI/blob/main/README.md)
distinguishes Preview 1 from the component-oriented later APIs. A core Preview 1
command would additionally need WASI imports, a compatible sysroot/libc, startup
and `_start`/exit behavior. Later component APIs require adapters/component and
canonical ABI work; none is delivered by this core-module checkpoint.

## Proposed checkpoints

1. **Backend:** new crate, explicit supported-operation preflight, pointer-free
   scalar bytes, typed locals, direct calls, arbitrary ordinary CFG dispatcher,
   atomic output publication. Tests: constant/add exports, diamond, nested loop,
   multiple returns, switch/default, irreducible two-entry loop, self-edge and
   cyclic phi swap, distinct arguments on two edges to the same block. Validate
   every binary independently, then execute finite fixtures in a local engine.
2. **Layout and frontend:** wasm32 target and explicit layout, ILP32 C type
   selection, macros/header isolation, pointer arithmetic/conversion policy,
   target-sensitive optimizer audit. This can proceed alongside checkpoint 1,
   but must land before C pointer/memory support. Tests: sizeof/alignof/offsetof
   for pointer-containing nested structs/unions/arrays; long vs long long rank
   and constants; mode(DI)/pointer; size_t/ptrdiff_t; pointer globals/addends,
   null and high-bit addresses; negative tests for native macros/headers. Native
   layouts must remain unchanged. Start C validation without optimization.
3. **Module and runtime:** explicit export/import manifest, aligned data/fixups,
   static frame software stack, memory operations. Agree the layout, options and
   error API with checkpoints 1 and 2 first.
   Tests: data alignment/BSS/string/global-pointer fixups, unaligned exact-width
   accesses with sentinels, recursion and SP restoration on all returns,
   stack overflow/data overlap, OOB traps, wrong/missing import signatures,
   undeclared imports, and memmove overlap. First useful C test: exported buffer
   checksum with local array and a direct helper call, host supplies offset/len.
4. **Validation and integration:** CLI output branch before native
   assembly/object emission, target/feature diagnostics, reproducible local
   validator/engine harness, and cross-checks with a pinned Clang wasm32 toolchain
   only for the claimed C subset. Differential tests cover integer edge values,
   shift counts, zero clz/ctz, signed div/rem traps, NaN/infinity/signed-zero FP,
   saturating conversions and narrow ABI extension; run supported optimized
   cases only after the layout audit. Reject every excluded family even in unused
   declarations/bodies where its ABI or feature contract matters.
5. **Later, separately:** table-based indirect calls and null/signature traps;
   aggregate ABI/varargs; generic SIMD; relocatable objects plus wasm-ld tests;
   explicitly versioned WASI. Each adds its own acceptance boundary. Do not
   bundle these into the initial emitter or a general backend refactor.

The go/no-go gate is a validated, locally executable scalar CFG module plus a
reviewed wasm32 layout plan. Useful freestanding C additionally requires
checkpoints 2–4. There is no present evidence supporting broad application,
WASI, object-linking, or performance claims.
