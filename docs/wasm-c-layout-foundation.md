# C data model for wasm32

Implementation note for the internal C type-layout model that lets `keel-c` size
types for either native LP64 or wasm32 ILP32. This was the first C step of the
[wasm32 layout plan](wasm32-layout-plan.md). The public behavior is described in
the [freestanding C profile](wasm-c-freestanding.md); the later steps are
[integer evaluation](wasm-c-integer-evaluation.md),
[scalar declarations](wasm-c-scalar-declarations.md),
[scalar lowering](wasm-c-scalar-lowering.md) and
[pointers and data](wasm-c-pointer-data.md).

## The model

`types::CDataLayout` (crate-private) has two constants, `NATIVE_LP64` and `WASM32`,
with `pointer_bytes()` and `pointer_align()` queries. It controls the width of
`long` and of pointer storage. It does not choose `char` signedness, the
long-double format or a calling convention, and it is never inferred from the host.

- `IntKind::size_with_layout(layout)` makes `long` four bytes on wasm32 while
  `long long` stays eight. Nominal type identity, rank and signedness are
  unchanged: `long` still ranks above `int` and below `long long`.
- `Ty::ir_with_layout(layout)` uses that width for integer IR types. Pointers stay
  `Type::Ptr`.
- `Types::new(structs, layout)` stores the model. Integer and pointer
  size/alignment, arrays, complex and atomic storage, and natural record layout use
  it.
- The old no-argument `IntKind::size` and `Ty::ir` keep LP64 behavior for native
  callers.

`StructDef` stores completed layout, and `Types` does not relayout a table it is
given. The record table must therefore be built with the same model that later
queries use; mixing an LP64 record table with WASM32 queries would silently keep
LP64 offsets. Each compilation selects one model before parsing and threads it
through.

## Native-only paths

Native ABI classification is fenced off for the wasm32 model:

- `abi_layout` and `aapcs_layout` assert LP64.
- `complex_abi_layout` and `atomic_size` return `None` for wasm32. Knowing an atomic
  type's storage size does not mean atomics are supported.
- Non-native record layout rejects bit-fields, packed members and `#pragma pack`
  before running the existing (AAPCS64) record algorithm. Ordinary explicit
  alignment uses the existing algorithm.
- Large-array *object placement* alignment is separate from a type's natural
  alignment; it is applied when objects are allocated
  (see [pointers and data](wasm-c-pointer-data.md)).

Expression evaluation, literal typing, GNU modes, initializers, IR generation,
debug info and inline assembly each needed their own migration or rejection; the
follow-up pages above record how each was handled.

## Testing

Unit tests in `crates/keel-c/src/types.rs` cover every integer kind under both
models, the native compatibility wrappers, rank and nominal identity, IR widths,
pointer-containing records, unions, nested arrays, an explicitly aligned
self-referential record, alternating model queries over shared type nodes, and the
non-native ABI, atomic and packing rejections. The full C suite is the native
regression gate.

```sh
cargo test -p keel-c
```
