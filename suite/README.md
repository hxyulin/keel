# The suite

Real programs that keel must build correctly and that measure how fast its
code is. Each directory here with a `suite.toml` is one program; the
`keel-suite` tool (`crates/keel-suite`) finds them by scanning.

```sh
cargo suite list                    # suites, benchmarks, configurations
cargo suite test                    # test keel and keel -O, with selection off and on
cargo suite test sqlite -c keel-g   # one suite, another configuration
cargo suite run                     # build every configuration, time everything
cargo suite run lua/fib -n 10 -c clang-O0,keel
cargo suite show                    # the latest run on this host again
cargo suite compare aarch64-apple-darwin x86_64-unknown-linux-gnu
```

Sources are downloaded (and checked against their SHA-256) into
`target/suite/<name>/`, and built there in `build/<config>/`. `clang -E`
runs once per suite: every configuration compiles the same preprocessed
files, so the only difference between them is the compiler. clang builds
are cached; keel builds are redone each time.

## Configurations

`configs.toml` lists the ways a program is built, in the order tables show
them: a name, a `compiler` (`clang`: `clang -c` on the preprocessed file;
`keel`: `keel cc`, then clang assembles), and its `flags`. `baseline` is
what benchmark ratios are against; `test` is what `cargo suite test` builds
unless given `-c`.

`keel-select` and `keel-O-select` enable `--isel all`; ordinary `keel` and
`keel-O` leave optional target selection off. `keel-O-trees`,
`keel-O-folds`, and `keel-O-hints` enable one selection pass at a time for
ablation measurements. The default test configurations include both modes.


## A suite

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

Commands are split at whitespace and run without a shell. Their
variables:

| variable    | value                                                  |
|-------------|--------------------------------------------------------|
| `{bin}`     | the executable built in the configuration under test   |
| `{suite}`   | this directory, `suite/<name>`                         |
| `{work}`    | the work directory, `target/suite/<name>`              |
| `{gen}`     | `{work}/gen`, for what `prepare` generates             |
| `{defines}` | the `-D` flags, one argument each                      |
| `{libs}`    | the `-l` flags, one argument each                      |

## Results

`cargo suite run` prints a table per suite, the mean time per benchmark
and configuration and its ratio to the baseline, and saves the run as JSON
in `target/keel-suite/results/<target>/`. `show` and `compare` take a
results file or a target triple, meaning that target's latest run. Runs on
one target are compared by time. Runs on two targets are compared by each
configuration's ratio to its own baseline, since the machines differ. To
compare with another machine, copy its `results/<target>` directory over.

## Paired pass measurements

`measure_passes.py` compares explicit optimizer pipelines on Lua, QuickJS and
SQLite, keeping selection and allocation at their defaults. Run the suite
correctness tests first to prepare the preprocessed files, build release
compilers, and copy the binaries before changing their source. The compiler
copies stay fixed for the whole measurement. For example:

```sh
cargo suite test -c keel-O-combine --release
cp target/release/keel /tmp/keel-pass-candidate
python3 suite/measure_passes.py \
  --compiler /tmp/keel-pass-candidate \
  --baseline-passes simplify-cfg,instcombine,simplify-cfg,gvn,dce,gcm \
  --variant no-gcm=simplify-cfg,instcombine,simplify-cfg,gvn,dce \
  --variant sccp=simplify-cfg,sccp,instcombine,simplify-cfg,gvn,dce,gcm \
  --output /tmp/keel-pass-comparison.json
```

Use `--baseline-compiler` to compare against a different immutable binary.
Repeat `--variant NAME=passes` to sweep alternatives. Every build finishes
before timing begins. Runtime samples alternate baseline/candidate order and
shuffle variant order in each round; every workload's stdout must match.
Compiler samples compile the same files serially, excluding assembly and
linking. Code size reports the executable's text section. The JSON records
raw samples, paired median ratios and bootstrap intervals, compiler/source
hashes, pipelines, host, and the measurement script's hash. Runtime geometric
means weight each of the eight workloads equally; compiler means weight each
of the three applications equally. A ratio below one is an improvement.

Defaults are seven runtime pairs after two warmups and five compiler pairs
after one warmup. Keep the host idle during timing. These short application
workloads measure this backend and machine; a neutral median or an interval
crossing one does not establish a speedup. Benchmark stdout checks complement
the full correctness suites and semantic pass matrices.

