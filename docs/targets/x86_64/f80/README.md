# F80 and x87

F80 is an x87 extended value, never an alias for F64. `Type::bytes()` is
10, the exact load/store payload. SysV object layouts give it size and
alignment 16; six padding bytes are not part of the value. Existing Fconst
and its u64 payload remain unchanged. `F80const { significand, sign_exp }`
and `fconst.f80 0x3fff, 0x8000000000000001` preserve all payload bits,
including signed zeros, NaNs and noncanonical encodings. Decimal conversion
is absent until a correctly rounded converter exists.

The IR and text format accept F80 losslessly. Backends reject F80 forms they do
not implement, including in foreign signatures and aggregate layouts. AArch64
always rejects this format.

## FP environment contract

The x86 backend assigns a stack home to every F80 SSA value and uses short
x87 sequences. No x87 value survives an IR instruction except an ABI result
in ST0. Calls consume return ST0 immediately, including unused results.
Returns expose exactly ST0; calls and tail jumps otherwise have empty x87
stacks. Payload moves use integer copies to preserve noncanonical
encodings and avoid unintended exceptions. Arithmetic and conversions may raise
x87 exceptions and obey the inherited x87 control word (precision control,
rounding control, masks); the default SysV environment has extended precision.
No change to MXCSR or the x87 control word is implicit. Unmasked exceptions may
interrupt a sequence with temporary stack operands present, as in C-generated
code; completed sequences balance TOP. This is a value-level compiler contract,
not a raw x87 instruction or environment API.

## ABI

ABI lowering distinguishes X87/X87UP classification from argument placement:
long double and its wrappers are MEMORY arguments but ST0 results. Mixed
or larger aggregates use MEMORY. No synthetic XMM return is permitted.
Varargs long double consumes an aligned 16-byte overflow-area object and
never consumes gp_offset/fp_offset. Tail calls require the existing incoming
stack-area checks and return-layout agreement.

## Supported x86 operations

The x86 backend implements stack homes, raw constants and exact-width memory
transport, scalar and X87/X87UP aggregate ABI transport, all scalar float
arithmetic and unary operations, IEEE min/max, copy-sign, explicit FMA through
`fmal`, ordered and unordered comparisons, selects, block arguments and loops,
scalar direct and indirect calls and tails, and SysV overflow-area varargs.
F32/F64 extension and truncation use x87 loads and stores; integer conversions
keep the saturating, truncating, NaN-to-zero contract. Integer conversion and
explicit rounding temporarily select the required RC and restore the original
CW. Unsigned u64 construction normalizes its exact payload with integer
operations; u64 results in [2^63,2^64) use the explicit significand directly.
Neither path passes through binary64 or depends on reduced precision control.

F80 homes reserve 16 bytes each; control-word staging reuses the 16-byte
parallel-copy temporary between instructions. Aggregate tail restrictions and
i128/float conversion restrictions still apply.

## Raw x87 value intrinsics

The intrinsics in this section execute one named principal x87 instruction on
F80 SSA inputs. They do not change generic F80 arithmetic, optimization or
contraction rules, and generic F80 operations are not aliases for them. They
share this contract:

- Entry: empty x87 tags at any TOP, a valid CW with all six exception masks
  set, and no pending unmasked exception. CW, including PC and RC, is
  inherited and preserved. Sticky exception flags stay sticky.
- Inputs load with FLD m80 before the principal instruction, and their effects
  are part of the returned status. All inputs are on the x87 stack before any
  output home is written, so outputs may share dying input homes.
- The returned `i32` is FNSTSW AX taken immediately after the principal
  instruction, zero-extended without masking. Its TOP field shows the
  temporary physical stack. Cleanup then stores or drops results and restores
  the entry TOP with empty tags; cleanup effects are not in the snapshot.
- Hardware-undefined condition bits stay raw, and exceptional results follow
  the hardware. There is no libm substitution, completion loop or domain
  repair.
- Every F80 memory access is exactly ten bytes.
- Each intrinsic is pinned with `Barrier` effects and executes even when every
  output is unused. AArch64 rejects them.

