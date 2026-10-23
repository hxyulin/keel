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

## Independent review on current main

Candidate `254bfe01` (source `435d0418`) was copied into the isolated review
worktree on `0e92f93a`; main and the original implementation worktree were not
modified. Independent probes found one preservation regression: the blanket
local/global checks rejected `sizeof((Box){})` and `_Alignof((Box){})` because
parsing had recorded an unevaluated literal's backing storage. Source fix
`7ac8413e` records that distinction and checks literal materialization through
the evaluated-expression walk instead. Two new tests cover unevaluated and
evaluated local/file-scope literals (including nested initializers), late record
completion, recursive pointer graphs, incomplete pointees, atomic-pointer
transport, rejected unsupported atomic subjects, and scope shadowing.

The recursive predicate's visited-record set terminates repeated record visits;
arrays and atomic/complex wrappers recurse, while pointers/functions stop.
`abi_param` checks containment before ABI parameter construction;
`aggregate_layout` checks before result layout/classification. Definition and
expression validation precedes pruning. The evaluated walker checks call
signatures and arguments independently of result use, covering direct/indirect,
unprototyped and variadic calls. No additional evaluated-value escape was found
in this bounded review. Atomic binary128/record subjects remain rejected by the
existing atomic-type parser; these tests do not claim their value support.

Review validation on 2026-10-23, using the locked cargo command form above:

- Full local `keel-c`: **267 passed**. A full local rerun was appropriate because
  the fix changes parser backing-storage metadata and pre-pruning traversal.
- `unsupported_values`, debug and release: **8 passed** in each profile locally
  and on LinuxHost. Target-loop IR/codegen checks remain distinct from native runs.
- LinuxHost debug frontend batch: **37 passed**, adding `computed_goto` to the earlier
  focused batch. The known `raw_platform_complex_header` oracle was excluded;
  no passing full native-suite result is claimed.
- Existing LinuxHost `f80`, `x86_vector_wrappers`, `x86_vector_varargs`: **8 passed**.
  Release `x86_64::feature_guard_tests`: **1 passed**. Local `keel-c --tests`
  clippy completed with existing warnings and none on the review's added lines.

Independent native evidence is retained on LinuxHost under
`/tmp/keel-review-c8a5914d/`, with logs/scripts copied to the same local path.
`baseline.tar` is a fresh archive of `8bb21e9a` and `final-source.tar` is an
archive of `7ac8413e`. All **1966** archived patched files were hash-compared
against the native build checkout without a mismatch. `native-final.log`
records compiler/archive/fixture hashes, exact commands, statuses and object
disassembly. The separately built baseline compiler SHA-256 is
`bc93209d82e11e27ae54e71017376e0ee14f849f963a4fcd3a76736246d5a235`;
the patched compiler is
`b1aa6392561653c70fbc33fea09569c0fff1e498db50461936706eecc65e3bc5`.

The maintained ABI fixture again produces an old object that fails both native
GCC and Clang callers (exit **1**); both native implementation controls pass
(exit **0**). Patched strict-v1 compilation rejects it before object emission,
with and without optimization. Independent positive native controls pass for
both compilers, as do their callers of patched Keel objects in both modes.
They check pointer/callback transport, an unevaluated literal size query and a
supported integer member beside binary128 storage. Full-object byte comparison
confirms the member store changes only its expected bytes. These are finite
native x86-64 Linux checks; they do not demonstrate binary128 ABI support,
other-target execution, application compatibility or exhaustive FP semantics.
Release legality remains active. No performance measurements were taken.

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
