# Current compiler research: dbe0b1f6

Frozen comparison: `8690a355b2986288e5b53b6401a79ba342cc16f9` to
`dbe0b1f65f8ed0a18d84e27fe8644529a0d64810`, 2026-10-23. Production
128-bit preference is primary. Explicit library-wide preference is a separate
experiment. This work owns only the maintained SIMD runner, its focused test,
and documentation; it changes no compiler, feature, ABI, FP or default policy.
No sibling work was integrated and no agents were spawned.

The completed small-SIMD campaign establishes a modest improvement in one
explicit-IR reduction, with unchanged C workloads. The long-loop campaign
**was deferred for contention** and supports no runtime or compile-cost claim.
There is no application score, geomean, automatic-v3 win or width-policy verdict.

## Frozen method and correctness

Both revisions use the existing release/default-feature build, Full optimization,
Full selection, Full allocation, generic tuning and integrated object assembly.
No native tuning, LTO or fast-math was added. All Rust commands used
`python3 suite/fast_opt_locked.py /tmp/UNIQUE.jsonl
cargo build|test --offline --locked -j2 ...`. The existing toolchain
and lockfile were preserved. LinuxHost uses rustc 1.99.0 (`b940084d7`), Clang
23.1.1 and native Ryzen 9 9950X3D; Apple MacHost uses rustc 1.99.1
(`58cf8f9c3`) and Apple Clang 21. Each old/new pair uses the same host toolchain;
ARM and x86 timings are never pooled. Baseline rebuilds were no-ops and retained
the previously frozen binary hashes. Rust build/cache wall time is not a
compiler performance measurement.

The maintained width provenance helper verified archived Git blobs and dedicated
helper overlays. Source, driver, compiler, input, object and executable hashes
are retained in the reports. Both revisions compile each workload from the same
source path with identical arguments. All native work holds the shared
`/tmp/keel-perf-20261019.lock`; no wrapper is nested.

Before timing:

- The maintained SIMD runner passes all 165 x86 configurations: eleven cases,
  strict v1/v2/v3, C/IR Full at both revisions, plus matched Clang C `-O2`.
  Its feature-intersection check establishes native CPU/OS eligibility first.
  Each configuration checks 64 oracle rows, every result byte, exact aliases
  and canaries; explicit IR also checks unaligned accesses. C frontend
  acceptance, backend/object emission, disassembly and native results remain
  separate report fields. Wide capability diagnostics are checked independently.
- The same runner passes 55 native Apple ARM configurations. Every old/new
  assembly matches. No Apple timing was attempted on the loaded host.
- Maintained width controls pass at both revisions for v1, v2, v3, v3-minus-AVX2
  and native Apple ARM. CLI/default-library objects and optimized IR match;
  standalone and same-process paired objects/IR match. Both width requests are
  identical outside v3. Each feature/width guard checks 26,568 finite signed-sum
  cases, including tails, alignments, guarded-page edges and seeded sums.
- The paired long-loop objects pass 2,838 independent Python-oracle checks for
  positive32, adjacent and widen32, lengths 0–81, 257, 4096, 65536 and 1048576.
  CLI/library and paired-process parity is rechecked using the common input path.
  A finite mnemonic/operand exclusion scan supplements release legality.
- The release emission-feature guard passes on Apple and LinuxHost. Seven SIMD
  Python tests, nine runtime-kernel tests and three width-helper tests pass.

These are bounded tests on newer CPUs, not execution on strict-v1/v2-only CPUs,
exhaustive ISA decoding, every FP exception configuration or arbitrary ABI proof.
The SIMD FP case uses exactly representable finite inputs, with Clang
`-ffp-contract=off`; it establishes no additional FP-environment semantics.
Integer reduction changes preserve wrapping arithmetic. V2/v3 narrow PMOVSXDQ
reads remain exact 8-byte inputs, explicit-wide VPMOVSXDQ reads exact 16-byte
inputs, and scalar tails exact 4-byte inputs. V1 retains its existing fallback;
the maintained guards validate the finite tested footprints.

## Completed paired SIMD measurements

`suite/simd-compare/run.py --baseline-keel OLD --keel NEW --variants full
--profiles v1 v2 v3 --benchmark --rounds 7 --repeats 1000000 --seed 20261023
--cpu 2` uses one discarded warmup and seven seeded interleaved measured rounds.
Profile/case/route groups are shuffled, then old/new/Clang arms within each
group run adjacent. One retained driver per profile is shared by both revisions;
Clang receives matching `-march=x86-64[-v2|-v3] -O2 -ffp-contract=off`.
The driver includes external call, checksum and volatile sink overhead, with
64 million calls per sample. It is a warm small-kernel experiment.