Native numerical validation of these intrinsics on x86-64 Linux has not been
done; current tests cover compilation, encoding and effect modeling.

### Constants

`x86_x87_fldpi()`, `x86_x87_fldl2e()`, `x86_x87_fldl2t()`, `x86_x87_fldlg2()`
and `x86_x87_fldln2()` return `(f80, i32)`. They round the internal hardware
constant by RC, not PC, without raising precision or reporting rounding in
C1. They are not equivalent to pre-rounded F80 literals. Snapshot TOP is
`(entry_TOP-1)&7`.

Exact positive one and zero use F80 literals:
`fconst.f80 0x3fff,0x8000000000000000` and
`fconst.f80 0x0000,0x0000000000000000` match FLD1 and FLDZ under every RC and
PC. Literals do not give the raw status of a principal FLD1 or FLDZ.

### Unary and classification

`x86_x87_fabs`, `x86_x87_fchs`, `x86_x87_fsqrt` and `x86_x87_frndint` take an
F80 and return `(F80, i32)`. `x86_x87_ftst` and `x86_x87_fxam` take an F80 and
return `i32`. Snapshot TOP is `(entry_TOP-1)&7`.

- FABS clears the sign and FCHS toggles it; both set C1 to zero.
- FSQRT uses PC and RC. FRNDINT uses RC and leaves infinity unchanged. C1
  reports rounding, and C0, C2 and C3 are undefined.
- FTST compares with zero: C3/C2/C0 are 000 greater, 001 less, 100 equal
  (either sign of zero) and 111 unordered. NaNs raise masked invalid; C1=0.
- FXAM reports the class in C3/C2/C0: unsupported 000, NaN 001, normal 010,
  infinity 011, zero 100, empty 101, denormal 110. C1 is the sign. The API
  always loads an operand, so it cannot observe an empty ST(0), and it
  classifies the value after FLD, not the original memory encoding. FLD m80
  does not raise invalid for SNaNs or denormal for subnormals.

### Register arithmetic

These eighteen intrinsics take `(a:F80, b:F80)` and return
`(F80, i32)`. Operands load as FLD b, FLD a, so ST(0)=a and ST(1)=b before the
principal instruction. The F80 result is the destination register after it.

| Intrinsic (`x86_x87_`) | Principal instruction | Bytes | Result |
|---|---|---|---|
| fadd | fadd st(0),st(1) | D8 C1 | a+b |
| fmul | fmul st(0),st(1) | D8 C9 | a*b |
| fsub | fsub st(0),st(1) | D8 E1 | a-b |
| fsubr | fsubr st(0),st(1) | D8 E9 | b-a |
| fdiv | fdiv st(0),st(1) | D8 F1 | a/b |
| fdivr | fdivr st(0),st(1) | D8 F9 | b/a |
| faddp | faddp st(1),st(0) | DE C1 | b+a |
| fmulp | fmulp st(1),st(0) | DE C9 | b*a |
| fsubp | fsubp st(1),st(0) | DE E9 | b-a |
| fsubrp | fsubrp st(1),st(0) | DE E1 | a-b |
| fdivp | fdivp st(1),st(0) | DE F9 | b/a |
| fdivrp | fdivrp st(1),st(0) | DE F1 | a/b |
| fadd_st1 | fadd st(1),st(0) | DC C1 | b+a |
| fmul_st1 | fmul st(1),st(0) | DC C9 | b*a |
| fsub_st1 | fsub st(1),st(0) | DC E9 | b-a |
| fsubr_st1 | fsubr st(1),st(0) | DC E1 | a-b |
| fdiv_st1 | fdiv st(1),st(0) | DC F9 | b/a |
| fdivr_st1 | fdivr st(1),st(0) | DC F1 | a/b |

The algebraic result does not license operand exchange, NaN substitution or
choosing another opcode. Destination identity, NaN operand priority and
status follow the listed opcode. Snapshot TOP is `(entry_TOP-2)&7` for D8 and
DC, and `(entry_TOP-1)&7` for DE. DC cleanup pops the original ST(0), then
stores or drops the ST(1) result. C0, C2 and C3 are undefined, and C1 reports
rounding on valid results only.

