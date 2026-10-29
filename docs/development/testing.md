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

Large matrices keep separate boundary and exhaustive tests: naming a target
with `--quick` runs its boundaries, and full qualification runs the complete
matrix. Test preparation parses and optimizes each fixture once per test and
shares it across configurations. Every configuration still compiles its own
output, but a configuration whose assembly or object is byte-identical to one
already run is not linked and run again. Native C oracles compile once per test
and link against each configuration's object.

Independent assembler comparisons run where the encoding can differ. The NEON
matrices compare bytes with Clang in 4 of their 12 configurations and execute
all 12. The x86 string, dot-product and compare-predicate tests sweep every
immediate only in the native x86-64 Linux configuration and use boundary values
elsewhere, because codegen copies the immediate into the instruction unchanged.

Add a target to the core list only for a shared backend contract with bounded
cost.

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

Tool availability and host architecture determine which native checks run.
Shared export directories belong to one run; do not point separate Cargo or
nextest invocations at the same exports.

## Timing and runner checks

```sh
python3 tools/test.py --report target/test-reports/full.json
python3 -m unittest discover -s tools/tests -p test_test_runner.py
```

The JSON report records selected packages, quick/full coverage, commands, elapsed
wall time and exit codes. It does not infer test counts. Build time is included;
compare warm execution separately before attributing a change to test code.
A full runner exit succeeds only when all commands, including doctests, succeed.

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


## Test layout

End-to-end tests need `clang` (and `clang++` for C++ drivers); ARM64 debug
tests need `lldb`. Tests run on macOS arm64 and x86-64 Linux.

| Tests | Location | Format |
| --- | --- | --- |
| IR golden | `crates/keel-ir/tests/golden/` | `BLESS=1 cargo test` rewrites `.out` files |
| Optimizer passes | `crates/keel-opt/tests/golden/` | first line `; passes: a, b` |
| Codegen programs | `crates/keel-codegen/tests/programs/` | `; expect: exit N` or `; expect: trap`; optional same-named `.c` driver; built in 8 allocator/selection configurations |
| C programs | `crates/keel-c/tests/programs/` | output must match Clang, with and without `-O` |
| Applications | `suite/c/` | see the [suite guide](../../suite/README.md) |

## Instruction metrics

`tools/instruction_metrics.py catalog` lists every AArch64 encoding form in
`crates/keel-asm/isa/a64.json` with its mask and four-byte size. With `--cpu`
and the LLVM tools, it adds `llvm-mca` latency, throughput and resource
estimates for one representative word per form, with an explicit reason for
each missing value. LLVM models `apple-m5` with CycloneModel, so these are
provisional estimates, not measured M5 timings. `analyze` reports a whole
assembly file or a straight-line region, and `compare` compares two reports
for the `size` or `speed` objective, requiring the same CPU, tool and
iteration count. The estimates prove neither equivalence nor a hardware
speedup.

```sh
cargo build -p keel --release
python3 tools/instruction_metrics.py -o encodings.json catalog
python3 tools/instruction_metrics.py -o apple-m5.json catalog \
  --cpu apple-m5 --llvm-mca "$(command -v llvm-mca)" --llvm-mc "$(command -v llvm-mc)"
# Exact metrics on an assembly file, including final branch and address fixups:
target/release/keel asm-metrics input.s -o exact.json
python3 tools/instruction_metrics.py -o before.json analyze before.s
python3 tools/instruction_metrics.py -o after.json analyze after.s
python3 tools/instruction_metrics.py compare before.json after.json --objective size
# For a straight-line region, add CPU estimates to each report:
python3 tools/instruction_metrics.py -o before.json analyze before.s \
  --cpu apple-m5 --llvm-mca "$(command -v llvm-mca)"
python3 tools/instruction_metrics.py -o after.json analyze after.s \
  --cpu apple-m5 --llvm-mca "$(command -v llvm-mca)"
python3 tools/instruction_metrics.py compare before.json after.json --objective speed
```

CPU analysis rejects regions with extra text bytes and needs one MCA region
with the same instruction count. It declines a speed score for warnings,
unmodeled side effects or missing throughput. The raw resource table includes
MCA's aggregate row, whose index equals the instruction count; do not count it
as an instruction.

## Profiling a slowdown

Use an ordinary release build for timing comparisons. On Linux, start with
hardware counters and sampled stacks on the exact failing workload:

```sh
cargo build --release -p keel
perf stat -r 10 -e task-clock,cycles,instructions,branches,branch-misses -- \
  target/release/keel cc --emit-object input.c -o target/profile-input.o
perf record --call-graph dwarf -o target/keel-perf.data -- \
  target/release/keel cc --emit-object input.c -o target/profile-input.o
perf report -i target/keel-perf.data
```

Counter access depends on the Linux host's perf permissions. Keep output writes
and input residency consistent. For source attribution, build with release debug
information (`CARGO_PROFILE_RELEASE_DEBUG=1`). Run sampling separately from the
uninstrumented timing measurement. On macOS use Instruments Time Profiler for
sampled stacks; Linux perf is not a macOS profiler.

Keel already has optional Tracy scopes for frontend work, optimization passes,
instruction selection and allocation. The CLI also marks C preprocessing,
integrated assembly, object finalization and object writing. Integrated assembly
runs inside the streaming codegen callback, so its scopes are nested in codegen;
do not add inclusive durations as if the stages were disjoint.

```sh
cargo build --release -p keel --features profile-with-tracy
# Start the matching Tracy capture application, then run the compiler workload.
target/release/keel cc --emit-object input.c -o target/profile-input.o
```

Use Tracy for phase timelines and repeated pass attribution. Use sampled stacks
and counters to explain time within those phases. The feature is off by default;
rebuild without it before collecting regression timings. No additional profiling
framework is required.
