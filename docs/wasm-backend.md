# WebAssembly backend

This reference describes current implementation contracts and limits. Use
[testing](testing.md) for validation commands; per-run results belong outside Git.

<a id="wasm-c-integer-evaluation-c-integer-constant-evaluation-for-wasm32"></a>
## C integer constant evaluation for wasm32

Implementation note for how `keel-c` evaluates integer constant expressions and
types literals under the wasm32 ILP32 model. It builds on the
C data model; the public behavior is in the
[freestanding C profile](wasm-c-freestanding.md).

<a id="wasm-c-integer-evaluation-explicit-model-evaluator"></a>
### Explicit-model evaluator

`eval_int_with_layout`, `eval_with_layout`, `eval_constant_with_layout` and
`eval_with_binding_and_layout` take a `CDataLayout`. Operands, address evaluation,
hint arguments, casts, arithmetic, normalization and `fits_i64_with_layout` all use
that same model; recursion never falls back to a native wrapper. The old `eval`,
`eval_with`, `eval_int`, `eval_constant` and `normalize` signatures keep native
behavior for native callers.

<a id="wasm-c-integer-evaluation-usual-arithmetic-conversions-and-literals"></a>
### Usual arithmetic conversions and literals

`usual_with_layout` preserves integer promotion, nominal identity and rank. With a
32-bit `long`:

- `unsigned int + long` gives `unsigned long`.
- `unsigned long + long long` gives signed `long long`.

Integer literal typing and cast folding use the parser's model. The decimal,
hexadecimal and `U`/`L`/`LL` suffix candidate lists include `long long` and
`unsigned long long` after `long` and `unsigned long`. On LP64 the first fitting
type is unchanged, and the existing `unsigned long long` extension for decimal
values above the signed 64-bit range remains.

`normalize_with_layout` truncates and sign-extends 32-bit `long`, and zero-extends
32-bit `unsigned long`, into `Const::Int(i64)`. Parser arithmetic, comparisons,
conditional expressions, array bounds, alignment expressions, enumerators, case
labels (including duplicate-case detection) and builtin constant arguments all use
the stored model.

<a id="wasm-c-integer-evaluation-pointer-constants"></a>
### Pointer constants

Numeric pointer constants keep the low 32 bits as an unsigned value on wasm32,
including through casts and pointer addition. Pointer difference divides the
address difference before normalizing the `long` result. Symbolic
`Const::Addr(base, addend)` values stay symbolic. A non-native symbolic address cast
to an integer does not evaluate (returns `None`); LP64 keeps its existing
eight-byte behavior.

<a id="wasm-c-integer-evaluation-unchanged-behavior"></a>
### Unchanged behavior

The general evaluator keeps its wrapping arithmetic, masked shifts and saturating
float-to-integer casts; it is not a strict ISO integer-constant-expression checker.
Division by zero returns `None`. The x86 immediate-operand and vector-broadcast
constant proofs remain native-only and reject the wasm32 model (see
scalar declarations); they continue to reject
invalid shifts, signed overflow, MIN/-1 division and invalid FP casts on native
targets.

<a id="wasm-c-integer-evaluation-testing"></a>
### Testing

Unit tests alternate the native and WASM32 models within one arena to catch hidden
model state. They cover literal candidate identity, rank and signedness, nested
arithmetic, casts and conditionals, truncation and sign normalization, unsigned
division and shifts, numeric pointer casts, addition and difference, member
address recursion across pointer wrap, symbolic address handling, and comma and
short-circuit safeguards. PCLMUL and AES intrinsic tests check that the native
proof boundary still rejects undefined constant expressions.

The fixture
[`wasm-integer-constants.c`](../crates/keel-c/tests/fixtures/wasm-integer-constants.c)
is an independent Clang oracle for type identities and constant values:

```sh
clang --target=wasm32-unknown-unknown -std=gnu11 -ffreestanding \
  -nostdinc -Werror -fsyntax-only \
  crates/keel-c/tests/fixtures/wasm-integer-constants.c
cargo test -p keel-c
```

The oracle deliberately excludes invented-address pointer arithmetic, out-of-range
literal fallbacks and wrapping or saturating evaluator extensions, which are not
portable C.

<a id="wasm-ir-layout-explicit-ir-storage-layout"></a>
## Explicit IR storage layout

`keel-ir` exposes explicit storage-layout queries so that the same IR types can be
sized for native 64-bit targets and for wasm32. This was stage 1 of the
wasm32 layout plan. It adds no target tag to modules or
serialized IR: the caller must keep track of which layout an IR producer used.
Native IR cannot be relabeled as wasm32 IR.

