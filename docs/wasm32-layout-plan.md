# Bounded wasm32 C layout implementation plan

Audited source: main `666093b8`, 2026-10-24. This is a proposed implementation
contract, not implemented C target support. Read together with
[Wasm feasibility](wasm-feasibility.md), [userspace ISA status](userspace-isa-status.md),
[C x86 intrinsics](c-x86-intrinsics.md), and [GNU modes](gnu-vector-mode.md).
The supplied AGENTS instructions apply; no local/ancestor AGENTS.md was found.
Only this document and `suite/wasm32-layout/declarations.c` belong to this task.
The backend worker owns the new crate; the parent owns interface agreement,
foundation delegation, integration, and independent review. No production code,
lockfile, feasibility document, native contract, or target default changes here.

The private [C integer expression/evaluator checkpoint](wasm-c-integer-evaluation.md)
records the later bounded helper migration; the production target and lowering
stages below remain a plan. The later [private scalar lowering checkpoint](wasm-c-scalar-lowering.md)
implements a test-only integer-function subset. The subsequent
[private pointer/data checkpoint](wasm-c-pointer-data.md) adds bounded storage
and runtime tests; production target selection remains gated. The later
[public target foundation](wasm-target-foundation.md) exposes the exact target
identity and storage query while explicitly rejecting public C, preprocessing,
optimization and native output routes. It does not enable the guarded C profile.

## Recommendation and handoff contract

Land additive IR memory-layout queries first, without adding a `Target` variant.
Then migrate C width-dependent decisions under an explicit per-compilation model.
Only expose `Wasm32UnknownUnknown` together with frontend rejection gates and
explicit rejection in every native entry point. Start C with no optimizer passes.
Retain `ptradd(ptr, i64)`, `ptrtoint -> i64`, and `inttoptr(i64)` in the IR.
Define wasm32 normalization at those operations; do not change the verifier's
operand types or make `Type::Ptr.bytes()` host-dependent.

Proposed shared seam for parent to confirm with backend owner:

- `keel_ir::DataLayout::WASM32` specifies four-byte pointer storage/alignment;
  `DataLayout::NATIVE64` preserves current eight-byte behavior. Neither is an ABI
  classifier or a runtime configuration. Do not infer either from the host.
- The separate emitter continues to accept `&Module` and explicit backend options,
  and return complete bytes or a diagnostic. Its first scalar result need not
  depend on these additions. The future memory profile must use `WASM32` queries
  and the normalization below. Exact emitter function/options/error names remain
  the backend owner's proposal, to be confirmed through the parent.
- A module has no target field (`keel-ir/src/module.rs:9`). The caller owns the
  explicit target/layout pairing. Raw IR cannot prove whether C field offsets
  were produced for LP64 or ILP32. Never relabel native C IR as wasm32. Keep
  target-tagged artifact transport unavailable for Wasm until separately audited.
- No edits to optimizer context, native register allocation, assembly IR, calling
  convention framework, or module serialization are prerequisites for stage 1.

## Exact additive IR API proposal

