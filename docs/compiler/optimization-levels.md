# Optimization levels

`keel cc`, `keel compile` and `keel opt` accept `-O0`, `-O1`, `-O2`,
`-O3` and `-Os`. An unqualified `-O` retains its historical meaning: `-O3`.
With no optimization option, `cc` and `compile` use `-O0` and `opt` uses the
`-O` tier. `opt` resolves its target as `cc` does, the host unless `--target`
is given, so `keel opt` on `cc -O0 --emit-ir` output runs the passes
`cc -O` runs. `-Og`, `-Oz`, `-Ofast` and unknown levels are rejected.

`--mode` selects four presets:

| Mode | Equivalent level | Policy |
|---|---|---|
| `speed` | Compile-speed preset | No optional IR optimization. Direct AArch64/x86-64 scalar emission where supported; Baseline selection and Fast allocation for fallback functions. |
| `no-opt` | `-O0` | Default native preset, with no optional IR optimization and ordinary code generation. |
| `balanced` | `-O2` | Bounded optimization effort for runtime performance. |
| `full` | `-O3` | Full semantics-preserving optimization pipeline. |

For example, `keel cc --mode speed --emit-object input.c -o input.o` selects
compilation latency over generated-code performance. Speed gives every value
a stack home and skips general selection and allocation for supported functions.
Fixed scalar calls, including supported multiple scalar results, and
`memcpy`/`memmove`/`memset` use direct emission. Variadic, aggregate and other
unsupported call shapes fall back. Unsupported functions use the ordinary backend
in the same object. Modules with debug source files use ordinary code generation. Unwind, linkage, sections and target
validation retain their normal contracts. See the [AArch64](../targets/aarch64/codegen.md#direct-emission) and
[x86-64](../targets/x86_64/codegen.md#direct-emission) direct-emission contracts.
`--codegen-report` counts supported functions as `direct_functions`.

Full does not imply fast-math, extra CPU features, experimental passes or
unbounded optimization. More optimization effort does not guarantee better
runtime performance for every program. Existing `-O1` and `-Os` remain available.

| Flag | IR policy | Selection | Allocation | Intended use |
|---|---|---|---|---|
| `-O0` | None | Covers | Full | Inspect unoptimized IR and debug. |
| `-O1` | Fast | Trees | Fast | Short builds with bounded cleanup. |
| `-O2` | Balanced | Full | Bounded + caller saves | Three bounded inlining rounds and scalar/memory optimization, without GCM. |
| `-O3`, `-O` | Full | Full | Bounded | Three inlining rounds and guarded loop transformations. |
| `-Os` | Size | Full | Compact + caller saves | Scalar cleanup and bounded helper inlining, without loop expansion. |

`-O2` and `-O3` also enable bounded integer pressure scheduling before target
selection. Bounded allocation retains Full's homes and caches repeated spilled
scalar reads within call-free windows. `--regalloc full --no-schedule-pressure`
restores the previous backend policy. Both overrides work before or after `-O`.
Compact allocation at `-Os` uses Full's register scan but reuses spill homes
across nonintersecting lifetime ranges and prefers branch-edge copy partners.
`--regalloc full` retains contiguous-span spill homes. Both allocation policies
remain independently selectable.
At `-O2` and `-Os`, `CallSiteAllocator` compares whole-value GPR spills with
explicit saves at clobber sites. It accepts the alternative only when both
static and loop-weighted memory traffic decrease. Eligible Darwin AArch64
functions can also allocate x0–x7, with ABI staging and scratch clobbers handled
by the adapter. Float and vector call restrictions retain the base tier's policy.
Explicit `--regalloc` selects the base tier without this adapter, in either flag
order. `-O0`, `-O1`, `-O3` and library defaults retain their existing allocation.
At `-Os`, this adapter also enables scalar ABI register preferences. Values
prefer their first incoming, call or return register when it is available,
reducing register copies. Preferences do not admit reserved registers or bypass
liveness and call-clobber checks. `-O2` retains its existing copy preferences.
See the [allocation and scheduling contracts](design.md#register-allocation).

Size and AArch64 Balanced promote private stack fields with pruned SSA: a field
gets a block parameter only where its incoming value is live. This lets the
release policy admit larger inlined functions within its field-analysis budget.
Balanced x86 and Portable retain the smaller promotion bounds.
Balanced omits GCM, whose placement after stack promotion regressed Lua
dispatch. The exact pass lists are `OptimizationTier::pipeline`,
`BALANCED_PIPELINE`, `SIZE_PIPELINE`, `FULL_ROUND` and `FULL_EPILOGUE` in
`crates/keel-opt/src/lib.rs`; the [optimizer reference](optimizer.md) gives
each pass's contract. The Basic tier, available through `--opt-tier basic`,
runs scalar CFG and combining cleanup without inlining. Fast and Basic expand
constant memory copies and fills of at most 128 bytes into loads and stores;
Balanced, Size and Full expand at most 32 bytes; None leaves them unchanged.

On AArch64 and x86-64, Full also batches ordered floating sums and absolute-value sums
when a literal count or existing dominating guards prove at least 32 trips.
Literal byte counts must fit u64. Unproven and shorter counts stay scalar. The batched scalar
schedule retains floating addition order; see [SIMD IR](simd-ir.md).

These are Keel policies, not the GCC or Clang pass lists, and more effort does
not guarantee a faster executable. Inlining and loop expansion remove call and
loop overhead but add code size, register pressure and spills. No level
enables unsafe floating-point transformations or CPU features. `-O0` keeps the
Full allocator: IR transformations and allocation effort are separate choices.

## Overrides and preprocessing

The last `--mode` or `-O` preset wins, including the backend strategy. `--opt-tier none|fast|basic|balanced|full|size` and
preset options change the IR tier in command-line order. `-p pass,pass,...`
overrides the tier's pass list regardless of order. `--isel-tier`/`--isel`
and `--regalloc` override the preset's corresponding stage regardless of order;
repeated explicit stage options use the last value. In speed mode these stage
overrides apply to fallback functions; direct emission has no selector or allocator. An explicit
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
unhinted helpers with variable arguments can remain calls. Size expands memory
intrinsics of at most 32 bytes before stack cleanup. After the last scalar
round, GCM moves pure computations, GVN merges newly exposed equivalents,
and GCM restores local constant and address copies to reduce register pressure.
On x86-64, Size revisits non-cold callers that require a frame after module
cleanup. If inlining changes the module, scalar and stack cleanup run again,
and code motion restores local copies of constants and addresses after
value numbering.
On x86-64, a bounded exact unroll can eliminate a private array after exposing
constant stack offsets. Size omits general loop unrolling and automatic
vectorization. It retains
CFG, load, stack, constant-argument and unreachable-function cleanup, including
DCE after empty-call elimination to remove unused symbol references. Required
language/inline-assembly inlining remains active.

The `-Os` preset also selects [size-oriented x86 emission](../targets/x86_64/codegen.md):
less alignment padding, eligible scalar leaf-frame removal, unused leaf
call-padding removal, smaller epilogues and profitable return sharing.
[AArch64](../targets/aarch64/codegen.md#calls-and-copies) shares ordinary return
restoration after register allocation at both size and speed goals when the
epilogue has at least three instructions.
`--opt-tier` and `-p` change IR passes without resetting the preset's codegen goal.

Optional Size inlining also uses a target byte estimate. It scans at most
128 IR instructions, estimates fixed
four-byte AArch64 instructions (including wide-constant materialization), and
uses bounded x86 instruction allowances. The x86 estimate includes short
integer immediates in their consuming instructions when every use can encode
them. Duplicated bodies have a 16-byte allowance on AArch64 and a 32-byte
allowance on x86-64. Private x86-64 helpers with an inline hint and a required
frame pointer get 144 bytes to account for the out-of-line call and frame.
Bound x86-64 read-only leaves that require a frame can use up to 224 estimated
bytes and can inline without a hint. These leaves have at most 48 IR
instructions, an acyclic CFG and no surviving stack slots. The per-caller
and module growth limits still apply.
Removable single-use private bodies bypass this filter because their out-of-line code
disappears. Empty x86 compiler barriers have zero emission cost. The estimate
also accounts for recognized x86 memory bit tests and single-instruction
memory subtractions with flag outputs.
Both preserve their memory effects when inlined. Unrecognized assembly and
intrinsics have no estimate and remain calls.
Portable targets keep the existing IR policy. Required inlining and flatten
retain their contracts. This admission filter can reduce text while adding
calls and runtime cost. It does not predict final object size after selection
and allocation.

Balanced and Size fold scalar reads from bounded local immutable data before
their cleanup rounds. Both share equivalent terminal blocks after the release
epilogue. External inputs, block-argument types, memory flags and instruction
contracts must match; local definitions cannot escape the shared block.
The [optimizer reference](optimizer.md#constant-data) defines the data and
placement limits.

Size does not guarantee the smallest machine code: selection and allocation
have no target byte-cost model, and losing profitable
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
