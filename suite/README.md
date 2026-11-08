# The suite

The suite builds real C programs with Keel and Clang, checks that every build
passes the program's own tests, and times benchmark workloads. It is driven by
the `keel-suite` tool (`crates/keel-suite`), available through the `cargo suite`
alias.

Each directory under `suite/c/` that contains a `suite.toml` is one workload.
The native-C registry currently contains:

| Workload | What runs |
|----------|-----------|
| `lua` | Lua 5.4.7 interpreter, its portable test suite and benchmarks |
| `quickjs` | QuickJS interpreter, its tests and benchmarks |
| `sqlite` | SQLite shell, query checks and benchmarks |
| `yyjson` | JSON parsing, traversal and round trips |
| `lz4` | LZ4 block compression and decompression |
| `zstd` | Zstd portable C library, compression and decompression |

These use unmodified, pinned upstream library code with deterministic drivers.
They are not exhaustive upstream test suites or full application builds.

The other directories hold opt-in harnesses for narrower questions (see
[component groups](#components-and-commands)). Generated reports and raw
evidence belong in the ignored `suite/reports/` directory or under `target/`;
they are not stored in Git.

For small timing differences on Linux, see the
[controlled userspace timing procedure](../docs/benchmarks/linux-timing.md).
It covers resident inputs, executable backing, identical-image controls and
balanced run order; ordinary suite runs do not automatically apply these controls.

## Quick start

```sh
cargo suite list                    # suites, benchmarks, configurations
cargo suite test                    # test keel and keel -O, with selection off and on
cargo suite test sqlite -c keel-g   # one suite, another configuration
cargo suite test lua --release      # use a release build of keel
cargo suite run                     # build every configuration, time everything
cargo suite run lua/fib -n 10 -c clang-O0,keel
cargo suite show                    # the latest run on this host again
cargo suite compare aarch64-apple-darwin x86_64-unknown-linux-gnu
cargo suite test-frontends --out target/frontend-check-1
cargo suite codegen-diff --baseline old/keel --candidate new/keel
cargo suite stats --pass gvn          # where the optimizer's passes change code
```

Global options: `-j N` sets parallel compile jobs (default: one per CPU, or two
for `test-frontends`), and `--color auto|always|never` controls coloring.
`test` uses a debug build of Keel, with its assertions, unless `--release` is
given. `run` accepts `-n` (timed runs, default 5), `-w` (warmup runs, default 1)
and `--no-save`.

## How it works

Sources are downloaded, checked against their SHA-256, and unpacked into
`target/suite/<name>/`. Each configuration builds in `build/<config>/` there.

By default each compiler preprocesses the original C sources itself. Every
source remains a separate translation unit and produces its own object before
linking. Keel currently has one C language mode; the manifest's `std` selects
Clang's mode. Both compilers receive the same manifest defines and include paths.

Use `--input-mode shared` for a diagnostic comparison in which every compiler
consumes the same Clang-preprocessed `.i` files. This excludes preprocessing from
the build timings. Shared inputs are regenerated and all configurations are
rebuilt on every invocation; saved build times never represent cache hits.

Downloads and generated-source preparation happen before the build timer.
`wall_ms` includes compilation at the recorded `jobs` setting and linking;
`compile_ms` sums per-TU compiler/assembler durations, and `link_ms` measures
linking separately. These are single build observations, not regression samples.
Use `-j 1` for serial build measurements and a fixed `-j N` for project throughput.
The platform runner provides repeated serial source-to-assembly/object timings.

`cargo suite test` builds the selected configurations and runs each suite's
test commands. `cargo suite run` builds every configuration, checks that each
benchmark prints the same output as the baseline build, then times it.

## Configurations

`suite/configs.toml` lists the ways a program is built, in the order result
tables show them. Each entry has a `name`, a `compiler` (`clang`: `clang -c`; `keel`: `keel cc`, then Clang assembles unless its flags request native object emission), and its
`flags`. The top-level `baseline` is what benchmark ratios are computed against
(`clang-O0`), and `test` is what `cargo suite test` builds unless given `-c`.

All registered Keel configurations declare `--link-intent executable`, because
these workloads link executable programs. This enables strong-definition body
inlining in Full without source annotations. The platform benchmark runner uses
the same intent for its built-in Keel configurations; custom candidate flags
remain explicit. Results from before this policy change used different binding
assumptions, so retain the configuration flags when comparing runs.

Commonly used configurations:

| Configuration | Meaning |
|---------------|---------|
| `keel` | Default unoptimized Keel with covers selection, the `-O0` default |
| `keel-O` | The CLI `-O` preset; enables all selection passes |
| `keel-O-lto` | `keel-O` for the whole program: each unit is emitted as unoptimized IR, and `keel compile --internalize main` merges and compiles them as one module ([whole-program compilation](../docs/toolchain.md#whole-program-compilation)) |
| `keel-select`, `keel-O-select` | Explicit `--isel all` |
| `keel-g` | Keel with debug info |
| `keel-basic`, `keel-fast`, `keel-full` | Explicit IR tiers (`keel-fast`: Fast IR with Full selection and allocation; `keel-full`: Full for all three) |
| `keel-O-trees`, `keel-O-folds`, `keel-O-hints`, `keel-O-covers` | One selection pass at a time, for ablations |
| `keel-O-memory`, `keel-O-cleanup`, `keel-O-bits-*`, ... | Opt-in pass recipes for experiments; not defaults |
| `keel-m5`, `keel-m5-fast` | Explicit Apple M5 tuning (experimental) |
| `keel-basic-fast-alloc` | Historical Basic IR with Fast allocation, kept so allocation experiments do not stand in for the Fast IR profile |
| `clang-O0` ... `clang-O3` | Clang references |

Selection flags interact as follows. Ordinary `keel` uses covers selection,
which enables every [cover](../docs/compiler/design.md#covers) and no other
optional pass;
`-O` enables all selection passes. To compare optimized IR with Baseline
selection, which runs no optional pass and no cover, use `-O --isel none`
(either order). Explicit selection selectors override the preset, and the last
explicit selector wins. `--opt-tier` and `-p` control IR independently and do
not reset selection. A config with
`-O --isel all,-<cover>` drops one cover, so one binary can compare a cover
against `keel-O`.

Full IR runs three bounded rounds of inlining, scalar/memory cleanup, code
motion and loop optimization. Private aggregate transport and its conditional
cleanup run before the first round and again near the end. This is a fixed
schedule, not an unbounded fixed-point loop. Fast retains its own bounded pipeline.

## Adding a workload

1. Create `suite/c/<name>/suite.toml` (format below). Put any driver programs,
   inputs or benchmark scripts in the same directory and refer to them as
   `{suite}/...`.
2. Run `cargo suite list` to check that the manifest parses.
3. Run `cargo suite test <name> -c clang-O2` to check the reference build and
   test commands, then `cargo suite test <name>` for the Keel configurations.
4. Run `cargo suite run <name>` to check that benchmark outputs agree and to
   take a first timing.

`suite/c/benchmark_platforms.py` reads the manifests through `cargo suite manifest`;
a new workload is only built there when selected explicitly with `--apps`.

### `suite.toml` format

```toml
description = "Lua 5.4.7"

# Archives to download and unpack into the work directory.
[[fetch]]
url = "https://www.lua.org/ftp/lua-5.4.7.tar.gz"
sha256 = "9fbf5e28..."

[build]
dir = "lua-5.4.7/src"       # where `prepare` and the preprocessor run
std = "gnu99"               # Clang language mode
sources = ["*.c"]           # files, or `*` in a file name
exclude = ["luac.c"]
defines = []                # -D, without the -D
includes = []               # -I, relative to `dir`
libs = ["m"]                # -l, without the -l
prepare = []                # commands run first, e.g. to generate sources
binary = "lua"

# Added on matching hosts: `os` is macos or linux, `arch` aarch64 or x86_64.
[[build.when]]
os = "linux"
defines = ["LUA_USE_LINUX"]
libs = ["dl"]

# Each command must succeed. With `compare`, it must also print exactly
# what the named configuration's build prints.
[test]
dir = "lua-5.4.7-tests"     # relative to the work directory
run = ["{bin} -e _U=true all.lua"]
stdin = "{suite}/input.txt" # optional
compare = "clang-O2"        # optional

# A timed workload. Before timing, every configuration's output must
# equal the baseline's.
[[bench]]
name = "fib"
run = "{bin} {suite}/bench.lua fib"
stdin = "{suite}/input.txt" # optional
```

Commands are split at whitespace and run without a shell. Available variables:

| Variable    | Value                                                  |
|-------------|--------------------------------------------------------|
| `{bin}`     | the executable built in the configuration under test   |
| `{suite}`   | this directory, `suite/c/<name>`                         |
| `{work}`    | the work directory, `target/suite/<name>`              |
| `{gen}`     | `{work}/gen`, for what `prepare` generates             |
| `{defines}` | the `-D` flags, one argument each                      |
| `{libs}`    | the `-l` flags, one argument each                      |

`cargo suite manifest [suites] [--target aarch64|x86_64]` prints the manifests
as JSON for scripts. It applies the `[[build.when]]` entries for the target
(default: this host) to `defines` and `libs`, drops `when`, and leaves the
variables in commands and paths unexpanded.

## Results

`cargo suite run` checks outputs, then interleaves configurations in rotating
rounds with reversed rotations. Exact position/order balance requires complete
cycles. It includes a copied baseline executable as an identical-binary control.
Each timed sample must still match the checked output. Timings include process
startup and output capture; output comparison happens after the timer stops.

Tables show mean times and median paired time ratios, with bootstrap 95%
intervals when at least five rounds are available. JSON retains raw samples,
execution positions, the control and build metadata. Results are saved in
`target/keel-suite/results/<target>/`. `show` and `compare` take a results file
or a target triple (meaning that target's latest run).

Runs on one target are compared by time. Runs on two targets are compared by
each configuration's ratio to its own baseline, since the machines differ. To
compare with another machine, copy its `results/<target>` directory over.

## Frontend correctness checks

```sh
cargo suite test-frontends --out target/frontend-check-1
```

This runs `suite/integration/frontend_validate.py`, which covers:

- the C attribute and preprocessing regression tests and a representative C CLI
  compatibility check;
- every `suite/ir/llvm_import*_validate.py` validator for `keel-llvm-import`:
  scalar/CFG, indirect calls, scalar FP, global objects, ODR local copies,
  integer flags, EH, two-word aggregates, booleans, fastcc, memory intrinsics
  and exported ODR/RTTI, plus the ODR translation-policy tests;
- all 14 required C++ profiles with an independent Rust C ABI boundary, at
  None/Fast/Full, including virtual dispatch and user-defined RTTI;
- a separate `cpp-odr` stage for runtime constexpr helper localization.

It downloads no applications and runs no performance measurements.

The output directory must be new. It receives `report.json`, per-stage command
logs, and each child runner's artifacts in a directory named after its stage
(`llvm-import/`, `llvm-calls/` and so on, `cpp-coverage/` and `cpp-odr/`). Any
failed build, test, native comparison or profile fails the command. Existing
artifacts are never overwritten.

Options:

| Option | Effect |
|--------|--------|
| `-j N` (before the subcommand) | Cargo jobs for the private `target/` (default 2) |
| `--keel PATH` | Use an existing Keel binary for native checks; Rust regression tests still build from source |
| `--importer PATH` | Use an existing `keel-llvm-import` binary instead of building one |
| `--libllvm PATH` | LLVM 23 shared library; default is discovered from `rustc --print sysroot` |
| `--require-llvm` | Fail instead of skipping the LLVM stages on unsupported hosts |

Python 3, Cargo and Clang are required. LLVM import currently supports only
Apple AArch64, which additionally needs Clang++, rustc and an LLVM 23 shared
library; missing prerequisites there are failures. Other hosts run the C checks
and report the LLVM stages as skipped (`passed_with_skips`) with reasons.

See [C compatibility](../docs/frontends/c/README.md),
[LLVM import](../docs/frontends/llvm.md) and C++ compatibility profiles
for semantic boundaries and standalone runner commands. This command does not
replace the full workspace tests or application suites.

## Frontend prerequisites

The LLVM/C++ path imports Clang/rustc-produced LLVM IR; it is not a C++ source
frontend. Its native route requires Apple AArch64 and an LLVM 23 shared library
with the required C API. Other hosts skip unsupported LLVM stages unless
`--require-llvm` is set. Use `--libllvm PATH` to select the library explicitly.
Run `cargo suite test-frontends --out target/frontend-check-1` rather than
invoking the individual stage scripts. To debug one validator, run it with
`--libllvm PATH --keel PATH --out NEW_DIR`; it uses the `keel-llvm-import` next
to `--keel` unless `--importer` names one. The validators share
`suite/ir/harness.py`, which runs and logs commands and checks that a rejected
import writes nothing. Crate regressions own the detailed contracts; see
[LLVM semantics](../docs/frontends/llvm.md).

## Components and commands

| Directory | Responsibility | Entry point |
| --- | --- | --- |
| `c/` | Native C application manifests, drivers and paired benchmarks | `cargo suite test lua`, `cargo suite run lua/fib` |
| `ir/` | LLVM import fixtures and native validators for `keel-llvm-import` | `cargo suite test-frontends --out target/frontend-check-1` |
| `asm/` | Compiler-generated assembly, independent object inspection and [x86 sequence timing](asm/sequence_bench/README.md) | `python3 suite/asm/compiler_asm_compat/run.py --help` |
| `linker/` | Assembly/link benchmark and independent runtime oracle | `python3 suite/linker/run.py --help` |
| `integration/` | Kernel build/boot, privileged guests, frontend orchestration and compiler benchmarks | Each runner's `--help` |
| `common/` | Shared artifact storage and execution locking | Used by the component runners |

Run only the runner tests for a component, without application builds, downloads
or timings:

```sh
cargo suite check                         # All runner tests
cargo suite check c ir                    # C and IR runner tests
cargo suite check asm linker integration  # Other component runner tests
```

Each component runs `unittest` in every directory under `suite/<component>`
that holds a Git-tracked `test_*.py` file, so a new test directory needs no
runner change. The command needs a Git checkout.

These check runner protocols and fixtures. They do not qualify native compiler,
linker or kernel behavior. Native LLVM cases that need explicit library/tool
arguments run through `test-frontends`, not ordinary test discovery. Use
[the Rust test runner](../docs/development/testing.md) for crate regressions.

## Measurements

The [performance guide](../docs/development/performance.md) says which
measurement decides a compiler change and what result accepts it.
Use [`cargo suite ab`](#timing-two-compilers) to compare the runtime of two
Keel compilers. Use `python3 suite/c/benchmark_platforms.py --help` to check and
time the compilation of a matrix of Clang, GCC and Keel configurations.
[`asm/sequence_bench`](asm/sequence_bench/README.md) times x86 instruction
sequences to screen candidates; it does not accept changes. `integration/compiler-bench/` and `linker/` own
public README measurements. Workload names such as `lua` remain unchanged;
prepared sources and build caches remain under `target/suite/<name>/`.

Freeze binaries before timing, check outputs before measuring, alternate
baseline/candidate order and retain flags and input hashes. Write results in
new ignored `target/` or `suite/reports/` directories. See the
[controlled timing procedure](../docs/benchmarks/linux-timing.md) for the
page-backing harness and the limits of small Linux timing differences.

## Integration harnesses

- [Linux kernel](integration/linux-kernel/README.md): pinned 64-bit build, link and boot.
- [CPL0 guest](integration/x86-cpl0/README.md): privileged operations and owned-state contracts.
- [Page backing](integration/page-backing/README.md): controlled executable-backing comparisons.
- `integration/frontend_validate.py`: C/LLVM/C++ orchestration behind `test-frontends`.
- `integration/compiler-bench/`: whole compiler and IR backend comparisons.

Synthetic compiler regressions belong beside their crate tests. Add a standalone
harness only for a reusable boundary this infrastructure cannot exercise.
Temporary probes, progress tracking and report renderers belong in ignored
directories; the README chart renderer and its evidence files are the
exception. Git history retains retired experiments.

## Comparing generated code

```sh
cargo build --release -p keel && cp target/release/keel target/keel-base
# Change the compiler, then:
cargo build --release -p keel
cargo suite codegen-diff --baseline target/keel-base --candidate target/release/keel \
    --x86-64-sysroot /path/to/linux-root --expect-identical
```

`codegen-diff` compiles every translation unit of the selected suites (default:
all) with both Keel binaries, to assembly, for `aarch64-apple-darwin` and
`x86_64-unknown-linux-gnu`, in the `keel` and `keel-O` configurations. It uses
the same downloads, `prepare` commands, defines and include paths as `test` and
`run`, with each target's `[[build.when]]` entries. The assembly goes to
`target/keel-suite/codegen-diff/{baseline,candidate}/<target>/<config>/<suite>/`.

For each object it reports identical (byte for byte) or changed. For a changed
object it lists each function that changed, was added or was removed, with the
candidate's instruction and byte deltas from `keel asm-metrics`. A function
whose only differences are comment lines (the IR and allocation notes) is
listed as `comments`. Differences outside every function, such as data, appear
as `(outside functions)`. The candidate binary runs `asm-metrics` for both
sides, so the deltas measure the assembly text alone.

| Option | Effect |
|--------|--------|
| `--target aarch64,x86_64` | Targets to compile for |
| `-c keel,keel-O` | Keel configurations from `suite/configs.toml` |
| `--x86-64-sysroot DIR` | Linux headers (`DIR/usr/include`), required unless this host is x86-64 Linux |
| `--aarch64-sysroot DIR` | macOS SDK, required unless this host is an Apple silicon Mac |
| `--icount` | Record each compile's retired instructions: `/usr/bin/time -l` on macOS, `perf stat -e instructions:u` on Linux |
| `--expect-identical` | Exit non-zero if any object changed |
| `--json FILE` | Write the full report, including every changed function |
| `--show N` | Changed functions to print per object (default 10) |
| `--candidate-isel LIST` | Also pass `--isel LIST` to the candidate |
| `--covers` | Count each cover's firings with `--codegen-report` and print them per target and configuration |

With `--candidate-isel`, one binary can be both sides, which compares one
[cover](../docs/compiler/design.md#covers) against the rest:

```sh
cargo suite codegen-diff --baseline target/release/keel --candidate target/release/keel \
    -c keel-O --candidate-isel all,-x86-mask-test --covers
```

Use `--expect-identical` as the gate for refactors that must not change
generated code. Objects that both binaries fail to compile are reported and not
compared. Retired-instruction totals are per configuration. They come from one
run, so repeat the run before reading a difference under about one percent.

## Pass and backend statistics

```sh
cargo suite stats                                   # every suite, keel-O, this host's target
cargo suite stats lua --pass loop-vectorize --show 50
cargo suite stats -c keel-O --target x86_64 --x86-64-sysroot /path/to/linux-root --json stats.json
```

`stats` compiles every translation unit of the selected suites once, with
`keel cc --opt-report --codegen-report`, and totals the reports. It uses the same inputs and
flags as `codegen-diff`, for one target and one configuration. Without
`--keel`, it builds a release `keel` from this checkout. Outputs and reports go
to `target/keel-suite/stats/<suite>/`.

The pass table lists passes in pipeline order. A pass that runs several times
in the pipeline has one row.

| Column | Meaning |
|--------|---------|
| `runs` | Invocations, one per function or per module |
| `changed` | Invocations that reported a change |
| `functions` | Distinct functions that a function-scope invocation changed |
| `modules` | Module-scope invocations that reported a change |
| `insts`, `blocks`, `params` | IR instructions, blocks and block parameters after minus before, summed over changed invocations |
| `time` | Wall-clock time in the pass, summed over invocations |

The inline table counts call sites by outcome: `inlined`, or the first check
that failed, for ordinary calls and for `alwaysinline` callees. `--pass NAME`
also lists the functions that pass changed, largest instruction change first.
The pass and inline counts describe optimizer IR.

The backend table sums each `--codegen-report` counter over every function,
with the number of functions where it is nonzero and the total per suite.
The counters are those the keel binary reports, such as spill stores, reloads
and register moves; the [toolchain reference](../docs/toolchain.md) defines
them. They count emitted instructions, not executions.

`--json FILE` writes all three tables, with each pass's and each counter's
totals per suite.

## Timing two compilers

```sh
cargo suite ab --baseline main --candidate HEAD --placements 4 --cpu 8 --controller-cpu 0
cargo suite ab --baseline target/keel-base --candidate target/release/keel --apps lua,yyjson/api
```

`ab` builds the chosen suites (default: every benchmark) with two Keel
compilers in one configuration (default `keel-O`) and times them against each
other. Each side is a Keel binary or a Git revision. A revision is unpacked
with `git archive` and built with `cargo build --release -p keel` in
`target/keel-suite/ab/keel-<commit>/`, which later runs reuse.

Before any timing, both builds run the suite's test commands. With `compare`,
the candidate must print what the baseline prints. Each benchmark must also
print the same with both builds. `--no-tests` skips the test commands but not
the benchmark check.

Placement 0 is the normal build. Every further placement reassembles both
builds' assembly with never-executed fill in front of each function's
alignment: 0 to 48 bytes in 16-byte steps on x86-64, 0 to 60 bytes in 4-byte
steps on AArch64. The fill depends only on the function's name and the
placement, so both builds pad a function the same way. Each placement changes
every function's offset in its cache line and its distance to the other
functions, so a slowdown that comes from where the linker happened to put an
unchanged hot function does not repeat at every placement. Placements catch
layout effects that an identical-image control cannot. They need a
configuration that emits assembly, not `--emit-object`.
`suite/c/ab_timing.py` then runs each benchmark in blocks. A block runs the
baseline, the candidate and the control (a copy of the baseline image) twice
each, in a shuffled order followed by its mirror, so linear drift cancels.
Placements are shuffled within each round.

On Linux, each placement is one executable path and inode. The timing loop
overwrites that file in place between runs and keeps a read-only mapping of
its file-backed `PT_LOAD` pages locked, so every image in a placement runs
from the same page-cache pages. Children run with ASLR disabled and, with
`--cpu`, pinned to one CPU. `perf stat` counts `cycles:u` and
`instructions:u`, plus any `--counters` events. Locking needs enough
`ulimit -l`; `--no-lock` prefaults the pages without locking them. On macOS,
each image runs from its own copy with an equal-length path, and
`/usr/bin/time -l` counts cycles and instructions of the whole process.

A placement's ratio is the median over rounds of the candidate's block sum over
the baseline's. A workload's change is the geometric mean over placements. The
verdict applies the acceptance rule to cycles:

- **reject** if a workload is more than 1.5% slower at every placement, and
  its smallest slowdown exceeds its control's largest distance from no change;
- **neutral** if the geomean over workloads is inside the identical-control
  spread, the control's largest distance from no change overall or at any one
  placement;
- **accept** if the geomean is -0.5% or better;
- **reject** otherwise.

The table lists each workload's cycles change, its range over placements, the
control, the instruction and wall-time changes, and the baseline's own range
over placements. The run ends with a commit trailer block for the commit
message. The plan, raw runs and report go to `target/keel-suite/ab/runs/<time>/`.

| Option | Effect |
|--------|--------|
| `--apps lua,yyjson/api` | Suites or benchmarks to time |
| `-c keel-O` | Keel configuration from `suite/configs.toml` |
| `--placements N` | Code placements (default 4) |
| `-n N`, `-w N` | Measured and warmup rounds per placement (default 5 and 1) |
| `--counters EVENTS` | More perf events on Linux, comma-separated |
| `--cpu N`, `--controller-cpu N` | Benchmark and controller CPUs on Linux |
| `--no-lock` | Do not lock executable pages on Linux |
| `--seed N` | Seed of the run order |
| `--candidate-isel LIST` | Also pass `--isel LIST` to the candidate, so one binary can time one cover; the trailer records it |
| `--candidate-regalloc TIER` | Override the candidate allocator; Local/Split need a compiler built with `experimental-regalloc` |
| `--candidate-pressure-schedule` | Enable the candidate’s bounded integer pressure scheduler |
| `--candidate-lto` | Also build the candidate as one program, as `keel-O-lto` does, so one binary can time whole-program compilation; the trailer records it |

## Checking runtime slowdowns

```sh
cargo suite run lua/fib -c clang-O0,keel-O -n 20
# Save the printed result path, change the compiler revision, and repeat.
cargo suite regressions /path/to/baseline.json /path/to/candidate.json --threshold 0.05
```

The regression command compares the same configuration across Keel revisions.
It requires compatible host/target, inputs, flags, jobs and timing protocol,
at least ten valid samples in each run, and quiet identical-binary controls.
It independently bootstraps the two runs; rounds collected in different runs
are not pairs. A slowdown is reported only when the interval's lower bound
exceeds the practical threshold. The command fails for established slowdowns;
inconclusive comparisons are printed and do not fail. Repeat a suspected
slowdown on an otherwise idle host before treating it as confirmed.

The default five percent threshold is a starting policy, not a measured noise
floor. Calibrate it using unchanged compiler runs on the measurement host.
Separate runs cannot exclude host drift. Keep the CPU, OS, SDK/system headers,
compiler build profile and dependencies fixed; metadata is not a complete hash
of the host environment. Build times remain descriptive single observations.

## Zstd workload

Zstd builds its pinned upstream common, compression, decompression and dictionary
C files separately, plus a deterministic public-API driver. Both compilers use
`ZSTD_DISABLE_ASM=1` and `ZSTD_LEGACY_SUPPORT=0`: this measures the portable C
implementation without external assembly or old-format decoders. It does not
measure an amalgamation or the upstream CLI. The driver checks round trips and
runs compression and decompression separately on deterministic resident inputs.

### Fast-math kernel baselines

`suite/c/fastmath/run.py` compares division, the x component of 3D
normalization (structure-of-arrays), and dot product in single precision:

```sh
cargo build --release -p keel
python3 suite/c/fastmath/run.py --keel target/release/keel \
  --out target/fastmath-baseline
```

The six rows are scalar C compiled by Clang/LLVM at `-O3` with strict math
or `-ffast-math`, handwritten four-lane SIMD C with strict math or fast math,
and the same scalar C through Keel with and without `-O`. Keel has no
fast-math mode yet. The handwritten paths use SSE on x86-64 and NEON on
AArch64. They establish a portable ISA baseline, not the ceiling for AVX,
AVX-512 or wider/unrolled NEON implementations. There is no handwritten
assembly row yet. Kernel assembly and objects remain in the output directory
for inspection. The approximate SIMD path uses one Newton step after x86
reciprocal/reciprocal-square-root estimates, or two steps after NEON estimates.
Its dot product uses two vector accumulators; the strict path retains scalar
addition order. Scalar tails use division and `sqrtf`.

A common driver is compiled separately with strict math and contraction off.
No LTO or fast-math link flag is used, so the link does not enable process-wide
flush-to-zero handling. Inputs are deterministic binary fractions with mixed
signs and magnitudes from 0.25 through just under 4.25. Denominators are nonzero
and normal. This is a finite, bounded-domain baseline: it makes no accuracy
claim for NaNs, infinities, zeros, subnormals or extreme exponents.

Before timing any row, every build checks sizes 1, 3, 4, 7, 8 and 17 plus the
requested sizes against a long-double oracle. Division and normalization must
have relative error at most `2e-6`. Dot error divided by the sum of absolute
products must be at most `5e-5`; this also permits scalar accumulation error.
On hosts where long double equals double, the oracle still has more precision
than the float kernels. Each timed invocation checks again before and after
its loop, outside the measured region. These tolerances are benchmark gates,
not a proposed compiler fast-math contract.

Defaults are 4099 and 262147 elements, 1000 iterations and five rounds with
seeded shuffled configuration order. Use `--sizes`, `--iterations` and
`--rounds` to change them. Timing covers repeated out-of-line kernel calls;
allocation, input preparation and oracle checks are excluded. Small sizes
include call overhead. `report.json` records commands, compiler versions,
source hashes, the Keel binary hash, host, raw samples, numerical error and median/min/max wall time
per element. Reports and binaries belong under ignored `target/`. These are
screening measurements, not the application-level performance acceptance gate.

This focused runner uses numerical error checks because the ordinary suite
runner requires byte-identical output across configurations. It is not part of
the default application benchmark set.

Run the numerical-gate regressions with
`python3 -m unittest discover -s suite/c/fastmath -v`. They check a valid
scalar implementation and reject NaN output and an incorrect reduction.