Add `crates/keel-ir/src/data_layout.rs`, exported from `src/lib.rs`. The existing
[Layout implementation](../crates/keel-ir/src/layout.rs),
[data initializer representation](../crates/keel-ir/src/data.rs), and
[target enum](../crates/keel-ir/src/target.rs) are the foundation seams:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLayout { /* private pointer width/alignment */ }
impl DataLayout {
    pub const NATIVE64: Self; // pointer size/alignment 8, little endian
    pub const WASM32: Self;   // pointer size/alignment 4, little endian
    pub const fn pointer_bytes(self) -> u64;
    pub const fn pointer_align(self) -> u64;
}
```

Only these two validated constants are needed; no public arbitrary-layout
constructor, endianness matrix, `Default`, or general target registry. Scalar
non-pointer storage remains as today (including the existing 16-byte storage
rule for F80); storage knowledge never authorizes an unsupported value type.

Add methods, preserving every existing no-argument method/signature:

```rust
// impl Layout
pub fn checked_size_with_layout(&self, layout: DataLayout) -> Option<u64>;
pub fn align_with_layout(&self, layout: DataLayout) -> u64;
pub fn leaves_with_layout(&self, layout: DataLayout) -> Vec<(u64, Type)>;
// impl DataItem
pub fn size_with_layout(&self, layout: DataLayout) -> u64;
// impl DataInit
pub fn checked_size_with_layout(&self, layout: DataLayout) -> Option<u64>;
```

`Layout::{size,checked_size,align,leaves}` and `DataItem::size` delegate to
`NATIVE64`; retain `DataInit::size`'s existing native behavior and `PTR_BYTES = 8`
as a compatibility constant. New Wasm callers must check sizes before invoking
`leaves_with_layout`, just as native ABI entry points validate aggregate sizes
before convenience traversal. Recursion must pass the same layout through every
field alignment, array stride, padded size and leaf collection. Check addition,
alignment rounding, and multiplication; `DataInit` uses checked summation.

Concrete consumers and why:

- `keel-ir/src/layout.rs:29-94`: current recursive field/array operations call
  native `align/size`, and `scalar_bytes` substitutes `PTR_BYTES`. Migrate the
  recursion, leaving compatibility wrappers at the boundary.
- `keel-ir/src/data.rs:71-111`: `Ptr` is currently documented/sized as eight bytes.
  Make its new query four under `WASM32`; integer items retain their typed width,
  and byte/zero items retain their counts. `RttiName` is a native tagged **eight**
  byte item, not a portable pointer: keep its size eight and reject it in Wasm
  preflight. Update the pointer documentation to distinguish default/query sizes.
- Future `keel-wasm` data placement/fixup and memory preflight must use the new
  queries consistently for both total size and each item's cursor advance.
  Native `keel-codegen`, `keel-asm`, ABI consumers and existing tests keep wrappers.
  Do not mechanically replace all native uses of `PTR_BYTES`.
- There is deliberately no `Layout::Union`. C unions/packing already have explicit
  offsets in `StructDef`; `Types::abi_layout` is a native ABI classification
  approximation, not a new source of Wasm field offsets.

Foundation acceptance: `{ptr,i8}` has size/alignment 8/4 and leaf offsets 0,4 on
Wasm, 16/8 and 0,8 through old methods. Test nested arrays/structs, empty elements,
overflow and pointer initializer followed by an integer/bytes. A `Ptr` plus i32
has size eight on Wasm, twelve natively. Native tests/serialization are unchanged.

## C model and width migration

The public `keel_c::compile(src, path, Options)` and `Options { debug_info, target }`
remain unchanged. Do not add a required public options field. Internally introduce
`CDataLayout` (private to `keel-c`) with `NATIVE_LP64`, `WASM32`, and a memory-layout
query. It controls long width and pointer storage, not C nominal identity or
native char/long-double policy. Proposed internal signatures:

```rust
impl IntKind {
    pub fn size_with_layout(self, layout: CDataLayout) -> u64;
    // size(), rank(), signed(), to_unsigned() remain available
}
impl Ty<'_> {
    pub fn ir_with_layout(&self, layout: CDataLayout) -> Type;
    // ir() retains native LP64 semantics
}
impl<'a> Types<'a> {
    pub fn new(structs: &'a [StructDef<'a>], layout: CDataLayout) -> Self;
    // size/align/atomic_size/ABI-layout methods use stored model
}
```

`Types` is currently a tuple struct in a private module (`types.rs:638`); changing
its internal construction is permitted without changing public crate APIs.
Replace `Types(&structs)` with the constructor, including parser `types()`
(`parse.rs:925`) and generator `types()` (`irgen.rs:559`). Store the model in the
parser; generator uses that same parser model. Existing `layout_struct(types,...)`
signature remains sufficient. New model constants can be tested before a public
Wasm target exists; all production target-to-model mappings stay LP64 until the
guarded target commit.

Width-dependent helper signatures must also carry the model; examples to implement:

```rust
fn int_literal_type(value: u64, decimal: bool, unsigned: bool, longs: u8,
                    layout: CDataLayout) -> Ty<'static>;
pub(crate) fn usual<'a>(a: &Ty<'a>, b: &Ty<'a>, layout: CDataLayout) -> Ty<'a>;
pub fn normalize(v: i64, ty: &Ty, layout: CDataLayout) -> i64;
pub fn eval_with(e: &Expr<'_>, nonnull: &dyn Fn(usize) -> bool,
                 layout: CDataLayout) -> Option<Const>;
