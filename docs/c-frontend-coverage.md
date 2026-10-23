# C frontend compatibility coverage

This milestone audits GNU attributes in `crates/keel-c/src/parse.rs` against
Keel's declaration and emission contracts. It adds diagnostics, and hidden
symbol visibility in the IR and both code generators. Ordinary weak binding is
now implemented as described in [C weak symbols](c-weak-symbols.md). It
supersedes the older
`docs/design.md` statement that all of the attributes below are ignored.

## Linkage and lifetime boundary

The parser rejects `weakref`, `weak_import`, `alias` and `ifunc`, including GNU
`__name__` spellings. These contracts need additional symbol identity semantics.
[Custom section placement and `retain`](c-sections-retain.md) now support a bounded
non-TLS static-storage/function definition subset on both native targets. Diagnostics apply
also to prototypes, late redeclarations and unused code.

The [bounded lifecycle subset](c-lifecycle.md) now implements default-priority
`constructor`/`destructor` registration and compiler-only `used` emission.
Other priorities and malformed lifecycle contracts diagnose. `used` does not
supply linker retention. Inapplicable placements are rejected even where
GCC/Clang might warn and ignore them.

Ordinary `weak` now works on external function/object definitions and undefined
references on both native targets, including declarations after definitions.
Static weak symbols, TLS and hidden undefined combinations diagnose; see the
[supported scope and validation](c-weak-symbols.md). Weak on a typedef, member,
parameter, automatic variable, type name or tag remains rejected.

