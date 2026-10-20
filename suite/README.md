# The suite

Generated reports and raw evidence belong in the ignored `suite/reports/` directory.
They are local run outputs and are not stored in Git. Historical report links
have been removed; benchmark and profiling commands can generate fresh outputs.

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

`keel-select` and `keel-O-select` explicitly enable `--isel all`; `keel-O`
also enables all selection passes through the CLI `-O` preset. Ordinary
`keel` keeps baseline selection. To compare optimized IR with baseline
selection, use `-O --isel none` (either flag order). Explicit selection
selectors override the preset, with the last explicit selector winning.
`--opt-tier` and `-p` control IR independently and do not reset selection. `keel-O-trees`,
`keel-O-folds`, and `keel-O-hints` enable one selection pass at a time for
ablation measurements. The default test configurations include both modes.

`keel-fast` selects the Fast IR profile with Full selection and allocation;
`keel-full` selects Full for all three. Full IR now runs three bounded rounds
of inlining, scalar/memory cleanup, code motion and loop optimization; Fast
retains its existing bounded pipeline. The inliner freezes templates per round,
so later rounds revisit helpers exposed by earlier cleanup. This is a fixed
three-round schedule, not an unbounded fixed-point loop.

The corrected-compiler suite comparison on MacHost, LinuxHost and the VPS measured
about 8.5–15.6% less runtime than one Full round for about 50–53% more native
object compilation time. Gains concentrate in QuickJS; code size increases and
some individual workloads gain little or regress slightly. Shorter schedules
did not offer a consistent runtime/code-size improvement. The measured compiler
included the sparse-switch default-edge fix (`ad85339a`); pre-fix failed Lua
arms are not valid performance evidence. Full remains about 1.67–2.08× Clang O2
runtime on this corpus, while compiling about 6.5–7.8× faster at the measured
preprocessed-C-to-object boundary. These are within-host paired comparisons,
not end-to-end build times or a Cranelift comparison.

The historical Basic-IR/Fast-allocation
recipe is retained as `keel-basic-fast-alloc`, so allocation experiments do not
silently stand in for the Fast IR profile.

## Native platform comparisons

`benchmark_platforms.py` records serial preprocessed-C-to-assembly and
preprocessed-C-to-object compilation separately, then native CLI runtime over
the eight Lua/QuickJS/SQLite workloads. Keel's object boundary uses its native
emitter; Clang/GCC use `-c`. The common Clang linker and all preprocessing,
compiler builds, and dependency setup are outside these timing boundaries.
There is no LTO or native CPU tuning; Linux reference compilers explicitly
target baseline x86-64. Every configuration must pass the application tests
and match Clang O2 benchmark outputs before it is timed. Failures are retained
and excluded, rather than timed as successful builds.

```sh
python3 suite/benchmark_platforms.py --root /path/to/frozen/source \
  --work /path/to/new/results --keel /path/to/frozen/keel \
  --gcc /usr/bin/gcc --clang /usr/bin/clang --cpu 2
```

Use `--prepare-only` to prepare native inputs and `--reuse-prepared` to reuse
their verified hashes. Two Linux hosts can share that prepared corpus, including
generated QuickJS sources. `--normalize-prepared` preserves raw `.i` files and
renames explicit glibc `_Float32`-family compatibility typedef identifiers for
GCC/Clang interoperability; it does not change their underlying types or rewrite
application operations. Verify unchanged Clang assembly before using normalized
inputs. Do not share preprocessed inputs across Linux and macOS.

The initial 2026-10-20 MacHost measurement omitted `LUA_USE_MACOSX` because
the runner compared Darwin against the manifest name `macos`. All measured
MacHost arms used that same configuration. The runner now maps Darwin to `macos`;
preserved reports describe the original measured configuration, not this fix.

Clang's preprocessed C11 atomics are compiler-specific. When a GCC build cannot
consume QuickJS's `__c11_atomic_*` builtins, run a separately labelled supplement:
`--apps quickjs --configs clang-O2 gcc-O0 gcc-O2 --gcc-native-preprocess`.
This uses the same upstream/generated C sources and defines but GCC's native
preprocessor and headers. Its preprocessing remains outside the timing boundary;
do not describe those GCC inputs as byte-identical to the shared Clang/Keel `.i`
files. The supplement needs its own new work directory and the same host lock.

Defaults are five paired compile rounds after one warmup and seven paired
runtime rounds after two warmups. Order is seeded and shuffled. An identical
Clang O2 executable control, load/CPU-pressure/steal counters, compiler/input
hashes, correctness results, full command logs and raw samples are retained.
Ratios and paired bootstrap intervals are against Clang O2 on each host;
cross-host absolute times are not interchangeable. Runtime includes process
startup and uses ordinary ASLR/page backing, not a physical-page selection
protocol. Run under a per-host exclusive performance lock and avoid concurrent
builds or profiling on that host.


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

For committed compiler comparisons, `--baseline-passes tier:full --variant
current=tier:full` runs each revision's actual Full preset. `--jobs 1`,
`--compile-warmups 2`, and `--command-log PATH.jsonl.gz` support serial builds
and retained commands; `--runtime-path PATH` copies each application to one
pathname before timing. The FunctionAttrs consumer comparison
uses fresh release targets on MacHost and LinuxHost, 11 paired samples, whole-app
machine-code parity, C compilation/RSS, generated contract scaling and a separate
same-compiler SSE2/AVX flag comparison. Its identical application executables
show no runtime/code-size change, with roughly 2–3% more compilation time.

The checked-in pass ablations
use five pairs after one warmup on Apple M5 Max. Removing GCM costs 4.2% in
runtime geometric mean. Removing GVN has mixed runtime effects and grows
aggregate text by 5.1%; Full keeps it, while Basic omits it. Late phi cleanup
avoids the earlier 2.2× Lua regression, but has no established runtime gain
and adds 2.8% compiler cost. Adding SCCP is near-neutral for runtime with
5.6% compiler overhead. Both additions remain opt-in. CFG and DCE also stay
in the default pipelines. These choices concern this measured pipeline;
individual pass profitability can change with the selector or allocator.

The opt-in `keel-O-memory` and `keel-O-memory-select` presets add conservative
block-local memory forwarding after GVN. The paired memory sweep
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

The first M5 application comparison
is neutral overall (runtime ratio 1.0013), with a 2.3% QuickJS Fibonacci regression
and 160 fewer text bytes. The isolated multiply dependency-chain improvement
is about 33%, but this does not translate into a general application speedup.
M5 tuning is experimental and remains explicit.

The simplification presets `keel-O-cleanup`, `keel-O-bits-early`,
`keel-O-bits-late`, and `keel-O-bits-repeat` isolate late cleanup, known-bit
analysis placement, and repetition. Their ordering sweep
compares runtime/code size/compile cost on identical preprocessed inputs with
Keel assembly. See the [simplification audit](../docs/simplification.md) for
proof tests and the remaining gaps.

