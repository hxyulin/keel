# The suite

The suite builds real C programs with Keel and Clang, checks that every build
passes the program's own tests, and times benchmark workloads. It is driven by
the `keel-suite` tool (`crates/keel-suite`), available through the `cargo suite`
alias.

Each directory under `suite/` that contains a `suite.toml` is one workload.
The native-C registry currently contains:

| Workload | What runs |
|----------|-----------|
| `lua` | Lua 5.4.7 interpreter, its portable test suite and benchmarks |
| `quickjs` | QuickJS interpreter, its tests and benchmarks |
| `sqlite` | SQLite shell, query checks and benchmarks |
| [`yyjson`](yyjson/README.md) | JSON parsing, traversal and round trips |
| [`lz4`](lz4/README.md) | LZ4 block compression and decompression |

These use unmodified, pinned upstream library code with deterministic drivers.
They are not exhaustive upstream test suites or full application builds.

The other directories hold opt-in harnesses for narrower questions (see
[Companion harnesses](#companion-harnesses)). Generated reports and raw
evidence belong in the ignored `suite/reports/` directory or under `target/`;
they are not stored in Git.

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

1. Create `suite/<name>/suite.toml` (format below). Put any driver programs,
   inputs or benchmark scripts in the same directory and refer to them as
   `{suite}/...`.
2. Run `cargo suite list` to check that the manifest parses.
3. Run `cargo suite test <name> -c clang-O2` to check the reference build and
   test commands, then `cargo suite test <name>` for the Keel configurations.
4. Run `cargo suite run <name>` to check that benchmark outputs agree and to
   take a first timing.

`benchmark_platforms.py` discovers native-C manifests too; a new workload is
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
| `{suite}`   | this directory, `suite/<name>`                         |
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

This runs `suite/frontend_validate.py`, which covers:

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

See [C compatibility](../docs/c-frontend-coverage.md),
[LLVM import](llvm_import_README.md) and [C++ profiles](cpp_coverage_README.md)
for semantic boundaries and standalone runner commands. This command does not
replace the full workspace tests or application suites.

## Measurement scripts

The Python scripts in `suite/` implement controlled measurements beyond
`cargo suite run`. They share a few conventions:

- Build and copy ("freeze") the compiler binaries under test before timing, and
  keep the host free of other builds and heavy jobs while timing runs.
- Every configuration must pass correctness checks and match reference outputs
  before it is timed. Failures are recorded and excluded, never timed.
- Rounds are paired and order-randomized with a fixed seed. Reports keep raw
  samples, commands, compiler/input hashes, paired median ratios and bootstrap
  intervals. Write them into a new directory under `target/` or
  `suite/reports/`.
- Ratios are within one host. Absolute times across machines are not
  comparable, and an interval crossing one does not establish a speedup.

### Native platform comparisons

`benchmark_platforms.py` measures serial preprocessed-C-to-assembly and
preprocessed-C-to-object compilation separately, then native CLI runtime over
the Lua/QuickJS/SQLite workloads. Keel's object boundary uses its native
emitter; Clang/GCC use `-c`. Preprocessing, linking, compiler builds and
dependency setup are outside the timing boundaries. There is no LTO or native
CPU tuning; Linux reference compilers explicitly target baseline x86-64.

```sh
python3 suite/benchmark_platforms.py --root /path/to/source \
  --work /path/to/new/results --keel /path/to/keel \
  --gcc /usr/bin/gcc --clang /usr/bin/clang --cpu 2

# Other workloads and configurations must be selected explicitly.
python3 suite/benchmark_platforms.py --root . --work target/new-apps-run \
  --keel /absolute/path/to/keel --apps yyjson lz4 \
  --configs keel-none keel-fast keel-full clang-O2
```

| Option | Effect |
|--------|--------|
| `--prepare-only` / `--reuse-prepared` | Prepare native inputs once, then reuse them after verifying their hashes. Two Linux hosts can share a prepared corpus; never share preprocessed inputs between Linux and macOS. |
| `--normalize-prepared` | Keep raw `.i` files and rename glibc `_Float32`-family compatibility typedef identifiers for GCC/Clang interoperability. Types and application code are unchanged; verify unchanged Clang assembly before using normalized inputs. |
| `--gcc-native-preprocess` | For GCC builds that cannot consume Clang's preprocessed C11 atomics (QuickJS's `__c11_atomic_*`), e.g. `--apps quickjs --configs clang-O2 gcc-O0 gcc-O2 --gcc-native-preprocess`. Uses GCC's own preprocessor and headers, so those inputs are not byte-identical to the shared `.i` files. Use a separate work directory. |
| `--checks-only` | Preparation, builds and correctness checks without timing |
| `--candidate-configs FILE.json` | Add explicitly named Keel binaries/flag lists |

Defaults are five paired compile rounds after one warmup and seven paired
runtime rounds after two warmups. An identical Clang O2 executable control,
load/CPU-pressure/steal counters and full command logs are kept. Ratios and
intervals are against Clang O2 on each host. Runtime includes process startup
and uses ordinary ASLR/page backing, not a physical-page selection protocol
(see [executable page backing](../docs/executable-page-backing.md)).

Preparation commands expand `{defines}` and `{libs}` as argument lists,
preserving macro quoting; generic test commands support `{suite}`, `{work}`,
`{gen}` and `{bin}`. A failed configuration gives a failing exit code, and an
invalid Clang O2 reference aborts before timing.

C++ library pilots use the separate LLVM-import route; see the
[pugixml pilot](workload-pilots/pugixml/README.md). Passing C++ feature profiles
do not establish complete application compatibility.

### Paired pass measurements

`measure_passes.py` compares explicit optimizer pipelines on Lua, QuickJS and
SQLite, keeping selection and allocation at their defaults. Run the suite
correctness tests first (this prepares the preprocessed files), then copy the
release compiler before changing its source:

```sh
cargo suite test -c keel-O-combine --release
cp target/release/keel /tmp/keel-pass-candidate
python3 suite/measure_passes.py \
  --compiler /tmp/keel-pass-candidate \
  --baseline-passes simplify-cfg,instcombine,simplify-cfg,gvn,dce,gcm \
  --variant no-gcm=simplify-cfg,instcombine,simplify-cfg,gvn,dce \
  --variant sccp=simplify-cfg,sccp,instcombine,simplify-cfg,gvn,dce,gcm \
  --output /tmp/keel-pass-comparison.json
python3 suite/summarize_passes.py --report /tmp/keel-pass-comparison.json
```

- `--baseline-compiler BIN` compares against a different binary; repeat
  `--variant NAME=passes` to sweep alternatives.
- `--baseline-passes tier:full --variant current=tier:full` runs each binary's
  actual Full preset. An empty `--baseline-passes ''` selects
  `--opt-tier none`.
- `--baseline-arg` / `--compiler-arg` (repeatable) pass extra compiler arguments
  to builds and compile-time samples. Use `=` for leading-dash values, e.g.
  `--compiler-arg=--tune --compiler-arg=apple-m5`.
- `--assembler keel` assembles with the Keel driver (default `clang`).
- `--jobs 1`, `--compile-warmups 2`, `--command-log PATH.jsonl.gz` and
  `--runtime-path PATH` support serial builds, retained commands and a single
  executable pathname for timing.

Every build finishes before timing. Runtime samples alternate baseline and
candidate order and shuffle variants each round; every workload's stdout must
match. Compile samples compile the same files serially, excluding assembly and
linking. Code size is the executable's text section. Runtime geometric means
weight each workload equally; compile means weight each application equally.
A ratio below one is an improvement. Defaults are seven runtime pairs after two
warmups and five compile pairs after one warmup. `summarize_passes.py` adds
bootstrap intervals for aggregate ratios by resampling pairs within each
workload.

`measure_ir_counts.py` reports static IR counts (with `--baseline-compiler` for
version comparisons). See the [simplification audit](../docs/simplification.md)
for pass-ordering results and proof tests.

### Apple M5 tuning

The `keel-m5` and `keel-m5-fast` configurations enable explicit Apple M5 tuning
with full instruction selection. `measure_m5_integer.py` measures scalar
profitability alternatives on M5 hardware, with Keel-assembled kernels and
independently checked results, and records nanoseconds rather than estimated
cycle counts. `generate_m5_tuning.py` generates the Rust tuning data from its
report. M5 tuning is experimental and stays explicit.

### Object emission and compile-time boundaries

`measure_compiler_object_api.py` compares the assembly-text round trip with
direct `--emit-object`, verifying whole-object equality before timing. Build
`keel` and the `keel-asm` `profile_pipeline` example in release mode first:

```sh
python3 suite/measure_compiler_object_api.py \
  --input-dir target/suite/lua/pre \
  --work-dir target/assembler-bench/object-api \
  --output suite/reports/compiler-object-api.json \
  --lua-tests target/suite/lua/lua-5.4.7-tests
```

`--baseline-compiler BIN --baseline-commit REV` also times a baseline binary's
text workflow; add `--baseline-has-object-api` when it supports
`--emit-object`.

`measure_compile_boundaries.py` and `measure_assembly_driver.py` separate
C-to-assembly, complete object generation, external assembly and library
stages; the `profile_passes` example reports per-pass time:

```sh
cargo build --release -p keel --examples
python3 suite/measure_compile_boundaries.py \
  --work-dir target/compilation-profile/current \
  --output target/compilation-profile/compile-boundaries.json
python3 suite/measure_assembly_driver.py \
  --input-dir target/compilation-profile/current/keel-S \
  --work-dir target/compilation-profile/current-driver \
  --compiler target/compilation-profile/current/keel-fixed \
  --output target/compilation-profile/assembly-driver.json
target/release/examples/profile_passes 5 target/suite/lua/pre/*.i
```

The CLI runner uses seven seeded randomized paired rounds after a preflight and
warmup, and checks that native objects match the text-assembler objects.
Internal stage timings exclude I/O and startup; they are diagnostic, not an
exact decomposition of the CLI medians.

### Linux debugger checks

After building a native release compiler on x86-64 Linux:

```sh
python3 suite/validate_linux_debugger.py --compiler target/release/keel \
  --work-dir target/linux-debugger --output target/linux-debugger/report.json
```

This requires GDB, Clang and `llvm-dwarfdump`. It checks types, bitfields,
pointer chains, TLS, scopes, live variable mutation and resulting execution for
both allocators, with and without optimization, against a Clang reference.

### C frontend and preprocessor profiling

`profile_c_frontend.py` profiles the native preprocessor and the C-to-IR
frontend on every Lua, QuickJS and SQLite translation unit (44 files):

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --examples
python3 suite/profile_c_frontend.py \
  --source-root target/suite \
  --input-root target/frontend-profile/native-inputs \
  --output suite/reports/c-frontend-profile.json
python3 suite/summarize_c_frontend.py suite/reports/c-frontend-profile.json \
  --output suite/reports/c-frontend-profile-analysis.json
```

Native preprocessing preparation uses the macOS suite definitions and SDK. Omit
`--source-root` to profile already prepared `.i` directories on other hosts;
`--sample-seconds 0` disables macOS stack sampling. The staged helper is behind
the optional `keel-c/profile` feature. Before measurement every file is compiled
through both the staged and the public API, both modules must verify, and their
Debug representations must match. These are warm-cache, serial library
measurements, not cold builds or end-to-end compiler timings.

Paired comparisons of frozen profiling binaries:

| Script | Measures |
|--------|----------|
| `measure_preprocessor.py --profile P --baseline BIN --variant NAME=BIN ...` | Preprocessing time; checks byte-identical output and verifies IR |
| `measure_preprocessor_allocations.py`, `measure_preprocessor_storage.py` | Allocation calls, requested bytes and process peak RSS (never used for latency) |
| `profile_preprocessor_current.py` | Preprocessor stack profile of one binary |
| `measure_c_frontend.py --profile P --variant NAME=BIN ...` | C-to-IR time, normal and debug-info modes; `--resume` extends a run after verifying identical inputs |
| `measure_frontend_storage.py` | Frontend allocation counters |
| `capture_frontend_stacks.py` | Stack captures for one or two binaries |

`summarize_*.py` and `render_*.py` derive statistics and HTML views from saved
reports. The summarizers demangle with `c++filt` and derive inclusive shares from
leaf weights, counting recursive frames once per sample; categories can overlap.

### Application loop profiles

`profile_app_loops.py` profiles the optimized Lua, QuickJS and SQLite workloads.
It prints optimized IR and adds line tables while compiling that IR, then checks
exact object and linked `.text` equality against ordinary `keel cc -O`. It does
not use frontend `-g`, which would keep locals in memory and change the workload.

```sh
python3 suite/profile_app_loops.py \
  --compiler target/release/keel --work-dir target/app-loop-profile \
  --seconds 6 --rounds 2
python3 suite/compare_app_clang.py \
  --compiler target/release/keel --profile target/app-loop-profile/profile.json \
  --work-dir target/clang-comparison
python3 suite/bench_profile_guided.py \
  --compiler target/release/keel --work-dir target/profile-guided-benchmark
```

On macOS the profiler uses `sample`; on Linux it uses task-scoped
`sudo -n perf record` (choose `--event` and `--cpu` for the machine). Use
`--variants keel clang` to profile both compilers, `--build-only` to keep builds
without sampling, and `--reuse-builds --suites <names>` to profile kept builds.
`probe_app_inline.py` and `probe_app_divide.py` build diagnostic source-level
application variants; they are not compiler features. `bench_profile_guided.py`
builds four Keel pipelines (base, vectorization only, inlining plus cleanup
only, both) plus Clang `-O2 -ffp-contract=off`, runs every application's tests
before timing, and measures ASCII scan controls separately.

## Companion harnesses

| Harness | Purpose |
|---------|---------|
| `arm-codegen/` | Download-free AArch64 macOS kernels linking selected source patterns to native correctness and emitted instruction shapes (below) |
| [`runtime-kernels/`](runtime-kernels/README.md) | Ten integer C kernels with independent Python oracles; None/Fast/Full/Clang comparisons |
| [`simd-compare/`](simd-compare/README.md) | Eight explicit IR and C SIMD cases across x86 v1/v2/v3 and baseline Apple ARM64 |
| `width-comparison/` | Integer-widening kernel comparisons with guard-page correctness drivers |
| [`wasm-comparison/`](wasm-comparison/README.md) | Freestanding Wasm: eight C cases, Keel versus Clang |
| [`x86-cpl0/`](x86-cpl0/README.md) | Freestanding x86-64 payloads booted at CPL0 in a disposable QEMU guest |
| [`compiler_asm_compat/`](compiler_asm_compat/README.md) | Real compiler-generated assembly through `keel asm` |
| [`pattern-lab/`](pattern-lab/README.md) | Native assembly patterns checked against independent C oracles, with opt-in paired benchmarking |
| [`workload-pilots/pugixml/`](workload-pilots/pugixml/README.md) | C++ library pilot through LLVM import |
| [LLVM import](llvm_import_README.md), [C++ profiles](cpp_coverage_README.md), [non-C workloads](non_c_workloads.md) | LLVM IR importer validation |
| [Fast tier](fast_opt_README.md) | The bounded Fast optimization tier |

### Targeted native ARM code generation

`suite/arm-codegen/` complements the whole-application suite on AArch64 macOS.
Lua, QuickJS and SQLite remain the application regression gauges; a kernel
result does not establish an application gain.

```sh
python3 -B -m unittest discover -s suite/arm-codegen -p 'test_*.py'
# Copy/edit config.example.json under target/; supply already-built compilers.
python3 -B suite/arm-codegen/run.py --config target/arm-config.json \
  --work-dir target/arm-check-01
# Optional bounded timing: the named cases plus the call-floor control.
python3 -B suite/arm-codegen/run.py --config target/arm-config.json \
  --work-dir target/arm-timing-01 --benchmark \
  --timing-cases and64,movn-negative --rounds 6 --target-seconds 0.12
python3 -B suite/arm-codegen/run.py \
  --verify-report target/arm-timing-01/report.json
```

`--cases NAME` reruns selected functional fixtures plus the control. Each
variant lists its binary, flags and optional `features`, which request shape
gates for that variant only: `logical-immediate` checks each simple mask's
opcode, width and immediate; `constant-planner` checks specific MOVZ/MOVN seed
fields. Leave baseline features empty; a missing shape is reported separately
from semantic pass/fail. Clang O2 is always added. `["-O", "--isel", "none"]` is
an optional same-binary ablation, not an old-compiler baseline.

Clang preprocesses each case once and all variants compile the same bytes. A
Clang-built driver with an independent bit-at-a-time oracle is linked unchanged
with each kernel object. The integer ISA is baseline ARMv8-A (`-march=armv8-a`
for Clang). Checksum failure exits nonzero. Timing is optional: calibration
targets 100-250 ms for the slowest variant, rounds counterbalance variant order,
and reports keep raw times, paired median ratios and bootstrap intervals.
Load/backedge counts are a work-preservation screen; inspect the retained
disassembly before interpreting an optimization gain. `--verify-report` detects
later source or binary mutation.
