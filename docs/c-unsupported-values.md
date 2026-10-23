# C unsupported-value safety checkpoint

The existing `__float128` placeholder supports declarations and layout, not
binary128 values or ABI classification. At `8bb21e9a`, a record containing it
could bypass the immediate-type checks and enter ordinary integer aggregate
classification. This checkpoint closes that path before adding any extended
floating type support.

## Reproducer and bounded proof

The maintained [function fixture](../crates/keel-c/tests/fixtures/extended-float-aggregate-abi.c)
returns the second of two `struct box { __float128 x; }` arguments. Distinct
inputs in the [native harness](../crates/keel-c/tests/fixtures/extended-float-aggregate-harness.c)
avoid accidentally passing by leaving the first input in XMM0.

An independent clean `8bb21e9a` archive was built on LinuxHost under the shared
build lock with native-link caching disabled. Its real x86 ELF object uses
integer argument/result registers; both GCC and Clang callers return **1**.
Each compiler's own implementation with its own harness returns **0**. This
reproduces the ABI failure independently of the earlier research binary.
The patched compiler instead diagnoses `values containing '__float128' are
not supported` (with backticks in the diagnostic), before object emission,
both without optimization and with `-O --opt-tier none`, at explicit
`--target x86_64-unknown-linux-gnu --cpu-level v1`.

This establishes rejection of an observed miscompilation. It does not
establish working binary128 transport.

## Guard boundaries

`Types::unsupported_value` traverses record/union fields, arrays, atomic
wrappers and complex components. It stops at pointers and function types and
tracks visited records. Type aliases already resolve to these same types.
The parser uses this predicate for object definitions, including initialized
`extern` objects. ABI parameter and aggregate-result classification check it
before constructing an IR layout.

IR generation also validates definitions, locals, evaluated expressions and
static initializers before pruning. Calls, by-value results and arguments,
varargs, reads/copies and materialized function symbols cannot escape through
unused static or inline bodies. The existing evaluated-expression walker is
shared with computed-goto recognition; it distinguishes address computations
from value reads. Arrays decay to pointers. Taking a field's address and
accessing a supported member beside an unsupported field do not read the
containing aggregate.

Unused typedefs, extern object declarations, function prototypes and
function-pointer types remain accepted. Data pointers and existing callback
pointers can be transported without interpreting their pointees or calling an
unsupported ABI. Unevaluated size, alignment, offset and type-identity queries
retain their existing behavior. No type is rejected merely for appearing in an
unused extern declaration.

Opaque numeric identity is now included in the existing numeric redeclaration
check, including pointer/array/function nesting. For example,
`extern __float128 x; extern double x;` diagnoses instead of changing the
numeric identity. Identical opaque redeclarations remain accepted. This does
not broaden the frontend's general policy for unrelated C redeclarations.

## Validation

All Rust commands used the existing pinned toolchain and default managed cargo
cache, serialized by `suite/fast_opt_locked.py`, with this command form:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-opaque-safety-fd060343/test.jsonl \
  cargo test --offline --locked -j2 -p keel-c --test unsupported_values
```

`unsupported_values` has six tests covering the native reproducer; aliases,
nested records/unions/arrays; definitions and static initializers; direct and
indirect calls; varargs and the existing `va_arg` rejection; dropped bodies;
numeric conflicts; and accepted declaration/layout/pointer controls. Positive
controls verify IR and assemble real Mach-O/ELF objects on all three supported
targets, with and without debug information. These target-loop checks are
frontend, lowering and encoding checks, not native execution on each target.

On LinuxHost, native GCC 16.2.1 20260810 and Clang 23.1.1 independently passed the
positive controls. Both also linked and ran Keel's strict-v1 objects with and
without optimization. The harness checks object/field addresses, existing
callback-pointer transport, and a supported integer member beside an opaque
field. A byte comparison checks that the member store changes only the
expected bytes. The two native compiler controls also pass. No binary128
value operation is attributed to Keel.

The final full local `keel-c` suite passed **265 tests**. Local
`cargo clippy --offline --locked -j2 -p keel-c --tests` completed with existing
repository warnings; none identifies the new guard or regression-test lines.

The final focused LinuxHost batch passed **33 tests** across `unsupported_values`,
`system_header_compat`, `gnu_types`, `gnu_vector_abi`, `x86_intrinsics` and
`complex` (excluding the previously documented `raw_platform_complex_header`
oracle failure). Existing native codegen `f80`, `x86_vector_wrappers` and
`x86_vector_varargs` passed **8 tests**. The release
`x86_64::feature_guard_tests` emission gate passed **1 test**. No backend or
release ISA legality policy changed.

Logs, exact commands, source archives, disassembly, objects and executables
are outside Git under `/tmp/keel-opaque-safety-fd060343/` on LinuxHost; local logs
and copied native evidence use the same path on the implementation host.
There are no timing or performance conclusions.

## Remaining limits

This is a rejection checkpoint, not the broader
[extended-float declaration proposal](c-system-header-compat.md). It adds no
`TC`, `_FloatN`, F128 arithmetic, IR type, helper, ABI class or layout redesign.
Existing opaque layout limitations remain; a successful layout query does not
establish value support. Ordinary aggregate, complex and vector behavior is
covered by existing regression controls; backend F80 support is unchanged.
Native execution evidence here is finite x86-64 Linux evidence. It is not an
application-compatibility result or an exhaustive ABI/FP/memory proof.
