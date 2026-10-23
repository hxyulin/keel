# Controlled signed i32 → i64 reduction width comparison

At frozen main `8690a355b2986288e5b53b6401a79ba342cc16f9`, explicit 256-bit
preference remains profitable for the tested long, warm signed sum on LinuxHost.
Two nine-pair campaigns show 26.7–31.0% lower median time at 1,048,576 inputs,
with short-input regressions. Further bounded width-policy investigation is
justified; **all production defaults remain 128 bits**. This work changes only
`suite/width-comparison` and its dedicated documentation.

The [historical pre-add-selection campaign](controlled-signed-sum-width-8bb21e9a.md)
is retained separately with its original measurements and artifact locations.
Its 49.45 µs wide versus 56.23 µs narrow result and roughly +15% compile cost
belong to `8bb21e9a`, not the current compiler. The
[direct-add emission experiment](x86-avx2-direct-add.md) measures another
comparison: fixed wide IR across compiler revisions. Neither is pooled with
this width comparison or used as a current campaign sample.

## Frozen sources and treatment

The new `provenance.py` freezes the explicitly requested Git revision, verifies
all 1,966 archived base blobs against Git IDs, overlays only the dedicated
width-comparison helpers, and copies the maintained `compiler.rs` to
`crates/keel/examples/width_comparison.rs`. The example itself is unchanged
from the base. The runner rechecks the base and overlay manifest before work
and retains it inside `results.json`. There is no hardcoded historical base
in the runner. No concurrent address-folding or reduction-finish worker is
included. Existing toolchain, lockfile, features and release profile remain.

The unchanged example follows C preprocessing/frontend, initial verification,
`DEFAULT_PIPELINE` and default verification policy, canonical restoration,
generic tuning, `SelectionOptions::ALL`, Full allocation, effective features
and chunked integrated object assembly. Release emission legality stays active;
final IR is also verified. The only treatment is the library capability
`with_wide_preference(false|true)`. No textual IR is reparsed and no frontend
intrinsic surface, ABI rule, FP rule or compiler/default policy changes.

For strict v1, v2, v3 and v3 minus AVX2 on LinuxHost, and native Apple ARM64:

- Entire 128-bit library object **and optimized IR** match ordinary CLI output
  from the same build, input path and flags.
- Both library compilations in one `pair` process match their standalone
  objects and IR. There is no reliance on separate-process compilation alone.
- Both width requests have identical objects and IR in v1/v2/AVX-only/ARM.
  Only v3 activates the wide reduction. Among twelve functions, independent
  exact function-byte comparison finds only `rk_widen32` changes.

Both final campaigns have identical compiler/helper/source/oracle hashes,
objects, linked executables and instruction audits. The manifest SHA-256 is
`4ce83527aef18fe3430b322fc669f079402af7189617f8cb504d9456659a6672`.
LinuxHost comparison compiler SHA-256 is
`b3893c6f3c49c03b67695e32b98388a2f160c805d73bef7806530083261a9140`;
CLI SHA-256 is
`b5758a82e674b5dc4ef68312b28fae7596ef849d7f589f43645b1a6bf0485d1a`.
Hashes describe these retained builds, not a promise of reproducible linking.

## Correctness, memory and ABI

The maintained `runtime-kernels/kernels.c`, driver and Python input generator
are unchanged. Both v3 arms pass the independent wrapping signed-sum oracle
at lengths 0–81, 257, 4096, 65536 and 1048576 before timing. Each timed process
also checks the result before its timer and checks the final checksum. One
frozen Clang `-O2` driver object links both arms without LTO; external calls,
warmup, volatile sink and input generation remain unchanged.

The dedicated guards pass every profile/width on LinuxHost and both Apple widths:
lengths 0–81 cover 0–17, group boundaries and tails; starts 0–5, all sixteen
byte alignments, both protected-page edges, signed extrema/negative elements,
and initial sums 0, UINT64_MAX and 0xfedcba9876543210. Each combination checks
26,568 seeded cases plus maintained-kernel cases, inaccessible-pointer empty
and reversed ranges, negative end, and a valid negative start. Native execution
is finite testing on an AVX2-capable CPU, not strict-v1/v2-only hardware.

The existing release `x86_widen_sum` test passes all 48 feature/selection/
allocation/pool profiles, including typed operand-dependent legality,
text/typed/chunked byte parity and native guarded ranges/address-overflow
fallback. All six release `x86_signed_wide_load` tests pass, including the
Clang encoding oracle and emission/export corpus; that export test does not
itself execute its standalone C bundles. The release
`x86_64::feature_guard_tests` gate passes locally. Three dedicated Python
runner/audit/provenance tests pass. These checks do not extend ISA support.

