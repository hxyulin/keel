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
Full now start with canonicalization; explicit historical recipes retain their
listed stages, with prerequisites restored for consumers that request them.
