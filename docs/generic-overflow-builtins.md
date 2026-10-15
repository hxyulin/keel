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
fixed-type hardware-flag path. The dedicated 192-case fixture's AArch64
assembly was 3,302,109 bytes without IR optimization and 2,530,484 bytes with
optimization (assembly text, including the whole test program). These are
artifact sizes, not runtime or compiler-performance measurements; no benchmark
was run.

## Validation evidence (2026-10-16)

Initial branch `feat/generic-overflow-builtins`, worktree
`~/.t3/worktrees/keel/t3-56fb70cd`, was clean at
`fca0aeec316ed2884d828bcee9fcc988bebef975`. No applicable `AGENTS.md` was found
in this checkout or its ancestor chain. Owned changes are limited to C
`ast.rs`, `expr.rs`, `features.rs`, `irgen.rs`, the feature-query regression,
`tests/generic_overflow.rs`, `tests/programs/generic_overflow.c`, and this file.

All commands below used `CARGO_BUILD_JOBS=1` and tests used
`--test-threads=1`. Native AArch64 ran on MacHost, with rustc
`1.99.0 (b940084d7 2026-09-28)` and Apple Clang
`21.0.0 (clang-2100.3.34.2)`, target `arm64-apple-darwin27.0.0`:

```sh
cargo check -p keel-c
cargo test -p keel-c --test generic_overflow --test contracts --test memory --test environment -- --test-threads=1
cargo test -p keel-c --test run overflow_builtins_compile_on_both_targets -- --test-threads=1
cargo test -p keel-c --test run rejected -- --test-threads=1
```

All passed: 4 dedicated tests, 3 contracts, 9 memory, 7 environment, and the
two filtered frontend tests. The environment suite also compiled all fixture
sources with Keel preprocessing.

LinuxHost was checked for live load and cargo/rustc jobs before use (no build jobs;
load `0.00, 0.00, 0.02` at the final preflight). A source export was extracted
only into `/home/hxyulin/.t3/checkouts/generic-overflow-fca0aeec-20261016`.
Native x86 validation used the same rustc version and Clang `23.1.1`, target
`x86_64-pc-linux-gnu`:

```sh
cargo test -p keel-c --test generic_overflow -- --test-threads=1
```

All 4 passed. Both native hosts tested:

- Mixed input pairs across 12 integer types, rotated destination types, and
  every destination type for all four signed/unsigned i128 operand pairs.
  Each of 192 cases checks add/sub/mul on a 26-by-26 boundary grid and a
  deterministic random grid: 778,752 arithmetic calls, plus effect checks.
- Both the overflow flag and all 128 stored bits contribute to output hashes.
  Cases include zero, signed minima/maxima, unsigned maxima, limb boundaries,
  products with nonzero high halves, negative zero, and narrowing/widening.
- Clang reference executables at `-O0` and `-O2`; Keel with both Clang and
  native preprocessing and with IR optimization disabled/enabled. All outputs
  agree. Existing fixed-type families run through the same differential helper.
- Side-effecting operand/pointer functions, indexed expressions, aliasing,
  ignored return flags, volatile stores, boolean/enum operands, and `_Bool`
  result typing.
- Source-located diagnostics for noninteger operands, invalid result types
  (including enum/const typedefs and atomic destinations), and wrong arity.
- A separate 432-function type matrix verifies IR and code generation on
  AArch64 and x86, before/after optimization, and asserts all volatile stores
  remain present.

SHA-256 identifiers for the tested code/export (documentation was added after
validation):

```text
source export tar.gz  faf21e38f6c2f34adfdb6743093268d442d7d92ba2cc3d33a5ca8e6f8c25c5ee
irgen.rs             5bb1ece6650c48fa43b9a6de5a26b5e435fd68188a7fba83855f9517dc065589
generic_overflow.rs  87734d7113d5abea92af1f5f512bd5792d2515d105b1627908906e485524145a
generic_overflow.c   04cc9c999f997f93798eb72232c47c5d3a15f55d88824e4426aed032a4b0518e
MacHost keel-c rlib    16183605e48e20554cf8273b74e2c8cff685190ee9bfce6db055d5d2fadf627b
MacHost test binary    d8e96600091cd6acd84262aa6059b26cbec9ac9a9b6967ec8f43fad1f9273cb8
LinuxHost keel-c rlib    f44450f3f6032ed42484ba52ff37b9c7bff1f8b1d9761340f2421a8b88e8a1d7
LinuxHost test binary    b9873e92487691307a06a34a0534dd903b9766ab235d599354b57c4bd52bb58e
native stdout       57ebad9a5f6e4b61bcc99d3f074dc8b8e4a9b0d5bc6072c715446e39a1ef7b62
```

The stdout digest agrees across both hosts. No supported integer-width cases
remain incomplete. Independent review and serialized integration are left to
the coordinator; no main-branch edits, merge, push, nested worker, persistent
host configuration change, or rm-nuc-01 use was performed.
