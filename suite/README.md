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
```

Global options: `-j N` sets parallel compile jobs (default: one per CPU, or two
for `test-frontends`), and `--color auto|always|never` controls coloring.
`test` uses a debug build of Keel, with its assertions, unless `--release` is
given. `run` accepts `-n` (timed runs, default 5), `-w` (warmup runs, default 1)
and `--no-save`.

## How it works

Sources are downloaded, checked against their SHA-256, and unpacked into
`target/suite/<name>/`. Each configuration builds in `build/<config>/` there.

`clang -E` runs once per suite: every configuration compiles the same
preprocessed files, so the only difference between builds is the compiler.
Clang builds are cached; Keel builds are redone each time.

`cargo suite test` builds the selected configurations and runs each suite's
test commands. `cargo suite run` builds every configuration, checks that each
benchmark prints the same output as the baseline build, then times it.

## Configurations

`suite/configs.toml` lists the ways a program is built, in the order result
tables show them. Each entry has a `name`, a `compiler` (`clang`: `clang -c` on
the preprocessed file; `keel`: `keel cc`, then Clang assembles), and its
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
| `keel` | Default unoptimized Keel with baseline instruction selection |
| `keel-O` | The CLI `-O` preset; enables all selection passes |
| `keel-select`, `keel-O-select` | Explicit `--isel all` |
| `keel-g` | Keel with debug info |
| `keel-basic`, `keel-fast`, `keel-full` | Explicit IR tiers (`keel-fast`: Fast IR with Full selection and allocation; `keel-full`: Full for all three) |
| `keel-O-trees`, `keel-O-folds`, `keel-O-hints` | One selection pass at a time, for ablations |
| `keel-O-memory`, `keel-O-ranges`, `keel-O-cleanup`, `keel-O-bits-*`, `keel-O-sccp`, ... | Opt-in pass recipes for experiments; not defaults |
| `keel-m5`, `keel-m5-fast` | Explicit Apple M5 tuning (experimental) |
| `keel-basic-fast-alloc` | Historical Basic IR with Fast allocation, kept so allocation experiments do not stand in for the Fast IR profile |
| `clang-O0` ... `clang-O3` | Clang references |

Selection flags interact as follows. Ordinary `keel` keeps baseline selection;
`-O` enables all selection passes. To compare optimized IR with baseline
selection, use `-O --isel none` (either order). Explicit selection selectors
override the preset, and the last explicit selector wins. `--opt-tier` and
`-p` control IR independently and do not reset selection.

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

`suite/c/benchmark_platforms.py` discovers native-C manifests too; a new workload is
only measured there when selected explicitly with `--apps`.

### `suite.toml` format

```toml
description = "Lua 5.4.7"

# Archives to download and unpack into the work directory.
[[fetch]]
url = "https://www.lua.org/ftp/lua-5.4.7.tar.gz"
sha256 = "9fbf5e28..."

[build]
dir = "lua-5.4.7/src"       # where `prepare` and the preprocessor run
std = "gnu99"               # clang -E -std=
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

## Results

`cargo suite run` prints a table per suite with the mean time per benchmark and
configuration and its ratio to the baseline, and saves the run as JSON in
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
- the LLVM importer's native/rejection, indirect-call/ABI, scalar FP,
  global-object, integer-flag, EH and ODR-linkage validation;
- all 14 required C++ profiles with an independent Rust C ABI boundary, at
  None/Fast/Full, including virtual dispatch and user-defined RTTI;
- a separate `cpp-odr` stage for runtime constexpr helper localization.

It downloads no applications and runs no performance measurements.

The output directory must be new. It receives `report.json`, per-stage command
logs, and each child runner's artifacts under `llvm-import/`, `llvm-calls/`,
`llvm-fp/`, `llvm-globals/`, `llvm-linkage/`, `llvm-flags/`, `llvm-eh/`,
`llvm-odr/`, `cpp-coverage/` and `cpp-odr/`. Any failed build, test, native
comparison or profile fails the command. Existing artifacts are never
overwritten.

Options:

| Option | Effect |
|--------|--------|
| `-j N` (before the subcommand) | Cargo jobs for the private `target/` (default 2) |
| `--keel PATH` | Use an existing Keel binary for native checks; Rust regression tests still build from source |
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
invoking the individual stage scripts. Crate regressions own the detailed
contracts; see [LLVM semantics](../docs/frontends/llvm.md).

## Components and commands

| Directory | Responsibility | Entry point |
| --- | --- | --- |
| `c/` | Native C application manifests, drivers and paired benchmarks | `cargo suite test lua`, `cargo suite run lua/fib` |
| `ir/` | LLVM import fixtures, ABI checks and C++/Rust importer comparisons | `cargo suite test-frontends --out target/frontend-check-1` |
| `asm/` | Compiler-generated assembly and independent object inspection | `python3 suite/asm/compiler_asm_compat/run.py --help` |
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

These check runner protocols and fixtures. They do not qualify native compiler,
linker or kernel behavior. Native LLVM cases that need explicit library/tool
arguments run through `test-frontends`, not ordinary test discovery. Use
[the Rust test runner](../docs/development/testing.md) for crate regressions.

## Measurements

Use `python3 suite/c/benchmark_platforms.py --help` for paired application
compile/runtime comparisons. `integration/compiler-bench/` and `linker/` own
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
directories. Git history retains retired experiments.
