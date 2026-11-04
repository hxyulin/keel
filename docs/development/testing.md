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
python3 tools/test.py optimizer --quick            # Optimizer tests plus program oracles
python3 tools/test.py codegen --unit --quick       # Library unit tests only
python3 tools/test.py linker --test archive        # One integration-test target
python3 tools/test.py --changed main --quick       # Changed packages and affected consumers
python3 tools/test.py --dry-run                    # Print packages and commands without building
python3 tools/test.py full                         # Merge-boundary workspace verification
python3 tools/test.py lint                         # Merge-boundary format and clippy checks
python3 tools/test.py generators                   # Generated-file staleness and generator tests
python3 tools/test.py cranelift                    # The separate keel-cranelift workspace
```

Oracle tests find external tools through `crates/keel-test-support`. Set
`KEEL_CLANG`, `KEEL_GNU_AS` or `KEEL_GNU_LD` to name a tool explicitly.
Otherwise GNU as and GNU ld are searched on `PATH`: `as` and `ld` on Linux,
then the `x86_64-linux-gnu-` and `x86_64-elf-` cross tools; off Linux, `as` is
not GNU as, so only the cross tools qualify. A bare-ELF GNU as such as
`x86_64-elf-as` gets `--divide`, so `/` is division rather than a comment.

A test that cannot run because a tool or host capability is missing records a
skip: it prints `KEEL-SKIP: <reason>` and appends a line to the skip log
described under [Test environment](#test-environment). A skipped test still
reports as passed to Cargo. Set `KEEL_REQUIRE_ORACLES=1`
on a qualification host to turn every skip into a failure.

All main-workspace crates inherit the lint policy in the root manifest.
`tools/test.py lint` runs `cargo fmt --all --check`, then
`cargo clippy --workspace --all-targets -- -D warnings` on default features,
then the same clippy command with every workspace feature enabled except
`profile-with-tracy`, which builds the Tracy C++ client. Run it at each merge
boundary.

`tools/test.py generators` runs `generate.py --check` for the x86 tables in
`tools/x86-isa` and the AArch64 tables in `crates/keel-asm/tools`, then each
directory's `test_*.py` unit tests. A generated file that differs from its
inputs fails the check. Run it when a generator, its inputs or a generated file
changes. Both generators rewrite only outputs whose content changed, so a no-op
regeneration does not trigger rebuilds.

`tools/test.py cranelift` builds and tests `crates/keel-cranelift`, which has its
own workspace and lock file, with
`cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked`. Run it at
merge boundaries; `--changed` also selects it for changes that reach `keel-ir`.

These check modes run every step even after a failure, then list each step's
result. They accept `--dry-run`, `--cargo` and `--report`, and cannot be
combined with test components or test selection options.

Use component and test-target selections during development. Full workspace
verification belongs at a merge/fast-forward boundary or an explicitly requested
qualification/timing run. Shared IR, ABI or fixture changes may need focused
consumer coverage. Invoking the runner without a component still selects the
whole workspace; that is not the default development check.

Named components run only their own packages by default. Building their test
binaries still builds required dependencies, but does not run those dependencies'
test suites. Add `--consumers` to include workspace consumers: packages that
depend on the component through normal or build dependencies, transitively, plus
the packages that dev-depend on any of those. The walk does not continue past a
dev-dependency, because Cargo builds a dependency without its dev-dependencies.
This can select much of the workspace for shared IR or assembler changes.
`--direct` remains accepted and means the default component selection.
`--changed` uses the same consumer walk.

| Component | Packages |
| --- | --- |
| `assembler` | `keel-asm-ir`, `keel-asm` |
| `decoder` | `keel-disasm` |
| `linker` | `keel-link` |
| `ir` | `keel-ir`, `keel-ir-interp` |
| `frontend` | `keel-c`, `keel-frontend` |
| `optimizer` | `keel-opt` |
| `codegen` | `keel-codegen` |
| `cli` | `keel` |
| `wasm` | `keel-wasm` |
| `llvm` | `keel-llvm` |
| `util` | `keel-util` |
| `full` or no component | Entire workspace |

Combine named components in one invocation. Use `--unit` for library unit tests
without integration tests, or `--test TARGET` for a specific integration-test
binary; repeat `--test` to select several. Both target options omit doctests.
They work with Cargo and nextest and can be combined with `--quick` to exclude
exhaustive tests within the selected targets. Named selections without target
options or `--quick` include the component's integration tests and doctests.

`--changed BASE` includes differences between BASE and the current working tree,
including staged, unstaged and untracked files. Each path selects packages and
checks:

| Changed path | Selects |
| --- | --- |
| `tools/x86-isa/**`, `crates/keel-asm/tools/**`, `crates/keel-asm/isa/**` | the `generators` checks |
| `*.md`, `docs/**` | nothing |
| `crates/keel-cranelift/**` | the `cranelift` checks |
| `suite/<group>/**` for `c`, `ir`, `asm`, `linker`, `integration` | `cargo suite check <group>` |
| other `suite/**`, including `suite/common` | `cargo suite check` for every group |
| other `tools/**` | the `tools/tests` unit tests |
| `crates/<pkg>/tests/**` | `<pkg>` |
| other files in `crates/<pkg>` | `<pkg>` and its consumers |
| anything else, such as root manifests and `Cargo.lock` | the entire workspace |

The first matching row applies. The first six rows are the `PATH_POLICY` table
in [the runner](../../tools/test.py).

A Rust source that reads a changed path also selects its package. The runner
finds readers in two ways:

- Literal `include!`, `include_str!`, `include_bytes!` and `#[path]` arguments,
  with all three macro delimiters. Imports into library sources also select the
  importing package's consumers. Module path attributes use suffix matching to
  cover inline module bases. A cross-package import of a `src/` tree counts as a
  package dependency, which covers edits to its ordinary child modules.
- String literals that name a repository path, for fixtures that tests list at
  run time. Leading `..` components are dropped, so
  `join("../keel-c/tests/programs")` in keel-asm makes every file under
  `crates/keel-c/tests/programs` select keel-asm. A path built from separate
  components is not recognized.

`crates/keel-cranelift` sources are indexed as readers too, so a workspace
fixture they read selects the `cranelift` checks.

Computed include paths, macro-supplied include names, identifier
concatenation and conditional path attributes select the entire workspace.
Generated `OUT_DIR` includes are build outputs rather than Git changed-file
inputs. The index does not expand procedural macros or run generators. This is a
file and dependency selection, not execution coverage or proof that unselected
code cannot be affected.

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
python3 tools/test.py codegen --test x86_strings --quick
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

keel-opt's own tests check IR text. Under `--quick`, a selection that includes
keel-opt also runs the program-level oracles: `-p keel-codegen --test run`,
`-p keel-c --test run`, and the `OPTIMIZER_E2E_TARGETS` in crates/keel/tests.
Those targets compile C or IR through optimizing pipelines and compare the
program's behavior with an independent oracle. A selected package already runs
all of its targets, so it gets no extra command. A warm
`tools/test.py optimizer --quick` took 33 to 36 seconds on an Apple M5 Max.
keel-codegen's `run` target took 18 seconds of that, and keel-c's `run` target 10.

### Experimental codegen features

`experimental-regalloc` (the Local and Split allocator tiers and
rematerialization) and `experimental-dispatch` (AArch64 dispatch tails) are off
in default builds. `--features` passes a Cargo feature list to every test
command. Run this lane when changing allocation, selection or AArch64 emission:

```sh
cargo clippy -p keel-codegen --all-targets \
  --features experimental-regalloc,experimental-dispatch -- -D warnings
python3 tools/test.py codegen --unit --features experimental-regalloc,experimental-dispatch
python3 tools/test.py codegen --features experimental-regalloc,experimental-dispatch \
  --test arm_compare_select --test arm_dispatch_tail --test commutative_two_address \
  --test narrow_extensions --test null_rematerialization --test private_leaf_calls \
  --test run --test stack_frame --test x86_scalar_selection
```

These are the targets with feature-gated tests. A tier that cannot keep this
lane passing is deleted rather than left uncompiled.

## Differential optimizer tests

The keel-opt `golden` target runs the reference interpreter in
`keel-ir-interp` before and after optimization. It covers every registered
pass on its own and every tier from Fast to Full. A run that returned must
return the same values and leave the same bytes in its pointer arguments' buffers.
A run that trapped must still trap. Runs that reach the step or call-depth
limit, or use an operation the interpreter does not model, are skipped. The
module comments in `crates/keel-opt/tests/golden/` give the input rules.

- `differential.rs` runs the exported functions of every parsable fixture
  under `tests/golden/` and `tests/fixtures/` on fixed inputs.
- `emitted_c.rs` lowers six C test programs with `keel cc --emit-ir` and runs
  their exported functions before and after every tier. It needs the
  `KEEL_TEST_KEEL` binary. With `KEEL_FUZZ_ITERS` set, it lowers every program
  under `crates/keel-c/tests/programs/` and `crates/keel-codegen/tests/programs/`.
- `random_ir.rs` generates seeded functions over integer arithmetic,
  conversions, selects, diamonds, switches and counted loops. The normal run
  checks 24 seeds. Set `KEEL_FUZZ_ITERS` for a qualification lane and
  `KEEL_FUZZ_SEED` for the first seed. A failure names its seed, so
  `KEEL_FUZZ_SEED=<seed> KEEL_FUZZ_ITERS=1` reruns it alone.

```sh
python3 tools/test.py optimizer --test golden --quick
KEEL_FUZZ_ITERS=20000 cargo test --profile test-fast -p keel-opt --test golden random_ir
```

## Test environment

The runner sets these variables for its commands. It keeps a value already set
in the environment, except for `KEEL_SKIP_LOG`.

| Variable | Value |
| --- | --- |
| `KEEL_TEST_LIBLLVM` | `$(rustc --print sysroot)/lib/libLLVM.dylib` on Apple silicon, when keel-llvm is selected |
| `KEEL_TEST_KEEL` | a `keel` binary built with the selected profile, when keel-opt or keel-llvm is selected without `--unit` |
| `KEEL_SKIP_LOG` | a fresh temporary file for each run |

The keel-llvm import tests and the native keel-opt and keel-llvm checks run
only when these are set. Plain `cargo test` does not set them.

A test that cannot run because a host tool or host is missing calls
`keel_test_support::skip!(reason)`, which returns from the test, or
`skip_part!(reason)`, which continues with the checks that need no tool. Both
append one line to the file named by `KEEL_SKIP_LOG`: `<package>::<test>`, a
tab and the reason. After the results list,
the runner prints the skipped tests grouped by reason, and `--report` records
them under `skips`.

## Native x86-64 programs on another host

Tests that link and run an x86-64 program describe it with
`keel_test_support::native_program!`. On x86-64 Linux the test builds and runs
the program itself. Elsewhere it records the skip
`requires x86-64 Linux; set KEEL_TEST_EXPORT_DIR to export`. When
`KEEL_TEST_EXPORT_DIR` names an absolute directory, every such program is also
written to `<dir>/<package>/<test>/<program>/`. The packet holds the program's
files and a `manifest.json` with the build steps, the run command, the expected
exit status and output, and the required CPU features. Copy the directory to an x86-64 Linux host and
run every packet there:

```sh
KEEL_TEST_EXPORT_DIR="$PWD/target/x86-exports" python3 tools/test.py codegen --quick
# On the Linux host, with clang on PATH:
python3 suite/integration/x86-exports/run.py target/x86-exports
```

`python3 tools/test.py <component> --export-x86 target/x86-exports` sets
`KEEL_TEST_EXPORT_DIR` to that directory for the run and then prints the
packet count and the commands that copy and run the packets.

The runner prints the failures and a `passed, failed, skipped` count. It skips
a packet when the host lacks one of its CPU features or a build tool. Add
`--filter <text>` to run a subset and `--keep <dir>` to keep the build
directories. The manifest format is documented in
`crates/keel-test-support/src/native.rs`.

Packets of kind `cpl0` hold privileged payloads for the QEMU campaigns in
`suite/integration/x86-cpl0`; see its README. The runner runs them on any host
with QEMU and GNU binutils. On a host other than x86-64 Linux it skips the
native packets and still runs the `cpl0` ones.

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
cargo test --profile test-fast -p keel-codegen --test x86_system x86_string_port_io::
cargo test --profile test-fast -p keel-codegen --test x86_system x86_string_port_io::source_matrix_boundary_smoke
```

Tool availability and host architecture determine which native checks run.
Shared export directories belong to one run; do not point separate Cargo or
nextest invocations at the same exports.

## Timing and runner checks

```sh
python3 tools/test.py --report target/test-reports/full.json
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

The JSON report records selected packages and checks, quick/full coverage,
commands, elapsed wall time, exit codes and skipped tests. It does not infer test counts. Build time is included;
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
| IR interpreter | `crates/keel-ir-interp/tests/interp/` | textual IR, one module per instruction family |
| Optimizer passes | `crates/keel-opt/tests/golden/` | first line `; passes: a, b` |
| Codegen programs | `crates/keel-codegen/tests/programs/` | `; expect: exit N` or `; expect: trap`; optional same-named `.c` driver; built in 8 allocator/selection configurations |
| Codegen assembly snapshots | `crates/keel-codegen/tests/programs/<stem>.<target>[.opt].s` | baseline and optimized assembly for `aarch64` and `x86_64`, compile only; `BLESS=1 cargo test -p keel-codegen --test asm_snapshots` rewrites them |
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
target/release/keel asm-metrics --target aarch64-apple-darwin input.s -o exact.json
# Exact x86 layout, with instruction/data/padding boundaries and no CPU tools:
target/release/keel asm-metrics --target x86_64-unknown-linux-gnu input.s -o x86.json
python3 tools/instruction_metrics.py -o x86-analysis.json analyze input.s \
  --target x86_64-unknown-linux-gnu
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

The [x86 report contract](../targets/x86_64/assembler.md#encoding-reports)
defines final section offsets, instruction bytes, prefixes, branch distances,
relocations and padding. Its `analyze` route does not use LLVM or load the A64
inventory. Size comparisons require matching assembler, target and features.
The x86 route has no CPU timing model.

CPU analysis rejects regions with extra text bytes and needs one MCA region
with the same instruction count. It declines a speed score for warnings,
unmodeled side effects or missing throughput. The raw resource table includes
MCA's aggregate row, whose index equals the instruction count; do not count it
as an instruction.

## Profiling a slowdown

To decide whether a compiler change is faster, follow the
[performance guide](performance.md). This section explains where time goes.

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
