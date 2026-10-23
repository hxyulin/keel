# SSE4.1 signed low dword register widening

The first pilot in [the SIMD selection audit](x86-simd-selection-audit-dbe0b1f6.md)
is implemented from clean base `81b2e71c6f69844a549e9ba916ff164332171a8a`.
Only `x86_64/vector.rs::vector_convert` changes production behavior:
`swiden_low.i64x2` from exactly `i32x4` selects
`pmovsxdq destination, source` when the effective `sse41` bit is enabled.
The destination is the allocated result register, or reserved FS0 for a spill.
This reuses `V2Opcode::Pmovsxdq`; a CPU-level label is not the eligibility test.

The source is resolved/read before defining the destination. Register aliasing
is legal, live allocated sources are not overwritten, and a spilled input is
reloaded from its complete **16-byte vector home** before the register operation.
The existing completion helper stores a spilled result at its full 16-byte
width. This selector never emits a memory-source PMOVSXDQ or folds a user load.
The earlier m64/m128 covers retain priority and their existing footprints.

V1 and explicit `-sse41` retain the SSE2 recipe. Unsigned/high widening, other
lane widths, 64-bit vectors, FP conversions/bitcasts and any 256-bit source or
result retain their existing paths. PMULLD selection and the AVX-only wide-result
fallback are unchanged. No dispatch, output-state, ARM, FP, ABI, tuning or default
policy changes accompany the pilot. C frontend recognition and vectorizer
formation remain separate: this improves already formed IR, including ordinary
scalar loops in the maintained corpus; it adds no numeric vector C builtin.

## Finite gates

`vector_widen_x86` now explicitly selects v1 for its older SSE2 corpus. Its new
register tests select v1/v2/v3, v1 plus SSE4.1, and v2/v3 minus SSE4.1, through
all three selectors, both allocators and normal/constrained/empty vector pools.
Controlled layouts test equal/distinct registers, independent input/result
spills and all spills. Exact instruction windows require the allocated
destination and adjacent full-width reload/store; resolved SSA input aliases
and shape/operation negatives are checked separately. Text, typed and chunked
objects agree, and every typed instruction fits the effective feature set.
An independent Clang encoding test compares all 256 XMM register pairs and
checks feature rejection/acceptance. The ordinary emission guard remains
active and passes in debug and release.

Both profiles passed 29 focused tests (including the separately added register
encoding test), plus one emission-guard unit test. Retained suites are
`vector_widen_aarch64`, `vector_widen_x86`, `x86_signed_widen_load`,
`x86_signed_wide_load`, `x86_avx_widen_fallback`, `x86_wide_conversions` and
`x86_pmulld_selection`; the latter's two ignored measurements were not run.
Clippy completed with existing library warnings and none in the changed test.
The ARM widening suite executes natively on Apple ARM64, and the unchanged
maintained ARM kernel object is byte-identical, retaining `sshll` controls.

On LinuxHost (Ryzen 9 9950X3D), Clang passed all 126 new debug register bundles and
159 release bundles (126 register bundles plus 33 retained widening/bitcast/stream
bundles). GCC passed the 126 release register bundles. These use independent
unsigned bitwise signed-extension oracles, signed extrema, every source bit,
512 deterministic random vectors, offsets 0..15, aliased outputs, sentinels,
live sources and source/result capture across external calls that clobber all
sixteen XMM registers. Exact 16-byte input/output page edges and crossing-input
faults before publication are checked, together with unchanged MXCSR.
The retained 108 m64 and 72 wide load-cover bundles also passed with Clang.
An optional GCC m64 run failed at compilation of existing harnesses because
`-Werror` promoted `misleading-indentation` and `clobbered` warnings; it provides
no GCC native result for those controls. Their source was not changed.
Execution is on a newer host, not feature-masked hardware; these are finite
value/ABI/footprint gates rather than exhaustive ISA certification.

## Actual kernel counts and paired samples

Frozen baseline/candidate compilers compile unchanged
`suite/runtime-kernels/kernels.c` through the maintained width driver. Independent
LLVM objdump decoding counts instructions inside ELF function symbol sizes,
excluding padding. At v2/v3 and v3 minus AVX2, for both width preferences:

- `rk_balance32`: four register widens; 163 -> 147 instructions and
  610 -> 542 bytes. Its vector-loop block changes from 85 to 69 instructions.
- `rk_clamped`: two register widens; 158 -> 150 instructions and
  602 -> 568 bytes. Its vector-loop block changes from 65 to 57 instructions.

Every other function's instruction-byte hash is unchanged. V1 whole objects
at both widths, explicit v2/v3 `-sse41` whole objects, and the baseline Apple ARM
object are byte-identical. This confirms the six observed five-to-one recipes;
it does not count the existing widening-load covers as new opportunities.

LinuxHost measurements hold the shared lock, pin CPU 2, and require load below 1
and sampled CPU busy fraction below 10% before each round. Eight randomized
paired rounds include one discarded warmup and seven measured rounds, with
identical seeded inputs, the unchanged maintained driver and independent Python
oracle. There are 252 correctness checks over three kernels, three profiles,
two compilers and fourteen lengths, and 288 raw runtime samples. Timed lengths
are 65,536 and 1,048,576, with at least three calls per sample and one common
executable path. At those lengths the median elapsed time per call changes:

- V2 `balance32`: 21.235 -> 16.777 us; 333.662 -> 262.433 us.
- V3 `balance32`: 21.190 -> 16.794 us; 333.742 -> 262.623 us.
- V2 `clamped`: 33.125 -> 27.531 us; 523.650 -> 433.654 us.
- V3 `clamped`: 33.221 -> 27.503 us; 524.016 -> 433.481 us.

Byte-identical v1 controls vary within 0.31%; unchanged v2/v3 `widen32` varies
up to 1.33%. No host settings changed. Short samples, physical code placement
and one execution host limit extrapolation. These are bounded kernel samples,
not application performance or a general width-profitability verdict.

Compile cost is separate: seven measured pairs of the **debug** width driver
on MacHost compile the entire twelve-kernel corpus, excluding Rust build time
and retained final verification/text capture. Baseline/candidate median pipeline
times are v1 26.099/26.133 ms, v2 26.145/25.937 ms and v3 26.000/26.241 ms.
This does not establish a compiler-throughput change.

## Reproduction and evidence

Use the existing pins and default cargo cache/managed checkout targets:

```sh
KEEL_X86_WIDEN_EVIDENCE=/tmp/OWNED/register \
  KEEL_SIGNED_LOAD_EVIDENCE=/tmp/OWNED/m64 \
  KEEL_WIDE_LOAD_EVIDENCE=/tmp/OWNED/wide \
  python3 suite/fast_opt_locked.py /tmp/OWNED/tests.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test vector_widen_x86 --test x86_signed_widen_load \
  --test x86_signed_wide_load --test x86_avx_widen_fallback \
  --test x86_wide_conversions --test vector_widen_aarch64 \
  --test x86_pmulld_selection
```

Repeat with `--release`; run `--lib feature_guard_tests` separately in both
profiles. Execute exported ELF+C bundles on Linux through the existing
`tests/x86_widen/run_native.py --dir DIRECTORY`, under the shared lock.
Builds and all compiler/native commands were serialized; link caching remained
disabled. No toolchain, compiler wrapper, other checkout or main branch changed.

Artifacts are under `/tmp/keel-register-widen-ec82` on MacHost and LinuxHost.
`compiler-hashes.json`, `static-counts.json`, `disabled-arm-control.json`,
`static-compile.json`, `bench-results.json`, `bench-summary.json`, native result
JSON, logs, lock records and task-owned reproduction scripts retain provenance
and raw evidence. Baseline/candidate source snapshots and frozen debug binaries
are on MacHost; LinuxHost retains exact exported objects, C oracles and executables.
Bulk outputs are outside Git. Independent review/integration remains separate.

## Independent review, 2026-10-23

Candidate `fbee40a1cec5e490ccebf24445eb88792ce1eabb` was reapplied onto
unchanged main `81b2e71c6f69844a549e9ba916ff164332171a8a` as `ae9d0816`
in the isolated `review/x86-sse41-signextend` worktree. No production defect
was found. Review confirmed resolved use locations, allocated destinations,
legal same-register operation, reserved FS0 for independent/all spills,
full-width reload/completion, earlier load-cover priority and typed SSE4.1
metadata through the ordinary release-active emission guard. No frontend,
FP, ABI, unsigned/high/wide conversion or reduction behavior was changed.

Fresh local debug and release gates each passed the 29 focused tests above
and one emission-guard test. The retained 128-bit wrapping reduction suite
also passed its three active tests in each profile. Its one compile-cost test
and PMULLD's two measurement tests remained ignored. The gates include all
three object routes, effective feature overrides and the 256-register-pair
Clang encoding comparison. Local execution is Apple ARM64; x86 objects were
cross-emitted there, then executed on LinuxHost.

Fresh LinuxHost runs passed 126 debug register bundles with **each of Clang and
GCC**, plus 159 release widening/bitcast/stream bundles, 108 release m64 load
bundles and 72 release wide-load bundles with Clang. Native commands held the
shared lock. This review did not rerun the optional GCC m64 batch or claim
its previously failed harness compilation as a pass.

The frozen benchmark baseline's production sources, Cargo files and suite
match main; the candidate's match the submitted production change. The copied
width driver, runtime driver and kernel input match their maintained sources,
and all four frozen compiler hashes match the recorded provenance. The
`candidate-source` compiler-build snapshot contains the older
`vector_widen_x86.rs`, before the final pilot tests were added; that test file
is not linked into the measured compiler. Final test evidence instead comes
from the submitted tests and the fresh review runs above. This distinction
does not alter the measured production-source identity.

Re-decoding reproduced all 192 recorded symbol-bounded function inventories;
only `rk_balance32` and `rk_clamped` differ in the enabled profiles, with the
documented counts. V1, disabled-SSE4.1 and ARM whole-object controls are still
byte-identical. Review recomputed the saved runtime medians and verified the
252 checks, 288 samples, object hashes, eight quiet preflights and shared-lock
record. No identity issue requiring another timing campaign was found, so
timing was not rerun. Quiet checks were sampled before each round, not a
continuous proof of an idle host. The kernel-only, single-host and finite
ISA/value/footprint limits above remain in force; no application claim follows.

Review scripts, logs, fresh objects and reports are under
`/tmp/keel-review-f9ac` on MacHost and LinuxHost. Main, the original candidate
worktree and sibling workers were left untouched. Integration remains the
parent's responsibility.