<a id="wasm-ir-layout-datalayout"></a>
### `DataLayout`

`keel_ir::DataLayout` has a private representation and exactly two public constants:

| Constant | Pointer size / alignment | Byte order |
| --- | --- | --- |
| `DataLayout::NATIVE64` | 8 / 8 | little endian |
| `DataLayout::WASM32` | 4 / 4 | little endian |

The `const fn` accessors `pointer_bytes(self)` and `pointer_align(self)` return
`u64`. There is no `Default`, arbitrary constructor, host inference or target
registry. `Target::data_layout()` maps each `Target` to one of these constants (see
target foundation).

<a id="wasm-ir-layout-layout-queries"></a>
### Layout queries

```rust
// Layout
pub fn checked_size_with_layout(&self, layout: DataLayout) -> Option<u64>;
pub fn align_with_layout(&self, layout: DataLayout) -> u64;
pub fn leaves_with_layout(&self, layout: DataLayout) -> Vec<(u64, Type)>;
// DataItem
pub fn size_with_layout(&self, layout: DataLayout) -> u64;
// DataInit
pub fn checked_size_with_layout(&self, layout: DataLayout) -> Option<u64>;
```

- Field alignment, padded size, array stride and leaf offsets all use the same
  model recursively.
- Size queries check addition, alignment rounding and multiplication, returning
  `None` on overflow rather than saturating. A zero-count array still checks its
  element's size.
- Empty structs have size 0 and alignment 1; zero-count arrays keep their element
  alignment.
- Non-pointer storage is unchanged, including F80's 16-byte aggregate storage.
  Knowing a type's layout does not make F80 or SIMD values legal in Wasm.

Example: `{ptr, i8}` is size 8, alignment 4, leaves at 0 and 4 under `WASM32`;
size 16, alignment 8, leaves at 0 and 8 under `NATIVE64`.

The existing no-argument methods (`Layout::{size, checked_size, align, leaves}`,
`DataItem::size`) keep native semantics by delegating to `NATIVE64`. `PTR_BYTES`
stays 8, `Type::Ptr.bytes()` stays `None`, and the text format is unchanged.

`leaves_with_layout` requires a successful checked size under the same model
first, and callers must bound the leaf count before materializing it. It is not a
validating or streaming API: offset overflow panics if reached rather than
wrapping.

<a id="wasm-ir-layout-initializers"></a>
#### Initializers

Initializer items are packed with no implicit padding. `DataItem::Ptr` uses the
selected pointer width; integer items keep their typed width; byte and zero items
keep their counts. A pointer followed by an i32 is 8 bytes under `WASM32` and 12
natively. `RttiName` remains an 8-byte native tagged address under both models;
Wasm backends must reject it. `DataInit::checked_size_with_layout` uses checked
summation; the existing `DataInit::size()` keeps its native behavior. Object
alignment is not added as trailing padding. These queries do not validate item
types or relocations; that is the verifier's job.

<a id="wasm-ir-layout-layout-aware-module-verification"></a>
### Layout-aware module verification

```rust
pub fn verify_module_with_layout(module: &Module, layout: DataLayout)
    -> Result<(), Vec<VerifierError>>;
```

This runs every existing structural, function, reference, debug, type and metadata
check on the supplied module. Only initializer item cursors, pointer alignment and
relocation-target sizes depend on the layout. `verify_module` delegates to
`NATIVE64`, so native diagnostics are unchanged.

Under `WASM32`, adjacent pointer items consume four bytes each and pointer-plus-i32
consumes eight; no padding is invented. Relocation addends are checked against the
target initializer's checked size under the same model, and an addend equal to the
size (one-past) is accepted. Cursor or size overflow produces a diagnostic instead
of wrapping or panicking; later items are still type- and symbol-checked.

RTTI names keep their native 8-byte size and alignment, and function-pointer
relocations keep their existing symbol-kind rules. Passing verification does not
make these features legal in a backend: the Wasm backend rejects RTTI and function
relocations separately. Always verify the actual module; do not replace
initializers with zero items to get past native verification.

The verifier does not enforce a backend's address-space or memory limits; zero runs
can describe objects larger than 4 GiB, which the Wasm backend's placement then
rejects.

<a id="wasm-ir-layout-testing"></a>
### Testing

- `crates/keel-ir/tests/data_layout.rs`: both models, nested pointer fields and
  arrays, empty members, overflow in addition, products, rounding and padding,
  initializer sizing (including RTTI), unchanged vector-type storage and native
  text serialization.