The integer canonicalization comparison
uses separate immutable compiler binaries with the same pipeline. Literal-left
integer forms fall from 518 to zero; IR instruction counts are unchanged but
text shrinks by 1,256 bytes, with neutral aggregate runtime and compile cost.
`measure_ir_counts.py` accepts `--baseline-compiler` for version comparisons and
verifies each binary and source hash against the report before adding counts.

The explicit canonicalization stage comparison
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

The expanded-scalar ordering comparison
uses seven paired runtime and five paired compile samples. Moving CFG cleanup
before operand normalization preserves all 44 IR and assembly files and reduces
compile time 2.83%; Basic and Full adopt that order. Extra late cleanup and the
`keel-O-facts` recipe remain opt-in because they increase code size and compile
cost without an established runtime gain. Static-only second/third rounds test
corpus convergence; their runtime has not been measured.

The final 15-pair before/after comparison
and Full/facts versus None comparison
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

The completed AArch64 migration
(raw data) reaches
85,988/85,988 native Lua instructions with zero remaining text bytes. Current
codegen plus object generation takes 39.92 ms versus 67.69 ms through text;
full CLI batches take 217.87 ms versus 322.61 ms. These are separate timing
boundaries. The previous direct API takes 229.21 ms in the same CLI run; the
median paired ratio corresponds to 5.2% less time after completing the migration.
All 33 objects match all four paths, and the linked interpreter passes Lua's
portable tests. The earlier partial API report
remains available for historical context.

## Native x86-64 Linux bring-up

The initial NUC report (raw logs) contains the
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

The allocator validation
adds native Fast/Full coverage, including optimized/unoptimized Lua with
baseline/full selection, and retained GDB sessions against a Clang reference.
The report includes the temporary Fast configuration definitions. To repeat
the Linux debugger checks after building a native release compiler:

```sh
python3 suite/validate_linux_debugger.py --compiler target/release/keel \
  --work-dir target/linux-debugger --output target/linux-debugger/report.json
```

This requires GDB, Clang and `llvm-dwarfdump`. It checks types, bitfields,
pointer chains, TLS, scopes, live variable mutation, and resulting execution
for both allocators with and without optimization. The existing macOS
`debugger` and `debug_info` tests now cover both allocators as well.

## Full macOS and Linux measurements

The platform benchmark report
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

The compilation-speed analysis
(aggregate data,
macOS CLI samples,
Linux CLI samples) separates
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
SQLite CPU sample
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

The 15 October baseline report
(raw data) uses local main
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

The optimized preprocessor report
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

The source/build details
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

The allocation data show
76–77% fewer allocation/reallocation calls and 53–55% fewer cumulative requested
bytes. Requested bytes include full reallocation sizes and are not peak live
memory or RSS. Counting binaries are never used for timing.

## Arena and header-source cache comparison

The arena/cache report
(raw timing,
storage measurements)
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
build details:

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

Paired measurements compare frozen
release binaries with identical build flags. Variants are cumulative: `buffer`,
`paths`, `macros`, then `all` (validated string scanning). Nine pairs per
application show an equal-application geometric mean time reduction of 8.6%
(1.094x speedup; bootstrap 95% reduction interval 5.5–8.9%). Median application
latencies are Lua 223.66 → 205.89 ms, QuickJS 117.05 → 109.05 ms, and SQLite
93.39 → 83.66 ms. Lua's paired interval alone crosses no change, so its gain
remains less certain. These are warm-cache measurements on one Apple M5 Max.
Most incremental benefit comes from string slicing; smaller steps are sensitive
to noise and their contribution depends on order.

Allocation measurements
show 15.0%, 19.7%, and 24.1% fewer allocation/reallocation calls for Lua,
QuickJS, and SQLite. Retaining pending-vector capacity has a memory tradeoff:
single counting-process peak RSS changes from 43.69 → 51.84 MiB for Lua,
123.61 → 126.97 MiB for QuickJS, and 298.91 → 294.66 MiB for SQLite. These
full-process observations include warmups and are not isolated-call peaks.

All 65 frontend/driver tests pass, including a new Unicode/splicing regression.
All five variants produce byte-identical outputs for the 44 corpus files and
verify their generated IR. The rebuilt release driver's 44 CLI outputs also
match. Build details and incremental patches
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

Interactive report and
raw timings, stack summaries, and provenance
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

Saved report,
paired timings,
allocation/RSS measurements,
and refreshed stack profiles
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
hashes are in the build details.

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

Interactive report,
raw stage measurements,
stack and ownership analysis,
and validation / callsite evidence
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


### Scanner classification and directory canonicalization (2026-10-15)

The next pass from `2808137` uses a 256-byte classification table in the
identifier and preprocessing-number scanner loops, and caches canonical parent
directories within each translation unit. The final component is checked with
`symlink_metadata`: ordinary files can append their name to the resolved parent;
symlinks, missing paths and non-regular files retain full canonicalization and
its fallback. Parent paths are resolved by the filesystem, preserving symlink
and `..` semantics. Include spelling and token locations remain unchanged.

The 15-pair comparison
against the prior pass's frozen `deeper/profile-all` binary shows **10.9% less
preprocessing time** (1.122x speedup; 95% paired bootstrap reduction interval
10.5–11.3%). Median application times are Lua 170.95 → 145.57 ms, QuickJS
84.48 → 75.01 ms and SQLite 73.98 → 69.09 ms. These are warm-cache, serial
library measurements on the same 44 TUs and Apple M5 Max; the ratio uses paired
samples rather than ratios of these independent medians. A local rebuild of
the exact parent source is neutral against the frozen binary (aggregate ratio
0.9999; 95% interval 0.9971–1.0042). All binaries use identical release flags
with `CARGO_PROFILE_RELEASE_DEBUG=1` and the `profile` feature.

The seven-pair screen
shows independent reductions of 7.1% for directory caching and 4.0% for byte
classification. Specializing the scanner for spliced/unspliced location updates
adds only a small incremental benefit: the final direct paired ratio versus
the selected candidate is 0.9946 (95% interval 0.9846–0.9999), while growing the
page-aligned Mach-O text segment by 16 KiB. That specialization was not retained;
the existing location implementation stays intact. Exact variant patches,
build logs, source/binary hashes, test logs and the incremental calculation are
in the investigation details.

