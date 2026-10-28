# Optimization levels and address models

`keel cc`, `keel compile` and `keel opt` accept `-O0`, `-O1`, `-O2`,
`-O3` and `-Os`. An unqualified `-O` retains its historical meaning: `-O3`.
With no optimization option, `cc` and `compile` use `-O0`; `opt` retains its
Full default. `-Og`, `-Oz`, `-Ofast` and unknown levels are rejected.

| Flag | IR policy | Selection | Allocation | Intended use |
|---|---|---|---|---|
| `-O0` | None | Baseline | Full | Inspect unoptimized IR and debug. |
| `-O1` | Fast | Trees | Fast | Short builds with bounded cleanup. |
| `-O2` | Balanced | Full | Full | Three bounded inlining rounds and scalar/memory optimization, without GCM. |
| `-O3`, `-O` | Full | Full | Full | Three inlining rounds and guarded loop transformations. |
| `-Os` | Size | Full | Full | Scalar cleanup and bounded helper inlining, without loop expansion. |

The balanced schedule omits GCM after stack promotion. Screening on Lua found
a dispatch regression at that placement; Full retains its existing ordering.

These are Keel policies, not promises to run the same passes as GCC or Clang.
More optimization effort does not guarantee a faster executable.
The distinction follows the compile-time and code-growth tradeoffs described by
[GCC](https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html) and
[Clang](https://clang.llvm.org/docs/CommandGuide/clang.html). Inlining and
loop expansion can remove call/loop overhead but increase code size, register
pressure, spills and instruction-cache demand. `-O1` limits compiler work;
`-O2` spends more effort while reserving loop expansion for `-O3`. None of these
levels enables unsafe floating-point transformations or additional CPU features.

The presets are initial policies. Measurements must justify future changes to
selector and allocator defaults. `-O0` retains the existing Full allocator:
optional IR transformations and allocation effort are separate choices.

## Overrides and preprocessing

The last `-O` level wins. `--opt-tier none|fast|basic|balanced|full|size` and
`-O` options change the IR tier in command-line order. `-p pass,pass,...`
overrides the tier's pass list regardless of order. `--isel-tier`/`--isel`
and `--regalloc` override the preset's corresponding stage regardless of order;
repeated explicit stage options use the last value. Tuning, CPU feature policy,
verification and address models remain independent.

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

This starts size optimization infrastructure; it does not guarantee the
smallest machine code. Size does not yet have a target byte-cost model for
all inlining, selection or allocator decisions.
Losing profitable inlining can make an individual program larger. The benchmark
records executable section bytes separately from file bytes so debug, symbol,
alignment and object-container overhead cannot masquerade as instruction size.
Unwind emission is independent of `-Os`; see [unwind metadata](../linking/unwind.md)
for explicit enable/disable flags.

Per-function `optimize(...)`, `hot`, `cold`, `optnone` and `flatten`
override function policies. Supported explicit
presets override the invocation pipeline and optional native selection.

## x86-64 address models

`--code-model MODEL`, `--code-model=MODEL` and `-mcmodel=MODEL` select the
supported x86-64 address materialization policy. An explicit model requires the
`x86_64-unknown-linux-gnu` target. These options do not change C type layout,
the ABI, CPU features, optimization effort or unwind emission.

| Model | Symbol address materialization | Contract |
|---|---|---|
| `pic` (default) | RIP-relative definitions; GOT-bound external addresses | Existing position-independent userspace policy. Relative references must fit their encoded reach. |
| `small` | Zero-extended 32-bit absolute symbol addresses (`R_X86_64_32`) | Static low-address image; symbol addresses must fit unsigned 32 bits and direct calls must remain reachable. |
| `kernel` | Sign-extended 32-bit absolute symbol addresses (`R_X86_64_32S`) | Static negative-2-GiB kernel image; direct calls must remain reachable. |

`small` and `kernel` reject `--link-intent shared`. Kernel rejects userspace TLS.
Small retains the existing userspace TLS lowering. The existing `pic` spelling
combines position independence and the bounded relative-address policy; this
interface does not yet provide every combination of GCC relocation/code models.
`medium` and `large` reject explicitly. A large model would also need distant
calls, constants and tables; using `movabs` for symbol addresses alone is not
sufficient. Linkers diagnose relocation overflow rather than silently truncating.

Kernel previously used 64-bit absolute address materialization. Its corrected
signed-32-bit relocations produce different code; the earlier full kernel boot
qualification does not qualify this new codegen checkpoint. Focused tests cover
text/typed/chunked parity and accepted/rejected final-link address ranges.

## Measurement

The existing native application runner now generates presets and a factorial
selector/allocator comparison with the IR pipeline held at `-O2`:

```sh
cargo build --release -p keel
python3 suite/c/benchmark_platforms.py --root . --work /tmp/keel-levels \
  --keel target/release/keel --apps lua --optimization-matrix \
  --compile-rounds 5 --runtime-rounds 7
```

`--experimental-allocators` extends the matrix to Local, Split, Weighted and
FullAffinity and requires a compiler built with `--features experimental-regalloc`.
These remain experimental; requesting the matrix does not promote them to presets.
`--configs` can select named cases and must retain `clang-O2` as a correctness
and timing reference. `--cpu` pins subprocesses on Linux.

The runner checks application tests and benchmark output against Clang before
timing. Failed/incorrect configurations stay in the report and are excluded from
timings. Compilation measures preprocessed C to assembly and C to object
separately, excluding preprocessing and linking. Timings use warmups and randomized
blocks. Runtime includes process startup. Reports retain raw samples, paired ratios,
bootstrap intervals, hashes, tool versions, executable bytes and text/allocated
section sizes. A result on one application and CPU does not establish a universal
best allocator or selector.

For in-process stage timings, build the example before measuring:

```sh
cargo build --release -p keel --example profile_compile
target/release/examples/profile_compile 5 balanced object \
  --isel-tier full --regalloc fast input.i
```

The example supports all native targets and reports frontend, verification,
optimization, codegen, assembly and destruction times. Codegen includes selection
and allocation; the factorial comparison measures the end-to-end effect of changing
each stage. Debug assertions or `verify-each` change compiler costs, so benchmark
release builds consistently.

## Native x86-64 measurements

Measured on 2026-10-28 using a release compiler with experimental allocators
available, Lua 5.4.7, and an AMD Ryzen 9 9950X3D. All 26 final configurations
passed Lua's portable test suite and matched Clang's output for all four
benchmarks. Three measured compile blocks and five measured runtime blocks
followed warmups, pinned to one logical CPU with randomized configuration order.
No exclusive CPU reservation or frequency-policy changes were made.

Runtime below is the geometric mean of the four workload time ratios to Clang
`-O2`, calculated from paired sample medians. Lower is better. Compile time sums
all prepared translation units at the C-to-object boundary. Text counts all ELF
executable sections, rather than total executable file size.

| Compiler policy | Compile time (ms) | Executable sections (bytes) | Runtime / Clang O2 |
|---|---:|---:|---:|
| `keel-O0` | 176.8 | 257,984 | 1.741 |
| `keel-O1` | 177.4 | 240,231 | 1.698 |
| `keel-O2` | 212.5 | 264,464 | 1.538 |
| `keel-O3` | 287.1 | 335,092 | 1.450 |
| `keel-Os` | 190.8 | 230,368 | 1.633 |
| `clang-O2` | 1400.6 | 199,724 | 1.000 |
| `clang-O3` | 1427.9 | 208,908 | 0.978 |
| `clang-Os` | 1153.2 | 147,167 | 1.109 |

On this application, O2 cut compile time by 26% and executable section bytes by
21% versus O3, with a 6% higher runtime ratio. Os cut section bytes by another
13% versus O2, with a 6% higher runtime ratio. Clang Os remains much smaller.
O1 and O0 compile times are nearly equal here; a level's intended effort is not
a universal compile-time ordering.

Holding the final O2 pipeline fixed separates backend effects:

| Selector | Allocator | Compile time (ms) | Executable sections (bytes) | Runtime / Clang O2 |
|---|---|---:|---:|---:|
| baseline | fast | 221.8 | 299,339 | 1.765 |
| baseline | full | 219.4 | 292,619 | 1.679 |
| trees | fast | 215.0 | 277,657 | 1.720 |
| trees | full | 212.9 | 271,577 | 1.612 |
| full | fast | 215.8 | 272,963 | 1.640 |
| full | full | 212.8 | 264,464 | 1.542 |

Full selection and Full allocation remain the release defaults. Weighted
allocation was a promising experimental candidate on this application, but
these samples do not establish a general ranking. The explicitly overridden
Full/Full configuration produces the same executable bytes as the O2 preset;
its small timing difference is a useful noise check.

The first balanced schedule placed GCM after stack promotion. Fibonacci took
about 3.8–4.0 times the Clang O2 time across selectors and allocators. A five-way
screen compared removing GCM, removing inlining and using one to three scalar
Full-order rounds. Removing GCM restored the intended balance; the final matrix
above was rerun after that correction. This is a schedule-sensitive observation,
not a correctness failure or a reason to remove GCM from Full.

The historical machine-readable summary, retained in Git history, records
per-workload ratios, every allocator combination, source/compiler hashes,
screening results and stage medians for Lua's parser and interpreter. Generated
reports, raw samples, intervals and command logs remain local under
`.git/optimization-policy/20261028`; the runner reproduces that report format. The workspace run passed 2,997 tests with 15 ignored before the
final GCM removal. After that correction, the focused execution/address-model
tests and the complete native application matrix passed. Strict Clippy remains
blocked by existing diagnostics outside this change. Kernel boot qualification
was not repeated for the changed address emission.

The function-policy repair changed Balanced from one to
three bounded inline/cleanup rounds and added Size helper inlining. Earlier
optimization-level measurements describe their recorded older source revisions;
they do not measure these revised pipelines.
