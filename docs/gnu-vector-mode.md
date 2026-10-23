# GNU C integer modes and vectors

Keel C implements a bounded GNU type-attribute subset on
`aarch64-apple-darwin`, `aarch64-unknown-linux-gnu` and
`x86_64-unknown-linux-gnu`. These attributes change
C type identity before layout, initialization, and IR generation. Unsupported
sizes and element types produce diagnostics; they never fall back to a scalar.
Double-underscore attribute and mode-name spellings are accepted.

## Integer modes

`__attribute__((mode(DI)))` on an integer variable, parameter, member, typedef,
or scalar type name selects a real integer type. Signedness comes from the
original integer type. The supported names and canonical signed types are:

| Mode | Bytes | Signed type on the supported LP64 targets |
| --- | --- | --- |
| QI, byte | 1 | signed char |
| HI | 2 | short |
| SI | 4 | int |
| DI, word, pointer | 8 | long |
| TI | 16 | __int128 |

Unsigned bases select the corresponding unsigned type. Alignment, conversion
rank, promotions, arithmetic and ABI behavior follow the selected type.
For example, `typedef unsigned int U __attribute__((mode(DI)));` is compatible
with `unsigned long`, occupies eight bytes, and performs unsigned 64-bit
arithmetic. Numeric conflicts in object, typedef and function redeclarations
are diagnosed. This does not replace the frontend's existing policy for all
unrelated C redeclaration categories.