```

These are private-module functions despite their `pub` spelling. Thread the same
argument through `eval`, `eval_int`, `eval_constant`, recursive address evaluation,
`fits_i64`, and callers; do not let constant folding silently use native wrappers.
Migration is not complete until all `.size()` on `IntKind` and `.ir()` on `Ty`
under `keel-c/src` are classified as explicit-model or proven width-invariant.

The important call sites are:

- `types.rs:589-601,641-689`: integer IR type and size/alignment (currently long,
  long long, double and pointer are all eight). wasm32 long becomes I32; long
  long stays I64. Plain char remains signed for this triple.
- `expr.rs:3014-3124`: `promoted`, `usual`, `int_literal_type`. Keep rank ordering
  `int < long < long long` even when int/long both occupy four bytes. Small
  integer promotion to int remains valid; default float promotion remains double.
  `unsigned int + long` becomes unsigned long; `unsigned long + long long`
  becomes signed long long. Extend the unsuffixed and L/UL literal candidate
  sequences with long long alternatives: current LP64 shortcuts omit them.
  Preserve current native literal choices, including the existing extension
  fallback outside the representable standard candidate list.
- `expr.rs:742-819`: pointer element scale, `size_expr`, pointer difference type;
  choose unsigned long for sizeof and signed long for ptrdiff, with wasm32 widths.
  `irgen.rs:2591-2615` must truncate the computed i64 element difference to I32.
  Subtract zero-extended pointer addresses in i64 and divide by element size
  **before** truncation; a defined ptrdiff result must fit signed long.
- `eval.rs:42-66,126-132,272`: normalize 32-bit long constants, casts and numeric
  pointer constants. Keep symbolic base/addend expressions until fixup; preserve
  rejection of negative/unrepresentable data addends. `irgen.rs`'s standalone
  `fold_int`/constant-proof helpers also need the model, as do vector broadcasts,
  overflow builtins, initializer packing and bit-field container widths.
- `parse.rs:2325,2759-2780`: split DI from word/pointer. wasm32 DI selects nominal
  long long; word/pointer select int, not long. SI stays int; QI/HI/TI stay their
  existing nominal types. Unsigned bases select unsigned counterparts. On native
  LP64 DI/word/pointer still select long. Preserve double-underscore spellings.
- `parse.rs:3656` and `types.rs:907`: ordinary struct/union/array layout uses the
  model, preserving explicit alignments/packing. Initially diagnose wasm32
  bit-fields and packed records until their target-specific rules have oracles;
  do not claim the AAPCS64 bit-field algorithm implements this ABI.
- `irgen.rs:634,862-887,3175-3247`: scalar ABI types, SSA locals, pointer spill
  slots (currently hardcoded 8/8), casts, FP/integer conversions and all loads/
  stores. `debug.rs:98,225-251` also encodes native pointer/integer widths: reject
  Wasm debug output initially, then migrate before enabling it.

The checked fixture demonstrates long/long long nominal distinctions and GNU
mode identities with Clang. A successful sizeof-only test is insufficient.

## Pointer initializers and IR semantics

`irgen.rs:997-1116` builds object bytes and symbolic pointer relocations. Currently
it requires address width eight, checks offset alignment modulo eight, and
advances `pos` by eight. Change all three decisions to the compilation's pointer
width/alignment together. For the first subset permit symbolic relocations only
in actual pointer-typed storage; diagnose integer-valued symbolic address
initializers until separately specified. Numeric null/high-bit pointers are
normalized then written as exactly four little-endian bytes. Retain the u32
`DataItem::Ptr.addend` and rejection of negative or too-large addends.
Validate relocation bounds/overlap before slicing; alignment diagnostics must
use four, while packed pointer relocation support remains deferred.

For `struct { char tag; char *p; long n; } = {1, data+3, 7}`, expected wasm32 items
are four bytes including tag/padding, a four-byte symbolic pointer at offset 4,
and the four-byte long at offset 8; total 12, alignment 4. Test arrays of these
and adjacent sentinels to catch a stale `pos = offset + 8`. Resolve symbol kinds:
data address plus addend must fit u32 and lie in the configured module allocation
contract. Until table support exists, reject function-address relocations,
indirect calls and external data. `RttiName`, TLS and native section/linkage
metadata need explicit rejection, including unused objects.

Keep verifier signatures at `verifier.rs:1060-1066,1935-1936`. For the wasm32
profile define the following invariant for every produced pointer:

- Physical pointer values/locals/loads/stores are i32; null is zero.
- `inttoptr(i64 x)` uses low 32 bits. `ptrtoint(p)` zero-extends its unsigned i32
  representation to i64, including addresses with bit 31 set.
- `ptradd(p, i64 d)` computes `(u32(p) + low32(d)) mod 2^32`. Negative offsets
  work; an eventual C dereference still requires a valid object/address.
- Pointer comparisons compare the normalized unsigned address bits; equality
  must agree with inttoptr truncation. Function pointers are outside this stage.
- Do pointer addition as i32 arithmetic before load/store. Wasm's memory
  instruction offset calculation does not wrap like i32 addition; folding a
  wrapping ptradd into a memarg offset can change traps. Start with offset zero
  and test `0xffffffff + 1`, negative offsets, and OOB widths.

These are proposed Keel target semantics, not claims that all C pointer arithmetic
is defined. See the primary [Wasm memory execution rules](https://webassembly.github.io/spec/core/exec/instructions.html#memory-instructions).
`ptrtoint(inttoptr(x))` is **not** identity in wasm32; high bits are lost. Frontend
constant evaluation must agree with backend normalization. IR text syntax stays
unchanged; raw IR compilation must explicitly select this target. Do not broaden
module/artifact compatibility or feed wasm32 IR to a native backend.

## Macros, headers, declarations and target safety

Use the [Basic C ABI](https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md)
for the selected freestanding triple: ILP32, natural pointer alignment four,
long long 8/8, long double binary128 16/16, and a 16-byte-aligned software stack.
The first release rejects aggregate/complex/vector call boundaries and variadics.
It does not claim the Emscripten long-double alignment variant.

The [Clang WebAssembly target definition](https://github.com/llvm/llvm-project/blob/main/clang/lib/Basic/Targets/WebAssembly.h)
confirms unsigned long size_t, signed long ptrdiff_t/intptr_t, a void-pointer
va_list declaration, binary128 long double, and long long for explicit 64-bit
integer types. Our local declaration oracle agrees. Large array **object**
placement needs separate treatment: the ABI asks for at least 16-byte alignment
for sufficiently large local/global arrays. Keep array type/member alignment
natural. In local Clang `__alignof__(large_array)` still reports one for char[17];
that expression does not prove allocation placement. Before memory support, add
placement checks in generated data/frame evidence and align such allocations to
at least 16. Reject VLAs/dynamic stack allocation at this checkpoint.

`predefined.rs:14-119,360-403` has unconditional LP64/hosted sizes and a Linux/x86
fallback. Add a dedicated wasm32 branch before those definitions, sharing only
proven target-independent values. Emit `__wasm__`, `__wasm32__` (and Clang's
unadorned aliases), `__STDC_HOSTED__=0`, correct long/pointer/size widths and limits,
integer suffix/type families (including signed long sig_atomic_t), empty user
label prefix, and binary128 LDBL format
macros (mantissa 113). Explicit 64-bit and intmax families must use long long,
not long. Derive width families consistently; compare the complete relevant
macro inventory rather than changing only `__SIZEOF_POINTER__`.

Do not emit LP64, Apple, ARM/NEON, x86/SSE/AVX, Linux/ELF/Unix, WASI or Emscripten
macros. Do not copy local Clang's default optional Wasm feature or lock-free
atomic macros: the prototype has no atomics/threads and must reject their use.
Define `__STDC_NO_ATOMICS__=1`, use conservative capability queries, and
diagnose atomic declarations/operations;
lock-free capability macros must not advertise support. Likewise reject SIMD
source types/operations initially rather than silently scalarizing an external ABI.

`preprocess.rs:674-711` must use explicit freestanding include policy: user `-I`
and explicit `-isystem`, then embedded portable headers; no automatic SDK,
frameworks, `/usr/include`, or `sysroot/usr/include` policy borrowed from native
libc. Initially reject `--sysroot` for this target; later sysroot support is an
explicit separate policy. `headers.rs::get` currently has no target argument;
add `get_for_target(name: &Path, target: Target) -> Option<&'static str>` and use
it at include lookup and has-include queries, retaining `get` for native
compatibility if needed. Allow
portable stddef/stdint/limits/float/stdbool/stdalign/stdnoreturn, declaration-only
stdarg, and diagnose compiler-owned x86/private intrinsic headers. `stdint.h`
and `limits.h` already gate include_next on hosted mode. Explicit user include
paths remain the user's inputs; never invent host search paths behind them.

