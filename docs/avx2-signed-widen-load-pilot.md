# AVX2 signed m128 widening load pilot

This backend-only pilot originally started at
`6ff8911b975a7d18007d1416c9c40abfa457b67f`; review rebased it onto
`96a1fad9eac3e9aa24dc42760ae3706aa6c5c14d`, including the reviewed conservative
context and default 128-bit policy. Final integration review rebased onto
`3a5f4756416c3dbbd27d83ed2f0f425e0f49f5d6`, which repairs the preexisting
AVX-only result-width bug. The combined numerical gates below pass.
It emits `vpmovsxdq ymmResult,xmmword ptr [address]` for an existing native
`load.i32x4` immediately followed by `swiden_low.i64x4`. The optimizer,
capability context, CLI, ABI, and default 128-bit reduction policy are unchanged.
The earlier m64/XMM PMOVSXDQ cover is unchanged.

## Proof boundary

Both effective AVX and AVX2 are required, independently of the nominal CPU
level. The load must be nonvolatile and have exactly one resolved use. Both
instructions must have canonical results, identical complete source locations,
and no existing cover, match, constraint, or condition. The closed pair must
occupy adjacent positions in the same block. Other element types, unsigned or
high widening, shared inputs, and intervening instructions retain existing
lowering. In particular, this does not rewrite generic legacy SIMD loads.

Selection transfers the load's existing selected operands to the widening
root. This keeps selected address leaves live until emission, including resolved
aliases and spilled leaves. Emission uses the existing address materialization
path before writing the allocated full YMM destination, and reads exactly the
original 16 bytes. GPR address registers cannot alias the SIMD destination.
Normal result completion handles a spilled 256-bit destination using the
reserved scratch register. Unrelated live YMM upper halves remain allocated and
untouched; the removed XMM intermediate has no other observer. No FP arithmetic
or FP control instruction is introduced. Adjacency preserves fault order.

## Historical validation before the result-width repair, 2026-10-23

The dedicated `x86_signed_wide_load` integration suite passes six tests in debug
and release. It covers all three selection tiers, both allocators, normal,
one-SIMD-register and empty pools; typed/text/chunked object parity; effective
feature overrides; constraints and full source-location boundaries; shared,
volatile, unsigned, high, other-load-shape and side-effect exclusions; optimized
and dead results; and an independent Clang byte oracle for low/high registers,
indexed memory and negative displacement. Three private wide-cover unit tests
exercise selected leaves, result aliases, conditions, matches, existing covers,
and both independent feature gates.

LinuxHost (Ryzen 9 9950X3D, Linux 7.2.10-1-cachyos) passed all 18 AVX2 native
configurations. Each runs five shapes: direct, offset, indexed, unrelated live
YMM, and wrapping accumulation from UINT64_MAX. The oracle includes signed
extrema, deterministic random inputs, all 16 byte alignments, guard pages on
both ends, exact 16-byte valid/invalid boundaries, overlapping input/output,
live pointer preservation and MXCSR preservation. The C entry point checks
AVX2 support before calling generated code. Both normal and forced-spill cases
run on the CPU. Existing native controls also pass: 108 m64 signed-load objects
and 108 PMULLD objects.

**Historical failure, resolved by `3a5f4756` and the final combined gate below:**
all 54 AVX-without-AVX2 native objects failed the four-lane numerical oracle
(exit 2) before the repair. Their ELF objects **and C harnesses**
are byte-identical to those emitted from an untouched frozen `6ff8911b` archive
with the same dedicated test harness. The old fallback lowering widened only
into an XMM value before a YMM store. The cover itself neither fixes nor bypasses
that path. That historical 72-object native report correctly has `passed:false`;
only its 18 AVX2 cases passed at that time. v1/v2 without AVX retain their
existing rejection of explicit 256-bit IR. The old report provides no evidence
for repaired fallback correctness; that evidence comes from the final gate.

