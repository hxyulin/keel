# Targeted and full tests

Run the Rust suite through `tools/test.py` from any directory. It uses the
`test-fast` Cargo profile, with optimization level 2, limited debug information,
debug assertions and overflow checks. Plain `cargo test` retains its original
unoptimized profile and includes every nonignored matrix.

```sh
python3 tools/test.py assembler --unit --quick     # Assembler library unit tests
python3 tools/test.py assembler --quick            # Assembler packages only
python3 tools/test.py linker --quick               # Linker package only
python3 tools/test.py assembler linker --quick     # Both components
python3 tools/test.py frontend --consumers --quick # Frontend and downstream tests
python3 tools/test.py codegen --unit --quick       # Library unit tests only
python3 tools/test.py linker --test archive        # One integration-test target
python3 tools/test.py --changed main --quick       # Changed packages and affected consumers
python3 tools/test.py --dry-run                    # Print packages and commands without building
python3 tools/test.py full                         # Merge-boundary workspace verification
```

Use component and test-target selections during development. Full workspace
verification belongs at a merge/fast-forward boundary or an explicitly requested
qualification/timing run. Shared IR, ABI or fixture changes may need focused
consumer coverage. Invoking the runner without a component still selects the
whole workspace; that is not the default development check.

Named components run only their own packages by default. Building their test
binaries still builds required dependencies, but does not run those dependencies'
test suites. Add `--consumers` to include transitive workspace consumers,
including dev dependencies. This can select much of the workspace for shared IR
or assembler changes. `--direct` remains accepted and means the default component
selection. Changes selected with `--changed` retain their conservative consumer
coverage; the component default does not narrow automatic change selection.

| Component | Packages |
| --- | --- |
| `assembler` | `keel-asm-ir`, `keel-asm` |
| `decoder` | `keel-disasm` |
| `linker` | `keel-link` |
| `ir` | `keel-ir` |
| `frontend` | `keel-c`, `keel-frontend` |
| `optimizer` | `keel-opt` |
| `codegen` | `keel-codegen` |
| `cli` | `keel` |
| `wasm` | `keel-wasm` |
| `llvm` | `keel-llvm` |
| `toolchain` | `keel-toolchain` |
| `full` or no component | Entire workspace |

Combine named components in one invocation. Use `--unit` for library unit tests
without integration tests, or `--test TARGET` for a specific integration-test
binary; repeat `--test` to select several. Both target options omit doctests.
They work with Cargo and nextest and can be combined with `--quick` to exclude
exhaustive tests within the selected targets. Named selections without target
options or `--quick` include the component's integration tests and doctests.

`--changed BASE` includes differences between BASE and the current working tree,
including staged, unstaged and untracked files. A source change selects its
package and consumers. A top-level integration test source selects its package.
Shared test helpers and fixtures select the entire workspace because several
crates include each other's test inputs without Cargo dependency edges. Root
manifests, generator changes, unknown paths and deleted packages select the
entire workspace. This is a conservative file/dependency selection,
not execution coverage or proof that unselected code cannot be affected.

## Quick and exhaustive coverage

`--quick` skips test names containing `exhaustive_` and skips doctests. The five largest
codegen matrices have separate boundary smoke tests:

- String port I/O: all 288 recipes, both raw and optimized modules, baseline with
  normal registers and full selection/allocation with empty pools, at V1.
- Legacy and AVX strings: both narrow and wide fixtures, baseline/fast with
  registers and full/full with empty pools.
- D8 ABI: every fixture and feature policy, baseline/fast with normal registers
  and full/full with empty pools, each checked against Clang.
- NEON modified immediates: every record and shape, immediate values
  0, 1, 127, 128 and 255 across all selectors, allocators and normal/spill pools, including
  Clang byte comparisons and native execution on macOS ARM64.

Full runs retain exhaustive string and D8 ABI matrices, independent assembler
comparisons and host-conditional native execution. For NEON modified immediates,
all 256 values run on baseline/fast with normal registers and full/full with
spills; boundary values run on all twelve backend configurations.
Exhaustive native string tests are also excluded by `--quick`. Other tests
remain selected; quick is a coverage selection, not a fixed time budget.
Existing ignored tests stay ignored. Privileged guest qualification and external
corpus/native campaigns remain separate from this Rust runner.

Test preparation reuses verified parsed modules and lazily optimized modules
across allocator/pool combinations. String port I/O checks literal bytes once
per distinct instruction and checks whole-object parity for every configuration.
Smoke tests do not write the exhaustive export files.

## Parallel execution

Install `cargo-nextest` to run test binaries in parallel:

```sh
cargo install cargo-nextest --locked
python3 tools/test.py --runner nextest
```

The runner selects nextest when available and otherwise uses Cargo. Override
with `--runner cargo` to compare behavior. Nextest runs at most four tests at
once; the codegen group is limited to two. These bounds limit concurrent compiler
processes and large modules. Tests use process-specific temporary paths. The
runner invokes Cargo doctests separately after a full nextest run, even if a
regular test failed. Nextest writes JUnit results under `target/nextest/default/`.

Both runners use the same Cargo profile. To compare with unoptimized tests:

```sh
python3 tools/test.py codegen --runner cargo --profile test
```

For a single test target, Cargo remains sufficient:

```sh
cargo test --profile test-fast -p keel-codegen --test x86_string_port_io
cargo test --profile test-fast -p keel-codegen --test x86_string_port_io source_matrix_boundary_smoke
```

Tests comparing external GNU assembly require `KEEL_GNU_AS` to point to the
appropriate assembler. Tool availability and host architecture still determine
which native checks can execute. Shared export directories should belong to one
run; do not run separate Cargo or nextest invocations against the same exports.

## Timing and runner checks

```sh
python3 tools/test.py --report target/test-reports/full.json
python3 -m unittest discover -s tools/tests -p test_test_runner.py
```

The JSON report records selected packages, quick/full coverage, commands, elapsed
wall time and exit codes. It does not infer test counts. Build time is included;
compare warm execution separately before attributing a change to test code.
A full runner exit succeeds only when all commands, including doctests, succeed.

## Component coverage

The runner separates assembler, decoder, linker, IR, optimizer, frontend and
codegen tests. Use component selections and `--consumers` for focused changes;
run the full workspace before treating an integration as validated. Optional
external-tool and platform tests remain subject to their prerequisites.

Historical timings and per-checkpoint test counts are not maintained here.
Record new measurements in ignored report directories.

## Suite runner checks

`cargo suite check` runs the Python runner tests without application builds or
benchmarks. Select components with `cargo suite check c ir` or
`cargo suite check asm linker integration`. These are infrastructure checks;
use `cargo suite test` for applications and `cargo suite test-frontends` for
native frontend/import validation. See the [suite index](../../suite/README.md).

## Regression scope

Keep tests that distinguish failure modes: valid and rejected inputs, immediate
boundaries, target/ABI differences, register pressure, flags and memory effects.
For stable linear immediate lowering, use endpoints, adjacent values and a
midpoint instead of expanding every count into a separate native function.
NEON opcode and shape coverage remains complete in those fixtures; the immediate
sampling is a regression subset, not exhaustive encoding qualification.

Object routes still need parity checks. Once complete object bytes agree, link
and execute one image against the independent oracle. Reuse the existing
immutable module preparation cache when selector/allocator combinations use
the same raw or optimized IR. Keep exhaustive numerical algorithms and unique
negative controls where sampling would hide different behavior.

The Python suite checks exercise runner failure handling and fixture protocols;
they are separate from native integration campaigns. Do not prune inexpensive
protocol checks to compensate for repeated native compiler or linker invocations.
