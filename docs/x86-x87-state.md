# Staged raw x87/MMX environment

All APIs are impure pinned `Barrier` intrinsics, including observations whose result
is unused. Barriers order explicit raw operations and memory; they add no hardware
memory fence. Numerical I16 values carry the exact control/status bits. No frontend
mapping or full v1/v2/v3 completion is implied.

- `x86_x87_read_cw_nowait()->i16`: FNSTCW through a private two-byte frame slot.
- `x86_x87_read_sw_nowait()->i16`: FNSTSW through a private two-byte frame slot.
- `x86_x87_read_cw_wait` / `x86_x87_read_sw_wait`: FWAIT then the matching FN store.
- `x86_x87_exchange_cw(i16)->i16`: FNSTCW then exactly FLDCW of caller bits;
  returns the previous control word. No masking, normalization or hidden restore.
  FLDCW itself checks pending exceptions; this is not a no-WAIT exchange.
- `x86_x87_clex_nowait()` / `x86_x87_clex_wait()`: FNCLEX / FWAIT + FNCLEX.
- `x86_x87_init_nowait()` / `x86_x87_init_wait()`: FNINIT / FWAIT + FNINIT.
- `x86_x87_wait()`: exactly FWAIT.
- `x86_mmx_emms()`: exactly EMMS, marking all physical x87 tags empty. This clears
  TOP to zero while retaining the other status bits and the control word. It does
  not zero register payloads or clear exception flags.
  The six existing numerical MMX-value conversions still do not access physical MMX.

Caller supplies architecturally valid CW bits, balances the environment at foreign
ABI boundaries, and accepts hardware exceptions. Masked exception controls are the
supported ordinary F80 execution environment; unmasked delivery/signal contexts
remain unproved. FNINIT resets CW/SW/tags/pointers and discards physical stack state;
FNCLEX clears exception status without resetting CW/TOP/tags. Built-in F80 lowering
keeps values in memory and balances each temporary x87 stack sequence. These APIs
observe the compiler instruction boundary, not function-entry physical registers.
Portable FP mixed with raw environment changes retains its existing unspecified
policy. No arbitrary nonempty stack ownership or restoration is claimed.

The target `Features` model has no fpu/mmx/fxsr toggles: these are implicit x86-64
baseline facilities (also with `Features::NONE`). OS enablement is a caller contract;
CR0.EM/TS and relevant OS controls may still cause native faults. ARM rejects.

Primary references: [Intel SDM 2A, September 2016, 253666-060US](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf),
entries EMMS, FCLEX/FNCLEX, FINIT/FNINIT, FLDCW, FSTCW/FNSTCW, FSTSW/FNSTSW;
[pinned Intel XED](https://github.com/intelxed/xed/blob/0bcb6237345c5066726dcc08b3d87928df3b5b26/datafiles/xed-isa.txt),
X87/PENTIUMMMX/FXSAVE ISA sets. Sources inform the contract; native execution is
separate evidence and remains pending in this local macOS stage.

## Owned legacy image

`x86_fxsave64(ptr)` and `x86_fxrstor64(ptr)` emit exactly one REX.W FXSAVE/FXRSTOR
at the original caller address. Both are pinned impure Barriers. Save captures the
compiler instruction boundary after argument preparation. Restore conservatively
clobbers all allocatable SIMD registers like XRSTOR, spilling live SSA values;
subsequent reloads may change physical registers. Plug-in allocators must honor the
same clobber/liveness contract. The image is independent of XSAVE mask-subset7 APIs.

Caller owns exclusive accessible 512-byte storage aligned to 16 bytes, disjoint from
compiler frames/spills/control data; no concurrent mutation or racing permissions.
Initialize the entire buffer before use. The 64-bit legacy layout has FCW at 0, FSW
at 2, abridged physical FTW at 4, FOP at 6, full 64-bit FIP at 8 and FDP at 16,
MXCSR at 24 and MXCSR_MASK at 28, eight ST/MM slots at 32 (16-byte stride; only 80
x87 bits significant) and sixteen XMM slots at 160. Bytes 464..511 are available
to software and FXSAVE does not write them; reserved/padding bytes and empty-stack
payload are not semantic. FTW stores physical register occupancy, not full FSAVE tags.
Pointer/opcode values retain processor-specific architectural latitude. No YMM upper
halves or newer extended components are saved/restored; no AVX/XSAVE gate is needed.

Restore requires a valid initialized image with **FTW=0, FSW.TOP=0, architecturally
valid FCW with all six exception masks set and no pending unmasked exceptions**.
MXCSR must contain only supported bits: derive them from a prior native MXCSR_MASK,
using architectural fallback 0x0000ffbf if that mask is zero. MXCSR_MASK itself is
not loaded by FXRSTOR. Caller balances controls at foreign ABI boundaries. This
supports balanced stack-resident F80; arbitrary nonempty x87 restoration remains
future work. The API does not validate or sanitize inputs. Alignment/access,
reserved MXCSR bits and disabled hardware/OS state can fault natively; faults are
not replaced with compiler checks. No signal context/continuation contract.

Primary interface corroboration: [Clang 19.1.7 fxsrintrin.h](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-19.1.7/clang/lib/Headers/fxsrintrin.h)
and [GCC 14.2 fxsrintrin.h](https://raw.githubusercontent.com/gcc-mirror/gcc/releases/gcc-14.2.0/gcc/config/i386/fxsrintrin.h).
These source wrappers do not prove compiler live-SSA preservation or native faults.


## EMMS TOP correction (native r1 diagnosis)

The earlier TOP-preservation statement and nondefault-seed test were incorrect.
[Intel SDM Volume3A, 253668-085US, section14.2, page14-2](https://cdrdv2-public.intel.com/835754/253668-sdm-vol-3a.pdf)
requires TOP=0 for every MMX instruction and preserves the other status fields,
control word and error pointers. [AMD APM Volume1, 24592 revision3.24,
section5.12](https://docs.amd.com/api/khub/documents/sfvvekC9mDflu6vd3R0NXA/content)
describes the same TOP clearing and preservation. Intel Volume1's shorter transition
list exempts EMMS from its combined normal-MMX description; that exemption does
not establish TOP preservation against the specific Volume3A state rules.

Parent raw-opcode and Keel observations both gave CW0a7f/SW5f21/FTWff ->
CW0a7f/SW4721/FTW00: only TOP3->0 and tags emptied. Original r1 packet/code52 failure
is preserved; no backend lowering defect was established. The replacement fixture
keeps the nondefault seed, requires TOP0 and unchanged other status fields/CW, and
compares an independently forced0F77 opcode through the same trusted probe.
New complete native24 acceptance remains parent-owned; the diagnostic observation
alone does not accept that packet or promote numerical/profile completeness.

## Ordinary legacy images

The additional 14/28-byte and 94/108-byte APIs and nonempty owned-region contract
are specified in [legacy images](f80/legacy-images.md). This extends the earlier
staged scope; the existing FX image API retains its empty-stack restriction.
