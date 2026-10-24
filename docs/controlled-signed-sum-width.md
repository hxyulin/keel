# Controlled signed i32 → i64 reduction width comparison

This study compares Keel's default 128-bit vector width against an explicit
256-bit (AVX2) preference for a signed i32 → i64 wrapping sum on x86-64.
For long, warm inputs the 256-bit form took 26.7–31.0% less median time at
1,048,576 inputs, but short inputs regressed. Further bounded width-policy work
is justified; **all production defaults remain 128 bits**.

The [direct-add emission experiment](x86-avx2-direct-add.md) measures a
different question (fixed wide IR across compiler revisions) and is not pooled
with this comparison.

## Method

The harness lives in `suite/width-comparison/`. `provenance.py` snapshots a
requested Git revision, verifies the archived base blobs against their Git IDs,
overlays only the width-comparison helpers, and copies `compiler.rs` to
`crates/keel/examples/width_comparison.rs`. The runner rechecks the snapshot
manifest before work and stores it in `results.json`.

The example runs the normal compilation path: C preprocessing and frontend,
initial verification, `DEFAULT_PIPELINE` with the default verification policy,
generic tuning, `SelectionOptions::ALL`, Full allocation, effective features and
chunked integrated object assembly. The only treatment is the library capability
`with_wide_preference(false|true)`. No IR is reparsed, and no frontend, ABI, FP
or default policy changes.

Parity controls, run for strict x86-64 v1, v2, v3 and v3-without-AVX2, and for
native arm64 macOS:

- The 128-bit library object **and optimized IR** match ordinary CLI output for
  the same input and flags.
- Both library compilations in one `pair` process match their standalone
  objects and IR.
- Both width requests produce identical objects and IR on v1, v2, AVX-only and
  arm64. Only v3 activates the wide reduction, and of twelve functions only
  `rk_widen32` changes.

## Correctness

The `runtime-kernels/kernels.c` kernels, driver and input generator are
unchanged. Both v3 arms pass an independent wrapping signed-sum oracle at
lengths 0–81, 257, 4096, 65536 and 1048576 before timing; each timed process
also checks its result before the timer and its final checksum. One Clang `-O2`
driver object links both arms without LTO.

Dedicated guards pass for every profile and width on x86-64 and both widths on
arm64: lengths 0–81 (covering group boundaries and tails), starts 0–5, all
sixteen byte alignments, both protected-page edges, signed extrema and negative
elements, and initial sums 0, `UINT64_MAX` and `0xfedcba9876543210`. Each
combination checks 26,568 seeded cases plus kernel cases, inaccessible-pointer
empty and reversed ranges, a negative end, and a valid negative start. Native
execution ran on an AVX2-capable CPU, not strict v1/v2-only hardware.

Supplemental release tests: `x86_widen_sum` (48 feature/selection/allocation
profiles, including byte parity across text/typed/chunked emission and native
guarded ranges), `x86_signed_wide_load`, and `x86_64::feature_guard_tests`.

## Generated code

| `rk_widen32` | 128-bit | Explicit 256-bit |
| --- | ---: | ---: |
| Function bytes | 256 | 313 |
| Steady-loop inputs | 8 | 16 |
| Steady-loop instructions / bytes | 15 / 61 | 21 / 82 |
| Add-stage vector copies in loop | 0 | 0 |
| Finish instructions / bytes | 8 / 31 | 9 / 44 |

For eight inputs, the narrow loop makes four exact **8-byte** PMOVSXDQ reads at
offsets 0/8/16/24 and advances 32 bytes. For sixteen inputs, the wide loop makes
four exact **16-byte** VPMOVSXDQ reads at offsets 0/16/32/48 and advances 64
bytes. Guards require at least 8/16 elements, round group counts down and keep a
scalar fallback for address overflow. Both tails read exactly one 4-byte element
per iteration. Neither loop spills accumulators or accesses the stack. Six
address-building instructions remain in the wide loop (three
`mov r12,rbx` / `add r12,offset` pairs). The v1/v2/AVX-only outputs contain no
AVX-family mnemonic or YMM/ZMM operand in the reduction.