Other passing local checks: all 114 codegen library tests; five baseline SSE2
vector-widen tests; five old m64 cover tests; three active PMULLD tests (two
measurement tests remain ignored); three generic AVX256 tests; wide conversion,
wide transport and explicit-feature suites. The existing illegal-emission
feature guard also passes in release. Existing generic AVX256 test helpers
produce three dead-code warnings; the dedicated pilot tests produce none.
Focused library/pilot Clippy completes successfully with existing repository
lints (no lint-driven source changes).

Builds/tests use the shared default cargo cache and per-checkout targets through
`suite/fast_opt_locked.py /tmp/keel-wide-followup-unique.jsonl`, with
`--offline --locked -j2`, preserving `CARGO_CACHE_LINKS=0`. No installs, toolchain
changes, shared CARGO_TARGET_DIR, or manual output deletion were used.

## One finite runtime probe

The frozen explicit-wide optimized IR was emitted by the pre-existing archived
`/tmp/keel-wide-candidate-4188196c` compiler, then compiled independently by the
frozen main backend and this pilot. No other active branch was merged or edited.
Both backend invocations used exactly:

```
keel compile --target x86_64-unknown-linux-gnu --cpu-level v3 --emit-object INPUT -o OUTPUT
```

Both objects linked the same frozen `driver.o`. All 86 lengths (0 through 81,
257, 4096, 65536, 1048576) passed the independent wrapping-sum reference before
timing. One seven-round paired probe, shuffled with seed 20261023, ran under
the shared investigation lock on CPU 2, at n=1048576 and 256 repetitions per
process. Each process checks and warms up before timing. Load averages remained
low (approximately 0.2). Median old-wide time was 208.603 microseconds; pilot
was 49.214 microseconds (4.239 times faster). This establishes neither the exact
microarchitectural cause nor general profitability across targets and sizes.
No additional timing sweep or compile-time profitability measurement was run.

Matched static loop counts are 45 -> 33 instructions per 16 inputs. Four
legacy loads and eight widening-stage copies disappear. Four fused m128
widenings, four adds, twelve add-stage copies, six address instructions and
seven loop/control instructions remain. There are no accumulator spills; reads
remain four bytes per input. The matched round-tripped symbol is 473 -> 426
bytes and 131 -> 119 instructions. Its size differs from the earlier 454-byte
historical compiler output because textual IR round-tripping changes layout
and allocation; those historical bytes are not a matched control here.

A reference main128 object measured 56.190 microseconds in the same run and has
15 loop instructions per eight inputs / a 256-byte symbol. Fresh pilot C
compilation with the original `cc -O --target ... --cpu-level v3 --emit-object`
flags produced byte-identical full `.text` to that frozen main object. However,
a subsequent check found that **round-tripping main128 through textual IR does
not reproduce that object** (the symbol becomes 305 bytes). Consequently the
observed 12.4% lower time than the main128 reference is not a controlled
same-input-path width-policy comparison. No controlled profitability claim
against main128 is made, and no second timing probe was run. These historical
timings predate the conservative-context rebase; no runtime profitability probe
of the combined context and cover has been performed.

## Historical independent review before the result-width repair, 2026-10-23

At rebased cover commit `309bd8af`, the dedicated suite again passes all six
tests in debug and release, including the independent Clang encoding oracle.
The 114 library tests and focused baseline widening, m64 signed-load, PMULLD,
generic AVX256, explicit-feature, wide-conversion and wide-transport suites
also pass (the two existing PMULLD measurement tests stay ignored).
Commands are recorded in `/tmp/keel-wide-load-review-unique.jsonl`, serialized
through `suite/fast_opt_locked.py`, using cargo with `--offline --locked -j2` and
`CARGO_CACHE_LINKS=0`.

That pre-repair LinuxHost run of all 72 exported configurations passed the 18 AVX2
cases and failed all 54 AVX-without-AVX2 cases with exit 2. All 72 ELF objects and C
harnesses are byte-identical to the original pilot exports; all 54 negative
ELF objects and harnesses are also byte-identical to the frozen `6ff8911b`
baseline exports. The local report is
`/tmp/keel-wide-load-review-native-results.json`; fresh exports are in
`/tmp/keel-wide-load-review-20261023` locally and on LinuxHost. This rerun is a
correctness oracle, not another timing probe.