The checked-in [pass ablations](reports/pass-ablations-aarch64-2026-10-15.json)
use five pairs after one warmup on Apple M5 Max. Removing GCM costs 4.2% in
runtime geometric mean. Removing GVN has mixed runtime effects and grows
aggregate text by 5.1%; Full keeps it, while Basic omits it. Late phi cleanup
avoids the earlier 2.2× Lua regression, but has no established runtime gain
and adds 2.8% compiler cost. Adding SCCP is near-neutral for runtime with
5.6% compiler overhead. Both additions remain opt-in. CFG and DCE also stay
in the default pipelines. These choices concern this measured pipeline;
individual pass profitability can change with the selector or allocator.

The opt-in `keel-O-memory` and `keel-O-memory-select` presets add conservative
block-local memory forwarding after GVN. The [paired memory sweep](reports/memory-forward-aarch64-2026-10-15.json)
records both forwarding alone and a subsequent instcombine, plus static load
counts on the same preprocessed inputs. Forwarding removes 1,758 static loads
and 7,584 text bytes, but its 0.5% lower aggregate runtime is within the observed
variation and compilation costs 2.4% more; it is not enabled by default.

The `keel-m5` and `keel-m5-fast` configurations enable explicit Apple M5 tuning
with full instruction selection, using the full and fast allocators respectively.
`suite/measure_m5_integer.py` measures the first scalar profitability alternatives
on actual M5 hardware, with kernels assembled by Keel and independently checked
results. It records nanoseconds, not guessed cycle counts. The checked-in native
report generates the Rust tuning data through `suite/generate_m5_tuning.py`.

For comparisons with the same IR pipeline, `measure_passes.py` accepts repeated
`--baseline-arg` and `--compiler-arg` options. Pass leading-dash arguments with
`=`, for example `--compiler-arg=--tune --compiler-arg=apple-m5`. Both sets of
arguments apply to application builds and compile-time samples. `--assembler
keel` assembles both variants with their Keel driver; the default is `clang`.
The selected assembler and compiler arguments are recorded in the report.

The first [M5 application comparison](reports/apple-m5-tuning-aarch64-2026-10-15.json)
is neutral overall (runtime ratio 1.0013), with a 2.3% QuickJS Fibonacci regression
and 160 fewer text bytes. The isolated multiply dependency-chain improvement
is about 33%, but this does not translate into a general application speedup.
M5 tuning is experimental and remains explicit.

The simplification presets `keel-O-cleanup`, `keel-O-bits-early`,
`keel-O-bits-late`, and `keel-O-bits-repeat` isolate late cleanup, known-bit
analysis placement, and repetition. Their [ordering sweep](reports/simplification-order-aarch64-2026-10-15.json)
compares runtime/code size/compile cost on identical preprocessed inputs with
Keel assembly. See the [simplification audit](../docs/simplification.md) for
proof tests and the remaining gaps.

The [integer canonicalization comparison](reports/canonical-constants-aarch64-2026-10-15.json)
uses separate immutable compiler binaries with the same pipeline. Literal-left
integer forms fall from 518 to zero; IR instruction counts are unchanged but
text shrinks by 1,256 bytes, with neutral aggregate runtime and compile cost.
`measure_ir_counts.py` accepts `--baseline-compiler` for version comparisons and
verifies each binary and source hash against the report before adding counts.

The [explicit canonicalization stage comparison](reports/canonical-stage-aarch64-2026-10-15.json)
measures the initial stage and canonical entry requirements against that compiler:
297 literal-left comparisons become zero, text shrinks another 1,056 bytes,
runtime ratio is 0.9923 (mostly intervals spanning one), and compile-time ratio
is 1.0768. The census also counts `left_comparison_literals`. Default Basic and
Full now run CFG cleanup before operand canonicalization; explicit historical recipes retain their
listed stages, with prerequisites restored for consumers that request them.

`keel-O-ranges` adds conditional-edge integer ranges after GVN, followed by
combining and CFG cleanup. The [simplification audit](../docs/simplification.md)
records separate Boolean, cast-chain, wrapping-algebra, carry-fact and
branch-range measurements. Incremental screens use three paired samples;
small timing differences should not be treated as established speedups.

The [expanded-scalar ordering comparison](reports/scalar-order-aarch64-2026-10-15.json)
uses seven paired runtime and five paired compile samples. Moving CFG cleanup
before operand normalization preserves all 44 IR and assembly files and reduces
compile time 2.83%; Basic and Full adopt that order. Extra late cleanup and the
`keel-O-facts` recipe remain opt-in because they increase code size and compile
cost without an established runtime gain. Static-only second/third rounds test
corpus convergence; their runtime has not been measured.