All 24 preflights pass: maximum one-minute load 0.97754, maximum CPU 2/18 busy
7.70% in 250 ms probes. Both CPUs are checked, including the SMT sibling.
The shared lock cannot exclude unrelated ROS work, which was visible in process
captures. Passing probes do not fix boost, interrupts, thermal state or physical
code placement. Compiler/source/artifact identity checks bracket measurements.

Only explicit-IR `reduce` changes native text at each level: 67 to 73 bytes,
20 instructions in both versions, frame reservation 32 to 16 bytes. Register
shuffle/add/extract replaces a vector stack store and four scalar lane reads.
C `reduce` uses ordinary lane expressions and does not gain this IR selection.

| Strict profile | Old ns/call, median [range] | Current ns/call, median [range] | Paired old/current, median [range] |
| --- | --- | --- | --- |
| v1 | 1.31385 [1.30909–1.33470] | 1.29723 [1.29364–1.30025] | 1.01281 [1.00875–1.03174] |
| v2 | 1.31293 [1.30996–1.31724] | 1.30073 [1.29474–1.30355] | 1.01004 [1.00703–1.01344] |
| v3 | 1.60037 [1.59427–1.60307] | 1.58493 [1.58261–1.58744] | 1.00886 [1.00603–1.01291] |

All 21 pairs favor current, but this approximately 1% effect belongs to this
call/checksum harness. It is not an isolated instruction throughput result.
The other 63 C/IR/profile combinations have identical old/new text; their median
paired ratios range 0.99247–1.00157. In particular, every C case is unchanged.
All 1,155 raw runtime and 1,155 compile samples survive, including ranges.
CLI-to-object compilation includes startup/I/O and excludes driver/linking;
old/current ratios of compile medians range 0.98634–1.01134 across cases.
This establishes no useful compiler-throughput improvement.

For current v3 C, matched Clang provides useful bounded reference points:

| Case | Keel median ns/call | Clang median ns/call | Paired Keel/Clang median [range] |
| --- | ---: | ---: | --- |
| widen | 5.26634 | 0.82643 | 6.37221 [6.24963–6.44724] |
| shuffle | 5.62853 | 0.85149 | 6.60552 [6.58776–6.63404] |
| reduce | 5.62647 | 0.85025 | 6.61744 [6.53665–6.69627] |
| add | 1.58381 | 0.84860 | 1.86072 [1.85269–1.86990] |

These are individual observations, not a combined score. Separate drivers are
compiled for each strict level. Cross-level runtime differences must not be
attributed to Keel instructions when identical function bytes use different
profile-specific drivers. Current v2/v3 SIMD function bytes match except
float-add's `addps` versus `vaddps`; v1/v2 additionally differ for multiply.

## Long loops: static evidence, timing deferred

The campaign uses unchanged maintained kernels/driver/oracle, one common Clang
`-O2` driver object, and Clang kernel controls at matched strict levels. Default
old/new arms and Clang are interleaved; explicit old/new 256-bit arms participate
only in the separate v3 widen32 group. Lengths are 7, 8, 15, 16, 17, 257,
4096, 65536 and 1048576. Repetitions follow the maintained width procedure:
`min(1000000, max(256, 67108864 // n))`.

After warmup and one measured round, the final quiet check records load
**1.06494**, exceeding the predefined maximum 1. CPU 2 busy is 3.85%, sibling
18 idle. The runner aborts with `complete:false`, preserving 261 runtime
samples and 11 compile rows. None is used for ratios, speedup, crossover or
compile-cost conclusions. No replacement campaign was attempted.

Exact function-byte hashes change for eight functions at all strict levels:
`positive32`, `positive64`, `balance32`, `balance64`, `clamped`, `unsigned_high`,
`widen32`, `widen_u32`. `adjacent`, `sensor_mix`, `buckets`, and `run_balance`
remain identical. The former group gains register-based wrapping finishing;
loop bodies are unchanged at default width. Current default-width function
bytes are identical between v2 and v3 for **all twelve functions**.

| Function/profile | Old → current symbol bytes | Old → current instructions |
| --- | --- | --- |
| positive32, default v1/v2/v3 | 397 → 398 | 110 → 109 |
| adjacent, default v1/v2/v3 | 137 → 137 | 48 → 48 |
| widen32, default v1 | 378 → 379 | 105 → 104 |
| widen32, default v2/v3 | 256 → 257 | 77 → 76 |
| widen32, explicit-wide v3 | 313 → 292 | 90 → 84 |

Instruction counts in `static-audit.json` use exact `nm` symbol extents,
excluding inter-function alignment. The initial `results.json` diagnostic
counts include trailing padding; use the corrected static audit for counts.
A byte increase can coexist with fewer instructions and less data-memory work.