`parse.rs:600-640` needs an explicit Wasm va_list declaration branch (void pointer)
so stdarg can parse; reject va_start/arg/end/copy and variadic callable contracts.
Do not take the current non-Apple SysV fallback. `parse.rs:2670-2708` should use
existing `OpaqueTy { name: "long double", size:16, align:16 }` for Wasm, with
complex long double size32/align16. Typedefs, sizeof/alignof/offsetof, pointer
formation and unused prototypes may parse. Reject evaluated values, conversions,
literals requiring extended FP, definitions/calls with extended FP parameters or
results, and aggregate values containing them, before dead-code pruning. An
unused prototype is declaration knowledge, not an emitted ABI promise. Existing
`Types::unsupported_value` traverses objects but stops at pointers/functions;
explicit signature validation is therefore required. Never lower this format to
F64, F80, I128 arithmetic or an implicit runtime helper. TC mode remains its
existing x86-only declaration extension.

## Variant and CLI migration gates

Adding a fourth `Target` early is unsafe even if exhaustive matches compile:

- Exhaustive matches in `target.rs::{ALL,triple}`, `section.rs::validate`, native
  `keel-codegen/src/lib.rs:326,359,379`, `main.rs::vector_target` and object sinks
  will force decisions. Add explicit unsupported-target errors in native text,
  typed/chunked/object APIs, section validation, assembler/linker/archive and
  other target consumers. Do not map Wasm to AArch64 or x86 selection targets.