Automatic-local cleanup has since gained a bounded implementation: see
[scope-exit actions and cleanup](c-language-implementation-plan.md#3-reusable-scope-exit-actions-and-cleanup--bounded-milestone-implemented).
It runs on normal scope exits, return, break and continue, with ordered VLA
restoration; unsupported placements, goto/switch and known nonlocal calls are
still diagnosed. Its feature query remains zero for general library fallbacks.


`visibility("hidden")` and `visibility("internal")` are implemented. The
visibility is recorded on the symbol and merged across declarations, so a
definition inherits it from an earlier declaration (as with Lua's `LUAI_FUNC`
and `LUAI_DDEC`) and a later redeclaration may add it. Internal is emitted as
hidden: the linker treats `STV_INTERNAL` as at least hidden, and dropping the
extra promise is conservative. Explicit visibilities that differ between
declarations, or within one, are rejected, like Clang. Static symbols remain
local and visibility has no effect on them. A nonlocal definition gets the IR
`hidden` flag (functions and data, including zero-initialized, constant and
thread-local objects), emitted as `.hidden` (ELF `STV_HIDDEN`) on x86-64 Linux
and `.private_extern` on AArch64 Mach-O, in both assembly text and objects.
Undeclared-here references stay default, the weaker assumption. Hidden symbols
are still `!local` to the optimizer, so no pass treats them less conservatively;
using hidden visibility as a noninterposability promise is future work.

`visibility("default")` is accepted as before. `protected`, unknown strings and
malformed arguments are errors, as are hidden or internal visibility on
typedefs, members, parameters, automatic variables, type names and tags.
Adjacent string literals and GNU attribute spellings work.
`__has_attribute(visibility)` and `__has_attribute(weak)` remain zero, so
feature-tested sources keep their fallbacks.

Diagnostic annotations such as `format`, `nonnull`, `deprecated`,
`warn_unused_result` and `unused`, and advisory allocation annotations such as
`malloc` and `alloc_size`, remain accepted. Existing alignment, packing and
function-contract handling is unchanged. Unknown attributes still follow the
existing permissive policy; this is a bounded audit, not a complete safety
classification of all GNU/Clang annotations.

## Reproducible representative library check

`crates/keel-c/tests/fixtures/attribute-library.c` models a small library header
and implementation: default-visible API declarations/redeclarations, an
internal-visibility function, an unused weak prototype, format and allocation
prototypes, unused parameters, a deprecated compatibility wrapper, and
feature-query checks for unsupported optional hooks. It is synthetic and
self contained; passing it is not a claim that a complete external project works.

Run from the repository root, with a fresh private directory:

```sh
work=$(mktemp -d /tmp/keel-c-coverage.XXXXXX)
export CARGO_TARGET_DIR="$work/target"
export TMPDIR="$work/tmp"
mkdir -p "$TMPDIR"
python3 suite/ir_lowering_locked_run.py --output "$work/c-tests.log" -- \
  cargo test --locked -j2 -p keel-c
python3 suite/ir_lowering_locked_run.py --output "$work/cli-tests.log" -- \
  cargo test --locked -j2 -p keel --test preprocess
python3 suite/ir_lowering_locked_run.py --output "$work/build.log" -- \
  cargo build --locked -j2 -p keel
python3 suite/ir_lowering_locked_run.py --output "$work/integration.log" -- \
  python3 crates/keel-c/tests/fixtures/check_attribute_semantics.py \
  --keel "$CARGO_TARGET_DIR/debug/keel" --out-dir "$work/integration"
```

The integration check builds twelve objects: eight library-macro probes cover
both targets, native and Clang preprocessing, with and without optimization;
four additional weak probes cover both targets with and without optimization. On supported native hosts it
links and executes six binaries, comparing exit status and output with Clang.
It also requires ten raw-C CLI failures across the two targets for invalid
weak static declarations, aliases, constructors, cleanup and protected visibility,
with no output object created. Cross-target objects are compiled but not executed. The focused
Rust suite additionally checks all rejected names, declaration placement, late
redeclarations after definitions, block declarations, unused inline functions,
weak declaration merging and invalid placements, malformed and conflicting
visibility, hidden flags in IR before and after optimization, and both code
generators. `crates/keel/tests/hidden_visibility.rs` checks hidden symbols in
assembly text and objects for both targets, and on a native host links a
Keel object with a Clang-compiled caller and checks that a shared library does
not export them. No performance measurements or claims are made.

## Remaining workload blockers

Libraries that use protected visibility,
startup registration, section registries unconditionally, still stop
with a diagnostic. They need explicit source/configuration fallbacks or actual
IR/object/runtime support (`STV_PROTECTED`); removing these annotations
mechanically can change behavior.
Lua's internal-visibility `LUAI_FUNC` builds now compile, as do glibc headers
with unused weak prototypes: the Clang-preprocessed Linux Lua 5.4.7, QuickJS
and SQLite inputs compile at `-S` and `--emit-object`, with and without `-O`.
That historical result is not a current glibc compatibility claim: the frozen
LinuxHost glibc 2.44 inputs stop at extended floating declarations on current main;
see [the unchanged-input evidence and proposed boundary](c-system-header-compat.md#declaration-only-boundary-investigation-at-8bb21e9a).
Native feature queries can select a fallback; Clang-preprocessed input reflects
Clang's answers and can still enable unavailable semantics.

Subsequent [GNU type support](gnu-vector-mode.md) implements bounded
`vector_size(16)` and integer `mode` attributes. Unsupported calling conventions,
`ext_vector_type` and `transparent_union` now produce diagnostics. Unknown
attributes still follow the permissive catch-all policy; this remains a bounded
audit. Visibility pragmas (`#pragma GCC visibility`),
`-fvisibility` remains outside this change; weak-import annotations now diagnose. The previously documented variable-length-array,
old-style definition, block-literal/runtime, remaining complex and x86 long-double
boundaries are tracked in [the implementation plan](c-language-implementation-plan.md).
Multidimensional automatic VLAs, bounded VM types and a first float/double
complex subset now work; explicitly typed old-style definitions now have a
fixed-size parameter milestone with promoted ABI entry conversions. Their
remaining limits are listed in that plan. This representative check does not establish coverage for
those features or whole Lua/QuickJS/SQLite builds.

## Validation for this milestone

Validated with private target/artifact directories under
`/tmp/keel-c-coverage-b9642f7b`, with native commands serialized through
`suite/ir_lowering_locked_run.py` and Cargo builds limited to `-j2`:

- `cargo test --locked -j2 -p keel-c --test attribute_semantics`: 4 passed
  (rerun after final fixture/test edits).
- `cargo test --locked -j2 -p keel --test preprocess`: 7 passed.
- Representative CLI script: 8 objects, 4 native executions matching Clang,
  10 expected semantic-attribute failures, passed.
- `cargo test --locked -j2 -p keel-c`: 148 passed, 2 failed. The failures are
  `run::aligned_objects_with_debug_info` and `run::programs`, involving the
  thread-local address-alignment output in `programs/alignment.c`. A separate
  archive of unchanged parent commit `1ac952c2`, with its own Cargo target,
  reproduces both failures via `cargo test --locked -j2 -p keel-c --test run`
  (14 passed, 2 failed). No alignment/backend changes were attempted here.
- Rust formatting and `git diff --check` passed.

The broad test command stops at the failed integration binary; it does not
establish a successful full suite or doc-test run. Logs and lock metadata are
local artifacts, not checked into the repository.

## Validation for hidden visibility and unused weak declarations

- `cargo test --workspace --release --no-fail-fast` on AArch64 macOS: 1709
  passed, 3 failed, all pre-existing: `keel-asm` `x86_bmi` (2) and
  `keel-c` `run::aligned_objects_with_debug_info`.
- The representative CLI script passed on AArch64 macOS and x86-64 Linux.
  `crates/keel/tests/hidden_visibility.rs` and the focused `keel-c` and
  `keel-codegen` suites passed natively on both hosts.
- All 44 Clang-preprocessed Linux Lua 5.4.7, QuickJS and SQLite inputs, with
  their `visibility("internal")` and unused `__weak__` prototypes, compile for
  `x86_64-unknown-linux-gnu` at `-S` and `--emit-object`, with and without
  `-O`. Their output equals that of the same inputs with those attributes
  removed, except for one added `.hidden` line (186 in all, all Lua) or one
  `st_other` byte per hidden symbol.
- On x86-64 Linux, a Lua interpreter built from those Keel objects and linked
  by Clang runs; with `-Wl,-E` it exports the `lua_`/`luaL_` API and none of
  the internal `luaX_` functions, which `readelf -s` shows as `HIDDEN` in the
  objects.
- Codegen for inputs without these attributes is unchanged: the compile-time
  corpus produced byte-identical outputs for all eight configurations.