ABI: both keep the scalar SysV signature `U(const int *, const I *, const U *, U)`
with an RAX return; no vector argument crosses the ABI. Both save/restore
RBP/RBX/R12. Narrow reserves 32 bytes; wide additionally saves R15, reserves 71
bytes, aligns RSP to 32, restores RSP from RBP and executes `vzeroupper` before
returning. Narrow finishes with a 16-byte stack store and two 8-byte lane reads;
wide finishes in registers. No FP arithmetic or FP-control mutation is
introduced.

## Measurements

Host: x86-64 Linux, Ryzen 9 9950X3D. Two campaigns (seeds 20261025 and
20261026) pin CPU 2, discard one warmup round and keep nine seeded, interleaved
paired rounds; each length's arms run adjacent in shuffled order. Repetitions
are `min(1000000, max(256, 67108864 // n))`. An idle-CPU and load-average check
before and after every round rejects timing above 10% busy or load 1; all checks
passed. Frequency/boost, interrupts and code placement were not fixed.

Median nanoseconds per call, including driver call/sink/checksum overhead:

| Inputs | A: 128 | A: 256 | B: 128 | B: 256 |
| --- | ---: | ---: | ---: | ---: |
| 8 | 2.548 | 5.113 | 2.564 | 5.113 |
| 17 | 2.829 | 2.869 | 2.826 | 2.869 |
| 257 | 17.242 | 9.843 | 17.419 | 10.424 |
| 4096 | 275.187 | 146.951 | 273.196 | 143.835 |
| 65536 | 4315.610 | 2280.420 | 4390.553 | 2233.554 |
| 1048576 | 57289.316 | 39553.973 | 59085.883 | 43291.008 |

At 1,048,576 inputs the median ratios are 1.4484x / 1.3649x (31.0% / 26.7% less
time for wide); all eighteen paired ratios favor wide (1.3513–1.6966x), as do
all pairs at 257, 4096 and 65536. Both short lengths favor narrow in every pair:
wide is about twice as slow at 8 and 1.4–1.5% slower at 17. The medians do not
establish an exact crossover or a fixed speedup.

Compile service time (preprocessing through object assembly for all twelve
functions) had medians of 2.212 → 2.649 ms (A, +19.7%) and 2.453 → 2.420 ms
(B, −1.3%), with paired ratios spanning 0.911–1.293. This supports no stable
compile-cost estimate.

## Conclusions and limits

Wider reductions look worthwhile for long eligible signed sums, but a production
policy would need runtime-size and cost guards and broader workload and host
validation. Short-input regressions and larger code and stack size remain. This
study makes no cold-cache, concurrent, whole-application, other-x86
microarchitecture, unsigned, multi-stream, FP or ARM profitability claim; arm64
serves only as a correctness and parity control.

## Reproduction

Snapshot a revision, then build and run inside the snapshot, keeping generated
artifacts outside the repository:

```sh
python3 suite/width-comparison/provenance.py --base <commit> --output <snapshot-dir>
cd <snapshot-dir>
cargo build --release -p keel --bin keel --example width_comparison
python3 suite/width-comparison/run.py \
  --compiler target/release/examples/width_comparison \
  --keel target/release/keel \
  --output <results-dir> --cpu 2 --seed 20261025 --rounds 9
python3 suite/width-comparison/audit.py <results-dir>
python3 -m unittest discover -s suite/width-comparison -p 'test_*.py'
```

Repeat with a fresh output directory and a second seed. If the preflight check
fails, the run records `complete:false` and skips timing. For arm64 macOS
controls, build on that host and add `--target aarch64-apple-darwin
--controls-only`, omitting `--cpu`; cross-target native linking is unsupported.
Supplemental checks:

```sh
cargo test --release -p keel-codegen --test x86_widen_sum --test x86_signed_wide_load
cargo test --release -p keel-codegen --lib x86_64::feature_guard_tests
```
