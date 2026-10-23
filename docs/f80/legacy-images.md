# Ordinary owned legacy x87 images

The twelve `x86_x87_{fnstenv,fstenv,fldenv,fnsave,fsave,frstor}{16,32}(ptr)`
intrinsics are impure pinned memory Barriers. Each emits the named architectural
instruction at the original address, with operand-size 16 or 32; the address size
stays 64. The 16-bit forms access 14-byte environments or 94-byte register images;
the 32-bit forms access 28 or 108 bytes. There is no alignment requirement beyond
native alignment checking (#AC when enabled), no bounce buffer, no overread or
extra store, and no feature beyond the implicit x86-64 x87 baseline. v2/v3 inherit
this route; `Features::NONE` also permits it. AArch64 rejects it. The ordinary
signature verifier checks pointer arity/types; it does not validate image bytes or
statically enforce the caller ownership contract below.

Caller exclusively owns the whole readable/writable range and keeps it disjoint
from compiler stack frames, SSA homes and control data. Images may alias each other,
including source=destination, and may alias explicit program data when the ordered
IR loads/stores require it. Initialize images before reading them. Loads require
architecturally valid control/status/tag/pointer fields and coherent payloads.
The accessible range must include the exact footprint; a failed instruction may
have partial native memory/state effects. No atomicity, rollback, hardware fence,
permission checks, signal resumption or arbitrary physical-register context is
promised. Barriers preserve order with explicit memory and other raw operations;
portable FP ordering retains its existing policy.

FNSTENV writes the pre-instruction environment and then sets all six CW exception
masks. It leaves payloads, TOP and full tags unchanged. FNSAVE writes that environment
and eight 10-byte logical ST(0)..ST(7) payload slots, then initializes the x87 state
as FNINIT (CW=037f, SW=0, tags=ffff and reset pointer/opcode state). Empty-register
payloads and reserved output fields are not values. FSTENV/FSAVE emit FWAIT followed
by the corresponding FN instruction: WAIT checks pending exceptions before any
memory access by the FN instruction. FLDENV restores the environment without
loading register payloads; FRSTOR additionally restores all eight payloads. Neither
loads MXCSR or XMM/YMM registers or clobbers architectural GPRs/flags. Full tags in saved images are
physical R0..R7 pairs. On restore the empty/nonempty distinction is loaded;
nonempty tags are classified from register contents on a subsequent save, so
mismatched 00/01/10 encodings do not force a different numerical class. FLDENV
reclassifies the payloads already present, while FRSTOR loads new payloads; payload slots are logical ST(i), rotated by restored TOP.
The 16-bit protected-mode environment fields are CW@0, SW@2, TW@4, IP16@6,
CS@8, DP16@10, DS@12. The 32-bit form is CW@0, SW@4, TW@8, IP32@12,
CS@16, FOP[10:0]@18, DP32@20, DS@24, with reserved halfwords at 2/6/10/26.
The 16-bit protected-mode image has no separate FOP field. These legacy pointer
formats truncate addresses and cannot encode full 64-bit signal context. Native
processor-specific pointer/selector retention and reserved fields follow hardware.

## Ownership region and live SSA

A valid FLDENV or FRSTOR may introduce **any coherent nonempty x87 or MMX state,
including nonzero TOP**, under explicit caller ownership. This begins an owned
region. Within it use raw environment/image/CW/SW/init/clex/wait/EMMS operations,
integer and SIMD IR, and raw memory transport of F80 payloads. Ordinary F80
arithmetic, conversions, comparisons, F80 ABI argument/result transport and all
calls/tail calls/returns are prohibited until the region ends. This includes
implicitly lowered helper calls. The compiler does not invent a second hardware
x87 stack or silently discard restored payloads. An image is observable inside
this region via another save; FRSTOR -> FNSTENV/FLDENV -> FNSAVE is a concrete
nonempty round trip, all inside a single generated function.

End the region with FNSAVE/FSAVE, FNINIT, or restoration of an empty full TW=ffff,
TOP=0 environment (or use EMMS, whose established contract empties tags/TOP).
FLDENV can change tags without changing data, so the caller must keep the image
consistent. Before ordinary F80 work resume a valid masked CW and no pending
unmasked exception; before a C ABI boundary also restore ABI control requirements.
Raw F80 memory copies and live F80 SSA values are safe across the region: every F80
value has a private stack home and these intrinsics use only reserved GPR address
scratch. Integer and SIMD SSA retain their ordinary register or spill locations;
these instructions have no GPR/SIMD clobber. In functions containing these
intrinsics, GCM retains every F80-producing/consuming instruction in its original
block. Pinning only the image operations would permit pure F80 computations to
sink/hoist into the full physical stack and corrupt values. Within-block placement
already remains unchanged in GCM; integer if-conversion excludes floating-point
speculation by its existing allowlist. The packet tests direct GCM and the full
optimized pipeline with pre-region binary/unary values and a post-reset F80 loop;
its full-tag MMX seeds expose accidental numerical x87 execution in the region. Physical restored x87/MMX payloads are
owned state, not SSA results. Source images containing MMX encodings are supported
as raw legacy images; no numerical MMX conversion API is changed.

Ordinary masked execution and flag-only unmasked CW seeds with no corresponding
pending exception are supported. For every waiting form, including FLDENV/FRSTOR,
caller must have **no pending unmasked x87 exception at entry** to the operation.
For no-WAIT saves, pending-unmasked capture/delivery remains a separate unproved
stage. In particular a WAIT-first access-fault test must meet that precondition;
it cannot claim #MF/#PF priority under an unmasked pending exception. Restoring
unmasked pending state followed by fault delivery/resumption is outside this stage.
This is a caller contract, like the existing owned FX image contract; no frontend
or universal opcode selector mapping is implied.

## Primary definitions and proof status

Intel SDM Volume 2, document 325383-079US (March 2023), source PDF
<https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf>, entries
FLDENV, FRSTOR, FSAVE/FNSAVE, FSTENV/FNSTENV; AMD APM Volume 5, document 26569,
revision 3.16 (November 2021),
<https://docs.amd.com/api/khub/documents/w13cmcpL4f9MCT4WPN6eDg/content>,
and Volume 2, document 24593 revision3.38 (November2021), section11.4.4,
figures11-4/11-6, publisher-authored PDF mirrored at
<https://www.scs.stanford.edu/~zyedidia/docs/x86/amd-manual.pdf> define these formats.
The AMD primary content endpoints currently return404; this retained manual's
SHA256 is `faab6c843145a7f45da7e3c14d9c4c3d131f3d4070f2bb2880297d077c3e8e91`.
Intel PDF SHA256 is
`bc4348020f5d5a27b0207c61e3c88d4e610eaf428658dc5d08a1cc84f98a719b`.
Tag classification also follows Intel SDM Volume3B section22.18.4, rather than
assuming arbitrary nonempty tags override the underlying payload class.
Assembler support predates these intrinsics; typed and textual routes must agree.
Native acceptance and full-profile completion remain false until the parent accepts
the sealed packet on native Linux x86-64. Existing accepted gates remain unchanged.
