# Explicit IR memory layout foundation

Stage 1 of the [wasm32 layout plan](wasm32-layout-plan.md) adds explicit storage
queries to `keel-ir`. This provides no new target, C lowering, memory backend,
ABI classifier, optimizer support, or CLI output mode. The caller must retain the
correct association between an IR producer and its layout; modules and serialized
IR do not acquire a target tag. Native IR cannot be relabeled as wasm32 IR.

`keel_ir::DataLayout` has private representation and exactly two public constants:
`NATIVE64` stores and aligns pointers at eight bytes; `WASM32` uses four bytes.
Both describe little-endian storage. The `const fn` accessors `pointer_bytes(self)`
and `pointer_align(self)` return `u64`. There is no `Default`, arbitrary public
constructor, host inference, or general target registry.

The additive interface is:

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

Every recursive field alignment, padded size, array stride and leaf offset uses
the same explicit model. Size queries check addition, alignment rounding and
multiplication, returning `None` on overflow rather than saturating. Even a
zero-count array checks its element's size, preserving native behavior. Empty
structs have size zero/alignment one; zero-count arrays retain element alignment.
Non-pointer storage rules are unchanged, including F80's 16-byte aggregate
storage. Layout knowledge does not authorize F80 or SIMD values in Wasm.

For `{ptr, i8}`, `WASM32` reports size eight, alignment four and leaves at zero
and four. Native queries report size sixteen, alignment eight and leaves at zero
and eight. `Layout::{size,checked_size,align,leaves}` and `DataItem::size` retain
native semantics by delegating to `NATIVE64`; `PTR_BYTES` remains eight.
`Type::Ptr.bytes()` remains `None`. Display/parser formats are unchanged.

`leaves_with_layout` retains the native traversal precondition: first require a
successful checked size with the same model, and independently bound the number
of leaves before materializing the vector. This traversal is not a validating or
streaming API. Offset arithmetic is checked and panics on overflow if reached;
it does not wrap in release builds. A successful byte-size check alone does not
make a huge leaf count safe to allocate. No new recursion-depth limit is added.

Initializer items are packed without implicit padding. `DataItem::Ptr` uses the
selected pointer width; integer items retain their existing typed width, and
bytes/zero items retain their count. The queries do not validate item types or
relocations. `RttiName` remains an eight-byte native tagged address under both
models, and future Wasm preflight must reject it. A pointer followed by i32 is
eight bytes with `WASM32` and twelve natively. `DataInit` checks the sum of all
item sizes; its existing `size()` retains its native summation behavior, including
its existing overflow behavior. Object alignment is not appended as padding.

## Validation

The focused `crates/keel-ir/tests/data_layout.rs` tests cover both models, nested
pointer fields/arrays, empty members, addition/product/field-rounding/trailing-
padding overflow, a pointer array fitting only the wasm32 model, initializer
sizing (including RTTI), and native text serialization. Initializer summation
uses checked addition; constructing enough actual initializer items to overflow
u64 is not a practical allocation-based test.

The complete existing `keel-ir` suite and seven focused tests passed locally on
2026-10-24 using the existing toolchain/features and shared build lock:

```sh
python3 suite/fast_opt_locked.py \
  /tmp/keel-wasm-ir-stage1-20261024-9f6ce30a.jsonl \
  cargo test --offline --locked -j2 -p keel-ir
```

Generated build artifacts remain in cargo-managed storage and lock metadata is
outside Git. No native backend behavior or Wasm execution claim follows from
these IR tests. Parent integration and independent review remain separate.
