# C integer constant evaluation for wasm32

Implementation note for how `keel-c` evaluates integer constant expressions and
types literals under the wasm32 ILP32 model. It builds on the
[C data model](wasm-c-layout-foundation.md); the public behavior is in the
[freestanding C profile](wasm-c-freestanding.md).

## Explicit-model evaluator

`eval_int_with_layout`, `eval_with_layout`, `eval_constant_with_layout` and
`eval_with_binding_and_layout` take a `CDataLayout`. Operands, address evaluation,
hint arguments, casts, arithmetic, normalization and `fits_i64_with_layout` all use
that same model; recursion never falls back to a native wrapper. The old `eval`,
`eval_with`, `eval_int`, `eval_constant` and `normalize` signatures keep native
behavior for native callers.

## Usual arithmetic conversions and literals

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

## Pointer constants

Numeric pointer constants keep the low 32 bits as an unsigned value on wasm32,
including through casts and pointer addition. Pointer difference divides the
address difference before normalizing the `long` result. Symbolic
`Const::Addr(base, addend)` values stay symbolic. A non-native symbolic address cast
to an integer does not evaluate (returns `None`); LP64 keeps its existing
eight-byte behavior.

## Unchanged behavior

The general evaluator keeps its wrapping arithmetic, masked shifts and saturating
float-to-integer casts; it is not a strict ISO integer-constant-expression checker.
Division by zero returns `None`. The x86 immediate-operand and vector-broadcast
constant proofs remain native-only and reject the wasm32 model (see
[scalar declarations](wasm-c-scalar-declarations.md)); they continue to reject
invalid shifts, signed overflow, MIN/-1 division and invalid FP casts on native
targets.

## Testing

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
