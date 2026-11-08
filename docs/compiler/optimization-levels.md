# Optimization levels

`keel cc`, `keel compile` and `keel opt` accept `-O0`, `-O1`, `-O2`,
`-O3` and `-Os`. An unqualified `-O` retains its historical meaning: `-O3`.
With no optimization option, `cc` and `compile` use `-O0` and `opt` uses the
`-O` tier. `opt` resolves its target as `cc` does, the host unless `--target`
is given, so `keel opt` on `cc -O0 --emit-ir` output runs the passes
`cc -O` runs. `-Og`, `-Oz`, `-Ofast` and unknown levels are rejected.

| Flag | IR policy | Selection | Allocation | Intended use |
|---|---|---|---|---|
| `-O0` | None | Covers | Full | Inspect unoptimized IR and debug. |
| `-O1` | Fast | Trees | Fast | Short builds with bounded cleanup. |
| `-O2` | Balanced | Full | Full | Three bounded inlining rounds and scalar/memory optimization, without GCM. |
| `-O3`, `-O` | Full | Full | Full | Three inlining rounds and guarded loop transformations. |
| `-Os` | Size | Full | Full | Scalar cleanup and bounded helper inlining, without loop expansion. |

Balanced omits GCM, whose placement after stack promotion regressed Lua
dispatch. The exact pass lists are `OptimizationTier::pipeline`,
`BALANCED_PIPELINE`, `SIZE_PIPELINE`, `FULL_ROUND` and `FULL_EPILOGUE` in
`crates/keel-opt/src/lib.rs`; the [optimizer reference](optimizer.md) gives
each pass's contract. The Basic tier, available through `--opt-tier basic`,
runs scalar CFG and combining cleanup without inlining. Fast and Basic expand
constant memory copies and fills of at most 128 bytes into loads and stores;
Balanced and Full expand at most 32 bytes, and None and Size none.

These are Keel policies, not the GCC or Clang pass lists, and more effort does
not guarantee a faster executable. Inlining and loop expansion remove call and
loop overhead but add code size, register pressure and spills. No level
enables unsafe floating-point transformations or CPU features. `-O0` keeps the
Full allocator: IR transformations and allocation effort are separate choices.

## Overrides and preprocessing

The last `-O` level wins. `--opt-tier none|fast|basic|balanced|full|size` and
`-O` options change the IR tier in command-line order. `-p pass,pass,...`
overrides the tier's pass list regardless of order. `--isel-tier`/`--isel`
and `--regalloc` override the preset's corresponding stage regardless of order;
repeated explicit stage options use the last value. An explicit
`--opt-tier fast` does not select the Fast allocator. Tuning, CPU feature
policy, verification and [code models](../targets/x86_64/codegen.md#code-models)
are independent. Library callers use `keel_opt::OptimizationTier`.

For example:

```sh
keel cc -O2 --regalloc fast --emit-object input.c -o input.o
keel cc -Os --opt-report size-passes.json --emit-object input.c -o small.o
keel compile -O3 --isel-tier baseline input.keel -o baseline.s
```

Raw C preprocessing defines `__OPTIMIZE__=1` for a non-None IR tier and
`__OPTIMIZE_SIZE__=1` for Size. Explicit `-D`/`-U` options can override these
macros. Already preprocessed `.i` inputs retain their original macro choices.
Wasm keeps its separate `--wasm-opt basic` contract and rejects native levels.
Required frontend inlining, initial IR verification and legal code generation
still run at `-O0`.

## Compatible optimization switches

Native `cc`, `compile` and `opt` accept `-finline-functions`, `-funroll-loops`
and `-ftree-loop-vectorize`, with their `-fno-` forms. `-finline` is an alias
for `-finline-functions`; `-ftree-vectorize` and Clang's `-fvectorize` control
Keel's loop vectorization. These names do not enable SLP vectorization.

The last switch for a family wins, independently of where `-O` appears.
They supply defaults before each function's `optimize(...)` requests, so explicit
function requests override them. Enabling a family adds its existing bounded
pass when the chosen tier omits it. Positive switches do not start optional
optimization at `-O0` or with an empty explicit pipeline, unless a function has
an explicit optimization preset. Disabling a family also filters explicit
`-p` pipelines. `optnone`, legality checks, growth limits and required inlining
retain their existing behavior. The switches select Keel's passes, not the
GCC or Clang pass lists.

For example:

```sh
keel cc -O3 -fno-unroll-loops -fno-vectorize input.c -o scalar.s
keel cc -O2 -funroll-loops input.c -o unrolled.s
```

`cc` also accepts `-fmath-errno` and `-fno-math-errno`; see
[floating-point mode](function-attributes.md#floating-point-math-mode).

## Size policy

The Size pipeline runs two bounded helper-inlining rounds interleaved with
scalar cleanup. It admits hinted helpers, constant-argument calls and single-use
non-escaping local helpers, capped at 128 IR instructions for optional cases.
Existing legality, recursion, stack and growth limits remain active. Shared
unhinted helpers with variable arguments can remain calls. Size omits
small-memory expansion, GCM, unrolling and automatic vectorization. It retains
CFG, load, stack, constant-argument and unreachable-function cleanup, including
DCE after empty-call elimination to remove unused symbol references. Required
language/inline-assembly inlining remains active.

The `-Os` preset also selects [size-oriented x86 emission](../targets/x86_64/codegen.md):
less alignment padding, eligible scalar leaf-frame removal, unused leaf
call-padding removal, smaller epilogues and profitable return sharing.
`--opt-tier` and `-p` change IR passes without resetting the preset's codegen goal.

Size does not guarantee the smallest machine code: there is no target
byte-cost model for inlining, selection or allocation, and losing profitable
inlining can make a program larger. Unwind emission is independent of `-Os`; see [unwind metadata](../linking/unwind.md)
for explicit enable/disable flags.

Per-function `optimize(...)`, `hot`, `cold`, `optnone` and `flatten`
override function policies. Supported explicit
presets override the invocation pipeline and optional native selection.

## Measurement

The native application runner checks and compares the compile time of the
presets and a selector and allocator matrix with the IR pipeline held at `-O2`:

```sh
cargo build --release -p keel
python3 suite/c/benchmark_platforms.py --root . --work target/keel-levels \
  --keel target/release/keel --apps lua --optimization-matrix --compile-rounds 5
```

`--experimental-allocators` adds the experimental allocators and needs a
compiler built with `--features experimental-regalloc`. `--configs` selects
named cases and must keep `clang-O2` as the correctness and timing reference.
`--cpu` pins subprocesses on Linux. The runner checks outputs against Clang
before timing. Compare the runtime of two compilers with `cargo suite ab`
(see the [suite guide](../../suite/README.md#timing-two-compilers)).

For in-process stage timings:

```sh
cargo build --release -p keel --example profile_compile
target/release/examples/profile_compile 5 balanced object \
  --isel-tier full --regalloc fast input.i
```

The example reports frontend, verification, optimization, codegen, assembly
and destruction times. Debug assertions and `verify-each` change compiler
costs, so benchmark release builds.
