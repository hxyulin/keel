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
`81b2e71c`; the candidate overlays the optimizer source and its regression tests; the only
production source difference is the optimizer rule. The maintained
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


## Independent review, 2026-10-23

Candidate `4b4f712dd4aa207d7ee20af9beb3c10cfd5e26d6` was reapplied onto
unchanged main `81b2e71c6f69844a549e9ba916ff164332171a8a` in the isolated
`review/c-lane-sums` worktree. No production correctness defect was found;
review changes own only the focused optimizer/C tests and this document.
No frontend, backend, framework, schedule or imported-byval pilot changes
were made, and no sibling work was integrated.

The audit checked resolved source identity, exact `i32x4` shape and `i32`
intermediates, checked extract indices, instruction ownership, single-use
counts, multiple root users, commutative operand ordering and canonicalization.
The walk has depth three below each root operand, at most fourteen node visits,
fixed-size state and no allocation or unbounded producer walk. Its shared lazy
use-count table can require one function-wide operand scan; this is existing
instcombine infrastructure, not constant total optimizer work. Fast's unknown
use counts disable the match. Existing pass invocation and worklist caps remain
unchanged. No new compile-cost improvement or negligible-overhead claim follows
from the short CLI samples.

All 24 lane permutations now exercise all five binary tree shapes against the
wrapping oracle. A dedicated instcombine test uses aliased vector operands,
multiple root consumers, signed/unsigned result extension and interleaved
volatile loads/stores and a call. It checks effect instruction identities,
operands, flags, layout order and source locations, plus the replacement root's
source location. Existing fixed-point tests also cover the new tree shapes.
Verified IR has checked immediate lane indices and no undef/poison vector
producer; no dynamic/undefined-lane form is inferred by this matcher.

The native C fixture now independently checks unsigned modulo before zero
extension, mixed signed/unsigned operands, signed sums before sign extension,
and signed widening **before** addition. Signed C sums use bounded inputs so
every intermediate stays in range; only unsigned sums claim C modulo overflow.
The widened scalar oracle includes the original signed extrema. A separate
call mutates the input, checking the original vector read before the call and
an observed scalar read afterward. Existing alias, guard-page, unaligned,
canary, single-call and forced-spill checks remain. Negative C cases retain
scalar operations for signed/unsigned short promotions, widening-before-add,
dynamic indices, repeated lanes, distinct vector arguments and FP sums. They
also compile with debug information. The C gate now checks chunked/text/direct
whole-object parity at v1/v2/v3 and effective v3 without AVX/AVX2/FMA/F16C
(normal pools; text/direct parity also covers empty pools).

Fresh review validation:

- Complete local debug optimizer suite: 585 passed, two existing ignored tests.
- Focused `vector_canonicalize`: 15 passed in debug/release locally and on LinuxHost.
- Expanded `c_lane_reduce`: two passed in debug/release on native Apple ARM64
  and LinuxHost. Cross-target emission is distinct from native execution. LinuxHost
  additionally executed all 18 higher/disabled-profile native object pairs
  after checking host feature eligibility.
- Release `x86_64::feature_guard_tests` passed locally and on LinuxHost, retaining
  illegal-feature rejection on text and structured paths. Native execution
  still uses a newer CPU, not a strict-v1-only machine.
- Formatting and patch-whitespace checks passed for the changed files.

The independent provenance audit compared all baseline archive files with
main, finding no differences. Among main's files the candidate archive differs
only in optimizer source and the original optimizer regression test file; this
corrects the earlier overly narrow archive description above. All 87 paired
report input/artifact identities, the two frozen CLI hashes and report hash
match. Re-running both frozen CLIs reproduces all six retained strict-level C
assemblies byte for byte; regenerated optimized IR contains the reduction only
for the candidate. Recomputed paired medians/ranges and 120/108 text sizes
match the figures above. The retained complete correctness reports contain
231 LinuxHost and 55 Apple passing rows; those full campaigns were inspected, not
rerun. No timing rerun or new performance conclusion is claimed.

Review evidence is under `/tmp/keel-lane-review-64c8f7ab` on both hosts,
`/tmp/keel-lane-review-*.{jsonl,log}` and
`/tmp/keel-lane-review-{apple,linuxhost}-{debug,release}-objects`. The LinuxHost snapshot
is a baseline archive plus this review's optimizer/test overlay, with its own
cargo-managed target. All Rust commands used the existing toolchain and
`python3 suite/fast_opt_locked.py LOG cargo
COMMAND --offline --locked -j2`; native/provenance helpers held the same shared
lock. Finite tests do not prove arbitrary fault ordering, every C program,
exhaustive ISA exclusion or FP-environment behavior. The C construction stack
traffic and large Clang gap remain; shuffle/widen recognition remains queued.
