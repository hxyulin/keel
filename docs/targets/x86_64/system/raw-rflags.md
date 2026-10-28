# Raw RFLAGS

Keel has two raw RFLAGS intrinsics on x86-64:

| ID | Signature | Status |
|---|---|---|
| 729 | `x86_rflags_read() -> i64` | Supported |
| 730 | `x86_rflags_swap(i64) -> i64` | Reserved; rejected by x86 codegen as a standalone intrinsic |

A swap is available only inside the explicit owned region described in
[x86-rflags-owned-region.md](rflags-owned-region.md). Native acceptance of
that region has not been established.

Both intrinsics are x86-only, pinned, possibly faulting memory barriers, and are
retained when their results are dead. Neither needs LAHF/SAHF, SSE, or any
feature beyond x86-64 v1.

## Read contract

Read emits `PUSHFQ; POP rax`, followed by flag-neutral MOV publication or spill
transport. The result is the architectural PUSHFQ image, including RF/VM cleared
by hardware; it is not a software reconstruction.

It observes the machine flags at that emitted PUSHFQ, after prologue, argument
transport, and earlier lowering. It does not observe function-entry flags.
Compiler arithmetic, comparisons, branch lowering, and optimization can change
flags, so successive reads do not imply a continuous FLAGS value. GCM is
disabled for functions containing either intrinsic so pure arithmetic is not
moved across the observation, but other transformations still determine what
happens before the emitted boundary. Source arithmetic is not a way to seed raw
FLAGS.

Stack and register behavior:

- Only the reserved RAX is clobbered; all allocated live SSA values are
  preserved.
- PUSHFQ writes exactly eight bytes at `[old RSP-8, old RSP)`, POP reads the
  same bytes, and RSP is restored exactly on successful completion.
- The backend has no red-zone locals, so fixed objects are above RSP, even with
  dynamic alloca. No call or stack realignment occurs within the pair.
- Functions using these intrinsics are never frameless; the `rbp+16` CFA stays
  valid at the intermediate PC. Normal return, callee saves, and epilogue are
  unchanged.

Faults are retained and pinned. There is no promise to recover RSP after a
fault, emulate inaccessible stack memory, or run in an arbitrary interrupt or
signal context. The caller supplies a normal ABI-compatible execution state and
an accessible stack.

## Why standalone swap is rejected

The intended sequence captures the old flags with PUSHFQ/POP, captures the input
before any aliased output is published, then executes PUSH r64/POPFQ. POPFQ
must apply the real CPL/IOPL, reserved-bit, RF, VM, VIF/VIP, IF, DF, TF, AC/SMAP,
and ID semantics, including faults. Unwritable privilege bits can stay unchanged
without a fault, so masking the input or restricting it to application flags
would not implement this API.

The IR cannot currently make this safe as a free-standing operation. Intrinsic
metadata carries only purity, memory effects, and trap properties; there is no
owned FLAGS token, CFG region, permitted instruction set, or restoration proof.

- Pinning the swap does not stop pure arithmetic from moving across it.
- Disabling GCM does not stop later lowering from generating ADD/CMP/TEST,
  library calls, or an epilogue after POPFQ.
- The verifier cannot prove that an arbitrary input restores DF=0 and legal
  TF/AC/IF/IOPL/execution state before ordinary calls or return, or account for
  debug and alignment faults inside that interval.
- Returning the old image cannot restore RF/VM, because PUSHFQ clears them in
  its image.

The existing low-flags boundary API and the x87-specific region do not provide
this proof either. Standalone swap therefore fails before allocation and
emission, including when its result is dead. The owned-region wrapper installs
an exact POPFQ expansion with structural and machine-resource checks under
declared entry and environment preconditions; it does not enable the
standalone API. 16-bit and compatibility-mode forms are not implemented.

## References

Intel SDM revision 093 (September 2026), Vol. 2B 253667-093US: POPF/POPFD/POPFQ
pp. 4-407 to 4-410 (Table 4-20 and the 64-bit pseudocode), PUSHF/PUSHFD/PUSHFQ
pp. 4-528 to 4-530; Vol. 1 sections 3.4.3.2-3.4.3.3 (DF, TF, IOPL, AC including
SMAP). Table 4-20 includes the 64-bit operand size for AC/ID, so they are not
masked out.
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