For v3 widen32, narrow's steady loop remains 15 instructions / 61 bytes per
eight inputs. Its finish changes 8 / 31 to 7 / 32 instructions/bytes. The wide
loop changes 21 / 82 to 15 / 61 per sixteen inputs: three `mov/add` address
pairs disappear in favor of `[base+16/32/48]`. Wide finishing remains 9 / 44.
These are static encoding observations. Neither the final address change's
runtime effect nor current wide/narrow profitability is established here.
Historical wide timing in [the earlier campaign](controlled-signed-sum-width.md)
belongs to its frozen revision and is not reused as a current result.

## Remaining work, ranked by this evidence

1. C lane-based widening/shuffle/reduction remains the clearest measured gap.
   Match supported C intent to existing typed operations while preserving
   evaluation, ABI, FP and exact memory behavior. New header wrappers do not
   fix these existing source expressions.
2. Long-loop recognition and lowering remain the main unmeasured opportunity.
   Adjacent stays scalar, positive32 retains its existing loop, and v3 adds no
   default-width function changes over v2 in this corpus. Establish a quiet
   seven-pair campaign before prioritizing by runtime rather than code shape.
3. Small-kernel frames and argument movement remain visible overhead: even
   simple add takes about 1.86x matched Clang time in this driver. Call overhead
   and backend prologues matter; no allocator spill count is inferred from stack
   references.
4. Explicit-wide signed sum is a bounded follow-up. Address construction is
   improved and it processes twice the inputs with the same static loop count,
   but it remains an opt-in library experiment needing valid short/long timing.
   Production default128 remains unchanged. Further one-time finish tuning has
   less static scope than the still-unchanged loop/front-end paths.

New source acceptance is separate: representative `_mm_slli_epi32`,
`_mm_adds_epi8` and `_mm_cvtepi32_ps` functions reject at the frozen baseline
and pass C-to-IR plus strict-v1 object emission at current. These probes are
compile-only; their dedicated maintained native suites provide earlier semantic
coverage. No new shift/saturation/conversion timing case was added: the
unchanged workloads contain none, and no new paired typed-IR equivalence
benchmark was established. No speedup of existing loops is attributed to them.

## Artifacts and reproduction

All generated files are outside Git. LinuxHost paths:

- `/tmp/keel-round-dbe0-simd-linuxhost/`: report, sources/inputs, objects,
  assemblies/disassemblies, executables, all commands/samples and paired summary.
- `/tmp/keel-round-dbe0-runtime-linuxhost/`: incomplete report and raw samples,
  frozen common source, driver/object/binary hashes, static/loop audits.
- `/tmp/keel-round-dbe0-{baseline-width-controls,width-controls}/`:
  maintained native guards, feature controls and CLI/library/paired parity.
- `/tmp/keel-round-dbe0-new-source/`: separate source-acceptance probes.
- `/tmp/keel-round-dbe0-{runtime,summary,static,acceptance}.py`:
  retained campaign/audit helpers. The runtime helper runs only under the shared
  lock. Stop at its retained contention failure; reruns need a new output path.
- `/tmp/keel-round-dbe0-research-current/`: verified current source/build;
  `/tmp/keel-width-current-8690a355-frozen/`: verified baseline.

Local evidence copy: `/tmp/keel-round-dbe0-linuxhost-evidence/`.
Apple controls are `/tmp/keel-round-dbe0-simd-apple/` and
`/tmp/keel-round-dbe0-apple-{baseline-width,width}/`. Build/guard lock logs use
the `/tmp/keel-round-dbe0-` prefix on their respective hosts.

LinuxHost CLI SHA-256, old/current:
`b5758a82e674b5dc4ef68312b28fae7596ef849d7f589f43645b1a6bf0485d1a` /
`0a67deaa510b5eb75d03aefcd46096f5187f0e5ef0dd0e638ababfe2aff1ebcd`.
SIMD report SHA-256:
`58a58a04ce75b1cec2eb97b122e91a298a9c5faf723da1240c577b9306dcfdc6`.
Incomplete long-loop report SHA-256:
`592e216c43c38ce91332c8e54186bb2c8074b31d89c3c80de1f8c0afad8a6513`.
The frozen SIMD measurement runner hash is
`e30aebfd25a179dd0a14c6e99ef0856ff6cba3b3505bacfc66b55c4d7fead00a`.
Final runner changes additionally reject missing Linux affinity, verify identities
at round ends and preserve the original unpaired scheduling. They do not change
this campaign's paired order or measurements. Final check-only reruns validate
that maintained version without spending another timing campaign.

## Subsequent bounded C-intent pilot

The [C lane reduction pilot](c-lane-reduction-pilot.md) traces the actual C
SSA at baseline `81b2e71c`, recognizes only complete four-lane i32 wrapping
extract/add trees in instcombine, and records independent native controls plus
a separate seven-round paired campaign. Its modest reduction-only result does
not change the frozen measurements or the widening/shuffle priorities above.