GNU disassembly, symbol sizes and exact section bytes give identical A/B audits:

| `rk_widen32` | Current 128 | Current explicit 256 | Historical explicit 256 |
| --- | ---: | ---: | ---: |
| Function bytes | 256 | 313 | 407 |
| Steady-loop inputs | 8 | 16 | 16 |
| Steady-loop instructions / bytes | 15 / 61 | 21 / 82 | 33 / 137 |
| Add-stage vector copies in loop | 0 | 0 | 12 |
| Finish instructions / bytes | 8 / 31 | 9 / 44 | 18 / 83 |

The narrow symbol SHA-256 is unchanged from the historical campaign:
`7e14588e10fc6bb5117fd24164cd9f76cc9840ecf069af34a3807d5ca7121518`.
Current wide SHA-256 is
`e4ff15cc7c5e84a8135fd2f9a2188d790863327b0d8c2d95e3d4c152b8735ae2`.
The direct-add selection already present in main removes twelve loop copies
and nine finish copies. Six address-building instructions remain in the wide
loop (three `mov r12,rbx` / `add r12,offset` pairs). This is observed emission,
not a microarchitectural explanation of the measured gain.

For eight inputs, narrow makes four exact **8-byte** PMOVSXDQ reads at offsets
0/8/16/24 and advances 32 bytes. For sixteen inputs, wide makes four exact
**16-byte** VPMOVSXDQ reads at offsets 0/16/32/48 and advances 64 bytes. Guards
require at least 8/16 elements, round group counts down and retain address-
overflow scalar fallback. Both tails read exactly one **4-byte** element per
iteration, sign-extend and add. Neither loop spills accumulators or accesses
the stack. The auditor checks four widening reads and their operand widths,
counts copies and retains full function instructions for manual address,
prologue/tail/finish review. It verifies the measured objects' hashes and
asserts objcopy leaves its input unchanged. Both width outputs for v1/v2/AVX-
only have no AVX-family mnemonic or YMM/ZMM operand in the reduction; this
finite scan supplements release feature guards, not exhaustive ISA decoding.

Both retain scalar SysV `U(const int *, const I *, const U *, U)` and RAX
return; no vector argument crosses the ABI. Both save/restore RBP/RBX/R12,
and perform an unused scalar-argument stack store/reload before the loop.
Narrow reserves 32 bytes after saving RBX/R12. Wide additionally saves R15,
reserves 71 bytes and aligns RSP to 32; it restores RSP from RBP and executes
`vzeroupper` before return. Narrow finishes with a 16-byte stack store and
two 8-byte lane reads. Wide finishes in registers with three accumulator
adds, one copy, cross-half permutation, shift/add and extraction. No FP
arithmetic, implicit contraction or FP-control mutation is introduced; this
integer campaign establishes no additional FP-environment contract.

## Current measurements: 2026-10-23

LinuxHost is a Ryzen 9 9950X3D, native Linux x86-64. The shared
`/tmp/keel-perf-20261019.lock` serializes builds and campaigns. Both campaigns
pin CPU 2 (SMT sibling 18), discard one complete warmup round and retain nine
seeded interleaved paired rounds. Seeds are 20261025 (A) and 20261026 (B).
Each length's arms run adjacent in seeded shuffled order. Each process also
performs the driver's unchanged warmup. Repetitions are
`min(1000000, max(256, 67108864 // n))`.

Before **and after** each compile/runtime round, a 250 ms CPU-idle probe and
1-minute host load check reject timing above 10% selected-CPU busy or load 1.
All 40 checks per campaign pass: maximum loads 0.2305/0.2876, maximum busy
3.85% in both. Process captures before/after, timestamps, affinity, CPU and
SMT topology, kernel, governor and driver are retained. `amd-pstate-epp` and
`powersave` were recorded without changes. Frequency/boost, interrupts, SMT
activity and code placement are not fixed; passing preflight does not prove
zero interference or stable clocks. No rejected/uncontrolled campaign is
included, and no further timing campaigns were consumed.

Median nanoseconds per call, including driver call/sink/checksum overhead:

| Inputs | A: 128 | A: 256 | B: 128 | B: 256 |
| --- | ---: | ---: | ---: | ---: |
| 8 | 2.548 | 5.113 | 2.564 | 5.113 |
| 17 | 2.829 | 2.869 | 2.826 | 2.869 |
| 257 | 17.242 | 9.843 | 17.419 | 10.424 |
| 4096 | 275.187 | 146.951 | 273.196 | 143.835 |
| 65536 | 4315.610 | 2280.420 | 4390.553 | 2233.554 |
| 1048576 | 57289.316 | 39553.973 | 59085.883 | 43291.008 |