Storage measurements
show allocation/reallocation calls increasing 1.26%, 1.08% and 0.59% for Lua,
QuickJS and SQLite, with requested bytes increasing 0.038%, 0.028% and 0.011%.
The extra directory map lasts only for the current invocation. Single-process
peak RSS observations are 59.77 → 42.17 MiB, 147.05 → 150.94 MiB and
311.02 → 302.75 MiB; these include warmups and allocator history and do not
establish an isolated-call memory reduction. All profile counters, including
file opens, source/token bytes and cache hits, match the baseline exactly.
Remaining opens are for unique headers within each TU; no cross-TU cache was
introduced.

All 61 frontend tests and 24 driver tests pass. The new regression combines
parent-directory symlinks, final-component symlinks, `..`, pragma once and
`__FILE__` spelling. Every screen/final variant produces byte-identical output
for all 44 TUs and verifies the generated IR. Builds/tests started only after
the parent's shared-host timing gate opened, and timing and storage runs were
separate and serial.

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile \
  --example profile_preprocess --example profile_preprocess_allocations
# Freeze both examples before timing; use the exact variant patches in details.
python3 suite/measure_preprocessor.py \
  --profile suite/reports/c-frontend-profile-20261015.json \
  --baseline /path/to/frozen/deeper/profile-all \
  --variant rebuilt=target/frontend-profile/scanner-paths/profile-baseline \
  --variant final=target/frontend-profile/scanner-paths/profile-final \
  --variant location=target/frontend-profile/scanner-paths/profile-all \
  --rounds 15 --work-dir target/frontend-profile/scanner-paths/final-paired \
  --output suite/reports/c-preprocessor-scanner-paths-20261015.json
```


### Lexer rewrite, arena/interner, cleanup, and SSA investigation (2026-10-15)

Interactive report,
paired measurements,
storage measurements,
production and outlined stacks,
SSA visit counters, and
build provenance / validation
compare the frontend after commit `24ca853` with the rewrite. These are resident
preprocessed-C-to-IR measurements, excluding preprocessing and the backend.

The lexer dispatches punctuation by bounded leading bytes instead of searching
48 strings, scans identifiers with byte classification, borrows line-marker
text, and caches filename indices. Narrow literals copy SIMD-searched spans;
wide literals validate/decode only the next scalar. Numeric decoding folds at
most three suffix bytes on the stack rather than allocating a lowercase copy.
Longest match, source locations, escapes, signed/multichar constants, wide
literals, number classification, and pack state remain compatible.

The frontend now has a compilation-scoped Bump byte arena and separate intern
sets for UTF-8 names and arbitrary literal bytes. Lexer tokens are Copy values.
Names in scopes, declarators, typed trees, type definitions, enum values, labels,
and filenames borrow stable arena storage. Literal-ID lookup includes width;
string initializer bytes borrow slices rather than cloning owned vectors.
The typed tree and tables drop before the arena; output IR and debug metadata
own the strings/bytes they need after compile returns. No unsafe storage or
lifetime conversion was added. AST Box/Rc/Vec ownership remains ordinary.

Nine paired rounds across all three applications show 30.2% less normal frontend
time (1.434x speedup; 95% paired bootstrap reduction interval 28.9–31.1%). Debug
mode shows 31.3% less time (1.455x). Normal public compile medians are Lua
75.80 → 47.82 ms, QuickJS 165.81 → 115.82 ms, and SQLite 224.09 → 174.80 ms.
Full-variant lexing is 2.2–2.8x faster, parsing takes about 26–34% less time,
and cleanup takes 31–49% less time. Stage reductions compare stage medians;
headline ratios use equal-application geometric means of paired public calls.
The lexer-only variant shows the byte-path contribution separately. Interning
adds some lexer work on Lua/SQLite, repaid by parser/cleanup savings.

Normal allocation/reallocation calls fall 45–65%, and requested allocation bytes
fall 7–10%. Single counting-process RSS observations are Lua 29.70 → 28.50 MiB,
QuickJS 154.09 → 147.36 MiB, and SQLite 221.31 → 219.25 MiB. These observations
include resident inputs and warmups, not isolated-call peaks. Allocation
instrumentation is separate from latency measurement.

The SSA investigation adds diagnostic-only counters and a separate outlined
`find_preds` build; neither modification is present in production source.
Lua builds 9,958 blocks but visits 716,104 block entries while finding incoming
edges; QuickJS builds 36,898 and visits 2,778,251; SQLite builds 56,352 and visits
8,134,177. SQLite's largest functions account for 2.59M (`sqlite3VdbeExec`) and
2.24M (`do_meta_command`) visits. The outlined scan accounts for about 7.4% of
QuickJS and 14.2% of SQLite total samples after the frontend improvements.
The next targeted design is to maintain incoming edges as branches are emitted,
with correct handling of the builder's exposed mutable IR. A blind cache would
be invalid while control flow is still growing; SSA semantics remain unchanged
in this pass. Remaining cleanup is mainly typed-tree/type ownership.

All 77 frontend, driver, and SSA-builder tests pass, including executable C
fixtures. New coverage checks longest match over punctuation combinations and
buffer boundaries, literal Unicode/width/escape behavior, numeric compatibility
with the previous decoder, and interned reference stability through arena/table
growth. All 44 normal and 44 debug-mode IR snapshots are byte-identical across
baseline, lexer-only, and full variants (264 snapshots); all generated IR verifies.
The 324 timing batches retain 4,752 individual measured file calls.

Build all variants before timing; the builder temporarily switches source and
restores it in finally. The source-only check verifies the replay transformations.
The recorded lexer-base patch reconstructs the first lexer step from `24ca853`.

```sh
python3 suite/build_frontend_variants.py --base 24ca853 --check-sources
python3 suite/build_frontend_variants.py --base 24ca853
python3 suite/measure_c_frontend.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --variant baseline=target/frontend-profile/frontend-opt/profile-baseline \
  --variant lexer=target/frontend-profile/frontend-opt/profile-lexer \
  --variant arena=target/frontend-profile/frontend-opt/profile-arena \
  --rounds 9 --work-dir target/frontend-profile/frontend-opt/paired \
  --output suite/reports/c-frontend-optimized-20261015.json
python3 suite/measure_frontend_storage.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --baseline target/frontend-profile/frontend-opt/allocations-baseline \
  --candidate target/frontend-profile/frontend-opt/allocations-arena \
  --output suite/reports/c-frontend-optimized-storage-20261015.json
python3 suite/investigate_frontend_ssa.py
# Restore the normal example after the diagnostic build; sources are already restored.
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile --example profile_allocations
python3 suite/capture_frontend_stacks.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --binary target/frontend-profile/frontend-opt/profile-arena \
  --outlined target/frontend-profile/frontend-opt/profile-ssa-outline \
  --output suite/reports/c-frontend-optimized-stacks-20261015.json