Fresh m64 and PMULLD control exports are byte-identical to their previously
passing native controls (108 objects per suite, plus their C harnesses).
Those control suites were rerun locally; their native executions were not
repeated during review. Exports are in
`/tmp/keel-wide-load-review-{m64,pmulld}-20261023`.

No cover-path correctness blocker was found. At that historical review, approval
was conditional on the separately owned fallback correction and a combined
numerical rerun. Both conditions are now satisfied by the final review below.

## Final combined review, 2026-10-23

Tested clean source HEAD `4e34ad1b226a86aaff53afa48fd8ec0f84eda613` combines
cover `aa1cd06a` with main repair `3a5f4756`. The cover selector and dedicated
tests are unchanged from reviewed `5380135a`; the width repair is unchanged
from main. No unexpected source conflict or additional code change was found.

Fresh `x86_signed_wide_load` exports pass **72/72 native configurations** on
LinuxHost: 18 v3 AVX2, 18 v1+AVX, 18 v2+AVX, and 18 v3-minus-AVX2. Every object
builds and exits 0; none is filtered, skipped, or treated as an expected failure.
All 72 C harnesses and all 18 AVX2 objects remain byte-identical to the prior
review exports. The historical 54 AVX-only failures are resolved.

Fresh `x86_avx_widen_fallback` exports also pass **121/121 native bundles**:
72 widening matrices across the same four profiles, all 48 narrow/bitcast
controls, and the minimal signed-lane repro. These include ABI, source/live YMM,
alias, guard-boundary, exact-footprint and feature-legality checks described in
[the fallback repair](avx-widen-fallback.md). The independent C runner uses
`clang -O2 -march=x86-64 -msse2 -mno-avx -Wall -Wextra -Werror`.

The bounded local batch passes 40 active tests: debug cover/fallback/m64/PMULLD
(18), release cover/fallback (10), release illegal-feature emission (1), private
wide-cover gates (3), and CLI/library optimizer contexts (8). Only the two
existing PMULLD measurement tests are ignored. No AVX-only correctness case is
ignored. Default CLI/library contexts still retain 128-bit output, and explicit
wide optimization still requires the complete capability recipe and preference.
On LinuxHost, fresh combined-backend objects for both the frozen explicit-wide IR
and default-128 C compilation each pass all 86 independent wrapping-sum lengths
(0 through 81, 257, 4096, 65536, 1048576). This is a correctness-only check,
not a new timing campaign or a profitability claim.