Memory-operand arithmetic and arbitrary stack-register indices are not
supported. FLD followed by register arithmetic is not a substitute for a
memory-operand form: operand widths, load exceptions, source order and NaN
selection differ.

```sh
cargo test -p keel-codegen --test f80 x86_x87_dc::
```

### Comparisons

`x86_x87_fcom`, `fcomp`, `fcompp`, `fucom`, `fucomp` and `fucompp` take two F80
values and return the raw status word as an `i32`. `x86_x87_fcomi`, `fcomip`,
`fucomi` and `fucomip` return `(i32 flags, i32 status)`. The flags result is a
bit mask with CF at bit 0, PF at bit 2 and ZF at bit 6, and all other bits zero.

Inputs are ST(0) and ST(1) in argument order. The pop count of each form shows
in the returned TOP; cleanup balances both temporary entries. The I forms
capture RFLAGS with PUSHFQ and the reserved RCX, with no LAHF dependency.
Status is published before flags.

FCOM-family forms signal invalid for QNaN, SNaN and unsupported encodings.
FUCOM-family forms signal invalid only for SNaN and unsupported encodings.
Masked invalid gives an unordered result. Non-I forms define the relation in
C0, C2 and C3; I forms leave those bits undefined. Integer-memory FICOM and
FICOMP are available only through the [unmasked region](#unmasked-execution-region).

### Numerical operations

`x86_x87_fsin`, `x86_x87_fcos` and `x86_x87_f2xm1` take one F80.
`x86_x87_fpatan`, `x86_x87_fyl2x`, `x86_x87_fyl2xp1`, `x86_x87_fscale`,
`x86_x87_fprem` and `x86_x87_fprem1` take two, in ST(0), ST(1) order. So FPATAN
computes atan2(second, first), FYL2X second * log2(first), and FYL2XP1
second * log2(first + 1). Each returns `(F80, i32)`. Snapshot TOP is
`(entry_TOP-1)&7` for unary and popping forms and `(entry_TOP-2)&7` for
nonpopping ones.

FSIN and FCOS range failures expose C2 and the unchanged operand. FPREM and
FPREM1 execute once and keep the partial remainder and C2.

`x86_x87_fsincos`, `x86_x87_fptan` and `x86_x87_fxtract` take one F80 and
return `(F80, F80, i32)` in post-instruction ST(0), ST(1), status order:
FSINCOS returns cosine then sine, FPTAN one then tangent, and FXTRACT
significand then exponent. On an FSINCOS or FPTAN range failure nothing is
pushed. The first result is the unchanged operand and the second is
unspecified. Cleanup uses the snapshot TOP, not C2, to tell one physical
result from two.

`x86_x87_fiadd16/32`, `x86_x87_fimul16/32`, `x86_x87_fisub16/32`,
`x86_x87_fisubr16/32`, `x86_x87_fidiv16/32` and `x86_x87_fidivr16/32` take an
F80 and then an exact `i16` or `i32`, which the hardware reads as signed. They
return `(F80, i32)`. FISUB and FIDIV compute F80 minus or divided by the
integer; FISUBR and FIDIVR compute the reverse. The integer goes to its own
reserved stack slot of exactly two or four bytes, and the lowering emits one FI
memory instruction of that width. Snapshot TOP is `(entry_TOP-1)&7`. This
differs from FILD followed by portable arithmetic in operand order, rounding
and exception behavior.

```sh
cargo test -p keel-codegen --test f80 x86_x87_numerical::
```

`crates/keel-codegen/tests/x86_x87_numerical/` contains an assembly probe and
runner for comparing raw results and status on a native x86-64 Linux host.

### Packed BCD

`x86_x87_fbld(ptr) -> (f80, i32)` reads exactly ten bytes with FBLD.
`x86_x87_fbstp(f80, ptr) -> i32` writes exactly ten bytes with FBSTP.
Pointers need no alignment, and there is no wider access or speculative copy.
A memory fault returns no result. Overlapping F80 and BCD storage works
because the F80 loads before the store. FBLD snapshot TOP is
`(entry_TOP-1)&7`; FBSTP snapshot TOP is the entry TOP. Signed zero, rounding,
invalid and indefinite results and nondecimal digit nibbles follow the
current processor; there is no decimal validation or saturation.

## Control word, status and FX images

These intrinsics read and change the x87 environment. They are pinned with
`Barrier` effects, including unused observations, and add no hardware fence.
`i16` values carry the exact control and status bits. No C frontend mapping
exists.

- `x86_x87_read_cw_nowait() -> i16` and `x86_x87_read_sw_nowait() -> i16`
  store through a private two-byte frame slot with FNSTCW or FNSTSW.
- `x86_x87_read_cw_wait` and `x86_x87_read_sw_wait` emit FWAIT, then the
  matching FN store.
- `x86_x87_exchange_cw(i16) -> i16` emits FNSTCW, then FLDCW of the caller's
  bits, and returns the previous control word. There is no masking or hidden
  restore. FLDCW checks pending exceptions, so this is not a no-WAIT exchange.
- `x86_x87_clex_nowait()` and `x86_x87_clex_wait()` emit FNCLEX or FWAIT,
  FNCLEX.
- `x86_x87_init_nowait()` and `x86_x87_init_wait()` emit FNINIT or FWAIT,
  FNINIT.
- `x86_x87_wait()` emits FWAIT.
- `x86_mmx_emms()` emits EMMS. It marks all tags empty and sets TOP to zero,
  and keeps the other status fields, the control word and the error pointers.
  It does not zero payloads or clear exception flags. The six MMX-value
  conversions do not touch physical MMX state.

The caller supplies valid CW bits, balances the environment at foreign ABI
boundaries and accepts hardware exceptions. Ordinary F80 execution expects
masked exception controls. FNINIT resets CW, SW, tags and pointers and
discards the stack; FNCLEX clears exception status only. These APIs observe the
compiler's instruction boundary, not function-entry registers. Mixing portable
FP with raw environment changes is unspecified.

`Features` has no fpu, mmx or fxsr fields: these are x86-64 baseline and
available even with `Features::NONE`. OS enablement is a caller contract, and
CR0.EM, CR0.TS and OS controls may still fault.

`x86_fxsave64(ptr)` and `x86_fxrstor64(ptr)` emit one REX.W FXSAVE or FXRSTOR
at the caller's address. Save captures the state after argument preparation.
Restore clobbers all allocatable SIMD registers, as XRSTOR does, so live SSA
values spill and may reload into different registers. Plug-in allocators must
honor the same clobber. The caller owns 512 bytes aligned to 16, disjoint from
compiler frames and spills, initialized before use.

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

FTW records physical register occupancy, not full FSAVE tags. YMM upper halves
and newer components are not saved, so no AVX or XSAVE gate is needed.
Restore requires FTW=0, FSW.TOP=0, a valid FCW with all six exception masks
set, and no pending unmasked exception. MXCSR may contain only supported bits:
derive them from a native MXCSR_MASK, or use `0x0000ffbf` if that mask is
zero. The API does not validate the image; reserved bits and disabled state
fault natively.

## Legacy images and the owned region

The twelve `x86_x87_{fnstenv,fstenv,fldenv,fnsave,fsave,frstor}{16,32}(ptr)`
intrinsics emit the named instruction at the original address with operand
size 16 or 32 and address size 64. The 16-bit forms access 14-byte
environments or 94-byte register images; the 32-bit forms access 28 or 108
bytes. There is no alignment requirement beyond native #AC, no bounce buffer
and no extra access. `Features::NONE` permits them.

The caller owns the whole range, keeps it disjoint from compiler frames and
SSA homes, and initializes images before reading them. Images may alias each
other and explicit program data. Loads need valid control, status, tag and
pointer fields and coherent payloads. A failed instruction may leave partial
memory and state effects.

- FNSTENV writes the environment, then sets all six CW exception masks. It
  leaves payloads, TOP and tags unchanged.
- FNSAVE writes the environment and eight 10-byte ST(0)..ST(7) payload slots,
  then initializes the x87 state like FNINIT.
- FSTENV and FSAVE emit FWAIT before the FN instruction, so pending exceptions
  are checked before any memory access.
- FLDENV restores the environment without payloads; FRSTOR also restores all
  eight payloads. Neither loads MXCSR or XMM registers.

Saved full tags are physical R0..R7 pairs. A restore loads the empty or
nonempty distinction, and a later save classifies nonempty registers from
their contents. Payload slots are logical ST(i), rotated by the restored TOP.
The 16-bit environment fields are CW@0, SW@2, TW@4, IP16@6, CS@8, DP16@10 and
DS@12. The 32-bit form is CW@0, SW@4, TW@8, IP32@12, CS@16, FOP[10:0]@18,
DP32@20 and DS@24, with reserved halfwords at 2, 6, 10 and 26. These pointer
formats truncate addresses and cannot hold a 64-bit signal context.

A valid FLDENV or FRSTOR may install any coherent nonempty x87 or MMX state,
including nonzero TOP. That starts an owned region. Inside it, use raw
environment, image, CW, SW, init, clex, wait and EMMS operations, integer and
SIMD IR, and raw memory copies of F80 payloads. Ordinary F80 arithmetic,
conversions, comparisons, F80 ABI transport and all calls, tail calls and
returns are prohibited, including helper calls the lowering would insert.

These leaves also work inside the region. They take no arguments and return
nothing (IDs 732 to 742):

- `x86_x87_ffree0()` to `x86_x87_ffree7()` mark the physical tag of
  ST(i), `(TOP+i)&7`, empty, keeping TOP and contents.
- `x86_x87_fdecstp()` and `x86_x87_fincstp()` change TOP by -1 or +1 modulo 8
  and clear C1.
- `x86_x87_fnop()` executes FNOP with its native environment effects.

They take no status snapshot; capture an image at the next IR position if
needed. Pointer fields such as FIP, FCS, FOP, FDP and FDS follow the hardware
and are not part of the contract.

End the region with FNSAVE or FSAVE, FNINIT, an empty restore (TW=ffff,
TOP=0) or EMMS. Before ordinary F80 work, restore a masked CW with no pending
unmasked exception; before a C ABI boundary, also restore ABI control
requirements. F80 SSA values stay safe across the region because each has a
private stack home and these intrinsics use only reserved GPR scratch. In
functions that contain these intrinsics, GCM keeps every F80-producing and
F80-consuming instruction in its original block, because pure F80 work moved
into the region would run on the caller's full stack.

Every waiting form, including FLDENV and FRSTOR, requires no pending unmasked
x87 exception at entry. Pending-unmasked capture by no-WAIT saves, and fault
delivery or resumption after restoring unmasked pending state, are outside
the contract.

## Unmasked execution region

`x86_x87_region(descriptor:ptr, control:i32) -> i32` (ID 678) runs one
principal x87 instruction with one exception unmasked. The masked raw
intrinsics above keep their preconditions; the region is not a way to call
them outside those preconditions.

Entry requires empty tags at any TOP, no pending exception, and exactly one of
the six CW exception masks clear. PC and RC are inherited. Masked sticky flags
and condition bits may be seeded; the selected exception's flag, ES and B must
be clear. MXCSR is unaffected. Nonempty entry, asynchronous intervention and
resuming a faulting signal context are outside the contract.

The descriptor holds five pointers at offsets 0, 8, 16, 24 and 32: input A,
input B, output A, output B and observations. The descriptor and observations
must not overlap any operand or result. Operand and result buffers may alias,
because all floating inputs load before result stores; overlapping outputs are
last-store-wins. Only the pointers the operation uses are read. The
observation buffer is writable, 16-byte aligned and at least 2624 bytes.
Operand and result pointers may be unaligned.

`control` is a constant. Bits 0..7 select an operation from the stable table
`keel_ir::X87_REGION_OPS`. Bits 8..9 select the floating input format and bits
10..11 the output format (0 F80, 1 F32, 2 F64). Bit 12 discards floating
results instead of storing them. Format 3, other bits, unused format controls
and discard with a non-F80 output are rejected. Input formats apply only to FLD
preparation. FISTTP controls require SSE3. The 80 operations cover the
numerical, unary, D8, DC and DE arithmetic, comparison, signed integer
conversion, FI, BCD, constant, multi-result and FICOM families.

Stages, in order:

1. Save entry CW and SW without waiting, and set completion to 0.
2. Load B, then A, where applicable. After each load, capture SW and FXSAVE64
   without waiting, then FWAIT. Each completed load overwrites the load image.
   Operations with no floating input take one entry snapshot and wait.
3. Execute the principal instruction. Capture comparison flags when
   applicable, then SW and FXSAVE64, then FWAIT.
4. Store or pop the first result, capture SW and FXSAVE64, then FWAIT. Repeat
   for a second result if TOP shows one. DC destination-ST(1) results are
   exchanged after the principal wait.
5. FNINIT, restore entry CW, advance the empty TOP to entry TOP, capture the
   final SW and FXSAVE64, and set completion to 1.

Normal completion returns the zero-extended principal SW. Every stage stays
when the result is unused. The reset clears sticky flags and condition bits;
earlier observations keep them. A memory or FP fault stops at the faulting
instruction, with no result or reset. Deferred #MF delivery may report FWAIT or
a later waiting instruction; the saved FIP and FOP identify the producer.

Observation header, little endian: u32 phase at 0, u32 completion at 4, then
u16 entry CW at 8, entry SW at 10, load SW at 12, principal SW at 14,
first-store SW at 16, second-store SW at 18 and reset SW at 20, and a u32
FCOMI-family CF/PF/ZF mask at 24. Untouched fields keep caller bytes. Five
512-byte FXSAVE64 images start at 64, 576, 1088, 1600 and 2112: load,
principal, first store, second store and reset. Reserved image fields and
undefined condition bits are diagnostic only.

Phase values: 1 entry; 10 and 11 load B and A; 12 load snapshot; 13 load
wait; 20 principal; 21 principal snapshot; 22 principal wait; 30 and 40 first
and second store; 31 and 41 store snapshots; 32 and 42 store waits; 50 reset;
51 TOP restoration; 52 reset snapshot; 60 completed. The phase is the last
marker before an instruction; the RIP and the saved FIP and FOP attribute a
fault.

Native fault delivery with GCC and Clang on x86-64 Linux has not been
validated. The [SSE/AVX unmasked contract](../intrinsics.md#unmasked-sse-and-avx-exceptions)
covers the raw SSE and AVX intrinsics.

## References

- Intel SDM Vol. 2A,
  [253666](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf):
  EMMS, FABS 3-311, FADD 3-312, FBLD, FBSTP, FCHS 3-319, FCLEX, FDIV 3-334,
  FDIVR 3-337, FINIT, FLD 3-353, FLDCW, FMUL 3-361, FRNDINT 3-373, FSQRT
  3-385, FSTCW, FSTSW, FSUB 3-395, FSUBR 3-398, FTST 3-401, FXAM 3-406.
- Intel SDM Vol. 2,
  [325383-079US](https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf):
  FLDENV, FRSTOR, FSAVE/FNSAVE, FSTENV/FNSTENV.
- Intel SDM Vol. 3A
  [253668-085US](https://cdrdv2-public.intel.com/835754/253668-sdm-vol-3a.pdf),
  section 14.2: MMX instructions set TOP=0. Vol. 3B section 22.18.4: tag
  classification.
- AMD APM [Vol. 1, 24592 revision 3.24](https://docs.amd.com/api/khub/documents/sfvvekC9mDflu6vd3R0NXA/content),
  section 5.12, and [Vol. 5, 26569 revision 3.16](https://docs.amd.com/api/khub/documents/w13cmcpL4f9MCT4WPN6eDg/content).