Declaration-specifier attributes change the base before composing declarators;
final named-declarator attributes change their scalar subject. Attributes on
pointers/functions, `_Bool`, floating types, enum tags, and unsupported modes
(including vector machine modes and ordinary floating machine modes) are diagnosed.
The separate x86-64 Linux [TC declaration/layout subset](c-system-header-compat.md#implemented-declarationlayout-checkpoint)
accepts `mode(TC)` on complex float/double/long double, canonicalizing to complex binary128
(32-byte size, 16-byte alignment) while rejecting evaluated values and ABI use.
Use `vector_size`, rather than a vector `mode`, for vectors.

## Six lane shapes at 16 or 32 bytes

Supported elements are signed/plain/unsigned char, signed/unsigned short, int,
long and long long, and float/double. `_Bool`, `__int128`, `_Float16`, opaque
floating types, structs, pointers, and vector elements are unsupported.
`vector_size(16)` is supported on all targets. Bounded `vector_size(32)` is
supported only on x86-64, using the existing AVX ABI and backend feature gates.
Eight-byte vectors remain unsupported. The 16-byte shapes are `i8x16`, `i16x8`,
`i32x4`, `i64x2`, `f32x4`, `f64x2`; the 32-byte shapes are `i8x32`, `i16x16`,
`i32x8`, `i64x4`, `f32x8`, `f64x4`. C retains element signedness and nominal
integer identity. Size and natural alignment equal the selected byte width.
Lane zero is at the lowest address. Struct fields, arrays and pointer arithmetic use the complete size.

```c
typedef int V __attribute__((vector_size(16)));
typedef int __attribute__((vector_size(16))) *VP;
V add(V a, V b) { return a + b; }
int lane(V a, int i) { V b = {1, 2}; b[i] = a[i]; return b[i]; }
```

Attributes before the base, among declaration specifiers, and after a scalar
name are supported. For pointers, arrays and function returns put the attribute
in the declaration specifiers or use a vector typedef. A final attribute on a
pointer, array or function declarator is rejected, matching Clang. GNU type
attributes on tags or array-parameter qualifier positions are diagnosed.

Supported expressions:

- Braced lane initialization of globals and locals, zero-filled omitted lanes,
  compound literals, whole-vector initialization/assignment, conditional
  expressions with a scalar condition, pointers and memory loads/stores.
- Constant or dynamic lane reads, lane assignments and compound assignments.
  Constant out-of-range subscripts are diagnosed. Dynamic subscripts must be
  within the vector, as in C; no runtime bounds check is inserted. A lane is
  not addressable. Volatile lane access is diagnosed; whole-vector volatile
  loads and stores retain the existing memory flags.
- Matching-vector `+`, `-`, `*`; integer `&`, `|`, `^`, `~`, `<<`, `>>`;
  floating `/`; unary `+` and `-`; and matching-vector comparisons.
  Integer vectors keep their lane width without scalar integer promotions.
  Comparison results have signed integer lanes of the operand width, with
  all bits set for true and zero for false. Right shift follows element
  signedness. Shift counts must be valid C lane shift counts. The separate
  [bounded SSE2 wrappers](c-x86-intrinsics.md#bounded-sse2-packed-shifts) use
  architectural zero/sign-fill for oversized counts; they do not extend these
  ordinary C operators or add scalar broadcast shifts.
- Explicit equal-width casts between supported vectors, and between a 16-byte
  vector and signed or unsigned `__int128`, preserving all bytes. These are bit casts,
  not numeric lane conversions. The separate
  [three SSE2 conversion wrappers](c-x86-intrinsics.md#bounded-sse2-numeric-conversions)
  preserve architectural rounding/invalid/status behavior through dedicated IR.
- Bounded `__builtin_shufflevector` permutations with matching input/result
  shapes and constant in-range indices; see the
  [C x86 intrinsic subset](c-x86-intrinsics.md) for the builtin and header contract.

Two vector operands must have the same C vector type; implicit conversions
between distinct vectors are diagnosed. The bounded scalar broadcasts below
also preserve that vector type. Integer vector division/remainder, floating bitwise operations, vector logical
operators/conditions, vector `!`, and reading vectors with `va_arg` are
unsupported. Initializer designators into vector lanes are unsupported.
Full optimization can recognize an exact four-lane 32-bit wrapping sum after
C lowering exposes matching extract/add SSA operations. This bounded
[reduction pilot](c-lane-reduction-pilot.md) does not bypass C promotions,
reassociate floating arithmetic, recognize general shuffles/widening or
promise arbitrary automatic vectorization.

The backend may expand an operation into scalar **lanes**, preserving the full
vector type, layout and ABI; that is distinct from ignoring the type attribute.
Debug information currently presents a vector as an array of its elements.

The [bounded SSE2 saturation wrappers](c-x86-intrinsics.md#bounded-sse2-saturation)
provide explicit signed/unsigned byte/word clamping on x86-64 using existing
typed architectural operations. Ordinary vector `+`/`-` retain their current
semantics; the wrapper subset adds no numeric conversions or new vector widths.

## Scalar broadcasts

For `+`, `-`, `*`, integer `&`, `|`, `^`, floating `/`, and the six comparisons,
one operand may be a scalar. Either operand order is supported, including
noncommutative operations. The scalar is converted to the vector element type
and replicated into every lane. Integer lanes do not undergo promotions;
comparison masks retain the existing signed all-one/zero lane type.
`+=`, `-=`, `*=`, integer `&=`, `|=`, `^=`, and floating `/=` accept a scalar
right operand under the same conversion checks. Each operand and each
compound-assignment lvalue is evaluated once. Vector widths, memory accesses,
volatile whole-vector flags, ABI and target feature requirements are unchanged.
This applies to 16-byte vectors on all supported targets and x86 32-byte vectors.

The accepted conversions are deliberately bounded and lossless:

- Integer scalars through 64 bits: the complete source type range must fit the
  lane type, or a supported constant expression must have a value within the
  lane's signed/unsigned range. Widening with unchanged signedness, conversion
  from unsigned to a wider signed type, and `_Bool` to integer are accepted. Runtime signed-to-unsigned conversion is
  diagnosed, even at equal widths; unsigned constants are interpreted as
  unsigned values, including `ULONG_MAX`.
- Floating scalars: `float` to `float`/`double`, and `double` to `double`.
  A finite constant `double` also converts to `float` when exactly representable,
  including negative zero. Runtime `double` to `float` is diagnosed.
- Cross-domain integer/floating broadcasts, 128-bit integers, extended FP,
  scalar shifts, and implicit scalar-to-vector assignment/initialization remain
  diagnosed. Use an explicit **scalar** cast to the element type to request a
  conversion before broadcasting, for example `v + (float)n` or `v + (int)n`.

Constant proofs cover numeric literals, scalar casts, unary/binary numeric
expressions, logical expressions and conditionals with supported numeric
operands. Expressions with i128, extended FP or pointer intermediates, comma
operators, calls or side effects cannot prove a narrowing conversion. This
avoids using the frontend's 64-bit constant evaluator to prove an unsafe
wide-to-narrow conversion. Runtime operands with an already safe type can
still contain calls or side effects.

This follows the safe-conversion rule in
[GCC's vector extension documentation](https://gcc.gnu.org/onlinedocs/gcc/Vector-Extensions.html)
for the accepted subset, without claiming all GNU scalar conversion cases.
Clang additionally accepts some out-of-range integer constants with a warning;
Keel diagnoses them instead of silently truncating. Numeric lane conversion
builtins and automatic vectorization are separate work. The existing bounded
FP contract does not promise signaling-NaN payloads, exception flags/traps or
strict floating-environment compatibility.

`gnu_vector_broadcast` checks conversion boundaries, compounds, distinct-vector
rejection, all ten signed/unsigned/FP lane types at both widths, real x86 ELF
object emission at v1 (16 bytes) / v3 (32 bytes), and disabled-AVX rejection at v1. The expanded `gnu_vector_abi`
Clang oracle executes broadcasts over all ten 16-byte lane types, arithmetic,
bitwise operations, all comparisons, compound assignments, unsigned values,
NaNs/infinities/signed zero, operand/lvalue evaluation counts and adjacent
memory sentinels under three selection/allocation/optimization configurations.
On 2026-10-23 these passed native Apple ARM64 execution with Apple Clang 21;
Linux AArch64/x86 objects were cross-emitted, not executed. No genuine GCC was
installed on that host (`gcc` identifies as Apple Clang); GCC's documented
safe-conversion rule was consulted, but no native GCC oracle run is claimed.

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-broadcast-<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-c --test gnu_vector_broadcast \
  --test gnu_vector_abi --test gnu_types --test x86_intrinsics
```

The optional [legacy PCLMULQDQ subset](x86-pclmulqdq.md) reuses the existing
`i64x2` representation and 128-bit ABI through an explicit intrinsic and small
C wrapper. It adds no vector type, raw-wide ABI or automatic polynomial recognition.

## ABI boundary and validation

Direct vector arguments/results use the existing SIMD ABI: Darwin arm64 v0–v7
and v0 result, or Linux x86-64 XMM0–XMM7 and XMM0 result, with aligned stack
arguments when registers run out. With AVX enabled, direct 32-byte x86 vectors
use YMM0–YMM7 and YMM0 results, sharing the same SIMD argument bank and using
32-byte-aligned stack arguments on register exhaustion. Callers and callees
must agree on AVX ABI flags; Keel diagnoses disabled AVX rather than silently
selecting the non-AVX ABI. Direct/indirect calls and calls in both
directions with Clang are covered. Darwin homogeneous vector structs of one to
four members use vector registers; equal-width vector members may have different
lane types. Larger aggregates use the existing indirect convention. Arrays of
vector members and FP-register exhaustion are tested. By-value unions containing
vectors are diagnosed. Linux x86-64 by-value aggregates containing vectors are
diagnosed because the frontend/backend boundary does not yet provide SSEUP
classification. Those aggregates can still be laid out and accessed in memory.

`gnu_types` uses Clang syntax/layout/type-identity assertions for both targets.
`gnu_vector_abi` generates the same arithmetic kernels for Keel and Clang,
assembles Keel's typed programs into real Mach-O/ELF objects, and executes a
Clang harness on the native supported host. It covers all six shapes, both
integer signednesses, stack arguments, callbacks, indirect calls, lane mutation,
bit casts and integer modes under three selection/allocation/optimization
configurations. The Darwin aggregate harness also runs with debug information.
The initial validation on 2026-10-22 used arm64 macOS and Apple Clang 21.0.0.
The full `keel-c` suite and the three SIMD backend suites below passed.
Linux x86-64 was checked by IR verification and real cross-object emission;
its runtime ABI harness was not executed on this host.

Cross-target objects and the Clang harness are compiled on either host; cross
compilation is not evidence of runtime ABI validation on the other host.

```sh
cargo test -p keel-c
cargo test -p keel-codegen --test vector_ir --test vector_aarch64 --test vector_x86
```

## Completed stages and follow-up boundaries

1. Integer modes, signedness/canonical identity, numeric redeclaration checks,
   and explicit unsupported calling-convention diagnostics.
2. The bounded vector C type, layout/initialization, lane access, operators,
   bit casts and lowering to the existing SIMD IR.
3. Clang layout/behavior oracles, native/cross-object ABI tests, homogeneous
   Darwin vector aggregates, and diagnostics for unsupported aggregate ABIs.

Future stages can expand the bounded scalar conversion/shift subset,
add numeric lane conversions, integer division/remainder, volatile lane operations,
vector initializer designators, other widths/targets with explicit ABI
gates, union classification and SysV SSEUP aggregates. Each requires dedicated
semantic and ABI validation before expanding the accepted subset.

`ms_abi` is deliberately not implemented. It and other recognized unsupported
calling-convention attributes (`sysv_abi`, `cdecl`, `stdcall`, `fastcall`,
`thiscall`, `vectorcall`, `regcall`, `pcs`, ARM vector/SVE PCS attributes,
`preserve_most/all/none`, Swift conventions, `riscv_vector_cc`, `regparm`,
`sseregparm`, `interrupt`, and `naked`) produce diagnostics even on unused
prototypes and function-pointer typedefs. They cannot silently select the
ordinary C calling convention.

Clang's `ext_vector_type` and GNU `transparent_union` also produce diagnostics;
their distinct type and ABI semantics are not implied by GNU vector support.
Native `__has_attribute` queries remain conservative and do not advertise
these bounded vector/mode subsets yet.

## 32-byte extension and 16-byte audit

The 32-byte extension reuses the 16-byte type/layout, initialization, lane,
arithmetic, comparison, memory, debug and IR lowering machinery. The audit found
two deliberate width limits to change: attribute shape selection and the
16-byte cast check. The latter now requires equal widths; the existing
`__int128` bitcast remains limited to 16 bytes. Aggregate ABI and vector
`va_arg` restrictions are retained. The bounded scalar broadcasts above apply
at both widths. No integer division, volatile lane access, numeric lane
conversion or initializer-designator support is inferred from the wider type.
Whole-vector accesses keep their width and memory flags; header unaligned copies use exact-size byte-safe memory builtins.

`x86_intrinsics` adds layout/feature diagnostics, all-header object emission,
and wide ABI/value/footprint checks with GCC/Clang oracles. The existing
`gnu_types` and `gnu_vector_abi` tests continue to cover the 16-byte subset.
See [C x86 intrinsics](c-x86-intrinsics.md) for the finite supported names,
unsupported boundaries and validation commands. This extension does not claim
complete vendor header compatibility or general auto-vectorization.