All Rust invocations use `CARGO_CACHE_LINKS=0`, shared-default-cache cargo and the
checkout-managed target through:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-wide-load-combined-unique.jsonl cargo COMMAND
```

Exact command arrays, statuses and lock metadata are in that JSONL. The bounded
commands are `test --offline --locked -j2` with these arguments:

- `-p keel-codegen --test x86_signed_wide_load --test x86_avx_widen_fallback --test x86_signed_widen_load --test x86_pmulld_selection`
- `--release -p keel-codegen --test x86_signed_wide_load --test x86_avx_widen_fallback`
- `--release -p keel-codegen --lib emission_reports_illegal_features_in_text_and_structured_modes`
- `-p keel-codegen --lib x86_64::packed_load::wide_tests`
- `-p keel-opt -p keel --test vector_context`

The debug export environment is
`KEEL_WIDE_LOAD_EVIDENCE=/tmp/keel-wide-load-combined-20261023` and
`KEEL_AVX_FALLBACK_EVIDENCE=/tmp/keel-wide-load-combined-fallback-20261023`.
Those directories exist locally and on LinuxHost, with fresh ELF/C/assembly inputs
and `run_native.py`. Each remote runner was invoked separately under
`python3 /tmp/keel-wide-load-combined-lock.py /tmp/keel-wide-load-combined-unique.jsonl python3 DIRECTORY/run_native.py`;
there is no nested lock. The copied lock script is the repository's
`suite/fast_opt_locked.py`. Remote command metadata is copied locally as
`/tmp/keel-wide-load-combined-linuxhost-unique.jsonl`.

Fresh local reports and SHA-256 identities:

- `/tmp/keel-wide-load-combined-native-results.json`:
  `1cff7cc69c972d102e1dc6759233ac2f0c4c82c09b439b5c1fc99203c939779c`.
- `/tmp/keel-wide-load-combined-fallback-native-results.json`:
  `1f04307cf4c37208e9230f00d75660e6e97e5ae864ca7f988b7041914e5a227f`.
- `/tmp/keel-wide-load-combined-manifest.json`:
  `c65816d5a1d24734be2926215179dbbf2949ef3241f68290d41224e21401cdcb`.
  This records tested HEAD, every exported artifact hash, exact source hashes,
  command-log hashes, numerical-check runner and context results.
- Combined `x86_64/vector.rs`:
  `654bea43934bbb36fc2157f43ace8147115ad758559e7e948cfa8e2807b14a6a`.
- Cover `x86_64/packed_load.rs`:
  `3751e95f208ec50e7fca446fd7dd5dd3ec7646b4da02b1718505c6bc601d700c`.
- Dedicated cover test:
  `e2bf2423436999ffdc6f91adbdea02adfca4b2494b2ce776c229373dccc2512a`.
- Dedicated fallback test:
  `62418451c5c39e58ac842a6c1217e180db016223d8ad39dc90909601cb547396`.

Context numerical results are `/tmp/keel-wide-load-combined-context-results.json`
locally and `/tmp/keel-wide-load-combined-context-20261023/results.json` on LinuxHost.
The correctness runner is `/tmp/keel-wide-load-combined-check.py` on both hosts;
its hash is `709113cde8b28134de12a6d0ded625ccc687f99818d255076cf2811357e974ac`.
The explicit-wide input and frozen driver retain the provenance hashes below.
The fresh explicit-wide object is
`54399ae6cbe0e0eaf0e37c19198126e49b8948228ea9d22ead3f687524823e1b`;
the default-128 object is
`b2794a7a2bb9f49d9cd584250d84ec360124bb3f9740e9f0a3c2c680ff0c01ff`.

No new blocker was found: **unconditional integration is recommended**.
Validation is bounded to these suites and native execution on an AVX2-capable
CPU with AVX-only code generation; it does not include an AVX-only physical CPU,
a full suite, a new performance campaign, or automatic wide profitability.
The historical 208.603 -> 49.214 microsecond old-wide gain and the main128
round-trip parity limitation above remain historical evidence only.

## Evidence outside Git

Local artifacts include `/tmp/keel-wide-load-pilot-final-20261023`,
`/tmp/keel-wide-load-baseline-final-oracle-20261023`,
`/tmp/keel-wide-load-fallback-final-comparison.json`,
`/tmp/keel-wide-load-static.json`, `/tmp/keel-wide-load-probe-results.json`, and
`/tmp/keel-wide-load-{final,m64,pmulld}-native-results.json`.
LinuxHost retains the corresponding oracle directories, plus
`/tmp/keel-wide-load-probe-20261023/{results.json,*.disasm}`.
`/tmp/keel-wide-load-probe.py` records the exact runtime procedure.

Frozen input/provenance SHA-256 values:

- Explicit wide IR: `e4dd1f603a9de6d0c732d778c5db04030adcabc757f6e66be89ccf8da6cc5cc9`.
- Historical candidate archive: `d73f7fb130c5beb896e863037cd895f676c63d9426c893044308198db0ce488a`.
- Historical candidate compiler: `190070841cc9cb745fa08cca56b418c6400abca9f77891ad38db8c80eab98b95`.
- Frozen main compiler: `b592f1effcbc152760f6162ec312671e3be180683d55b8f3a1cb8d9ef3cf5728`.
- Pilot compiler: `aa5be10531916a8cd1abe7f42db2909012a71176ba7b4804879524d265497842`.
- Frozen driver object: `c65e69b958a6252f46a8ccb5246608fa5b410388d89481e8379107e3535a461f`.
- Runtime procedure: `635957505ab3cb8525f4ae419957359594605fd812b2b73accce2879ec762840`.

The runtime JSON additionally records each timed object's SHA-256 and all 21
observations. Generated artifacts and the patch/source-hash manifest are kept
outside Git. Integration and cleanup remain with the parent reviewer.
