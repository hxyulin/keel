# Generic integer overflow builtins

Keel supports `__builtin_add_overflow`, `__builtin_sub_overflow`, and
`__builtin_mul_overflow` for its integer types through signed and unsigned
`__int128`, including mixed operand widths, mixed signedness, and destinations
narrower or wider than either operand. `__has_builtin` advertises these names.
The return type is `_Bool`.

The contract follows [GCC's generic overflow specification](https://gcc.gnu.org/onlinedocs/gcc/Integer-Overflow-Builtins.html):
compute the exact mathematical operation, convert and store the result in the
pointed-to integer type, and return whether that mathematical value is outside
that type's range. In particular, operands are not first converted to the
result type. For example, adding signed `-1` to unsigned `1` stores zero without
overflow, even when the destination is unsigned. Adding unsigned `UINT_MAX` to
signed `1` with an `int` destination stores the low bits and reports overflow.

Integer operands include boolean and enum values. The destination must point
to a non-const integer other than boolean or enum; typedefs and qualifiers are
resolved when checking this restriction. Volatile integer destinations retain
the volatile store. Atomic destinations, float and pointer operands, vectors,
and `_overflow_p` are outside this interface. The fixed-type `sadd`, `uadd`,
`ssub`, `usub`, `smul`, and `umul` families retain their existing conversions
and lowering.

## Lowering and correctness

The generic AST node retains both original operand types and the pointer's
qualifier metadata. The existing builtin evaluation loop evaluates the two
operands and the destination expression once each, in frontend order, before
performing the store. The store also occurs when the return value is unused.
Aliasing a result pointer with an operand's storage therefore uses the operand
values captured before the store.

The lowering converts each operand to an unsigned work type by extending its
original signed or unsigned value, then extracts sign and unsigned magnitude.
The work type is i64 when all three integer types are at most 32 bits, otherwise
i128. Integer IR arithmetic wraps, so taking a signed minimum's unsigned
magnitude by modular negation is defined.

For addition/subtraction, equal mathematical signs add magnitudes and retain
the unsigned carry; different signs subtract the smaller magnitude from the
larger and select that operand's sign. Subtraction reverses the second sign.
A carry means the magnitude exceeds every supported destination's range.

Multiplication of operands at most 64 bits fits in the unsigned i128 work
value (32-bit operands fit in i64). For an i128 operand, split each magnitude
into 64-bit limbs `a0,a1,b0,b1`. Compute the three full products `a0*b0`,
`a0*b1`, and `a1*b0` in i128. Add the two shifted cross-products to `a0*b0`,
retaining both carries. Any nonzero upper half of a cross-product, either
carry, or both `a1` and `b1` being nonzero proves a magnitude above `U128_MAX`.
The low sum remains the correct low 128 product bits. This requires neither
an i256 value nor an i128 multiply-overflow instruction.

Normalize negative zero, compare the magnitude with the destination limit
(`2^(n-1)-1` for positive signed, `2^(n-1)` for negative signed, `2^n-1` for
unsigned), and combine that comparison with the retained carry. Negative
nonzero values also overflow unsigned destinations. Finally apply the sign
by modular negation and truncate for the store. Every generated shift is
strictly below its work type's width. No C signed-overflow expression,
division, wider IR type, backend modification, or runtime helper is involved.

Synthesis is a fixed sequence per call. The full i128 multiplication path
uses three i128 low multiplies, two unsigned add-with-carry checks, and limb,
sign, and range operations. It emits more operations than the existing
fixed-type hardware-flag path. No runtime benchmark has been run.

## Testing

```sh
cargo test -p keel-c --test generic_overflow
cargo test -p keel-c --test run overflow_builtins_compile_on_both_targets
```

The suite runs natively on AArch64 macOS and x86-64 Linux and covers:

- Mixed input pairs across 12 integer types, rotated destination types, and
  every destination type for all four signed/unsigned i128 operand pairs.
  Each of 192 cases checks add/sub/mul on a 26-by-26 boundary grid and a
  deterministic random grid.
- Both the overflow flag and all 128 stored bits contribute to output hashes.
  Cases include zero, signed minima/maxima, unsigned maxima, limb boundaries,
  products with nonzero high halves, negative zero, and narrowing/widening.
- Clang reference executables at `-O0` and `-O2`; Keel with both Clang and
  native preprocessing and with IR optimization disabled/enabled. All outputs
  must agree. Existing fixed-type families run through the same differential
  helper.
- Side-effecting operand/pointer functions, indexed expressions, aliasing,
  ignored return flags, volatile stores, boolean/enum operands, and `_Bool`
  result typing.
- Source-located diagnostics for noninteger operands, invalid result types
  (including enum/const typedefs and atomic destinations), and wrong arity.
- A 432-function type matrix that verifies IR and code generation on AArch64
  and x86, before/after optimization, and asserts all volatile stores remain
  present.
