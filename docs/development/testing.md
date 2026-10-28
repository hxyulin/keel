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
python3 tools/test.py codegen --quick              # Core codegen development check
python3 tools/test.py codegen --unit --quick       # Library unit tests only
python3 tools/test.py linker --test archive        # One integration-test target
python3 tools/test.py --changed main --quick       # Changed packages and affected consumers
python3 tools/test.py --dry-run                    # Print packages and commands without building
python3 tools/test.py full                         # Merge-boundary workspace verification
```

GNU assembler oracle tests require GNU binutils. On macOS, `as` is Apple's
assembler, so set `KEEL_GNU_AS` to the absolute path of an x86-64 GNU
cross-assembler before running the workspace suite. These tests retain both
the Clang and GNU comparisons; they do not skip a missing oracle silently.

All main-workspace crates inherit the lint policy in the root manifest. Run
`cargo fmt --all --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` to check
formatting and the full lint configuration, including experimental features.

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
package and consumers. Test sources and indexed fixtures select their owner and
packages that import them through literal `include!`, `include_str!`,
`include_bytes!` or `#[path]` dependencies. Imports into library sources also
select the importing package's Cargo consumers. Module path attributes use
conservative suffix matching to cover inline module bases. Cross-package imports
of a `src/` tree also count as package dependencies, covering edits to ordinary
child modules without parsing Rust's full module tree.

All three Rust macro delimiters are supported for literal includes. The index
does not expand arbitrary procedural macros or execute generators.

Unindexed fixtures, computed include paths, macro-supplied include names,
identifier concatenation, conditional path attributes, root
manifests, generator changes, unknown paths and deleted packages select the
entire workspace. Generated `OUT_DIR` includes are build outputs rather than
Git changed-file inputs. Runtime-only fixture discovery is not inferred from
filenames and retains the full-workspace fallback. This is a conservative
file/dependency selection, not execution coverage or proof that unselected code
cannot be affected.

## Development and qualification coverage

`--quick` excludes `exhaustive_` tests and doctests. For codegen it also selects
all library unit tests plus the core integration targets listed in
`CODEGEN_CORE_TARGETS` in [the runner](../../tools/test.py). These cover backend
selection/allocation contracts, stack frames, addressing, narrow values,
feature gates, ABI boundaries, vector IR and typed/text object routes. Cargo
only builds the selected test targets.

```sh
python3 tools/test.py codegen --quick                         # Core development regressions
python3 tools/test.py codegen --unit --quick                  # Library unit tests only
python3 tools/test.py codegen --test arm_constant_division --quick
python3 tools/test.py codegen --test x86_legacy_strings --quick
python3 tools/test.py codegen                                 # Full component qualification
```

An explicit `--test` always selects that target, including targets outside the
core list. `--quick` then only skips its exhaustive-named cases. Feature work
must run its owning target; the core profile does not cover every implemented
instruction, native oracle or ABI combination. Other components keep their
ordinary quick selection. Mixed selections still run the other packages' tests
and apply the core profile only to codegen.

The broad component run remains available without `--quick`, as does ordinary
`cargo test -p keel-codegen`. No tests are deleted or newly ignored by this
selection. Use broad qualification at merge boundaries; do not use it as the
normal edit/test loop.

The large string, D8 ABI and modified-immediate targets retain separate boundary
and exhaustive tests. Naming an individual target with `--quick` runs its
boundaries; full qualification retains the complete matrices. For modified
immediates, all 256 values run on baseline/fast with normal registers and
full/full with spills; boundary values run on all twelve configurations.
Independent assembler comparisons and host-conditional execution remain intact.
Existing ignored tests stay ignored. Guest qualification and external corpus
campaigns remain separate.

Test preparation reuses verified parsed modules and lazily optimized modules.
The native `run` corpus prepares each fixture once per test: its parsed IR,
optimized IR (including fixture-specific `opt-passes`) and C or C++ driver
objects are shared across configurations. Every configuration still compiles,
links and executes its own backend output. Preparations use separate temporary
directories so tests selecting the same fixture can run concurrently.
The `arm_division` and `adjacent_diff` matrices also compile their C oracle once
and reuse its object across backend configurations.
String port I/O checks literal bytes once per distinct instruction and object
parity for every configuration. Boundary smoke tests do not write exhaustive
export files. Add a target to the core list only for a shared backend contract
with bounded cost, not to track feature completion.

## Parallel execution

Install `cargo-nextest` to run test binaries in parallel:

```sh
cargo install cargo-nextest --locked
python3 tools/test.py codegen --quick --runner nextest
```

The runner selects nextest when available and otherwise uses Cargo. Override
with `--runner cargo` to compare behavior. Nextest runs at most four tests at
once; the codegen group is limited to two. These bounds limit concurrent compiler
processes and large modules. Tests use process-specific temporary paths. The
runner invokes Cargo doctests separately after a full nextest run, even if a
regular test failed. Nextest writes JUnit results under `target/nextest/default/`.

Both runners use the same Cargo profile. To compare with unoptimized tests:

```sh
python3 tools/test.py codegen --quick --runner cargo --profile test
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