python3 suite/render_frontend_optimization.py
```

### Incremental SSA predecessor index (2026-10-15)

The next comparison uses `7528fb6` (the lexer and arena/interner commit) as
baseline. `FunctionBuilder` now records incoming edges as source blocks finish
construction. Sealing takes a destination's bucket and sorts by source layout
and destination index, preserving the old SSA/IR order. Normal construction
avoids a full CFG scan for each sealed block. Appending after an already indexed
branch falls back to rebuilding so the original last-instruction behavior is
preserved even during temporarily invalid construction.

The public `FunctionBuilder::func` field is replaced by `function()` for shared
access and `function_mut()` for raw edits. Mutable access invalidates the index,
which rebuilds before the next seal. As before, a sealed block's predecessor set
must remain fixed. Use builder helpers for block parameters, stack slots, and
imports; these do not change CFG destinations and do not invalidate the index.
The C frontend and SSA tests use the new API. No `keel-ir` representation or
SSA variable/parameter semantics change.

All 81 C frontend, driver, and SSA tests pass. New regression coverage compares
the index to the previous full-scan algorithm through raw destination rewrites,
removal, movement, block removal, duplicate edges, out-of-order source visits,
and function replacement. SSA-level tests check duplicate destinations with
explicit arguments and raw destination redirection before sealing.

Build/freeze `profile` and `profile_allocations` at `7528fb6` in a separate
checkout for the baseline, then build/freeze the same examples from the new
sources. Both builds use `CARGO_PROFILE_RELEASE_DEBUG=1` and
`cargo build --release -p keel-c --features profile --example profile --example profile_allocations`.
The details report records the source and binary hashes. The historical lexer
variant builder and scan-counter investigation above describe the sources at
`7528fb6`; run those reproductions at that commit.

```sh
python3 suite/measure_c_frontend.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --variant baseline=target/frontend-profile/ssa-index/profile-baseline \
  --variant indexed=target/frontend-profile/ssa-index/profile-indexed \
  --rounds 9 --work-dir target/frontend-profile/ssa-index/final-paired \
  --output suite/reports/c-frontend-ssa-index-20261015.json
python3 suite/measure_frontend_storage.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --baseline target/frontend-profile/frontend-opt/allocations-arena \
  --candidate target/frontend-profile/ssa-index/allocations-indexed \
  --candidate-name indexed \
  --output suite/reports/c-frontend-ssa-index-storage-20261015.json
python3 suite/capture_frontend_stacks.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --binary target/frontend-profile/ssa-index/profile-indexed \
  --output suite/reports/c-frontend-ssa-index-stacks-20261015.json
python3 suite/render_ssa_index.py \
  --profile suite/reports/c-frontend-ssa-index-20261015.json \
  --storage suite/reports/c-frontend-ssa-index-storage-20261015.json \
  --output suite/reports/c-frontend-ssa-index-20261015.html
```

The nine-round paired result is **8.4% less normal-mode frontend time**
(equal-application geometric mean; 95% paired bootstrap interval 7.8–8.7%)
and **8.6% less time with debug info** (8.4–9.0%). Normal compile medians are
Lua 47.38 → 45.66 ms, QuickJS 114.24 → 106.47 ms, and SQLite 173.99 → 148.66 ms.
Normal IR-generation medians are 12.52 → 10.68 ms, 50.26 → 42.77 ms, and
97.27 → 72.83 ms, respectively: 18.4% less IR-generation time across applications
and 25.1% less for SQLite. These are comparisons to the already optimized
frontend, not the original pre-lexer baseline. Per-application headline ratios
use medians of paired ratios; they need not equal the ratio of marginal medians.
The report retains 216 batches / 3,168 measured file calls. All 176 normal/debug
IR snapshots match byte-for-byte across the two variants.

The incoming-edge index adds around 1% allocation calls. Allocation counters
and single full-process RSS observations are recorded separately in the storage
report; they are excluded from latency measurement. Four new six-second stack
captures cover all three applications plus SQLite with debug info. Raw profiles,
summary data, source/binary hashes, and a standalone interactive HTML report
are retained under `suite/reports/c-frontend-ssa-index-*`.

### Compact SSA variable definitions (2026-10-15)

This pass starts at `f39567e`, after incoming-edge indexing. The diagnostic
`suite/investigate_ssa_definitions.py` inspects the old definition table before
final trivial-parameter cleanup. Across whole applications, Lua stores 36,187
(variable, block) definitions, QuickJS 162,649, and SQLite 363,887. A full dense
variable-by-block matrix would contain 588,729, 604,744, and 2,249,605 cells,
respectively. Most blocks use few variables: all Lua/QuickJS blocks and all but
522 SQLite blocks have at most 32 definitions. The diagnostic report retains
occupancy histograms, largest functions, source instrumentation, and hashes;
the diagnostic binary is excluded from latency measurements.

`FunctionBuilder` now indexes sealed flags, incomplete parameters, and sealed
predecessor lists directly by block. Variable definitions stay sparse within a
block: four entries fit inline in `SmallVec`, sorted lists spill as needed up to
32 entries, and wider blocks promote to a `HashMap<Variable, Value>` to bound
reverse-insertion costs. This removes pair hashing from the common read path
without allocating a variable-by-block matrix. `smallvec` was already present
in the lockfile at 1.16.3; it is now a direct frontend dependency. No new API or
SSA semantic changes are introduced. Alias resolution and recursive predecessor
reads retain their behavior. Parameter argument construction copies one fixed
predecessor edge at a time, avoiding a cloned edge vector for every parameter.

All 83 C frontend, driver, and SSA tests pass. New coverage checks sparse IDs,
repeated definitions, inline growth, map promotion, and unregistered raw block
gaps, alongside the existing loop, alias/trivial-parameter, and raw CFG tests.

Build/freeze baseline examples at `f39567e` in a separate checkout, then build
current examples with the same release/debug-symbol settings used above.
The details report records source, manifest, lockfile, and binary hashes.

```sh
python3 suite/investigate_ssa_definitions.py \
  --base f39567e --profile suite/reports/c-frontend-stages-20261015.json \
  --work-dir target/frontend-profile/ssa-reads \
  --output suite/reports/c-frontend-ssa-definitions-20261015.json
# The diagnostic restores source; rebuild ordinary examples before freezing/timing.
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile --example profile_allocations
python3 suite/measure_c_frontend.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --variant baseline=target/frontend-profile/ssa-reads/profile-baseline \
  --variant compact=target/frontend-profile/ssa-reads/profile-compact \
  --rounds 18 --work-dir target/frontend-profile/ssa-reads/paired \
  --output suite/reports/c-frontend-ssa-reads-20261015.json
python3 suite/measure_frontend_storage.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --baseline target/frontend-profile/ssa-index/allocations-indexed \
  --candidate target/frontend-profile/ssa-reads/allocations-compact \
  --candidate-name compact \
  --output suite/reports/c-frontend-ssa-reads-storage-20261015.json
