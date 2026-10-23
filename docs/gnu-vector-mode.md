# GNU C integer modes and vectors

Keel C implements a bounded GNU type-attribute subset on both
`aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`. These attributes change
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
(including vector machine modes and floating machine modes) are diagnosed.
Use `vector_size`, rather than a vector `mode`, for vectors.

## Six 16-byte vector shapes

Supported elements are signed/plain/unsigned char, signed/unsigned short, int,
long and long long, and float/double. `_Bool`, `__int128`, `_Float16`, opaque
floating types, structs, pointers, and vector elements are unsupported. Only
`vector_size(16)` is supported; 8/32-byte IR shapes are intentionally not exposed
by this C subset. The shape is one of `i8x16`, `i16x8`, `i32x4`, `i64x2`,
`f32x4`, `f64x2`; C retains element signedness and nominal integer identity.
Size and natural alignment are sixteen bytes. Lane zero is at the lowest
address. Struct fields, arrays and pointer arithmetic use the complete size.

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
  signedness. Shift counts must be valid C lane shift counts.
- Explicit casts between supported vectors and between a vector and signed
  or unsigned `__int128`, preserving all sixteen bytes. These are bit casts,
  not numeric lane conversions.

Arithmetic operands must have the same C vector type. Scalar broadcast and
implicit conversions between distinct vector types are diagnosed. Integer
vector division/remainder, floating bitwise operations, vector logical
operators/conditions, vector `!`, and reading vectors with `va_arg` are
unsupported. Initializer designators into vector lanes are unsupported.
The backend may expand an operation into scalar **lanes**, preserving the full
vector type, layout and ABI; that is distinct from ignoring the type attribute.
Debug information currently presents a vector as an array of its elements.

## ABI boundary and validation

Direct vector arguments/results use the existing SIMD ABI: Darwin arm64 v0–v7
and v0 result, or Linux x86-64 XMM0–XMM7 and XMM0 result, with aligned stack
arguments when registers run out. Direct/indirect calls and calls in both
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

Future stages can add scalar broadcasts with GNU representability checks,
numeric lane conversions, integer division/remainder, volatile lane operations,
vector initializer designators, additional widths with target-specific ABI
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
