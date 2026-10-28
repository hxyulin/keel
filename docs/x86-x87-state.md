# Raw x87/MMX environment

Raw intrinsics for reading and changing the x87 control/status environment, plus
owned FXSAVE/FXRSTOR images. All APIs are impure pinned `Barrier` intrinsics,
including observations whose result is unused. Barriers order explicit raw
operations and memory; they add no hardware memory fence. I16 values carry the
exact control/status bits. There is no frontend mapping.

## Control and status

- `x86_x87_read_cw_nowait() -> i16`: FNSTCW through a private two-byte frame slot.
- `x86_x87_read_sw_nowait() -> i16`: FNSTSW through a private two-byte frame slot.
- `x86_x87_read_cw_wait` / `x86_x87_read_sw_wait`: FWAIT, then the matching FN store.
- `x86_x87_exchange_cw(i16) -> i16`: FNSTCW, then exactly FLDCW of the caller's
  bits; returns the previous control word. No masking, normalization, or hidden
  restore. FLDCW itself checks pending exceptions, so this is not a no-WAIT
  exchange.
- `x86_x87_clex_nowait()` / `x86_x87_clex_wait()`: FNCLEX / FWAIT + FNCLEX.
- `x86_x87_init_nowait()` / `x86_x87_init_wait()`: FNINIT / FWAIT + FNINIT.
- `x86_x87_wait()`: exactly FWAIT.
- `x86_mmx_emms()`: exactly EMMS. It marks all physical x87 tags empty and sets
  TOP to zero, preserving the other status fields, the control word, and the
  error pointers. It does not zero register payloads or clear exception flags.
  The six numerical MMX-value conversions do not touch physical MMX state.

The caller supplies architecturally valid CW bits, balances the environment at
foreign ABI boundaries, and accepts hardware exceptions. Masked exception
controls are the supported environment for ordinary F80 execution; unmasked
delivery and signal contexts are not validated (see
[x86-unmasked-fp.md](x86-intrinsics.md)). FNINIT resets CW/SW/tags/pointers and
discards physical stack state; FNCLEX clears exception status without resetting
CW/TOP/tags. Built-in F80 lowering keeps values in memory and balances each
temporary x87 stack sequence. These APIs observe the compiler's instruction
boundary, not function-entry physical registers. Mixing portable FP with raw
environment changes remains unspecified. Ownership or restoration of an
arbitrary nonempty stack is not supported by these APIs.

The target `Features` model has no fpu/mmx/fxsr toggles: these are implicit
x86-64 baseline facilities (available even with `Features::NONE`). OS
enablement is a caller contract; CR0.EM/TS and OS controls may still cause native
faults. ARM rejects these intrinsics.

## Owned legacy image (FXSAVE/FXRSTOR)

`x86_fxsave64(ptr)` and `x86_fxrstor64(ptr)` emit exactly one REX.W
FXSAVE/FXRSTOR at the caller's address. Both are pinned impure barriers. Save
captures the state at the compiler instruction boundary after argument
preparation. Restore conservatively clobbers all allocatable SIMD registers (as
XRSTOR does), spilling live SSA values; later reloads may use different physical
registers. Plug-in allocators must honor the same clobber/liveness contract. The
image is independent of the XSAVE mask-subset APIs.

The caller owns exclusive, accessible 512-byte storage aligned to 16 bytes,
disjoint from compiler frames, spills, and control data, with no concurrent
mutation or permission changes. Initialize the entire buffer before use.

64-bit legacy layout:

| Offset | Field |
|---|---|
| 0 | FCW |
| 2 | FSW |
| 4 | Abridged physical FTW |
| 6 | FOP |
| 8 | FIP (64-bit) |
| 16 | FDP (64-bit) |
| 24 | MXCSR |
| 28 | MXCSR_MASK |
| 32 | Eight ST/MM slots, 16-byte stride (80 significant bits) |
| 160 | Sixteen XMM slots |
| 464..511 | Available to software; not written by FXSAVE |

Reserved/padding bytes and empty-stack payload are not semantic. FTW records
physical register occupancy, not full FSAVE tags. Pointer and opcode values keep
their processor-specific architectural latitude. YMM upper halves and newer
extended components are not saved or restored, so no AVX/XSAVE gate is needed.

Restore requires a valid initialized image with **FTW=0, FSW.TOP=0, an
architecturally valid FCW with all six exception masks set, and no pending
unmasked exceptions**. MXCSR must contain only supported bits: derive them from
a prior native MXCSR_MASK, using the architectural fallback `0x0000ffbf` if that
mask is zero. FXRSTOR does not load MXCSR_MASK itself. The caller balances
controls at foreign ABI boundaries. This supports balanced stack-resident F80;
restoring an arbitrary nonempty x87 stack is not supported here. The API does
not validate or sanitize inputs: alignment/access errors, reserved MXCSR bits,
and disabled hardware/OS state can fault natively and are not replaced with
compiler checks. There is no signal-context or continuation contract.

## Other legacy images

The 14/28-byte and 94/108-byte environment/state APIs and the nonempty owned
region are specified in [legacy images](f80/legacy-images.md). The FX image API
above keeps its empty-stack restriction.

## References

- [Intel SDM Vol. 2A, 253666-060US](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf):
  EMMS, FCLEX/FNCLEX, FINIT/FNINIT, FLDCW, FSTCW/FNSTCW, FSTSW/FNSTSW.
- [Intel SDM Vol. 3A, 253668-085US, section 14.2](https://cdrdv2-public.intel.com/835754/253668-sdm-vol-3a.pdf):
  every MMX instruction (including EMMS) sets TOP=0 and preserves the other
  status fields, control word, and error pointers.
- [AMD APM Vol. 1, 24592 revision 3.24, section 5.12](https://docs.amd.com/api/khub/documents/sfvvekC9mDflu6vd3R0NXA/content):
  same TOP clearing and preservation.
- [Intel XED](https://github.com/intelxed/xed/blob/0bcb6237345c5066726dcc08b3d87928df3b5b26/datafiles/xed-isa.txt):
  X87/PENTIUMMMX/FXSAVE ISA sets.
- [Clang 19.1.7 fxsrintrin.h](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-19.1.7/clang/lib/Headers/fxsrintrin.h)
  and [GCC 14.2 fxsrintrin.h](https://raw.githubusercontent.com/gcc-mirror/gcc/releases/gcc-14.2.0/gcc/config/i386/fxsrintrin.h)
  for interface corroboration.