The final 15-pair [before/after comparison](reports/scalar-final-before-after-aarch64-2026-10-15.json)
and [Full/facts versus None comparison](reports/scalar-final-vs-none-aarch64-2026-10-15.json)
record the complete default change and its total optimizer cost. See the
[final benchmark interpretation](../docs/simplification.md#final-paired-benchmark-of-the-complete-change).
An empty `--baseline-passes ''` now selects explicit `--opt-tier none` in the
runner and IR census. To add reproducible confidence intervals for aggregate
ratios after timing completes:

```sh
python3 suite/summarize_passes.py --report suite/reports/example.json
```

The aggregate bootstrap resamples pairs within each fixed workload; it does not
estimate performance uncertainty on unmeasured programs or other machines.

## In-process assembly API

`measure_compiler_object_api.py` compares a current compiler's assembly-text
round trip with `--emit-object`, verifying whole-object equality before timing.
Build `keel` and the `keel-asm` `profile_pipeline` example in release mode first.
Run serially after builds and tests finish:

```sh
python3 suite/measure_compiler_object_api.py \
  --input-dir target/suite/lua/pre \
  --work-dir target/assembler-bench/object-api-complete \
  --output suite/reports/compiler-object-api-complete-20261015.json \
  --lua-tests target/suite/lua/lua-5.4.7-tests
```

`--baseline-compiler <binary> --baseline-commit <commit>` also verifies and times
the baseline text workflow. Add `--baseline-has-object-api` when that binary
supports `--emit-object` to compare prior and current direct paths in the same
randomized CLI rounds. The report retains commands, raw samples and hashes.

The [completed AArch64 migration](reports/compiler-object-api-complete-20261015.html)
([raw data](reports/compiler-object-api-complete-20261015.json)) reaches
85,988/85,988 native Lua instructions with zero remaining text bytes. Current
codegen plus object generation takes 39.92 ms versus 67.69 ms through text;
full CLI batches take 217.87 ms versus 322.61 ms. These are separate timing
boundaries. The previous direct API takes 229.21 ms in the same CLI run; the
median paired ratio corresponds to 5.2% less time after completing the migration.
All 33 objects match all four paths, and the linked interpreter passes Lua's
portable tests. The earlier [partial API report](reports/compiler-object-api-20261015.json)
remains available for historical context.

## Native x86-64 Linux bring-up

The [initial NUC report](reports/x86-linux-bringup-20261015.html) ([raw logs](reports/x86-linux-bringup-20261015.json)) contains the
host/toolchain versions, source commit, commands, logs and known blockers.
It records correctness, not benchmark comparisons. Run on the Linux host:

```sh
cargo test --workspace --exclude keel-asm --all-targets
cargo test -p keel-asm --lib --test structured
cargo suite test lua --release -c keel,keel-O,keel-select,keel-O-select
```

Clang/GNU assembly and ELF linking are part of this path; Keel's direct
`--emit-object` path is currently AArch64 Mach-O only. Lua, QuickJS and SQLite pass all four
modes after adding GNU aligned typedef support for glibc's `pthread.h`. The full ARM differential suite requires a
reference Clang with the requested CPU/features; the NUC's Clang 18 rejects
`-mcpu=apple-m5`, so that limitation is recorded separately from compiler
correctness.


## Full macOS and Linux measurements

The [platform benchmark report](reports/full-platform-benchmarks-20261015.html)
compares all eight workloads on Apple M5 Max/macOS and Intel i7-1360P/Ubuntu.
Raw suite results, configuration lists, commands and validation logs accompany
that report. Builds use one job and refresh cached Clang statistics; runtime
uses one warmup and five measured runs. All 32 configurations run on macOS;
Linux excludes the two Apple M5 tuning presets. Experimental pass recipes are
included, not enabled as defaults by this measurement.

These runs compare the same Clang-preprocessed sources, so native preprocessing
is outside the measured boundary. Keel emits text assembly and Clang assembles
it on both hosts, including macOS; the direct object API is not measured here.
Build times are one observation, while runtime samples are sequential by
configuration rather than paired/randomized. Cross-machine comparisons use
ratios to each host's Clang baseline; absolute times also reflect the hardware
and operating system.


## Compilation-speed boundaries

The [compilation-speed analysis](reports/compilation-speed-analysis-20261015.html)
([aggregate data](reports/compilation-speed-analysis-20261015.json),
[macOS CLI samples](reports/compile-boundaries-macos-20261015.json),
[Linux CLI samples](reports/compile-boundaries-linux-20261015.json)) separates
C-to-assembly, complete object generation, external assembly and library stages.
It reuses the exact compiler binaries from the full platform benchmark.

On macOS, the direct object API takes 1.135 s for all 44 inputs (1.568 s with
full optimization), versus Clang -O0's 1.842 s integrated C-to-object batch.
Keel text plus external Clang takes 10.165 s. On Linux, Keel emits assembly
in 1.589 s versus Clang -O0's 2.851 s; external assembly raises the full Keel
workflow to 5.672 s. These are different output boundaries, explicitly named.
The ordinary suite workflow still uses external Clang assembly; this analysis
does not change its default or claim equivalent optimized-code quality.

The optimizer's repeated verifier calls account for 57% of measured pass time
on macOS and 53% on Linux. The separate pass profiler checks its final IR against
the ordinary optimizer. Compiler verification policy is unchanged. A short
[SQLite CPU sample](reports/compilation-speed-keel-sqlite-sample-20261015.txt)
provides additional, partially symbolicated allocation/tokenization/verifier
leads; its percentages are not a whole-corpus profile.

Build tools before timing, and keep both machines idle during each measurement:

```sh
cargo build --release -p keel --examples
python3 suite/measure_compile_boundaries.py \
  --work-dir target/compilation-profile/current \
  --output /tmp/compile-boundaries.json
python3 suite/measure_assembly_driver.py \
  --input-dir target/compilation-profile/current/keel-S \
  --work-dir target/compilation-profile/current-driver \
  --compiler target/compilation-profile/current/keel-fixed \
  --output /tmp/assembly-driver.json
target/release/examples/profile_passes 5 target/suite/lua/pre/*.i
```

The command-line runner uses seven seeded randomized paired rounds after a
preflight and warmup. Native objects match the text-assembler objects for all
44 files both plain and optimized, and both resulting builds pass Lua, QuickJS
and SQLite correctness checks. Preprocessing and linking are outside timings.
Internal stage timings exclude input/output and startup, include destruction,
and report five samples after two warmups. They are diagnostic measurements,
not an exact decomposition of independently timed CLI medians.

## C frontend profiling

`profile_c_frontend.py` profiles the native preprocessor and the C-to-IR
frontend on every Lua, QuickJS and SQLite translation unit (44 files). Build
before timing, then use an existing suite work directory containing the
sources and generated QuickJS C files:

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --examples
python3 suite/profile_c_frontend.py \
  --source-root target/suite \
  --input-root target/frontend-profile/native-inputs \
  --output suite/reports/c-frontend-profile-20261015.json
```

Native preprocessing preparation currently uses the macOS suite definitions
and SDK. Omit `--source-root` to profile already prepared `.i` directories on
other hosts; use `--sample-seconds 0` to omit macOS stack sampling. Input
reading is outside frontend timing. Native preprocessing includes header IO,
macro expansion, output formatting and internal destruction. The independent
C-to-IR measurements use resident preprocessed text and separate lexing,
parsing/name resolution/type checking, IR generation and parser/AST/token
cleanup. Returned IR destruction is recorded separately. Both ordinary and
debug-info modes run nine rounds by default; applications and timing modes
are shuffled with a fixed seed, and file order rotates across rounds.

The staged helper is behind the optional `keel-c/profile` feature; the normal
compiler API is unchanged. Before measurement, every file is compiled through
both APIs, both modules pass the IR verifier, and their complete Debug
representations must match. The unchanged API provides the total-time
baseline to check stage instrumentation overhead. Builds and tests must finish
before timing. Raw samples, commands, source/binary hashes and stack samples
are retained. These are warm-cache, serial frontend library measurements,
not cold builds or end-to-end compiler timings. Preprocessing and C-to-IR
medians are measured separately; adding them estimates the combined cost.

The [15 October baseline report](reports/c-frontend-profile-20261015.html)
([raw data](reports/c-frontend-profile-20261015.json)) uses local main
`ae7c2f8` on Apple M5 Max. To regenerate its stack summaries and presentation:

```sh
python3 suite/summarize_c_frontend.py suite/reports/c-frontend-profile-20261015.json \
  --output suite/reports/c-frontend-profile-analysis-20261015.json
python3 suite/render_c_frontend_profile.py
```

The summarizer demangles with `c++filt` and derives inclusive sample shares
from leaf weights, counting recursive frames once per sample. Categories can
overlap. It asserts nonnegative leaf weights and conservation of total samples.

## Preprocessor optimization comparison

The [optimized preprocessor report](reports/c-preprocessor-optimized-20261015.html)
compares immutable original, compact-scalar and SIMD-search binaries against
local main `ae7c2f8`. All 44 application outputs are byte-identical and their IR
verifies. Eleven paired rounds show a 2.44× equal-application geometric mean
speedup on Apple M5 Max. SIMD contributes a further 4.6–5.1% reduction over the
compact scalar implementation; most of the gain comes from fewer allocations
and copies. These are preprocessing library times, including transitive header
IO and internal cleanup, with warm caches. They exclude reading the top-level
source, process startup, SDK discovery, returned-text destruction and C-to-IR.

Archive the original example binary before editing; build and archive each
candidate before timing. Then run:

```sh
python3 suite/measure_preprocessor.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/baseline/profile_preprocess \
  --variant compact-scalar=target/frontend-profile/baseline/profile_preprocess-compact-scalar \
  --variant simd=target/frontend-profile/baseline/profile_preprocess-simd \
  --work-dir target/frontend-profile/paired-preprocessor \
  --output suite/reports/c-preprocessor-optimized-20261015.json
```

The [source/build details](reports/c-preprocessor-optimization-details-20261015.json)
retain source hashes and the exact runtime-source patch that replaces the three
SIMD-backed searches with scalar searches. The rest of those variants is
identical. The timing runner checks output equality, verifies IR through each
binary, rotates variant order, shuffles applications, and bootstraps paired
ratios within each fixed workload. Keep builds/tests separate from timings.

The separate `profile_preprocess_allocations` example counts allocator calls
only across `preprocess`. Build that same example against the original and
candidate implementations, then run:

```sh
python3 suite/measure_preprocessor_allocations.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline PATH_TO_ORIGINAL_COUNTING_EXAMPLE \
  --candidate target/release/examples/profile_preprocess_allocations \
  --output suite/reports/c-preprocessor-allocations-20261015.json
python3 suite/render_preprocessor_optimization.py
```

The [allocation data](reports/c-preprocessor-allocations-20261015.json) show
76–77% fewer allocation/reallocation calls and 53–55% fewer cumulative requested
bytes. Requested bytes include full reallocation sizes and are not peak live
memory or RSS. Counting binaries are never used for timing.

## Arena and header-source cache comparison

The [arena/cache report](reports/c-preprocessor-arena-20261015.html)
([raw timing](reports/c-preprocessor-arena-20261015.json),
[storage measurements](reports/c-preprocessor-arena-storage-20261015.json))
measures another 2.83× preprocessing speedup over the SIMD/compact-token
version, and 6.95× versus the original in the same eleven paired rounds on
Apple M5 Max. Allocation/reallocation calls fall another 95.6–96.3%.

Tokens borrow source text. Macro nodes, macro parameter/body slices, generated
text and persistent hide sets use a bump arena. Exact-sized lexical token
buffers and source owners live in separate typed arenas, whose destructors
release boxed buffers, owned read strings and memory maps at call completion.
The returned output remains an ordinary owned string; bookkeeping maps also
use ordinary owned storage. The design avoids retaining discarded capacities
from growing large token vectors in the bump arena.

Source text and unexpanded lexical tokens are cached by canonical path and
disk/embedded-header identity within each `preprocess` invocation. Macro
expansion and conditional state remain per inclusion. Conservative complete
`#ifndef` guards can skip inactive bodies, with structural validation preserving
malformed-conditional diagnostics. `#undef`, conditional `#pragma once`, aliases,
`#line`, and `include_next` retain their behavior. Each new invocation sees
current file contents. Source-open counters exclude top-level input loading
and path-resolution metadata operations.

Headers at least 64 KiB use validated read-only COW mappings where available;
smaller/empty files and mapping failures use reads. Included files must remain
unchanged until the call returns, including avoiding concurrent truncation of
mapped files. Source and token caching supply the main file-access gain. MMAP
versus cached reads reduces the aggregate paired median ratio by just 0.27%;
two of three per-application intervals include no change. No cold-storage or
cross-CPU benefit is established by these warm-cache timings.

Build and freeze all variants before timing. The prior optimized binary is the
baseline; `original` is the original preprocessor. Final/ablation source hashes
and exact compile-time-constant patches are in the
[build details](reports/c-preprocessor-arena-details-20261015.json):

```sh
python3 suite/measure_preprocessor.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/arena-baseline/profile_preprocess \
  --variant original=target/frontend-profile/baseline/profile_preprocess \
  --variant arena=target/frontend-profile/arena-baseline/profile_preprocess-arena-only \
  --variant cache-read=target/frontend-profile/arena-baseline/profile_preprocess-cache-read \
  --variant cache-mmap=target/frontend-profile/arena-baseline/profile_preprocess-cache-mmap \
  --work-dir target/frontend-profile/arena-paired \
  --output suite/reports/c-preprocessor-arena-20261015.json
python3 suite/measure_preprocessor_storage.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/arena-baseline/profile_preprocess_allocations \
  --candidate target/frontend-profile/arena-baseline/profile_preprocess_allocations-cache-mmap \
  --profiler target/frontend-profile/arena-baseline/profile_preprocess-cache-mmap \
  --output suite/reports/c-preprocessor-arena-storage-20261015.json
python3 suite/summarize_preprocessor_arena.py \
  --timing suite/reports/c-preprocessor-arena-20261015.json \
  --storage suite/reports/c-preprocessor-arena-storage-20261015.json \
  --output suite/reports/c-preprocessor-arena-analysis-20261015.json
python3 suite/render_preprocessor_arena.py
```

`preprocess_with_stats` and the `profile_preprocess stats` mode require the
`profile` feature. They instantiate a separate counter-enabled implementation;
the ordinary API compiles the counters away. The storage runner independently
collects allocator calls, requested bytes and macOS counting-harness peak RSS.
Requested bytes exclude mappings and are not peak memory. RSS is a full-process
single-run high-water mark, including warmups, rather than isolated call memory.
Never use these counting binaries for latency measurements.


### Additional preprocessor fast paths (2026-10-15)

The next pass reuses expanded pending-vector capacity across directives,
precomputes include search directories and checks candidates lazily, reuses a
single macro lookup and bypasses token-paste buffers for ordinary object macros,
and slices tokens from validated strings instead of validating UTF-8 per token.
Spliced buffers receive one checked validation; no unchecked string conversion
was added. Include resolution itself remains uncached, preserving caller and
`include_next` origin behavior.

[Paired measurements](reports/c-preprocessor-easy-20261015.json) compare frozen
release binaries with identical build flags. Variants are cumulative: `buffer`,
`paths`, `macros`, then `all` (validated string scanning). Nine pairs per
application show an equal-application geometric mean time reduction of 8.6%
(1.094x speedup; bootstrap 95% reduction interval 5.5–8.9%). Median application
latencies are Lua 223.66 → 205.89 ms, QuickJS 117.05 → 109.05 ms, and SQLite
93.39 → 83.66 ms. Lua's paired interval alone crosses no change, so its gain
remains less certain. These are warm-cache measurements on one Apple M5 Max.
Most incremental benefit comes from string slicing; smaller steps are sensitive
to noise and their contribution depends on order.

[Allocation measurements](reports/c-preprocessor-easy-storage-20261015.json)
show 15.0%, 19.7%, and 24.1% fewer allocation/reallocation calls for Lua,
QuickJS, and SQLite. Retaining pending-vector capacity has a memory tradeoff:
single counting-process peak RSS changes from 43.69 → 51.84 MiB for Lua,
123.61 → 126.97 MiB for QuickJS, and 298.91 → 294.66 MiB for SQLite. These
full-process observations include warmups and are not isolated-call peaks.

All 65 frontend/driver tests pass, including a new Unicode/splicing regression.
All five variants produce byte-identical outputs for the 44 corpus files and
verify their generated IR. The rebuilt release driver's 44 CLI outputs also
match. [Build details and incremental patches](reports/c-preprocessor-easy-details-20261015.json)
record source hashes and the cumulative changes.

```sh
python3 suite/measure_preprocessor.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/easy/profile-baseline \
  --variant buffer=target/frontend-profile/easy/profile-buffer \
  --variant paths=target/frontend-profile/easy/profile-paths \
  --variant macros=target/frontend-profile/easy/profile-macros \
  --variant all=target/frontend-profile/easy/profile-all \
  --rounds 9 --work-dir target/frontend-profile/easy/paired \
  --output suite/reports/c-preprocessor-easy-20261015.json
```


### Current preprocessor stack profile (2026-10-15)

[Interactive report](reports/c-preprocessor-current-profile-20261015.html) and
[raw timings, stack summaries, and provenance](reports/c-preprocessor-current-profile-20261015.json)
record the preprocessor after all four fast paths. Each application has seven
warm timing rounds and a separate six-second macOS sample capture. Timing and
sampling run serially; stack sampling is not used for latency measurements.
Exact corpus hashes are checked against the original profile. Sample files are
saved beside the JSON with the application name suffix.

The exclusive phase view assigns each leaf sample to its nearest recognized
preprocessor operation; inlined output is grouped with flush overhead. The
separate inclusive copying/allocator/UTF-8 views overlap those phases. Current
scanning is 25–36%, expansion 21–26%, and memory copying 14–22% of all samples.
Copying inside expansion alone is 7.6–16.4%; token-stream splices are a candidate,
but the stacks do not isolate that exact operation. Search plus canonicalization
is 8–18%. UTF-8 validation is below 0.6%, compared with 4–5% in the prior short
Lua/SQLite survey. Sample shares describe attribution, not speedups.

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile_preprocess
# Freeze the binary before profiling; do not build concurrently with sampling.
mkdir -p target/frontend-profile/current
cp target/release/examples/profile_preprocess target/frontend-profile/current/profile_preprocess
python3 suite/profile_preprocessor_current.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --binary target/frontend-profile/current/profile_preprocess \
  --output suite/reports/c-preprocessor-current-profile-20261015.json
python3 suite/render_preprocessor_current.py
```


### Scanning, directives, header resolution, and macro streaming (2026-10-15)

[Saved report](reports/c-preprocessor-deeper-20261015.html),
[paired timings](reports/c-preprocessor-deeper-20261015.json),
[allocation/RSS measurements](reports/c-preprocessor-deeper-storage-20261015.json),
and [refreshed stack profiles](reports/c-preprocessor-deeper-profile-20261015.json)
cover the next pass. The baseline is the previously profiled four-fast-path
version, not the original preprocessor. All timing variants use identical release
flags with line-table debug information; binaries are frozen before measurement.

The final equal-application geometric mean time reduction is 20.1% (1.252x
speedup; 95% paired bootstrap reduction interval 19.8–20.4%). Median latencies
are Lua 203.31 → 161.89 ms, QuickJS 107.33 → 80.62 ms, and SQLite 83.74 →
71.22 ms. Cumulative comparisons attribute roughly 3.1% to next-newline tracking
and whitespace runs, 4.8% to borrowing directive lines and skipping inactive
ordinary code, 3.0% to resolution caching, and 10.3% to macro streaming plus
scratch reuse. These order-dependent reductions compare with the preceding
stage, are not additive, and do not compose exactly because medians differ.

The scanner searches ahead for each physical newline once, preserving sparse
splice mapping and byte columns. Directives borrow cached token slices unless
`#line` or an alternate file spelling requires location changes. Header resolution
caches both found and missing results per invocation; quoted including-file and
`include_next` origin contexts are distinct. Included files and search directories
must stay unchanged during a call. Macro expansion stores unconsumed tokens in
reverse order so replacing a prefix does not shift the original suffix; rescans,
hide sets, invocation boundaries, builtins, and expansion limits remain intact.
Drained input buffers with capacity at most 4,096 tokens can be recycled; larger
buffers are released. Standard owned vectors retain bounded scratch capacity
rather than adding scratch allocation churn to the bump arena.

Fresh profiles show copying inside expansion down from 7.6–16.4% of all samples
to 0.6–1.1%. Allocation/reallocation counts change by +2.2% (Lua), −2.5%
(QuickJS), and +0.8% (SQLite). One full-process RSS observation changes from
44.98 → 38.53 MiB, 126.50 → 133.72 MiB, and 296.59 → 300.42 MiB respectively.
RSS includes warmups and is not isolated-call memory. The extra macro output
vector and resolution cache have a memory tradeoff even though copying falls.

All 67 frontend/driver tests pass. New Clang parity coverage stresses replacement
prefix/suffix boundaries and argument order; header-cache coverage distinguishes
quoted callers, angle includes, and query misses across invocations. All six
variants produce byte-identical outputs on the 44 translation units and verify
IR, and the rebuilt driver matches all 44 CLI outputs. Exact patches and source
hashes are in the [build details](reports/c-preprocessor-deeper-details-20261015.json).

```sh
python3 suite/measure_preprocessor.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/current/profile_preprocess \
  --variant scan=target/frontend-profile/deeper/profile-scan \
  --variant directives=target/frontend-profile/deeper/profile-directives \
  --variant headers=target/frontend-profile/deeper/profile-headers \
  --variant stream=target/frontend-profile/deeper/profile-stream \
  --variant all=target/frontend-profile/deeper/profile-all \
  --rounds 11 --work-dir target/frontend-profile/deeper/paired \
  --output suite/reports/c-preprocessor-deeper-20261015.json
python3 suite/measure_preprocessor_storage.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline target/frontend-profile/deeper/allocations-baseline \
  --candidate target/frontend-profile/deeper/allocations-all \
  --profiler target/frontend-profile/deeper/profile-all \
  --output suite/reports/c-preprocessor-deeper-storage-20261015.json
python3 suite/profile_preprocessor_current.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --binary target/frontend-profile/deeper/profile-all \
  --output suite/reports/c-preprocessor-deeper-profile-20261015.json
python3 suite/render_preprocessor_deeper.py
```


### Non-preprocessor frontend stages (2026-10-15, commit 2808137)

[Interactive report](reports/c-frontend-stages-20261015.html),
[raw stage measurements](reports/c-frontend-stages-20261015.json),
[stack and ownership analysis](reports/c-frontend-stages-analysis-20261015.json),
and [validation / callsite evidence](reports/c-frontend-stages-details-20261015.json)
refresh the C-to-IR profile after preprocessing optimization. The same 44 native
preprocessed input hashes match the original corpus: 1.99 million tokens and
6,136 functions. Nine serial rounds measure public and staged APIs in normal
and debug-info modes, with IR verification and exact Debug IR equality in each
process's warmup. Baseline/staged median ratios are 0.983–1.003; no optimization
of these stages is claimed. Separate six-second stack captures cover Lua,
QuickJS, SQLite, and SQLite debug. The remaining preprocessor investigation is
in a separate subagent worktree; its builds/timings were gated until these
frontend timings and samples completed.

Normal-mode public compile medians are Lua 71.60 ms, QuickJS 158.15 ms, and
SQLite 214.84 ms, excluding preprocessing and returned-module destruction.
Staged lex / parse-and-types / IR generation / cleanup medians are respectively
28.65 / 27.75 / 11.87 / 4.04 ms for Lua, 52.89 / 49.09 / 47.39 / 7.13 ms
for QuickJS, and 53.74 / 57.93 / 92.42 / 10.14 ms for SQLite. Stage medians
do not add exactly to the separately measured public API median.

Three promising followups are the lexer's 48-entry linear punctuation matcher,
parser token consumption that clones discarded strings, and SSA predecessor
maintenance. Release disassembly ties 9.9–16.6% of total normal frontend samples
to the punctuation matcher's memcmp callsite; other lexer comparisons are mostly
file-name indexing. Parsing owns 3.2–6.8% of total samples under String::clone;
`eat` currently calls `advance` and discards the returned clone. SQLite has
16.8% of samples under SSA `seal_block`, whose inlined `find_preds` scans the
built CFG for each block. The profile does not isolate that scan from parameter
work. These are candidates, not promised speedups. Parser/token/AST cleanup is
about 4–6%; broad arena conversion should not precede the concrete hot paths.

The extra analysis assigns shared helper costs by pipeline entry frame rather
than namespace. In particular, Tok::clone lives in lex.rs but is generally
called by the parser; it should not inflate lexer attribution. Samples include
returned-module destruction, unlike the timed compile boundary. Inclusive
comparison, allocator, and clone percentages overlap. Debug mode changes local
storage and SSA work, so its different timings compare different output modes.

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile
mkdir -p target/frontend-profile/stages
cp target/release/examples/profile target/frontend-profile/stages/profile
python3 suite/profile_c_frontend.py \
  --input-root target/frontend-profile/native-inputs \
  --profiler target/frontend-profile/stages/profile \
  --rounds 9 --sample-seconds 6 \
  --output suite/reports/c-frontend-stages-20261015.json
python3 suite/summarize_c_frontend.py \
  suite/reports/c-frontend-stages-20261015.json \
  --output suite/reports/c-frontend-stages-analysis-20261015.json
python3 suite/analyze_frontend_stages.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --analysis suite/reports/c-frontend-stages-analysis-20261015.json
python3 suite/render_frontend_stages.py
```