- Equality tests/default branches do **not** force review: predefined macros and
  va_list fall through as above; `irgen.rs:672` defaults aggregate classification
  to `abi_layout`; `x86_features.rs:53` treats non-x86 as baseline capabilities.
  `Target::ALL` also expands test loops. Audit usages across every crate and test;
  compiler exhaustiveness alone is insufficient. `Target::host/Default` stay native.
- Add `Target::data_layout(self) -> DataLayout` when the variant lands: current
  three map NATIVE64 and only the new variant maps WASM32. `from_triple` accepts
  exactly `wasm32-unknown-unknown`; no WASI/wasm64 alias promises.

Parent CLI owner must route at the `cc`/`compile` dispatch before
`Options::target()`'s native tuning validation, native optimization context and
`emit_module`'s selector/allocator setup (`main.rs:214,596-622,660-717,770`).
Both dispatch arms currently optimize before emission; guarding only emit_module
is too late, including for `cc --emit-ir`. Validate
Wasm-specific options first. Until C foundation/runtime gates pass, C code
emission returns an explicit unavailable diagnostic; target-aware `-E` and
`--emit-ir` can be enabled independently when their frontend tests pass. A future
Wasm complete-module branch calls the sibling emitter before native
`compile_program_chunks_with_pipeline`/assembler/linker. Select a distinct
complete-module output mode (proposed `--emit-wasm -o file.wasm`), rejecting
`--emit-object`, native assembly, CPU tuning/levels/features, regalloc/isel knobs,
`-g`, `-O`, explicit optimization passes and incompatible report options. Native
CLI flags/default outputs remain unchanged. Final spelling belongs to parent.

## Optimizer safety and acceptance sequence

`VectorTarget::Portable` is not a data layout. `OptimizationContext` currently
contains SIMD target/capabilities only (`keel-opt/src/lib.rs:49`); avoid adding a
mandatory public field now. Gate Wasm at CLI before *any* default or requested
pass runs, including `opt --target wasm32...` and report paths. Initial API use
must document unoptimized modules as the supported input pipeline.

Concrete deferred audit: `stack_promote.rs:419-420` and
`stack_forward.rs:192-198,230,307-310` treat unsized pointers as eight bytes;
`instcombine.rs:535,624-657` folds/groups pointer offsets; `const_fold.rs:299`
does not fold ptradd, which is not proof that other passes are safe.
Also inspect memory alias/forward/load narrowing, loop pointer ranges,
`adjacent_diff`, `loop_vectorize`, private aggregate/internal fast-call rewrites,
and automatic vectorization. Require proofs/tests before enabling each pass;
do not merely disable two stack passes and enable the rest. Scalar FP, volatile
access order/exact widths, saturating casts and fused-operation contracts from
the existing native docs remain intact. Unsupported architectural operations
must fail preflight, even if unused code would otherwise disappear.

First three focused source changes, delegated only after parent reviews the
backend's first result:

1. **IR foundation owner:** `feat(ir): add explicit pointer layout queries`.
   Only data-layout export plus `data.rs`/`layout.rs` and unit tests; no variant,
   frontend, native backend or sibling files. Acceptance is the exact size/offset/
   overflow tests above plus existing keel-ir tests. Backend can adopt the query
   in its subsequent memory checkpoint without depending on C support.
