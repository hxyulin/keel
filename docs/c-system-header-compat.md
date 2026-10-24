# C system-header compatibility: extended floating types

Recent glibc headers declare GNU extended floating types (`_Float32`,
`_Float128`, complex binary128 via `mode(TC)`, and others) even in programs
that never use them. This document describes what Keel accepts today on
x86-64 Linux, why the remaining header failures occur, and the planned next
steps. See [the implemented boundary](#implemented-declarationlayout-checkpoint)
for the current support, and [the object and application results](provenance/linux-binary128-tc-layout.md)
for the twelve representative inputs and native application runs.

## Implemented declaration/layout checkpoint

On `x86_64-unknown-linux-gnu`, `mode(TC)` / `__mode__(__TC__)` on complex
float, complex double or complex long double canonicalizes to opaque
`_Complex __float128`, with size **32** and alignment **16**. Its real and
imaginary component type queries identify GNU `__float128`, size **16**,
alignment **16**. This is semantic declaration identity, not an IR floating
type. Opaque types carry alignment separately from size; `unusable()` and
recursive containment still deny value support. No opaque element is placed in
the value-bearing `Ty::Complex` variant.

**Accepted:**

- typedefs, unused prototypes and function-pointer types;
- extern object declarations;
- data and callback pointer transport, including atomic pointers;
- arrays, records and unions containing these types (layout only);
- unevaluated `sizeof`, alignment, `offsetof` and type queries, and
  unevaluated calls whose opaque parameter/argument types match exactly;
- unevaluated compound-literal backing storage.

**Rejected** (by the pre-pruning walker and ABI guards from the
[unsupported-value checkpoint](c-unsupported-values.md)):

- evaluated calls, returns and function definitions involving these values,
  and unsafe function-symbol materialization;
- scalar objects and objects of containing aggregates, initializers, copies,
  casts, arithmetic, comparisons, real/imaginary values and varargs;
- atomic TC or atomic containing-record subjects.

Numeric redeclaration conflicts recurse through pointers, arrays and functions.
Debug information describes TC as a two-element array of binary128 components,
not an invented 32-byte scalar float. Complex long double declarations retain
the component alignment (16 on Linux x86-64) rather than using their 32-byte
size. Other targets reject `TC` layout explicitly; their existing real opaque
declarations remain declaration-only.

No `_Float32` or other GCC family type is aliased to ordinary `float`, no
identity macro, header, include path or application flag is changed, and no
binary128 constant is rounded through a host floating value. There is no F128
IR type, arithmetic, helper, backend or ABI implementation.

### Effect on real code

With this checkpoint, the Lua 5.4.7, QuickJS 2024-01-13 and SQLite 3.46.1
representative translation units compile to full objects from raw source, from
Clang-preprocessed input and from Keel-preprocessed input. GCC-preprocessed
input still stops at GCC's builtin `_Float32`/`_Float128` declarations (see
below). Complete configured builds of all three applications from raw sources,
linked by native GCC and Clang, pass Lua's `_U=true all.lua`, QuickJS's
closure/language/builtin/loop/bignum tests, and SQLite's maintained query
workload. These are finite correctness results for those configurations, not
full upstream conformance or a performance result.

### Tests

```sh
cargo test -p keel-c --test binary128_layout --test system_header_compat \
  --test unsupported_values
```

`binary128_layout` covers canonical layout/component queries, nested
records/unions/arrays, recursive pointers and atomic pointers, conflicting
redeclarations, every forbidden value/ABI category, and (on Linux x86-64)
native GCC/Clang fixtures. The native fixture owns the extended storage in the
harness and checks only Keel's pointer computations, wrapper offsets, 32/16-byte
strides, alignment and the exact single-byte store beside TC storage; a
full-byte canary comparison catches any overwritten TC, padding or neighbor
bytes. The unsupported aggregate ABI reproducer still diagnoses before object
emission.

## Background: why glibc headers failed

The representative inputs are QuickJS `libregexp.c`, Lua `lvm.c` and SQLite
`sqlite3.c` on x86-64 Linux with glibc 2.44, preprocessed by GCC 16 and Clang 23
with each project's original flags:

```sh
gcc -E -std=gnu11 -D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"' \
  -DCONFIG_BIGNUM -I. libregexp.c -o libregexp.i
gcc -E -std=gnu99 -DLUA_USE_LINUX lvm.c -o lvm.i
gcc -E -std=c11 -DSQLITE_THREADSAFE=1 -D_GNU_SOURCE \
  -DLONGDOUBLE_TYPE=double sqlite3.c -o sqlite3.i
```

Compilation uses `keel cc -O --opt-tier none --target x86_64-unknown-linux-gnu
--cpu-level v1 INPUT --emit-object -o OUTPUT`.

### `__TC__`: a real complex binary128 contract

The reduced declaration is:

```c
typedef _Complex float C __attribute__((__mode__(__TC__)));
```

