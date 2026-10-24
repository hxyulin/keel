# C-to-IR lowering for wasm32 integer code

Implementation note for how `keel-c` lowers integer-only C functions to IR under
the wasm32 ILP32 model. It follows the
[declaration](wasm-c-scalar-declarations.md) and
[integer evaluation](wasm-c-integer-evaluation.md) work and is extended by
[pointers and data](wasm-c-pointer-data.md). The public behavior is in the
[freestanding C profile](wasm-c-freestanding.md).

## Entry point

The crate-internal test seam is
`irgen::wasm_scalar::lower(src: &str, debug: bool) -> Result<Module, Error>`. It
takes header-free, preprocessed C, selects the WASM32 model before parsing, and runs
no optimizer. The returned module has no target tag and must never reach a native
backend. The public path (`keel_c::compile` with the Wasm target) uses the same
model selection, validation gate and generator.

## Signatures and calls

Native ABI classification is bypassed. Function signatures use only plain i32/i64
parameters and at most one plain integer result (or void), with no extension
attributes, required prototypes and no variadics. The IR `cc(c)` spelling records
this bounded value interface; it is not native C interoperability or a complete
Wasm C ABI. Direct calls use the same signatures.

## Values and widths

Integer types up to 64 bits are supported, including narrow locals, casts and
promotions, assignment and compound assignment, arithmetic, comparisons,
conditions, loops, `switch`, `goto`, direct calls and scalar local initializers.
SSA values, memory types, loads, stores and numeric casts use the model: `long` is
i32 and `long long` is i64. A `volatile long` slot is 4 bytes with alignment 4.
Local alignment above 16 is rejected, matching the runtime's static-frame limit.

Generator constant folding, identity elimination and truth-value proofs use the
same model. For example, a `long long` to `long` cast cannot be dropped from a
condition under ILP32, folded `long` constants are truncated and sign-normalized,
and `unsigned long` masks normalize at 32 bits. Pointer differences are divided in
i64 **before** truncating to the 32-bit `long` result.

## Validation gate

Before the generator's worklist and dead-code pruning, a validator walks **all**
declarations, definitions, locals, statements and expressions, including unused
bodies and branches that are constant-false. It rejects anything outside the
supported subset, for example: indirect calls, unknown external data, FP and
complex values, vectors, i128, builtins (architectural intrinsics, atomics,
memory and stack builtins), VLAs, label addresses and computed goto, inline
assembly, cleanup attributes, nonordinary linkage or function metadata, debug
output, and narrow/pointer/aggregate signatures. Statement expressions and scalar
compound literals are excluded.

The gate validates the typed AST after the parser has already folded constant and
type queries; it is a value-lowering gate, not a token-level restricted grammar.
A GNU `case` range whose bounds span the full i64 range is diagnosed as oversized
(the subtraction is checked) instead of overflowing.

## Testing

Unit tests in `crates/keel-c/src/irgen/wasm_scalar.rs` verify the actual
unoptimized modules and check plain signatures and metadata, calls, 4-byte `long`
slots, truncation and sign extension, unsigned shifts and division, model-specific
mixed arithmetic, folded constants and truth proofs. Rejection tests cover unused
bodies and declarations, constant-false branches, short-circuit and conditional
operands, unknown or unprototyped callees, narrow signatures and over-aligned
locals.

The fixture
[`wasm-scalar-lowering.c`](../crates/keel-c/tests/fixtures/wasm-scalar-lowering.c)
is checked by Clang for wasm32 and the native target. For a set of constant
functions, the IR constant Keel returns is compared with Clang's optimized LLVM
(`clang -O1 -S -emit-llvm`) result in both profiles.

```sh
cargo test -p keel-c
```