- `crates/keel-ir/tests/layout_verifier.rs`: real WASM32 modules with adjacent
  pointers and pointer/i32 records, native-versus-Wasm alignment and cursor
  diagnostics, one-past and out-of-bounds addends (including near 2^32 via zero
  runs), unknown/external/TLS/function relocation controls, RTTI, malformed items,
  and the existing golden corpus through both entry points.

```sh
cargo test -p keel-ir --test data_layout --test layout_verifier
```

<a id="wasm-memory-backend-wasm32-memory-backend"></a>
## wasm32 memory backend

`keel_wasm::emit_wasm32_memory_module` extends the
[scalar backend](wasm-backend.md) with one fixed linear memory, loads and
stores, pointers and narrow-integer conversions. It uses the
[explicit IR storage layout](wasm-backend.md). The runtime profile
(`emit_wasm32_runtime_module`, see the [crate README](../crates/keel-wasm/README.md))
builds on this contract and adds static data and a software stack; the CLI always
uses the runtime profile.

This page is the reference for memory, pointer and conversion semantics shared by
both profiles.

<a id="wasm-memory-backend-api-and-memory-ownership"></a>
### API and memory ownership

```rust,ignore
use keel_wasm::{emit_wasm32_memory_module, Export, Options, Wasm32MemoryOptions};
let bytes = emit_wasm32_memory_module(&module, &Wasm32MemoryOptions {
    scalar: Options {
        exports: vec![Export { name: "read".into(), function: "read".into() }],
    },
    pages: 1,
    export_name: "memory".into(),
})?;
```

`emit_module` and its accepted subset are unchanged; only the memory and runtime
entry points accept the features below. All return complete validated bytes or a
`Diagnostic`, without partial output or input mutation. Supply raw, unoptimized IR
produced for wasm32; the caller owns that layout provenance.

The module defines exactly one owned, unshared memory, exports it under
`export_name`, and has no imports. Initial and maximum size both equal `pages`
(64 KiB units, 0..=65536). Validation uses u64 arithmetic, so the 4 GiB maximum is
representable; whether a host can allocate it is a separate matter.

The memory export name must be nonempty and distinct from every function export
name. Any valid UTF-8 string is accepted (including non-ASCII and embedded NUL).
Function exports follow the scalar rules.

Memory starts zeroed. The host may write the exported memory and observe effects
after synchronous calls. There are no data segments in this profile, no allocator,
no growth instructions and no imported or extra memories. The low/null region is
**not reserved or protected**: address zero is ordinary memory. The engine traps if
any byte of an access lies outside memory; there are no per-object, lifetime or
stack bounds.

<a id="wasm-memory-backend-pointers"></a>
### Pointers

Function parameters, results and direct-call signatures keep the plain i32/i64
scalar contract. Pointer and i8/i16 signatures are rejected even on internal
functions. A host passes an address as an i64 scalar and the IR converts it with
`inttoptr.ptr`; an i32 address must first be widened with `uext.i64`:

```text
func @read(i64) -> i64 {
block0(v0: i64):
    v1 = inttoptr.ptr v0
    v2 = load.i8 v1
    v3 = uext.i64 v2
    return v3
}
```

Pointer values live in Wasm i32 locals and occupy four bytes in memory
(`DataLayout::WASM32.pointer_bytes()`). The IR verifier keeps i64 pointer offsets
and conversions, with these wasm32 semantics:

- `inttoptr.ptr(i64)` keeps the low 32 bits.
- `ptrtoint.i64(ptr)` zero-extends the unsigned address to i64. Consequently
  `ptrtoint(inttoptr(x))` is not an identity: high bits are lost.
- `ptradd(ptr, i64)` adds the low 32 bits of the offset with i32 wrap-around.
  Every memory instruction uses memarg offset zero, so no folded offset can move
  this wrap after the engine's non-wrapping effective-address calculation.
- Pointer `eq`, `ne`, `ult`, `ule`, `ugt`, `uge` compare normalized address bits.
  Signed pointer comparisons are rejected.
- Pointers may flow through `select`, block parameters and parallel edge copies.
  There are no function addresses, tables or pointer-call signatures.

`iconst.ptr` is not valid IR; form a constant pointer with `iconst.i64` followed by
`inttoptr.ptr`. Integer constants of i8/i16/i32/i64 truncate to their width.

<a id="wasm-memory-backend-loads-and-stores"></a>
### Loads and stores

- i8/i16/i32/i64 accesses touch exactly 1/2/4/8 bytes, pointers exactly 4, in
  little-endian order. FP, i128 and vectors are rejected.
