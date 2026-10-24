# Explicit IR storage layout

`keel-ir` exposes explicit storage-layout queries so that the same IR types can be
sized for native 64-bit targets and for wasm32. This was stage 1 of the
[wasm32 layout plan](wasm32-layout-plan.md). It adds no target tag to modules or
serialized IR: the caller must keep track of which layout an IR producer used.
Native IR cannot be relabeled as wasm32 IR.

## `DataLayout`

`keel_ir::DataLayout` has a private representation and exactly two public constants:

| Constant | Pointer size / alignment | Byte order |
| --- | --- | --- |
| `DataLayout::NATIVE64` | 8 / 8 | little endian |
| `DataLayout::WASM32` | 4 / 4 | little endian |

The `const fn` accessors `pointer_bytes(self)` and `pointer_align(self)` return
`u64`. There is no `Default`, arbitrary constructor, host inference or target
registry. `Target::data_layout()` maps each `Target` to one of these constants (see
[target foundation](wasm-target-foundation.md)).

## Layout queries

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

### Initializers

Initializer items are packed with no implicit padding. `DataItem::Ptr` uses the
selected pointer width; integer items keep their typed width; byte and zero items
keep their counts. A pointer followed by an i32 is 8 bytes under `WASM32` and 12
natively. `RttiName` remains an 8-byte native tagged address under both models;
Wasm backends must reject it. `DataInit::checked_size_with_layout` uses checked
summation; the existing `DataInit::size()` keeps its native behavior. Object
alignment is not added as trailing padding. These queries do not validate item
types or relocations; that is the verifier's job.

## Layout-aware module verification

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

## Testing

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
