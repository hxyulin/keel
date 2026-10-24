# C unsupported-value safety checkpoint

The existing `__float128` placeholder supports declarations and layout, not
binary128 values or ABI classification. Previously, a record containing it
could bypass the immediate-type checks and enter ordinary integer aggregate
classification. This checkpoint closes that path before adding any extended
floating type support.

## Reproducer and bounded proof

The maintained [function fixture](../crates/keel-c/tests/fixtures/extended-float-aggregate-abi.c)
returns the second of two `struct box { __float128 x; }` arguments. Distinct
inputs in the [native harness](../crates/keel-c/tests/fixtures/extended-float-aggregate-harness.c)
avoid accidentally passing by leaving the first input in XMM0.

Before this checkpoint, the x86 ELF object used integer argument/result
registers, and both GCC and Clang callers returned **1** on x86-64 Linux, while
each compiler's own implementation with its own harness returned **0**.
Keel now diagnoses `values containing '__float128' are
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

Compound-literal backing storage is distinguished from declared objects.
The parser can record that storage while reducing an unevaluated `sizeof`
or alignment query. Such a query does not materialize the literal. Evaluated
literals are checked even when only their address or a supported member is
requested; file-scope literal initializers are checked recursively when reached.

Opaque numeric identity is now included in the existing numeric redeclaration
check, including pointer/array/function nesting. For example,
`extern __float128 x; extern double x;` diagnoses instead of changing the
numeric identity. Identical opaque redeclarations remain accepted. This does
not broaden the frontend's general policy for unrelated C redeclarations.

## Validation

```sh
cargo test -p keel-c --test unsupported_values
```

The tests cover the native reproducer; aliases, nested records/unions/arrays;
definitions and static initializers; direct and indirect calls; varargs and the
existing `va_arg` rejection; dropped bodies; numeric conflicts; unevaluated and
evaluated local/file-scope compound literals (including `sizeof((Box){})` and
`_Alignof((Box){})`, which must stay accepted); late record completion,
recursive pointer graphs, incomplete pointees, atomic-pointer transport and
rejected unsupported atomic subjects. Positive controls verify IR and assemble
real Mach-O/ELF objects on all three supported targets, with and without debug
information; these are frontend, lowering and encoding checks, not native
execution on each target.

On x86-64 Linux, native GCC and Clang callers link and run Keel's strict-v1
objects with and without optimization. The harness checks object/field
addresses, callback-pointer transport, an unevaluated literal size query and a
supported integer member beside an opaque field; a byte comparison checks that
the member store changes only the expected bytes. No binary128 value operation
is attributed to Keel.

## Remaining limits

This is a rejection checkpoint, not the broader
[extended-float declaration proposal](c-system-header-compat.md). It adds no
`TC`, `_FloatN`, F128 arithmetic, IR type, helper, ABI class or layout redesign.
Existing opaque layout limitations remain; a successful layout query does not
establish value support. Ordinary aggregate, complex and vector behavior is
covered by existing regression controls; backend F80 support is unchanged.
Native execution evidence here is finite x86-64 Linux evidence. It is not an
application-compatibility result or an exhaustive ABI/FP/memory proof.

## Binary128 declaration/layout follow-up

The subsequent [TC checkpoint](c-system-header-compat.md#implemented-declarationlayout-checkpoint)
extends the opaque declaration model with independent alignment and a canonical
complex-binary128 identity on x86-64 Linux. It reuses this checkpoint's recursive
containment, pre-pruning validation and ABI rejection. Unevaluated calls with
exactly matching opaque arguments and component type queries are now typed;
evaluated uses remain unsupported. The old aggregate ABI reproducer continues
to diagnose. This follow-up does not grant binary128 values or ABI transport.