python3 suite/capture_frontend_stacks.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --binary target/frontend-profile/ssa-reads/profile-compact \
  --output suite/reports/c-frontend-ssa-reads-stacks-20261015.json
python3 suite/render_ssa_index.py --kind reads \
  --profile suite/reports/c-frontend-ssa-reads-20261015.json \
  --storage suite/reports/c-frontend-ssa-reads-storage-20261015.json \
  --output suite/reports/c-frontend-ssa-reads-20261015.html
```

The paired timing run was extended from nine to eighteen rounds because large
latency spikes in both variants widened the initial confidence interval.
`measure_c_frontend.py --resume --rounds 18` verifies the same binaries, inputs,
C sources, commit, and complete prior rounds before appending work. Every initial
sample is retained; the final report records both measurement sessions. There
is no outlier filtering. Timings, allocation counters, and stack sampling run
serially in separate phases.

The final eighteen-pair result is **6.5% less normal-mode frontend time**
(equal-application geometric mean; 95% paired bootstrap interval 6.3–6.8%).
Normal compile medians are Lua 44.90 → 43.03 ms, QuickJS 104.89 → 97.93 ms, and
SQLite 146.46 → 133.24 ms. IR-generation medians are 10.47 → 8.92 ms,
41.87 → 35.06 ms, and 71.62 → 58.13 ms: **16.6% less IR-generation time**
across applications, up to 18.8% for SQLite. With debug info, total frontend time
falls 0.9% (95% interval 0.5–1.2%) and IR-generation time falls 3.5%. That path
uses stack slots for locals instead of SSA variables (`irgen.rs` local storage
selection), so the opportunity from definition-table changes is smaller.
Normal allocation/reallocation calls fall 2.8–4.8%. RSS observations are roughly
flat; the report makes no general peak-memory reduction claim.

All 176 cross-variant normal/debug IR snapshots are byte-identical and verified.
The final report retains all 432 batches / 6,336 measured file calls, including
the first nine rounds. Storage instrumentation and four six-second stack
captures run separately from latency measurement. Saved timing, memory, stack,
provenance, and interactive HTML reports are `c-frontend-ssa-reads-*`.

### Parser lookup and type-node reuse (2026-10-15)

This comparison starts from `b961ed0`, after compact SSA definitions. Ordinary
identifier lookup previously hashed a spelling once per enclosing scope. The
parser now performs one hash lookup into a visible-binding table, followed by
the last entry of a short per-name scope stack. A per-scope name list restores
bindings on exit. File-scope bindings introduced while local shadows exist are
inserted beneath those shadows, preserving the previous lookup/merge behavior.
Repeated declarations in one scope update their existing entry.

Type and reserved-keyword membership now uses compiler-generated string matches
instead of linear spelling lists. Builtin/typedef-name exceptions are preserved
and checked against the old membership rule. Immutable scalar and struct type
leaves are shared through a compilation-local cache; the plain qualifier node
is shared too. Pointer, array, and atomic constructors reuse these `Rc` nodes.
Composite types retain their existing representation and ownership. A struct
leaf still identifies the struct table by index, so completing a forward tag
updates the same semantic type. This introduces no public API or handwritten
unsafe code. `smallvec` was already locked at 1.16.3 and is now a direct keel-c
dependency for short scope stacks.

All 87 C frontend, driver, and SSA tests pass. New checks cover keyword spelling
membership and exceptions, differential scope-stack lookup against the old
scope search, deep shadows that spill inline stacks, an inner extern restoring
an outer local and file binding, and distinct shared scalar/struct nodes.
Existing qualifier-depth, volatile/atomic, ABI, debug-info, executable C fixture,
and preprocessing tests pass. All 176 normal/debug cross-variant IR snapshots
match byte-for-byte and verify, including public versus staged compilation.

Build/freeze the baseline examples at `b961ed0` in a separate checkout, then
build/freeze the current examples with the same release/debug-symbol settings.
Source, manifest, lockfile, and binary hashes are recorded in the details report.

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile --example profile_allocations
python3 suite/measure_c_frontend.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --variant baseline=target/frontend-profile/parser/profile-baseline \
  --variant parser=target/frontend-profile/parser/profile-parser \
  --rounds 13 --work-dir target/frontend-profile/parser/paired \
  --output suite/reports/c-frontend-parser-20261015.json
python3 suite/measure_frontend_storage.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --baseline target/frontend-profile/parser/allocations-baseline \
  --candidate target/frontend-profile/parser/allocations-parser \
  --candidate-name parser \
  --output suite/reports/c-frontend-parser-storage-20261015.json
python3 suite/capture_frontend_stacks.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --binary target/frontend-profile/parser/profile-parser \
  --output suite/reports/c-frontend-parser-stacks-20261015.json
python3 suite/render_ssa_index.py --kind parser \
  --profile suite/reports/c-frontend-parser-20261015.json \
  --storage suite/reports/c-frontend-parser-storage-20261015.json \
  --output suite/reports/c-frontend-parser-20261015.html
```

The thirteen-pair result is **4.4% less normal-mode frontend time**
(equal-application geometric mean; 95% paired bootstrap interval 3.8–5.1%)
and **4.8% less time with debug info** (4.4–5.3%). Normal compile medians are
Lua 43.97 → 41.47 ms, QuickJS 101.00 → 97.58 ms, and SQLite 137.09 → 130.96 ms.
Normal parse/type-checking medians are 19.18 → 17.12 ms, 39.40 → 35.76 ms, and
44.67 → 40.05 ms: **10.1% less parse/type-checking time** across applications.
The debug-mode parse/type reduction is 10.0%. Normal cleanup medians are
2.19 → 1.99 ms, 4.83 → 4.59 ms, and 7.43 → 7.00 ms (about 5–9% less).
There are 312 timing batches / 4,576 measured file calls. Stage reductions
compare staged medians, while headline ratios use paired public compilation.
All thirteen rounds are retained. This compares to the already optimized
frontend at `b961ed0`, not the original pre-lexer baseline. Allocation counters
and four six-second stack captures run separately from latency measurements.

Normal allocation/reallocation calls fall 3.0–7.8% (2.6–6.5% with debug info).
The visible-binding table trades some storage for fewer lookup probes: requested
allocation bytes rise 1.2% on Lua, are nearly unchanged on QuickJS, and fall
0.3% on SQLite in normal mode. Peak RSS observations are included in the report
as single full counting-process measurements, including resident inputs and
warmups; they do not establish isolated-call peak memory. Timing and counter
builds retain the same verified IR outputs.

Fresh samples identify the next focused candidate in IR generation:
`Gen::preserve_access_alignment` linearly searches all globals for every used
data symbol, followed by a possible module-data search. Its iterator/global-scan
frame accounts for about 1.7% of QuickJS and 3.6% of SQLite self samples. A symbol
alignment index could remove repeated searches while retaining conservative
alignment guarantees. This parser pass does not change alignment semantics.

