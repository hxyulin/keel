# Bounded terminal x87 execution region

`x86_x87_region(descriptor:ptr, control:i32) -> i32` is a physical-state
operation with intrinsic ID 678. Existing masked raw
value APIs retain their documented all-masks-set preconditions. The region is
not a way to call those value recipes outside their preconditions.

Entry requires empty x87 tags at any of the eight TOP values, no pending
exception, and exactly one of the six CW exception masks clear. CW's valid PC
and RC are inherited. Sticky flags for masked exceptions and condition bits may
be seeded; the selected unmasked exception's flag, ES and B must initially be
clear. MXCSR is unaffected. Nonempty entry, asynchronous intervention in the
region and resuming a faulting signal context are outside this contract.

The descriptor contains five native pointers at byte offsets 0,8,16,24,32:
input A, input B, output A, output B, observations. Descriptor and observations
must be disjoint from every operand/result. Operand and result buffers may alias:
all floating inputs are loaded before result stores; FI/integer/BCD principal
memory access precedes later FP result stores. Output buffers may overlap with
last-store-wins behavior. Only pointers used by the selected operation are read.
The observation buffer is writable, 16-byte aligned and at least 2624 bytes.
Unaligned operand/result pointers are allowed with exact selected access widths.

Control is constant i32: bits 0..7 select the stable operation table in
`keel_ir::X87_REGION_OPS`; bits 8..9 select floating input F80/F32/F64 (0/1/2);
bits 10..11 select floating output F80/F32/F64; bit 12 discards post-principal
floating results instead of writing them. Remaining bits and format 3 reject.
Input formats apply to FLD preparation only, not FILD/FBLD principal inputs.
Unused format/discard controls reject; discard requires output format F80 and
executes FSTP ST(0), never an implicit narrowing store. FISTTP controls require SSE3. There are
80 principal controls covering numerical, basic, binary D8/DC/DE, comparisons,
signed integer conversion, FI, BCD, constants, multi-result and FICOM recipes.
The stable control numbers do not depend on Intrinsic discriminants.

The exact ordered stages are:

1. Save entry CW and SW without waiting; initialize completion=0.
2. Load B then A where applicable. After each load capture no-WAIT SW and
   FXSAVE64 state, then FWAIT before the next load. The load image is overwritten
   by each completed load snapshot; a fault retains the last pending producer.
   Operations with no floating preparation take one entry-state load snapshot/wait.
3. Execute exactly one selected principal. Capture integer comparison flags when
   applicable, then no-WAIT SW and FXSAVE64; FWAIT.
4. Store/pop first FP result, capture no-WAIT SW and FXSAVE64; FWAIT. Repeat for
   second result if physically present. DC destination-ST1 is exchanged after
   the principal wait. FSINCOS/FPTAN range failure has no second result; TOP
   determines physical presence, never hardware-undefined condition bits.
5. FNINIT explicitly resets exceptions/tags; restore entry CW, advance empty
   TOP to entry TOP, capture final no-WAIT SW and FXSAVE64, set completion=1.

Normal completion returns zero-extended principal SW. Every stage is retained
if that SSA result is unused. Reset deliberately clears accumulated sticky flags
and condition bits; pre-reset observations retain them. Empty tags, entry TOP
and entry CW are restored. This reset is an explicit new region guarantee, not
a change to raw masked APIs' sticky-state preservation.

A terminal memory or FP fault interrupts the first faulting instruction. No
completion result or reset is promised after a fault. Deferred #MF delivery RIP
may identify FWAIT or a subsequent waiting operation; the saved x87 FIP/FOP
identifies the pending producer and is independently recorded. F32/F64 FLD can
produce input exceptions before the principal; F80 transport has its own FLD
semantics. FIST/FBSTP are principal memory writes, not a later FP-store stage.
No-WAIT snapshots expose pending state before any explicit wait. This is real
IR/codegen access to otherwise inaccessible intermediate stages.

Observation header (little endian): u32 phase at 0; u32 completion at 4;
u16 entry CW at 8; u16 entry SW at 10; u16 load SW at 12; u16 principal SW at 14;
u16 first-store SW at 16; u16 second-store SW at 18; u16 reset SW at 20;
u32 FCOMI-family CF/PF/ZF mask at 24. Untouched fields remain caller bytes.
Five 512-byte FXSAVE64 images start at 64,576,1088,1600,2112 for load, principal,
first store, second store, reset. Hardware-reserved/undefined image fields and
condition bits are diagnostic, never equality promises.

Phase values: 1 entry; 10/11 load B/A; 12 load snapshot; 13 load wait;
20 principal; 21 principal snapshot; 22 principal wait; 30/40 first/second store;
31/41 store snapshots; 32/42 store waits; 50 reset; 51 TOP restoration;
52 reset snapshot; 60 completed. Phase describes the last marker before an
instruction; exact RIP/opcode and saved FIP/FOP determine fault attribution.

Compilation, assembly parity and retention checks do not establish native fault
delivery semantics; native validation with GCC and Clang on x86-64 Linux has not
been completed. See [x86-unmasked-fp.md](../x86-unmasked-fp.md) for the related
SSE/AVX contract.
