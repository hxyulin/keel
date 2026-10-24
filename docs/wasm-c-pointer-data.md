# C pointers and static data for wasm32

Implementation note for how `keel-c` lowers pointers, arrays, records and static
data under the wasm32 ILP32 model. It extends the
[integer lowering](wasm-c-scalar-lowering.md) and targets the
[`keel-wasm` runtime emitter](../crates/keel-wasm/README.md). The public behavior is
in the [freestanding C profile](wasm-c-freestanding.md).

The lowering selects ILP32 before parsing, runs no optimizer, and verifies the
actual returned module with `verify_module_with_layout(&module, DataLayout::WASM32)`.
It never substitutes widened initializers to satisfy the native verifier.

## Storage

Integer and object-pointer locals, address-of, dereference, subscripts, member
access, and fixed arrays and records as storage are supported. Aggregate values,
aggregate assignment and copies, and aggregate ABI are not.

- Pointer slots, member layout, loads, stores and initializer bytes use four-byte
  pointers.
- Local alignment above 16 is rejected.
- Fixed local and static arrays of at least 16 bytes are placed with at least
  16-byte alignment. This does not change the type's `sizeof` or `_Alignof`.

## Pointer arithmetic

IR pointer arithmetic keeps i64 offsets:

- Dynamic `long` offsets are sign-extended before scaling.
- Integer-to-pointer casts, including constant shortcuts, normalize to the unsigned
  low 32 bits before `inttoptr(i64)`.
- `ptrtoint` yields the zero-extended address as i64, then the C integer conversion
  applies.
- Pointer differences subtract and divide (or shift) in i64 **before** truncating
  to the 32-bit `long`.
- `p--` and other pointer decrements lower through i64 `ineg` and `ptradd`.

## Static data

Defined integer, byte and numeric-pointer static data, and pointer-only symbolic
initializers targeting defined data, are supported.

- Serialization uses the parser's model, including 32-bit `long` truncation and
  numeric pointer normalization.
- Symbolic offsets must be nonnegative and at most the target object's size
  (one-past allowed). Pointer relocations need four-byte object and cursor
  alignment and advance the cursor by exactly four bytes.
- Initializer extents and nonoverlap are checked before serialization.
- Array dimensions are checked against u32 bounds while parsing, before any size
  multiplication. Record layout bounds member-array multiplication, alignment,
  accumulated size and trailing padding, including in unused definitions.
  Symbolic evaluation checks i64 overflow.
- Backend placement separately checks memory bounds and data/stack overlap.

String-literal symbolic pointer initializers are not supported; static byte arrays
may be initialized with strings.

## Rejected

The validation gate walks all globals and function bodies before pruning. In
addition to the integer-lowering rejections, it rejects unknown external data,
function pointers and indirect calls, TLS, weak, lifecycle, section and retention
attributes, aggregate values, FP, vectors, atomics, inline assembly, VLAs, debug
output, bit-fields, compound literals and builtins.

Function boundaries stay plain i32/i64 or void; there is no pointer, narrow or
aggregate ABI. A host passes an address as an explicit i32 offset and the C code
casts it internally.

Local byte-string initializers are rejected. Local aggregate initialization is
limited to objects of at most 64 bytes (larger ones would need an unsupported memory
intrinsic), even in unused bodies. Uninitialized local arrays may be larger, within
the slot and frame limits. There is no heap or dynamic allocation.

## Testing

The header-free fixture
[`wasm-pointer-data.c`](../crates/keel-c/tests/fixtures/wasm-pointer-data.c) is
compiled to IR, emitted with `emit_wasm32_runtime_module`, validated and run under
Node. It checks an unsigned checksum against a JavaScript reference, signed offsets,
array initialization and subscript stores, record fields and canaries, a direct
helper, repeated recursive calls, static pointer arrays and one-past fixups,
four-byte pointer spills, high-bit and truncated pointer round trips, pointer
decrement between canaries with guarded extreme offsets, and every byte around
adjacent volatile pointer and `long` stores. Clang independently checks the wasm32
and native layouts.

Large numeric-address pointer differences exercise the specified IR arithmetic
order; they are not a claim that C permits subtracting unrelated pointers.
High-bit and wrapping addresses are round-tripped and compared, never dereferenced.
Regression tests also cover oversized arrays and records in unused declarations,
nested dimensions and `sizeof` type names, and wrapping symbolic addends.

```sh
cargo test -p keel-c -p keel-wasm
```

Tests need `node` and `clang` on `PATH`.