2. **C foundation owner:** `refactor(c): thread explicit C integer layout`.
   Internal model, Types construction and width-sensitive helper migration listed
   above; production mappings remain native LP64. Tests directly exercise WASM32
   model long size/IR, usual conversions, literal candidate lists, mode selection,
   constant normalization and aggregate offsets. Existing GNU type/vector and
   native declaration/layout tests must retain results. No public target/CLI yet.
3. **C/target owner, coordinated with parent CLI owner:**
   `feat(c): add guarded wasm32 declaration target`. Introduce the variant only
   together with macro/header/va_list/opaque-FP/rejection changes and explicit
   unsupported branches in native consumers. Parent owns overlapping main.rs
   routing; agree one file owner before starting. Enable preprocessing/declaration
   IR only after frontend fixture and negative gates pass; executable C emission
   remains unavailable until separate data/stack/ABI backend acceptance. If this
   patch cannot be kept reviewable, leave the variant private to tests longer;
   do not publish a half-routed target.

The guarded-target acceptance tests must assert the intended diagnostics for
`-O`/explicit passes, native output paths and native library APIs; reject host
SDK/sysroot discovery, native macros and intrinsic headers. Check portable
header typedef identities/limits, unused extended-FP prototypes versus rejected
definitions/calls, rejected variadic/aggregate boundaries, atomics, SIMD, inline
assembly and dynamic stack use. Keep representative native CLI/layout tests as
controls; no target may silently become another target.

Follow-on memory work is a separate delegation: initializer cursor migration,
scalar C lowering/pointer normalization, four-byte data fixups, software stack,
explicit exports/imports and exact-width memory execution. At that gate test
null/high-bit/truncation, negative offsets and ptrdiff, pointer arrays/initializers,
recursion/all normal-return SP restoration, stack/data overlap and exhaustion,
OOB traps, volatile ordering, memmove overlap and forbidden symbol kinds. Scalar call tests must include signed/unsigned narrow parameters/results and
the existing `ArgExt` metadata; four-byte pointer storage does not authorize
reusing a native call ABI. Backend
first preflight and emitted-module validation must agree on the supported subset.
No aggregate ABI, function tables, varargs, SIMD, WASI or native integration is
implied by declaration tests.

Future Rust validation uses the existing pins/features/shared lock and default
cargo cache, with generated logs/artifacts outside Git. For example:

```sh
python3 suite/fast_opt_locked.py /tmp/wasm32-ir-<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-ir
python3 suite/fast_opt_locked.py /tmp/wasm32-c-<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-c \
  --test gnu_types --test gnu_vector_broadcast
```

Add the focused wasm32 tests when implemented, and run a workspace check with
the same wrapper after adding the target variant. Preserve existing features,
toolchain and warning flags; no shared CARGO_TARGET_DIR, installations, cache
changes, output deletion, remote hosts or performance campaign.

## Declaration oracle actually run

Local `/usr/bin/clang`: Apple Clang 21.0.0 (clang-2100.3.34.2), host arm64 Darwin.
The following passed on 2026-10-24 without Rust builds, objects, linking or a
Wasm engine. The maintained fixture is an independent declaration/layout oracle,
not a Keel acceptance test until the frontend migration is implemented.

```sh
clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding -nostdinc \
  -Werror -fsyntax-only -Xclang -fdump-record-layouts-complete \
  suite/wasm32-layout/declarations.c > /tmp/keel-wasm32-layout-20261024-dab583d1.records.txt
clang --target=wasm32-unknown-unknown -ffreestanding -nostdinc -dM -E \
  -x c /dev/null > /tmp/keel-wasm32-layout-20261024-dab583d1.macros.txt
```

Assertions passed for all integer/mode identities and nested offsets in the
fixture, including Pair 12/4, Payload 16/8, Nested 56/8, Extended 48/16 and pointer
initializer member offsets 4 and 8. Initial exploration incorrectly expected
`__alignof__(large_array)` to report placement alignment 16; Clang reported one.
The fixture now records that distinction explicitly; object placement remains a
future emission gate. Macro capture also shows Clang advertises optional Wasm
features and lock-free atomics; those are not adopted as Keel capabilities.
No runtime ABI, initializer bytes/fixups, actual stack/data allocation alignment,
optimizer compatibility, full target implementation or performance was tested.