- Unaligned accesses work. Alignment flags must be nonzero powers of two; the
  encoded hint is capped at the natural width and never widens an access or adds
  an alignment trap.
- `notrap` is a producer promise. It does not suppress engine bounds checks and the
  backend never removes or hoists loads because of it.
- Volatile and ordinary memory operations are emitted in IR layout order, including
  across calls. Volatile accesses are neither atomic nor fences; shared memory and
  atomics are unsupported.

<a id="wasm-memory-backend-narrow-integers"></a>
### Narrow integers

i8 and i16 values are kept as canonical zero-extended bit patterns in i32 locals.
Loads zero-extend; `sext` interprets the source sign bit; `uext` preserves unsigned
bits; `trunc` discards high bits and renormalizes. All widening and narrowing pairs
among i8/i16/i32/i64 are supported, implemented with MVP shifts rather than the
optional sign-extension instructions. Narrow stores write only the low byte or word.

Narrow arithmetic, comparisons and switches are rejected. i8/i16 values may be
shift counts for i32/i64 shifts, with the usual modulo-width count semantics.

<a id="wasm-memory-backend-rejected-in-this-profile"></a>
### Rejected in this profile

Both entry points verify the IR and then check the whole module (unused
declarations, callee and indirect-signature tables, unexported functions,
unreachable code) before encoding. Beyond the scalar rejection list:

- All `Module.data` and function data-symbol tables are rejected here; static data
  is a runtime-profile feature.
- Stack slots, stack addresses, dynamic stack allocation and all intrinsics are
  rejected; the software stack is a runtime-profile feature. Ordinary recursion
  uses Wasm locals, and call-stack exhaustion is engine behavior.
- Imports, indirect and tail calls, variadics, aggregate or extended-FP ABI,
  explicit operand ABI, multiple returns, `returns_twice`, exception handling,
  weak/ODR, lifecycle, retain and section metadata, and architectural
  instructions are rejected.

The encoder and the independent `wasmparser::Validator` use the WebAssembly 1.0
feature set.

<a id="wasm-memory-backend-testing"></a>
### Testing

`crates/keel-wasm/tests/memory.rs` covers the fixed maximum encoding, export name
errors, every integer and pointer access width with byte canaries, unaligned
addresses, end-of-memory and OOB accesses (with no partial OOB writes),
host-written bytes, high-bit pointer normalization, negative and wrapping offsets,
all 18 conversion width combinations, narrow values through CFG edges, volatile
ordering across calls and recursion, and unused unsupported contracts. Node checks
results against independent BigInt and byte oracles. The 65536-page case is
validated without being instantiated.

```sh
cargo test -p keel-wasm --test memory
```

<a id="wasm-scalar-backend-scalar-webassembly-backend"></a>
## Scalar WebAssembly backend

`keel_wasm::emit_module` is the base of the `keel-wasm` backend, as proposed in
the feasibility evaluation. It takes a `keel_ir::Module`
and returns a complete core WebAssembly binary for pointer-free integer code. It
has no memory, data, runtime or C ABI. The
[memory backend](wasm-backend.md) and the runtime profile in the
[crate README](../crates/keel-wasm/README.md) build on the same lowering. Native
instruction selection, register allocation and code generation are not involved.

<a id="wasm-scalar-backend-api"></a>
### API

```rust,ignore
use keel_wasm::{emit_module, Export, Options};
let bytes = emit_module(&module, &Options {
    exports: vec![Export {
        name: "public_name".into(),
        function: "ir_function_name".into(),
    }],
})?;
```

`emit_module(&Module, &Options) -> Result<Vec<u8>, Diagnostic>` borrows the input
without modifying it. `Diagnostic` implements `Display` and `Error`; the message is
in `.0`.

- Export names are exact UTF-8 strings, may differ from IR names, and must be
  unique. Every exported function must be defined. Local and hidden IR functions
  may be exported.
- An empty export list exports nothing; an empty module is valid. There are no
  implicit exports, imports, underscore prefixes, start function, memory or table.

Emission first runs `keel_ir::verify_module`, then checks the entire module against
the supported subset, including unexported functions, unused callee-table entries,
declarations and indirect signature tables. Encoding starts only after that check
succeeds, and `wasmparser::Validator` (with the WebAssembly 1.0 feature set)
validates the finished bytes before they are returned. A declaration without a
definition is rejected even if unused, and direct callee-table entries must name a
definition with an exactly matching signature. Writing the bytes to a file is the
caller's job.

<a id="wasm-scalar-backend-accepted-subset"></a>
### Accepted subset