### Indexed IR access alignment (2026-10-15)

This comparison starts at `308d0be`, after parser lookup/type-node reuse.
`Gen::preserve_access_alignment` previously scanned all parsed globals for each
function data symbol, followed by a possible linear module-data search.
A lazy per-compilation index now borrows declaration symbol spellings and keeps
the first matching declaration, including aliases. The index is built only
when a symbol alignment is queried. Generated literal and initializer objects
update a separate index as they are emitted; first-module-object precedence,
declaration precedence, and unknown-symbol alignment 1 are preserved.

Per-function stack-slot and data-symbol alignment tables now use vectors:
`StackSlot` and `DataRef` are append-only dense arena IDs. The sparse cache of
proven value alignments, address arithmetic, recursion bound, packed-access
rules, and existing alignment guarantees keep their prior behavior. There are
no API, dependency, or handwritten unsafe changes.

All 89 C frontend, driver, and SSA tests pass. New regression coverage compares
the index to the old lookup through aliased declarations, declaration/generated
name collisions, late narrow/wide literals, deduplicated initializer bytes,
first-object precedence (including no initializer), and unknown symbols. Dense
ID tests verify multiple slots/imports and conservative offset alignment.
Existing packed/over-aligned, volatile/atomic, debug, ABI, and executable C
fixtures pass. The comparison checks exact normal/debug IR equivalence.

Build/freeze the baseline examples at `308d0be` in a separate checkout, then
build/freeze current examples with the same release/debug-symbol settings.
The details report records source, manifest, lockfile, and binary hashes.

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p keel-c --features profile --example profile --example profile_allocations
python3 suite/measure_c_frontend.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --variant baseline=target/frontend-profile/alignment/profile-baseline \
  --variant alignment=target/frontend-profile/alignment/profile-alignment \
  --rounds 13 --work-dir target/frontend-profile/alignment/paired \
  --output suite/reports/c-frontend-alignment-20261015.json
python3 suite/measure_frontend_storage.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --baseline target/frontend-profile/alignment/allocations-baseline \
  --candidate target/frontend-profile/alignment/allocations-alignment \
  --candidate-name alignment \
  --output suite/reports/c-frontend-alignment-storage-20261015.json
python3 suite/capture_frontend_stacks.py \
  --profile suite/reports/c-frontend-stages-20261015.json \
  --binary target/frontend-profile/alignment/profile-alignment \
  --output suite/reports/c-frontend-alignment-stacks-20261015.json
python3 suite/render_ssa_index.py --kind alignment \
  --profile suite/reports/c-frontend-alignment-20261015.json \
  --storage suite/reports/c-frontend-alignment-storage-20261015.json \
  --output suite/reports/c-frontend-alignment-20261015.html
```

The thirteen-pair result is **2.1% less normal-mode frontend time**
(equal-application geometric mean; 95% paired bootstrap interval 1.8–2.5%)
and **2.8% less time with debug info** (2.3–3.2%). Normal public compile medians
are Lua 40.69 → 40.74 ms, QuickJS 94.58 → 92.76 ms, and SQLite 128.10 → 121.92 ms.
Paired time reductions are -0.1%, 1.8%, and 4.5%, respectively; Lua's interval
includes no change. Debug-mode reductions are 0.1%, 2.5%, and 5.5%, with Lua again
within timing uncertainty. Per-application paired ratios need not equal the ratio
of marginal medians.

Normal IR-generation medians are Lua 8.70 → 8.73 ms, QuickJS 35.06 → 32.70 ms,
and SQLite 57.96 → 52.74 ms: **5.2% less IR-generation time** across applications.
Debug-mode IR-generation time falls 7.0%. Lua is effectively flat; declaration-index construction may offset lookup
savings in its smaller translation units. The benefit is strongest where many
functions repeatedly reference global data.

All 176 normal/debug cross-variant IR snapshots match byte-for-byte and verify.
The report retains all 312 timing batches / 4,576 measured file calls.
Allocation counters and four six-second stack captures run separately from
latency measurement. This comparison uses the already optimized frontend at
`308d0be` as baseline, not the original pre-lexer implementation.

Normal allocation/reallocation calls rise by 0.12–0.22%, and requested allocation
bytes rise by 0.07–0.25%, paying for compilation-level lookup tables and generated
symbol keys. Debug-mode changes are similarly small. Peak RSS observations are
reported as single full counting-process measurements including resident inputs
and warmup; they do not establish isolated-call peak memory. Source, binary, and
sample hashes are rechecked before committing the reports.

## Application loop profiles (2026-10-16)

`profile_app_loops.py` profiles the optimized Lua, QuickJS, and SQLite workloads.
It prints optimized IR and adds line tables while compiling that IR, then checks
exact object and linked `.text` equality against ordinary `keel cc -O`. It does
not use frontend `-g`, which would keep locals in memory and change the workload.
Clang builds provide output references and retained vectorization remarks.

```sh
python3 suite/profile_app_loops.py \
  --compiler target/release/keel --work-dir target/app-loop-profile \
  --seconds 6 --rounds 2
python3 suite/probe_app_inline.py \
  --compiler target/release/keel --profile target/app-loop-profile/profile.json \
  --work-dir target/app-loop-inline-probe
python3 suite/probe_app_divide.py \
  --compiler target/release/keel --profile target/app-loop-profile/profile.json \
  --work-dir target/app-loop-divide-probe
