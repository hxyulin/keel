# C lane reduction recognition pilot

This focused follow-up to [the frozen SIMD research](current-compiler-research-dbe0b1f6.md)
starts from clean `81b2e71c6f69844a549e9ba916ff164332171a8a`.
It owns only target-independent instcombine recognition, focused tests and
these docs. No frontend, backend, public API, pipeline schedule, feature,
ABI, allocator, width policy or FP semantics change is included.

## Trace and exact rule

The measured `suite/simd-compare/shuffle.c` compound literal becomes private
stack stores and a vector reload, rather than an extract/insert SSA chain.
An insertion-chain rule would miss it. Widening is deferred too. The measured
`reduce.c`, however, already exposes this Full-optimized SSA:

```text
v4 = load.i32x4 v1
v7 = vextract v4, 0
v10 = vextract v4, 1
v11 = iadd v7, v10
v14 = vextract v4, 2
v15 = iadd v11, v14
v18 = vextract v4, 3
v19 = iadd v15, v18
```

The candidate replaces only the pure scalar tree with `v19 = vreduce_add v4`.
The input load, output store, private initialization stores and result reload
remain. Existing DCE removes dead pure extracts/adds. Existing lowering uses
packed register shuffles/adds; the result construction still uses the stack.

The rule accepts only a complete `i32x4` source and `i32` scalar Iadd tree,
with every source lane exactly once and single-use intermediate extracts/adds.
It inspects at most three producer levels below the root with fixed storage.
It rejects conversions, mixed vector sources, partial/repeated lanes, other
widths, shared intermediates, scalar operations other than addition and FP.
The root can have multiple consumers. Fast has no single-use proof and retains
the tree. Associativity is valid modulo 2^32 under existing IR semantics;
frontend C signedness, promotions and evaluation have already been resolved.
The rule never inspects memory or calls, invents lanes, duplicates evaluation,
or changes effect order. It adds no new target instruction or legality bypass.

## Correctness and encoding evidence

All generated material stays outside Git, under `/tmp/keel-c-intent*`.
Rust commands use `python3 suite/fast_opt_locked.py LOG
cargo COMMAND --offline --locked -j2`, with existing toolchains,
features and profiles. Native runs hold the same host lock; the maintained
SIMD runner acquires it itself.

- The complete local optimizer suite passes. `vector_canonicalize` has 14
  tests: all 24 lane permutations across left/right/balanced trees compare
  original and optimized values using 256 seeded wrapping inputs each.
  Negative tests cover missing/repeated lanes, mixed sources, shared sums and
  extracts, integer promotions/extensions and floating sums. Existing tests
  check fixed points, effects and out-of-layout consumers.
- `keel --test c_lane_reduce` passes locally on Apple ARM (debug/release) and on
  LinuxHost (release). It compiles actual C with unaligned byte-copy inputs,
  an intervening external call, a vector live across that call and a scalar
  result. Before/after native objects check 1,024 input vectors (625 exhaustive
  four-lane combinations of five edge values plus random inputs), exact input/
  output aliases, one-byte misalignment, both guard-page edges, canaries and
  single call evaluation. Normal and empty register pools cover forced spills
  across Baseline/Trees/Full selection. Input reads remain exactly 16 bytes.
- The test verifies identical text-assembled/direct objects for both original
  and optimized C at strict v1/v2/v3, v3 with AVX/AVX2/F16C/FMA disabled, and
  baseline Apple ARM. Native test execution defaults to v1 and native Apple;
  a retained LinuxHost helper additionally executes all 18 higher/disabled-profile
  pairs on its independently eligible host. Release emission guards remain
  active and their focused unit test passes.
- Maintained native LinuxHost SIMD correctness passes all 231 None/Fast/Full rows
  across eleven C/IR cases and strict v1/v2/v3, plus capability probes.
  A separate paired Full Apple control passes all 55 old/new/Clang rows.
  The reduction timing campaign also checks all 15 old/new/Clang configurations
  against its independent Python oracle before measurement.

These are finite value, footprint, feature and ABI checks on current machines;
there is no strict-v1-only CPU execution, exhaustive ISA decoding, unmasked FP
exception claim or proof for arbitrary C programs.

## Same-input paired measurement

LinuxHost baseline and candidate are separate archived checkouts built with the
same release/default-feature command. The baseline archive is exactly
`81b2e71c`; the candidate overlays only the optimizer source. The maintained
runner uses identical source paths, Full optimization/selection/allocation,
strict levels, matching Clang `-O2 -ffp-contract=off`, CPU 2 affinity, seed
20261023, one discarded warmup and seven interleaved measured rounds with
1,000,000 repeats over 64 inputs. There are no source/header changes to the
measured reduction case. All preflights pass: maximum one-minute load 0.50342,
CPU 2 and SMT sibling 18 probe busy 0%. Probes do not exclude interruptions,
boost or placement effects; v2 has visible outliers.

C reduction median ns/call (baseline → candidate; Clang reference):

- v1: 5.62615 → 5.44142; Clang 0.82890. Paired baseline/candidate median
  1.03395, range 1.03298–1.03453.
- v2: 5.62622 → 5.44077; Clang 0.84192. Paired median 1.03404,
  range 0.97185–1.18855; one pair regresses.
- v3: 5.62597 → 5.44842; Clang 0.84380. Paired median 1.03274,
  range 1.03144–1.09221.

C text is 120 → 108 bytes at every strict level. Explicit-IR reduction is
unchanged at 73 bytes. CLI-to-object median milliseconds, kept separate from
runtime, are v1 1.83491 → 1.84871, v2 1.91799 → 1.91772, v3 1.82180 →
1.80979. These short startup/I/O-inclusive samples establish no compile-cost
improvement. Build/cache wall time is not compiler-performance evidence.

The roughly 3% paired runtime effect belongs to this warm call/checksum harness,
not instruction throughput or an application gain. The remaining approximately
6.5x Clang gap is substantial. Private result construction, constant permutation
recognition and widening remain queued; this pilot does not address all three.

LinuxHost evidence: `/tmp/keel-c-intent-linuxhost/{base,candidate,paired,correctness}`,
`test.jsonl`, `release-guard.jsonl`, `extra-native.jsonl`, and the retained
`extra_native.py`. Local evidence: `/tmp/keel-c-intent/`, including before/after
IR, baseline binary, paired report copy, native/build logs and `apple-control/`.
Native end-to-end test artifacts use `/tmp/keel-c-lane-reduce-PID/` on each host.

Frozen SHA-256 identities: LinuxHost baseline CLI
`0a67deaa510b5eb75d03aefcd46096f5187f0e5ef0dd0e638ababfe2aff1ebcd`,
candidate CLI `7afd86b030d663834a71f0a1c049472c051157f54e30e950441af7b4c48a8969`,
paired report `ac885d608f72deceacab8068e9659fca9eecb81e987f31b4fb292ffced233c25`.
The measured optimizer source matches the final local source SHA-256
`1957b4b31981247977f2494992c492ea9ca90a0d50078d1fd23cf7df9ef10ba6`.