`__TC__` is GNU's complex binary128 machine mode, not an unexpanded macro. It
comes from `/usr/include/bits/floatn.h`; glibc selects this typedef for
compilers without native `_Complex _Float128` syntax, including Clang and Keel
(whose Clang-compatible identity macros select the same path). Before the
checkpoint, Keel reported `unsupported integer mode '__TC__'` here for raw
sources and for Clang- and Keel-preprocessed input.

Native Clang and GCC both give this type 32-byte size, 16-byte alignment, an
identity distinct from complex float and complex double, and binary128
arithmetic. Ignoring the mode would change layout and values even if the
typedef were unused in a particular TU.

### `_Float32`: GCC builtin versus Clang header typedef

GCC-preprocessed QuickJS and SQLite fail at `/usr/include/stdlib.h:145`:

```c
extern _Float32 strtof32(const char *restrict, char **restrict);
```

An unbound `_Float32` in a declaration or type-name context reports
`unsupported extended floating type '_Float32'` (rather than the earlier
misleading `expected ';', found 'strtof32'`). Ordinary identifiers with this
spelling and explicit header typedefs keep their lookup behavior; no builtin
type or implicit alias is introduced.

In GCC, `_Float32` has size/alignment 4/4 but is incompatible with both `float`
and `double`. Clang rejects the builtin spelling; the Clang glibc path instead
supplies `typedef float _Float32`, which Keel accepts with that typedef's float
identity, arithmetic and ABI. GCC rejects that typedef, so it cannot substitute
for the GCC builtin.

GCC-preprocessed Lua first encounters a different member of the family,
`extern int __fpclassifyf128 (_Float128 __value);`, and still fails with
`expected ')', found '__value'`.

In the GCC `.i` files, `_Float32`, `_Float64`, `_Float32x`, `_Float64x` and
`_Float128` appear 3/3/3/3/3 times in QuickJS, 0/0/0/0/8 in Lua and
455/467/460/472/487 times in SQLite, all inside system-header regions. GCC
input therefore needs five distinct builtin identities for QuickJS/SQLite and
one for Lua just to cover the declarations; the Clang/Keel header path already
supplies its own typedefs, which is why `TC` declaration support was the
smaller first step.

### Native identity, layout and ABI facts

[GCC's additional floating types documentation](https://gcc.gnu.org/onlinedocs/gcc/Floating-Types.html)
specifies `TC` for complex binary128 and the x86 `__float128`/`_Float128`
alias, and the `_FloatN`/`_FloatNx` identities remain distinct from ordinary C
floating types even for equal encodings. Native GCC on x86-64 Linux gives:

| Type | Size / align | Notes |
| --- | --- | --- |
| `_Float32` | 4 / 4 | distinct from `float` |
| `_Float64`, `_Float32x` | 8 / 8 | distinct from each other |
| `_Float64x` | 16 / 16 | x87 encoding, distinct from `long double` |
| `_Float128` | 16 / 16 | binary128, compatible with `__float128` |
| `TC` | 32 / 16 | compatible with `_Complex _Float128` |

The `extended-float-layout-oracle.c` fixture checks TC size/alignment, array
stride 32, real/imaginary order, offsets 16 and 48 in a char/TC/char wrapper,
total size 64 and sentinel bytes around a 32-byte copy with both native
compilers:

```sh
gcc -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o layout-gcc
clang -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o layout-clang
./layout-gcc && ./layout-clang
```

The [AMD64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
classifies real binary128 as SSE/SSEUP and complex binary128 as a pair of
those; the 32-byte pair becomes MEMORY after aggregate cleanup. It must not
inherit complex float/double register transport or `COMPLEX_X87` return rules.
Knowing the layout grants no call ABI.

`extended-float-aggregate-abi.c` (a `struct box { __float128 x; }` returned by
value) shows why: before the [unsupported-value checkpoint](c-unsupported-values.md),
Keel emitted an object using integer registers, and native GCC and Clang
callers linked with it (`extended-float-aggregate-harness.c`) failed. Keel now
rejects it before object emission.

## Next steps

1. Specify and implement a distinct `_Float32` scalar identity through type
   compatibility, generic selection, arithmetic rank, casts, literals and
   diagnostics. Specify promotions deliberately (`_Float32` does not get
   `float`'s default `double` promotion; see
   [GCC's rationale](https://gcc.gnu.org/pipermail/gcc-patches/2016-August/455079.html))
   and x86 SysV argument/result/varargs behavior. Sharing F32 storage/IR
   requires preserving frontend identity. This alone will not accept the other
   `_FloatN` declarations in these headers.
2. Separately specify real binary128 values and ABI, including conversions,
   constants and the necessary compiler-runtime helpers. Add complex binary128
   operations and ABI only after that real-type contract exists. Native
   Clang/GCC cross-call and arithmetic oracles must cover `mode(TC)` itself.
3. Re-run the original sources and GCC/Clang/Keel preprocessed inputs after each
   stage to expose the next genuine boundary, without removing prototypes or
   changing headers.

Neither stage implies arbitrary `_FloatN`, complex/tgmath, vendor-header,
automatic-vectorization or extended floating-environment completeness.