```

On macOS the profiler uses `sample`; on the NUC it uses task-scoped
`sudo -n perf record` with `--event cpu_core/cycles/u --cpu 0`. Choose a suitable
event/core on other Linux CPUs. It changes no perf access settings. Use
`--variants keel clang` to profile both compilers, `--build-only` to retain builds
without sampling, and `--reuse-builds --suites <names>` to profile retained builds.
All repeated workload outputs must match Clang. Single-run durations calibrate
repetitions and are not comparative latency results. Wall samples and weighted
user cycles have different denominators.

The two probes are diagnostic application variants, not compiler features.
The QuickJS probe expands three sampled helpers in one source file and runs all
nine suite tests in both variants. The SQLite probe replaces only two constant
division/remainder instructions in `sqlite3FpDecode`, validates native arithmetic
against a variable-divisor C reference on 100,011 inputs, and checks SQL outputs.
Both use two warmups and eleven alternating benchmark pairs, retaining raw
samples, binary/input hashes, and paired bootstrap intervals.

Results and ranked patterns are in
`reports/app-hot-loops-20261016.json`.
Forty-eight native captures cover MacHost and the NUC; LinuxHost additionally runs both
probes and machine-code parity checks. The helper probe reduces QuickJS benchmark
time by 12–19% on MacHost, 20–48% on LinuxHost, and 4–13% on the NUC. A bounded inliner
with aggregate-copy cleanup was therefore the strongest measured next feature
at that time. It is now implemented as described below; those probe timings
remain historical and do not measure the later attribute integration.
SQLite constant-division results are smaller (roughly 1–3%, with MacHost's interval
including no change). The vectorizer-specific candidates are bounded byte scans
with early exits and short pointer/aggregate copies. Their application benefit
has not been established; many token runs in these fixtures are only 1–9 bytes.

### Clang comparison and proposed next step

The completed three-host comparison is retained in
`reports/clang-vectorization-comparison-20261016.json`.
Clang already enables loop and SLP vectorization at `-O2`; driver checks and
successful optimization remarks confirm this for the installed compilers.
The comparison also disables both vectorizers and adds native CPU targeting
(`-mcpu=native` on MacHost, `-march=native` on x86), independently. All variants
use `-ffp-contract=off`, identical prepared inputs, and no LTO/PGO/fast-math.
Native targeting therefore measures code generation and tuning after
preprocessing; it does not enable CPU-specific source branches. Disabling the
two vectorizers does not disable explicit SIMD or SIMD in system libraries.

Each workload has two warmups and twelve measured rounds with rotated/reversed
configuration order, raw times, and paired bootstrap intervals. Application
stdout matches across every variant; SQLite's query fixture also matches.
Arithmetic maps use the self-checking harness's inner elapsed time. Linux
measurements pin CPU 0. The JSON retains both measurement script revisions:
application measurements completed before correcting a duration/stdout comparison
in the kernel harness runner; only the kernel stage was resumed.

The MacHost arithmetic maps take about 1.04–1.05 times Clang O2's time. The x86
maps take about 1.9–2.3 times as long. Application parity has not been reached:
Lua takes about 1.5–2.3 times Clang O2's time, unmodified QuickJS 2.6–8.5 times,
and SQLite 1.8–2.1 times. These ranges use ratios of absolute medians; the report
also retains same-round ratios and intervals. Compare compiler variants within
each host, since the installed Clang versions differ.

The next feature proposed at the time of this comparison was a bounded
direct-call inliner, followed by cleanup of temporary aggregate copies and
ordinary per-function optimization. Start
with small same-module helpers, reject recursion and unsupported call semantics,
and cap total code growth. Clone callee blocks, values and stack slots; map
arguments to parameters and route returns to a continuation. Preserve memory
and call contracts. Aggregate cleanup must respect aliasing and escaping
addresses. The QuickJS source-expansion probe motivates this work, but does
not predict the gain from an IR implementation. The proposal had not been
implemented at that stage. The compiler implementation and its separate
measurements are described in the following section.

To reproduce using retained profiles and diagnostic probe builds:

```sh
python3 suite/compare_app_clang.py \
  --compiler target/release/keel --profile target/app-loop-profile/profile.json \
  --inline-probe target/app-loop-inline-probe/probe.json \
  --divide-probe target/app-loop-divide-probe/probe.json \
  --work-dir target/clang-comparison
```

## Compiler inlining and bounded scans (2026-10-16)

Full optimization now runs a bounded module-level `inline` pass and
`stack-forward` cleanup. Full also expands ordinary constant memory intrinsics
of at most 32 bytes immediately before `stack-forward`; Basic and unoptimized
pipelines omit this pass. See [small-memory](../docs/small-memory.md).
The inliner clones small private direct-call functions,
preserving SSA, control flow, stack objects, aggregate argument snapshots,
return values, and lexical debug scopes. Its original ordinary limits were
64 instructions, eight blocks and 512 bytes of fixed stack storage, with
128 expanded sites and
2,048 added IR instructions per caller. The current
[function attribute contract](../docs/function-attributes.md#inliner-policy-and-hard-boundaries)
adds hint/always/noinline policies and module/stack safeguards. Newly cloned
calls wait until a later explicit inlining invocation. Recursion, variadic
definitions, dynamic frame operations, exception handling and returns-twice
semantics are excluded.
Public definitions remain calls. At the time of these measurements,
`always_inline`/`noinline` were not represented; they are now transported with
`inlinehint`, `noreturn` and `nounwind`, including C `nothrow`, under the
[ownership, merging and emission rules](../docs/function-attributes.md#c-declarations-and-contracts).
`alwaysinline` acts only when the pass runs, with legality and hard limits intact;
`inline_with_report` exposes skipped requests, without normal CLI warnings.
Basic and unoptimized tiers retain their existing pipelines. `nounwind` remains
promise metadata without C++ termination or EH rewriting. General assume and
distinct unreachable remain deferred; `__builtin_unreachable` still traps.

`stack-forward` reconstructs ordinary integer loads from prior writes to
nonescaping fixed stack objects within a block, including overlapping writes
to aggregate/union temporaries. Unknown bytes, escaping objects and volatile or
atomic accesses remain conservative. Debugger-visible homes retain both loads
and stores so debugger edits continue to take effect with `-g -O`.

The existing `loop-vectorize` pass additionally recognizes bounded ASCII prefix
scans such as QuickJS's original `JS_NewStringLen` loop. It peels scalar bytes to
sixteen-byte alignment, then tests chunks only within the bound. Aligned loads
cannot cross a protection granule on these targets, even when an early-exit scan
has a bound beyond an inaccessible page. Chunk failures and tails resume the
original scalar loop to produce the exact exit pointer. Both backends lower
the required `i64x2` lane extraction natively. Short runs and early failures
can pay extra guard overhead; microbenchmark improvements do not establish
whole-application benefit.

The compiler-only matrix uses original prepared inputs throughout:

```sh
python3 suite/bench_profile_guided.py \
  --compiler target/release/keel \
  --work-dir target/profile-guided-benchmark