Function parameters and the optional single result are plain i32/i64 with normal
purpose, no extension descriptors, not variadic, and `cc(c)`. That spelling selects
this scalar value contract; it is not a native C calling convention. `cc(fast)`, x86
vector ABI metadata, `returns_twice`, aggregate boundaries, multiple results and
explicit call operand ABI descriptors are rejected.

The supported instruction list is exhaustive:

- `iconst.i32`, `iconst.i64`; i32 constants truncate to 32 bits.
- i32/i64 `iadd`, `isub`, `imul`, `udiv`, `sdiv`, `urem`, `srem`, `and`, `or`,
  `xor`, `shl`, `ushr`, `sshr`, `rotl`, `rotr`.
- All ten i32/i64 integer comparison predicates, producing canonical i8 0/1.
- Scalar `select`, direct `call` to a defined function, `return`, `jump`, `brif`,
  and i32/i64 `switch`, with per-edge block arguments and a default edge.
- Internal i8 constants and transport through block arguments, `select` and
  conditions. Constants truncate to 0..255. There are no i8 signatures, i8
  arithmetic/comparison/switch or narrow conversions in this profile.
  Shift counts may be i8/i32/i64 independently of the data width.
- `ineg` on i32/i64 (wrapping `0 - x`), added later for all three profiles.

Semantics:

- Addition, subtraction and multiplication wrap at the data width.
- Shifts and rotates reduce the count modulo the width, including negative and
  mixed-width counts.
- Division and remainder trap on zero; signed division also traps on MIN/-1.
  Signed remainder MIN/-1 is zero.
- Any nonzero integer condition is true, including an i64 with only high bits set.

Everything else is rejected, including other unary operations (bit counting, byte
swaps), conversions, i16/i128, all FP/FMA, high/overflow multiplication, vectors,
intrinsics, inline assembly, pointers, data, loads/stores, atomics, stack slots and
allocation, indirect/tail calls, exception handling and explicit trap instructions.
There is no silent native, helper, import or approximate-FP fallback. Weak/ODR,
lifecycle, used/retain, custom sections, personality, stack-slot/data-symbol tables
and indirect signature tables are rejected. Optimization hints and debug metadata
are ignored and produce no sections.

<a id="wasm-scalar-backend-cfg-and-alias-lowering"></a>
### CFG and alias lowering

Each active SSA definition gets its own typed Wasm local; entry block parameters use
the function parameter locals. Operand reads resolve aliases with
`Function::resolve`, while definitions write to their raw value's local so that a
surviving aliased instruction cannot overwrite its replacement. Removed instructions
and blocks are skipped; unsupported instructions still in the layout are rejected
even in unreachable blocks.

In general, a structured outer loop dispatches on an i32 program-counter local with
an if-chain, one arm per IR block. This handles irreducible CFGs without
reconstructing source nesting. Only the taken edge's argument copies run: all
sources are pushed on the Wasm operand stack first, then popped into destination
locals in reverse order, which gives parallel-copy semantics for self-edges,
swaps, cycles and different arguments on two edges to the same block. The operand
stack is empty whenever the PC is set. Falling out of the dispatcher traps.

Simple shapes skip the dispatcher (straight-line single-return functions and a
terminal two-way `if/else`); see the [crate README](../crates/keel-wasm/README.md).
This lowering makes no code-size or performance claim.

<a id="wasm-scalar-backend-dependencies"></a>
### Dependencies

The crate depends on `wasm-encoder = 0.245.1` (feature `std`) and
`wasmparser = 0.245.1` (features `std`, `validate`, `features`,
`prefer-btree-collections`), pinned exactly. This avoids component, SIMD, serde
and hash-table features. Validation uses the separate parser/validator, not byte
snapshot comparison.

<a id="wasm-scalar-backend-testing"></a>
### Testing

`crates/keel-wasm/tests/scalar.rs` requires `node` on `PATH` (validated with Node
v24) and fails if it is missing. Every accepted fixture passes wasmparser
validation and Node's `WebAssembly.validate`, compilation, instantiation and
execution, with BigInt reference results for wrapping, signed and unsigned
arithmetic. Fixtures cover diamonds, nested loops, multiple returns, switches at
both widths, irreducible two-entry loops, self-edge and cyclic phi swaps, distinct
arguments to the same block, aliases and removed blocks, recursion, explicit
exports, mixed-width shifts, arithmetic extrema and division traps. Negative
fixtures pass the IR verifier, so they test backend rejection rather than malformed
IR.

```sh
cargo test -p keel-wasm --test scalar
```
