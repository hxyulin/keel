# Raw performance-counter read

`x86_rdpmc(i32 selector) -> i64` executes actual `0f 33` with the complete
selector in ECX and returns `EAX | (u64(EDX) << 32)`. It is pinned, may trap,
and has compiler `Barrier` effects. An unused result still executes; two reads
remain two reads. No hardware fence, index sanitization, counter setup or retry
is inserted. Register allocation preserves live SSA values; surrounding packing
instructions do not promise preservation of ambient machine FLAGS.

The caller owns the PMU environment and must establish implemented instruction,
vendor/model-appropriate selector and permissions (in protected long mode,
CPL0 or CR4.PCE). Counter availability is not implied by v1/v2/v3. Unsupported
indices or denied permission retain the instruction's #GP(0), and runtime/guest
qualification is pending. The compiler accepts raw indices so that an explicit
terminal monitor can observe these failures. AArch64 rejects the intrinsic.

The return is the actual hardware EDX:EAX container, including hardware zero
extension of unimplemented bits, model-specific fast reads, and implemented
per-counter user-disable behavior. It is neither a monotonicity nor a serialized
measurement API. Model-specific performance-metrics reads may clear hardware
resources when configured; Barrier effects cover this state change. CPU/OS
ownership and selector interpretation remain explicit external contracts.

Primary basis: Intel SDM093 Volume2B, RDPMC, printed4-551..553 (cached complete
`/tmp/keel-r4-intel093/v2b.pdf`, extracted entry lines26875..26989); no quoted
whole manual is embedded. The complete combined093 download was too large for
the browsing tool; the cached independently pinned volume was read. Intel's
[architecture extension reference059, chapter15](https://cdrdv2-public.intel.com/865891/319433-059-architecture-instruction-set-extensions-programming-reference_2025-0930.pdf)
separately specifies per-counter user-disable behavior. No library/hardware
capability shim or host PMU configuration is introduced.

This implementation does not establish native acceptance or any full-profile
completion flag. Frozen old evidence remains bound to its original registry.