At 1048576, ratios of arm medians are 1.4484x/1.3649x (31.0%/26.7% less
time for wide). All eighteen paired ratios favor wide, ranging 1.3513–1.6966x.
All eighteen pairs also favor wide at 257, 4096 and 65536. Both short lengths
favor narrow in every pair: wide is about twice as slow at 8 and 1.4–1.5%
slower at 17 by medians. Individual long-pair ratios vary substantially; the
campaign medians do not establish an exact crossover or a fixed speedup.

Compile service time is separate: source read/preprocessing through object
assembly for all twelve functions, including initial verification and the
unchanged Full pipeline, excluding final verification, diagnostic captures,
writes and module destruction. Each arm records its internal service timer
in a fresh process; same-process `pair` mode separately establishes parity.
After a discarded warmup pair, nine measured pairs give median 128→256 times
of **2.212→2.649 ms** (A, +19.7%) and **2.453→2.420 ms** (B, −1.3%). Paired
wide/narrow service ratios span 0.911–1.256 (A) and 0.953–1.293 (B). This
variability supports **no stable compile-cost improvement or overhead estimate**.
Fresh-process command wall times include diagnostic captures and are not
compile-only measurements. Historical +15% is not carried forward.

Further width-policy work is warranted for long eligible signed sums, with
runtime-size/cost guards and broader workload/host validation before any
production policy decision. Short regressions and greater code/stack size
remain, and compile cost needs a better controlled separate study. There is
no automatic default change. This work establishes no cold-cache, concurrent,
whole-application, other-x86-microarchitecture, unsigned, multistream, FP or
ARM profitability claim; ARM serves as native correctness/parity control.
Historical and current timings were collected at different times and linked
layouts, so their difference is not an isolated causal measurement of the
add-selection change. The separate direct-add study addresses that question.

## Reproduction and retained evidence

From an isolated worktree, freeze the desired base and then build/run **inside
the snapshot**. Keep all generated artifacts outside Git:

```sh
python3 suite/width-comparison/provenance.py \
  --base 8690a355b2986288e5b53b6401a79ba342cc16f9 \
  --output /tmp/width-current-snapshot
cd /tmp/width-current-snapshot
python3 suite/fast_opt_locked.py /tmp/width-current-build.jsonl \
  cargo build --offline --locked -j2 --release -p keel \
  --bin keel --example width_comparison
python3 suite/fast_opt_locked.py /tmp/width-current-run.jsonl \
  python3 suite/width-comparison/run.py \
  --compiler /tmp/width-current-snapshot/target/release/examples/width_comparison \
  --keel /tmp/width-current-snapshot/target/release/keel \
  --output /tmp/width-current-a --cpu 2 --seed 20261025 --rounds 9
python3 suite/width-comparison/audit.py /tmp/width-current-a
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s suite/width-comparison -p 'test_*.py'
```

Repeat with a fresh output directory and second seed if the host is quiet.
Run the entire campaign under one lock wrapper; never nest wrappers. If
preflight fails, retain `complete:false` evidence and defer timing. For Apple
ARM64 controls, build on that host, add
`--target aarch64-apple-darwin --controls-only` and omit `--cpu`.
Cross-target native linking is unsupported. Use the same locked Rust form
with `test --offline --locked -j2 --release -p keel-codegen` plus
`--test x86_widen_sum --test x86_signed_wide_load`, or
`--lib x86_64::feature_guard_tests`, for the supplemental checks.

Both hosts' frozen source: `/tmp/keel-width-current-8690a355-frozen`.
LinuxHost campaigns: `/tmp/keel-width-current-linuxhost-{a,b}` with 108 raw runtime
samples and 18 compile rows each, exact command/stdout/stderr/wall logs,
objects, IR, assembly, disassembly, audit, driver and binaries.
Campaign report SHA-256 (A/B):
`27420f77acffbd8668f446dc40b8ae370fed45fa15ced3d72210bf16e97c2dab` /
`1d6429171e710677745e2166bb00a164d020d08019a75cad25eb0cd100420b71`.
Identical A/B audit SHA-256:
`802b2c006d90d46e11cb6e4aeeb92bdcc25573118062e88d7a9736ae51cec196`.
LinuxHost logs: `/tmp/keel-width-current-linuxhost-{build,run,tests}.jsonl`.
Local copies: `/tmp/keel-width-current-linuxhost-{a,b}-evidence`.
Apple controls: `/tmp/keel-width-current-apple-controls`;
local logs: `/tmp/keel-width-current-apple-{build,run,guard}.jsonl`.
