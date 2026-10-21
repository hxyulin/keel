# C frontend compatibility coverage

This milestone audits GNU attributes in `crates/keel-c/src/parse.rs` against
Keel's declaration and emission contracts. It adds diagnostics, without new IR,
optimizer, backend, linker or LLVM importer behavior. It supersedes the older
`docs/design.md` statement that all of the attributes below are ignored.

## Linkage and lifetime boundary

The parser now rejects `weak`, `weakref`, `alias`, `ifunc`, `constructor`,
`destructor`, `cleanup`, `section`, `used` and `retain`, including their GNU
`__name__` spellings. These previously parsed successfully and were discarded.

These are observable contracts: weak hooks need symbol binding and unresolved
weak-reference behavior; aliases and resolvers need symbol definitions and
resolution; startup/shutdown functions need registration and retention; cleanup
needs calls on scope exits; section registries need placement; used/retain need
emission or linker retention. Keel emits private definitions only when referenced,
so accepting a registration function without its retention contract can remove
it completely. A prototype or late redeclaration can supply the contract, so the
diagnostic is issued when parsing the attribute, even in otherwise unused code.
The check is intentionally conservative about placement, including attributes
that GCC/Clang might warn about and ignore on an inapplicable declaration.

`visibility("default")` is accepted because it agrees with current public
emission; static declarations remain local. Hidden, internal and protected
visibility are rejected rather than exported with default visibility. Adjacent
string literals and GNU attribute spellings work. A malformed visibility
argument is an error. This does not advertise general visibility support:
`__has_attribute(visibility)` remains zero, as do the other rejected attributes.

Diagnostic annotations such as `format`, `nonnull`, `deprecated`,
`warn_unused_result` and `unused`, and advisory allocation annotations such as
`malloc` and `alloc_size`, remain accepted. Existing alignment, packing and
function-contract handling is unchanged. Unknown attributes still follow the
existing permissive policy; this is a bounded audit, not a complete safety
classification of all GNU/Clang annotations.

## Reproducible representative library check

`crates/keel-c/tests/fixtures/attribute-library.c` models a small library header
and implementation: default-visible API declarations/redeclarations, format and
allocation prototypes, unused parameters, a deprecated compatibility wrapper,
and feature-query checks for unsupported optional hooks. It is synthetic and
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

The integration check builds eight objects: both supported targets, native and
Clang preprocessing, with and without optimization. On supported native hosts it
links and executes four binaries, comparing exit status and output with Clang.
It also requires ten raw-C CLI failures across the two targets for weak hooks,
aliases, constructors, cleanup and hidden visibility, with no output object
created. Cross-target objects are compiled but not executed. The focused Rust
suite additionally checks all rejected names, declaration placement, late
redeclarations after definitions, block declarations, unused inline functions,
malformed visibility, source locations, IR verification and both code generators.
No performance measurements or claims are made.

## Remaining workload blockers

Libraries that enable weak hooks, hidden internal APIs, startup registration,
section registries or cleanup unconditionally now stop with a diagnostic. They
need explicit source/configuration fallbacks or actual IR/object/runtime support;
removing these annotations mechanically can change behavior. In particular, the
existing design audit identifies Lua's `LUAI_FUNC` internal visibility: a build
that expands it to non-default visibility now fails explicitly. Native feature
queries can select a fallback; Clang-preprocessed input reflects Clang's answers
and can still enable unavailable semantics.

The parser's catch-all still ignores other potentially significant attributes,
including `vector_size`, `ext_vector_type`, `mode`, calling-convention attributes
and `transparent_union`. They need separate type/ABI audits; this milestone does
not make those workloads safe. Visibility pragmas and weak-import annotations
are also outside this change. The previously documented variable-length-array,
old-style definition, block-literal/runtime, complex-value and x86 long-double
boundaries remain. This representative check does not establish coverage for
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