```

It builds four explicit Keel pipelines (base, vectorization only, inlining plus
cleanup only, and both), plus Clang `-O2 -ffp-contract=off`. All configurations
run Lua's portable suite, all nine QuickJS tests, and SQLite's query checks
before timing. Two warmups and seven measured rounds retain output checks, raw
times, paired bootstrap intervals, code sizes, source/binary hashes, remaining
helper calls, and scan recognition. Linux timings pin CPU 0. The scan control
separately measures empty/short/full ASCII inputs and early exits. No application
source expansion or decimal-division rewrite is used.

The three-host results are retained in
`reports/profile-guided-compiler-20261016.json`
and an interactive comparison.
Adding inlining and cleanup with vectorization enabled reduces QuickJS runtime
by 17–26% on MacHost, 30–40% on LinuxHost, and 18–29% on the NUC in this run. QuickJS
native code grows about 9–15%. Long ASCII control scans are about 5–9 times
faster than Keel's scalar control; short runs and early failures regress.
The whole-application scan benefit remains unestablished. Small application
changes should be interpreted with their intervals and code-layout sensitivity,
not extrapolated to general workload improvements. Hosts were not reserved for
exclusive use.

Validation also caught and fixed both a protected-page early-exit case and
debugger edits to forwarded stack homes. The final debugger fix changes debug
slots only; all 44 native application objects per host were recompiled with
the final compiler and matched the measured combined objects byte-for-byte.

Current feature scope and evidence are recorded separately from these timings:
[deterministic indexed GNU/Darwin static archives](../docs/static-archives.md)
(`keel archive`, with the Darwin common-only consuming-linker limitation;
static archives do not create DSOs),
[generic mixed-type overflow through 128 bits](../docs/generic-overflow-builtins.md)
(`_overflow_p` deferred; largest 128-bit multiply assembly-size caveat, no runtime
speedup claim), and [packed AVX arithmetic](../docs/x86-avx-subset.md#packed-arithmetic-extension)
(existing opt-in `--target-features avx`, 128-bit only, no AVX2/AVX-512).
The frontend cloning investigation rejected the
scoped expression change and implemented no gain; initializer incidence motivates
a separate future session. The final portability validation
records the focused ownership reruns and inherited round-four workspace evidence.

## Current application runtime profiles versus stock Clang O2 (2026-10-16)

The current runtime report measures
production-equivalent `6e6d2c2` application binaries, with two independent native
captures of every Lua/QuickJS/SQLite workload for both compilers on MacHost and
LinuxHost. Separate paired latency runs use stock Clang O2, canonical executable
paths, two warmups and eleven alternating pairs. It retains correctness checks,
text parity, matching IR/disassembly, sample-quality limits and five bounded
compiler recommendations. Raw evidence and reproduction
include hash manifests and losslessly compressed captures. Historical inliner
probes above remain historical; no source probes or compiler changes enter this
matrix.

## Targeted native ARM code generation

`suite/arm-codegen/` is a small, download-free AArch64 macOS companion to the
whole-app suite. Lua, QuickJS and SQLite remain the eight whole-app regression
gauges; a kernel result does not establish an application gain. Backend unit
regressions own exhaustive instruction semantics. This suite connects selected
source patterns to native correctness, emitted shapes and optional observations.

```sh
python3 -B -m unittest discover -s suite/arm-codegen -p 'test_*.py'
# Copy/edit config.example.json under target/; supply already-frozen compilers.
python3 -B suite/arm-codegen/run.py --config target/arm-config.json \
  --work-dir target/arm-check-01
# Optional bounded timing: the two named cases plus the call-floor control.
python3 -B suite/arm-codegen/run.py --config target/arm-config.json \
  --work-dir target/arm-timing-01 --benchmark \
  --timing-cases and64,movn-negative --rounds 6 --target-seconds 0.12
python3 -B suite/arm-codegen/run.py \
  --verify-report target/arm-timing-01/report.json
```

`--cases movz` reruns an affected functional fixture plus the control without
repeating a timing campaign. One variant suffices for baseline validation; Clang O2 is always added. Variant
paths, exact source/build-feature identities and flags are explicit. `-O` uses
each binary's actual optimized preset (current Keel includes all selection).
`["-O", "--isel", "none"]` is an optional same-binary ablation, not an old
compiler baseline. `features` requests shape gates for that variant only:
`logical-immediate` checks each simple mask's opcode, width and immediate;
`constant-planner` checks specific MOVZ/MOVN seed fields. Leave baseline features
empty: absence of a new shape is reported separately from semantic PASS. Shared
masks, mixed widths, a nonencodable mask and accumulator pressure are observations,
not blanket instruction-count minimization rules. No full assembly hashes are
compared across compilers. Forbidden patterns are scoped to individual cases.

Clang preprocesses each CASE once. All variants compile those identical bytes.
The Clang-built driver object is compiled once and linked unchanged with each
kernel object, without LTO, volatile payload accesses or compiler-specific hints.
The fixed integer ISA regime is baseline ARMv8-A (Keel's native default and
Clang `-march=armv8-a`); no native CPU tuning or FP work is involved. The driver
uses runtime-generated input and an independent bit-at-a-time logical oracle,
including zero-length, high-bit, all-ones and seeded random inputs. Checksum
failure exits nonzero, including an intentional negative control. Setup, input
allocation/generation, oracle checks and printing are outside CLOCK_MONOTONIC's
kernel-call repeat loop; outputs are consumed after it. Process wall duration
is retained as a diagnostic and bounds the reported inner time.

Every build and correctness check completes before optional timing. Keep the
host free of concurrent compiler builds and heavy benchmarks; normal desktop
noise stays in the records. The runner changes no governor, ASLR or other jobs.
Each case records object text bytes, opcode and materialization counts, decoded
logical immediates and retained object disassembly. Load/backedge evidence is a
necessary work-preservation screen, not a proof: inspect the disassembly before
interpreting any optimization gain. An eliminated payload is explicitly invalid
for its intended measurement, not a compiler correctness failure. A compiler may
legitimately find a better transform. A missing expected shape need not mean the
work vanished; the two checks remain separate.

Calibration targets 100–250 ms for the slowest variant, using one common repeat
count (bounded at one billion); all probes are retained. Faster variants can
have shorter intervals. Sizes 1 and 4096 distinguish call-entry from throughput;
half/full repeat counts check time scaling. An external-call-only control retains
the driver floor; no timing is subtracted. Constants can be hoisted out of loops:
static MOV counts describe emitted code, never implied reloads per iteration.
Pressure cases are opt-in timing cases. The default command runs no timing matrix.

Even-numbered paired rounds counterbalance forward/reverse variant order, with
one warmup per variant/size/repeat setting. All measured variants use one copied
executable pathname; copy/hash checks are untimed. Reports retain raw times,
checksums, work counts, process times, paired median ratios against the first
variant, and seeded paired-bootstrap percentile 95% intervals (4000 resamples).
Intervals describe these pairs, not other programs; small campaigns are harness
sanity checks. Performance has no hard CI threshold. Half/full ratios and call
floor are diagnostic observations, not pass/fail speed claims.

Generated `.i`, assembly, disassembly, objects, binaries and raw reports must stay
under ignored `target/` or `suite/reports/` in a fresh directory. Reports retain
commands, host/target/toolchain, compiler/source/driver/object/binary SHA-256s,
source IDs, flags, feature labels, harness revision/dirty state and boundary
process snapshots. Identity checks bracket timing and run before every round;
`--verify-report` detects later source or binary mutation. Compile command wall
times include process/I/O overhead and are metadata, not internal stage costs.
No compiler rebuilds, remote execution, downloads or whole-app reruns occur.
