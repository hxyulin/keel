# x86 system instructions

Keel encodes and decodes system instructions from the generated ISA forms.
Assembly support, typed compiler intrinsics and runtime qualification are
separate. This page states the contract of each system intrinsic and terminal
adapter. The [generated coverage ledger](../coverage/README.md) lists forms and
lowering recipes. The [CPL0 guest runner](../../../../suite/integration/x86-cpl0/README.md)
owns the emulated guest campaigns, their seeds and their results.

## Common contract

Unless a section says otherwise, every intrinsic on this page:

- is impure, pinned and a compiler memory barrier (`MemoryEffects::Barrier`),
  and stays after optimization when its results are unused;
- is x86-64 only; the AArch64 backend rejects it;
- needs no optional CPU feature bit at NONE, v1, v2 or v3;
- emits its named instruction with no permission probe, feature probe, helper
  call, software fallback, extra hardware fence or exception handler.

A compiler barrier orders compiler memory operations. It is not LFENCE, MFENCE,
CPUID, a C11 synchronization operation or a speculation barrier. Architectural
serialization is a property of the instruction and the CPU.

The caller owns privilege, the architectural state each instruction reads or
changes, and its restoration. Faults are hardware behavior and produce no
result. Every state change must leave code, stack and data mappings, 64-bit
execution and the calling convention valid for later compiled code. These
ordinary-function APIs do not implement a mode, stack, interrupt-entry or
event-return transition. They do not preserve a caller's RFLAGS image: packing
and splitting around an instruction may change arithmetic flags.

Allocated SSA values stay live across each operation. Lowering uses the
allocator-reserved RAX, RCX and RDX scratch registers where it needs fixed
registers.

## Terminal transitions

Transition bytes and a compiler-owned entry/return ABI are different APIs.
Keel assembles the imported system forms. Only these transports own the
register and stack restoration that a terminal transition needs:

- The Linux syscall adapter provides returning 64-bit Linux syscall transport.
  It does not own kernel entry, child entry or stack switching.
- The [IRETQ adapter](#iretq-transport) consumes an explicit image and does not
  return to the compiled caller.
- [SWAPGS](#gs-base-exchange) exchanges GS bases; the caller owns both contexts.
- [SYSEXIT and RSM](#sysexit-and-rsm) have assembly forms only.

Other transition families, software-interrupt vectors, far or segment
transfers and SMM resume need their own state and execution contracts. An
ordinary returning intrinsic, a `noreturn` annotation or a replacement RET
mnemonic cannot supply them. A terminal adapter must specify its input image,
restore every promised state component, install the target stack and transfer
without later compiler spills or epilogues. General `naked` or `interrupt` C
attributes do not replace such an adapter.

## Descriptor tables and selectors

### Table loads

`x86_lgdt(ptr)` and `x86_lidt(ptr)` (IDs 774 and 775) load GDTR and IDTR from
a packed ten-byte image: a 16-bit limit followed by a 64-bit base, little
endian. They require CPL0. The caller owns the image and the table lifetime.

### Observations

IDs 687 to 700 observe descriptor-table and machine-status registers.

`x86_sgdt(ptr)` and `x86_sidt(ptr)` write exactly ten bytes in the same packed
layout, with no padding write. `x86_sldt_mem(ptr)`, `x86_str_mem(ptr)` and
`x86_smsw_mem(ptr)` write exactly two bytes. The instruction performs the
memory access; there is no temporary image or wider store.

`x86_{sldt,str,smsw}{16,32,64}(i64 old) -> i64` exposes a physical register
container:

- r16 replaces bits 15:0 and preserves `old` bits 63:16.
- r32 and r64 use the hardware encoding of that width, not a normalized r16
  observation. The hardware overwrites the bits it defines.
- STR r32 and r64 zero-extend the selector. In IA-32e mode, SLDT r32 and r64
  also zero-extend it.
- SMSW r32 is CR0[31:0] with the upper 32 bits clear, and r64 is CR0[63:0],
  following the Intel SDM's 64-bit-mode examples. No non-64-bit guarantee is
  made.

`old` stays an independent SSA value even when the allocator reuses its home
for the result. The instructions preserve flags.

Preconditions: 64-bit execution with CPL0 or CR4.UMIP clear, and valid
writable memory for the memory forms. SLDT and STR require protected or IA-32e
mode. Unaligned memory is allowed when alignment checking permits it. Linux can
emulate UMIP queries, and CPUID.(7,0):ECX[2] reports UMIP capability rather
than CR4 enablement. A user-mode run can therefore show OS behavior without
showing the raw hardware contract.

### Selector queries

IDs 701 to 716 expose LAR, LSL, VERR and VERW. The selector input is a
register `i16` or an exact m16 operand (the `_mem` forms).

- `x86_lar{16,32,64}` and `x86_lsl{16,32,64}` take the old destination value
  and the selector. They return the destination container and the captured ZF
  as an `i32`. A failed query leaves the old destination. A successful wide LAR
  leaves bits 19:16 undefined.
- `x86_verr` and `x86_verw` return the captured ZF as an `i32`.

`tools/x86-isa/query-bindings.json` generates the metadata for the observation,
selector-query and scalar port-I/O intrinsics; see the
[x86 ISA tools](../../../../tools/x86-isa/README.md#intrinsic-registry-and-family-declarations).

### Mutation and invalidation

IDs 781 to 787:

- `x86_lldt(i16 selector)` and `x86_lldt_mem(ptr selector)`.
- `x86_ltr(i16 selector)` and `x86_ltr_mem(ptr selector)`.
- `x86_lmsw(i16 word)` and `x86_lmsw_mem(ptr word)`.
- `x86_invlpg(ptr address)`.

They return nothing and require 64-bit execution at CPL0. Register forms use
the low sixteen bits. Each `_mem` form uses one hardware m16 operand with no
separate load. LLDT and LTR also access the selected GDT descriptor, and LTR
writes its busy bit.

- LLDT installs an LDT selector and hidden descriptor state. A null selector
  makes LDTR unusable for LDT accesses. A non-null selector must name a present
  LDT descriptor in the GDT. The caller owns its sixteen-byte descriptor and
  the table lifetime.
- LTR requires an available 64-bit TSS descriptor in the GDT. The hardware
  loads TR and marks the descriptor busy. The intrinsic does not switch tasks
  or stacks, clear a previous busy bit or implement an interrupt-entry ABI.
- LMSW applies source bits 3:0 to CR0 and cannot clear PE. Bits above 3 stay
  unchanged. The caller owns the effects on MP, EM and TS and the FPU state. A
  compiler barrier does not make a precise exception boundary for ordinary
  FP/SIMD expressions.
- INVLPG invalidates the page of the computed address on the executing logical
  processor. It does not read the operand, need a present mapping or synthesize
  a data read. A noncanonical address is an architectural no-op. Hardware may
  invalidate more than one page. The API adds no interprocessor broadcast, PCID
  protocol or page-table ownership.

## Control and debug registers

`x86_read_{cr0,cr2,cr3,cr4,cr8}` and `x86_read_dr0` through `x86_read_dr7`
take no arguments and return an `i64`. Each `x86_write_*` takes an `i64` and
returns nothing. IDs 788 to 813 are read/write pairs for CR0, CR2, CR3, CR4,
CR8, then DR0 to DR7.

Each emits one MOV to or from the register with a 64-bit GPR. Spill and result
moves may surround it. Values keep their full bit patterns; the hardware
applies writable and reserved-bit rules. There is no masking or interrupt
masking.

The caller owns permission, reserved bits, legal value combinations, event
delivery and virtualization interception. DR4 and DR5 alias DR6 and DR7 only
with CR4.DE clear; they fault with DE set. Debug access also needs a compatible
DR7.GD. Enabling breakpoints needs a debug exception handler, which the API
does not supply. CR3 changes must keep the mappings the function and its
callers use. CR0 and CR4 changes must keep long mode and the processor state
the selected instructions need, including SIMD spills. CR8 changes affect
interrupt priority. Barriers do not give cross-CPU TLB invalidation or uniform
serialization across these registers.

The assembler accepts both MOV directions between any 64-bit GPR and CR0, CR2,
CR3, CR4, CR8 or DR0 to DR7. Typed validation rejects reserved CR indices, DR8
and above, narrower GPRs, memory operands and LOCK. CR8 uses REX.R, extended
GPRs use REX.B, and the canonical encoding has no REX.W.

## MSRs

`x86_rdmsr(i32 index) -> i64` and `x86_wrmsr(i32 index, i64 value)` (IDs 776
and 777) emit RDMSR (`0f 32`) and WRMSR (`0f 30`). The full 32-bit index goes
in ECX. Reads join EDX:EAX into one `i64`. Writes put value bits 31:0 in EAX and
63:32 in EDX. Both inputs keep their values after the operation.

The caller runs at CPL0, and the MSR must exist and permit the access in the
current CPU and virtual machine. Reserved-bit, canonical-address, feature and
mode rules are the hardware's. The API accepts runtime indices; v1, v2 and v3
select legal encodings and do not promise that every index is usable. The
protocol covers MSRs accessed through ECX and EDX:EAX only. Undefined read bits
keep the hardware result. Changing FS or GS bases requires code whose TLS and
segment-address assumptions stay valid; the intrinsic does not rebind TLS
addresses.

## GS-base exchange

`x86_swapgs()` (ID 818) emits one SWAPGS. It requires CPL0 in 64-bit mode.

The caller owns the active GS base and IA32_KERNEL_GS_BASE: their
canonicality, lifetime and every use of either context. SWAPGS exchanges the
two base values. It does not load a GS selector or change privilege level.
Both contexts must permit all later compiled code, stack accesses, callees and
possible asynchronous handlers. The API does not infer a user or kernel role,
pair a second exchange, save the previous state or define a syscall or NMI
entry protocol. The caller owns event-delivery and speculation policy.

## Interrupt control and halt

`x86_cli()`, `x86_sti()` and `x86_hlt()` emit the corresponding instruction.
`x86_sti_hlt()` emits adjacent STI and HLT as one IR operation. IDs 814 to 817.

CLI and STI require the applicable CPL/IOPL permission; HLT requires CPL0. CLI
changes maskable-interrupt acceptance on the current logical processor; it does
not exclude exceptions, NMI or SMI. STI can allow event delivery inside the
current function. Neither saves or restores the previous IF.

A caller that enables interrupts must provide event delivery, mappings, stacks
and handlers that preserve the interrupted ABI and all live machine state.
Ordinary function entry is not an interrupt-entry ABI. The compiler does not
synthesize an IDT, TSS, register-save wrapper, SIMD save or red-zone
protection.

HLT is a resumable wait, not an IR terminator. If execution resumes, the
instructions after it run. A wake event must exist, or the processor can stay
halted.

`x86_sti_hlt()` guarantees adjacent STI and HLT bytes, with no allocation,
spill, label or call between them. With IF clear on entry and a permitted STI,
the interrupt shadow covers HLT. If IF is already set, STI does not give that
guarantee. Separate STI and HLT operations have no adjacency promise. The
combined operation does not exclude NMI, SMI, debug or other events.

## IRETQ transport

`keel_codegen::x86_64::iretq` exposes a linkable ELF adapter through
`program()` and the equivalent Intel-syntax `assembly()`. Import
`declaration()` (or its signature and `noreturn` attribute) and link the adapter
once. The symbol is `__keel_x86_iretq`. Its SysV AMD64 argument is one image
pointer in RDI. It never returns to the compiled caller.

```text
decl @__keel_x86_iretq(ptr) noreturn
func @resume(ptr) noreturn {
entry(v0:ptr):
 call @__keel_x86_iretq(v0)
 trap
}
```

Keep the ordinary impure call contract and end the IR path explicitly. Do not
mark the declaration pure, readonly, returns-twice or `cc(fast)`. The caller's
stack frame is abandoned.

`iretq::Image` is 160 bytes with eight-byte alignment. Each field is one
little-endian 64-bit word:

| Offsets | Words |
|---|---|
| 0, 8, 16, 24 | RAX, RCX, RDX, RBX |
| 32, 40, 48 | RBP, RSI, RDI |
| 56, 64, 72, 80 | R8, R9, R10, R11 |
| 88, 96, 104, 112 | R12, R13, R14, R15 |
| 120, 128, 136, 144, 152 | RIP, CS, RFLAGS, RSP, SS |

The adapter reads all fifteen non-RSP GPR words. R10 holds the image pointer
until its last load. The final three instructions set RSP to image + 120,
restore R10 from image + 72 and execute IRETQ, which consumes the five-word
frame, including for a same-CPL return. After installing the target state the
adapter emits no stores, calls, spills or epilogue. It has no unwind metadata.

The caller exclusively owns the image and keeps it and the code mapped through
the transfer. At the final instructions the image is the stack, so events and
faults may write below the frame. Provide writable headroom with valid
handlers, or IST stacks and a matching event policy. Disable maskable
interrupts before the call; NMI, debug events and faults stay caller-owned.
IRETQ can unblock NMIs even when it faults.

Execute at CPL0 in 64-bit mode with NT clear and CET shadow stacks disabled.
The target is 64-bit code at the same or a less privileged level. The caller
supplies valid selectors and descriptor tables, canonical RIP and RSP,
mappings and legal flags. Selector words are zero-extended. The adapter does
not validate addresses or sanitize flags. The image covers GPRs and the return
frame only. SIMD and x87 state, FS and GS bases, address-space state and debug
registers need separate ownership. Compatibility-mode returns, task returns,
CET, VMX/SVM and FRED are outside this transport.

## SYSEXIT and RSM

The fixed-opcode adapters expose these operand-free instructions in Intel and
AT&T input:

| Canonical spelling | Aliases | Bytes | Public identity |
|---|---|---|---|
| `sysexit` | `sysexitd`, `sysexitl` | `0f 35` | `FixedOpcode::Sysexit` |
| `sysexitq` | none | `48 0f 35` | `FixedOpcode::Sysexitq` |
| `rsm` | none | `0f aa` | `FixedOpcode::Rsm` |

Bare `sysexit` selects the 32-bit return form. The decoder prints `sysexitq`
for the REX.W form, so reassembling canonical output keeps the distinction.
Explicit operands and LOCK are rejected. GNU Intel syntax requires `sysexitd`
for the 32-bit form; LLVM Intel syntax uses `sysexit`.

These are assembler interfaces, not IR intrinsics. The fixed-instruction
lowering recipe rejects these SYSRET-category forms, because an ordinary void
intrinsic cannot own terminal control flow. SYSEXIT needs a configured
selector and MSR and a target register and stack protocol. RSM needs an SMM
saved-state environment. Neither a CPU profile nor successful assembly
establishes those conditions.

## CPU and cache control

`x86_clts()`, `x86_invd()` and `x86_wbinvd()` (IDs 778 to 780) emit CLTS
(`0f 06`), INVD (`0f 08`) and WBINVD (`0f 09`). They require 64-bit mode at
CPL0.

- CLTS clears CR0.TS. It does not restore saved x87, MMX, SSE or XSAVE state,
  clear CR0.EM or establish FPU ownership. It is not a lazy-FPU handler ABI.
- INVD invalidates caches without writing back modified data. The caller must
  make sure that discarding dirty lines leaves the code, stack and data of
  later instructions valid.
- WBINVD writes back and then invalidates according to the cache hierarchy.

INVD and WBINVD are not interchangeable. Cache scope follows the hardware. Keel
adds no interprocessor broadcast, DMA ownership protocol or TLB shootdown.

### CLFLUSH

`x86_clflush(ptr)` emits one `CLFLUSH m8` at the original pointer, with no
alignment, masking, probing load, fallback or fence. The caller supplies a
readable address in live memory; read-only memory is valid. Invalid addresses
keep their faults. This is not an advisory prefetch. No C frontend mapping
exists.

CPUID leaf 1 EDX bit 19 reports CLFSH independently of SSE2. The psABI lists
CLFSH in none of the baseline, v2 or v3 levels. `Features::clflush`, `+clflush`
and `-clflush` represent it as an independent capability. NONE, the default
and all three levels leave it false, and `Features::all_known()` includes it.
The compiler and both assembler paths require the bit, so `--cpu-level v1`, `v2`
or `v3` without `+clflush` rejects CLFLUSH.

The instruction writes back modified data and invalidates the line across the
coherence domain. Permission and fault checks follow a byte load; the page
accessed bit can be set and the dirty bit is not. Speculative fetches may
refill the line. Intel orders CLFLUSH against CLFLUSH, writes, locked
operations and fences. AMD processors without CLFLUSHOPT order CLFLUSH weakly:
MFENCE is needed before and after it, and LFENCE, SFENCE and serializing
instructions do not order it. With CLFLUSHOPT, locked operations and fences
order it, and only same-line writes and flushes are ordered. This API inserts
no fence; code that needs ordering across vendors must add it.

## Port I/O

### Scalar

IDs 717 to 728 are `x86_in8/16/32`, `x86_out8/16/32`, then the `_imm` forms of
each. IN takes an `i16` port and returns `i8`, `i16` or `i32`. OUT takes an
`i16` port and same-width data and returns nothing. Dynamic forms use DX. The
`_imm` forms keep the `i16` parameter but require a constant in 0..255; other
values are rejected, not masked. Each operation moves exactly 1, 2 or 4 bytes;
there is no 64-bit form. Dead IN results stay because permission faults and
device effects are observable.

The caller supplies CPL, IOPL and TSS I/O-bitmap permission for the whole
access and a device contract for the port, width and effects. There is no
permission probe, IOPL change or emulation. Faults are terminal. Current tests
are compile-only.

### String

IDs 743 to 766 expose INS and OUTS, optionally with REP. For the normal and
`_owned_terminal` sets, the order is non-REP IN8/16/32, OUT8/16/32, then REP
IN8/16/32, OUT8/16/32. Normal names are `x86_ins8/16/32`, `x86_outs8/16/32`,
`x86_rep_ins8/16/32` and `x86_rep_outs8/16/32`. No C frontend syntax exists.

| Form | Signature | Results |
|---|---|---|
| Single | `(port:I16, address:Ptr) -> Ptr` | Hardware next address |
| REP | `(port:I16, address:Ptr, count:I64) -> (Ptr, I64)` | Hardware next address and remaining RCX |
| `_owned_terminal` | Adds a leading `image:I64` operand | Adds `(before:I64, after:I64)` FLAGS images after the pointer and count results |

The REP count is the full unsigned RCX value, with no truncation, signed check
or loop. A zero count reaches the hardware unchanged, so its permission and
exception order are native.

The lowering emits exactly one INSB/INSW/INSD or OUTSB/OUTSW/OUTSD, optionally
with an F3 REP prefix, 64-bit address size and the port in DX. INS writes the
RDI-addressed memory and OUTS reads the RSI-addressed memory. Each element
moves the index by 1, 2 or 4, down when DF=1. REP decrements the full RCX after
each element and returns RCX=0 on normal completion. Returned pointers come
from the hardware. Segments are flat DS and ES with no override.

Permission is checked for every port byte of an element, along with memory
access, canonicality, paging and AC. The caller owns the buffers, exclusive
device use and the device's rate and width rules. Buffers must not overlap
compiler private storage. Faults keep their partial effects: REP may finish a
prefix of the elements, and INS may read the port before a memory fault. There
is no rollback or restart guarantee.

Normal variants are ordinary ABI operations: entry and continuation have DF=0.
The `_owned_terminal` variants are instruction-scoped terminal FLAGS regions.
They capture operands, save the entry image, apply the requested image with
POPFQ, observe FLAGS, execute, observe FLAGS again, reapply the entry image,
restore the fixed register and then publish results. `before` and `after` are
the PUSHFQ images around the principal instruction. The input image is applied
unchanged; the instruction uses the resulting DF and other flags. No CLD, STD,
masking or permission helper is emitted. The entry and environment contract is
the [owned FLAGS region](#owned-flags-region) contract. String operations are
not accepted inside a general FLAGS region.

Functions that use these intrinsics reserve an aligned 80-byte private frame
area. Every SSA operand is copied through RAX into its own private home before
any implicit register changes. The ten m64 slots hold the saved index register,
port, address, count, requested image, entry image, next pointer, remaining
count and the two FLAGS images. The port loads with `MOV DX, m16`. The
allocated RDI (INS) or RSI (OUTS) is saved first and restored before any result
is published. A post-frame checker rebuilds the expected instruction list from
the API descriptor and rejects any difference. Typed and text output use the
same checked program. State-owning variants cannot be inlined or moved by GCM.

GNU as and Keel emit the REP word prefixes as `66 F3`; Clang's mnemonic emits
`F3 66`. Cross-assembler tests force Clang to an explicit `66` prefix with the
REP dword mnemonic, which gives the same bytes.

```sh
cargo test -p keel-codegen --test x86_string_port_io
```

## Performance-counter read

`x86_rdpmc(i32 selector) -> i64` (ID 731) executes `0f 33` with the full
selector in ECX and returns `EAX | (u64(EDX) << 32)`. An unused result still
executes, and two reads stay two reads. No index sanitization, counter setup or
retry is inserted.

The caller owns the PMU environment: implemented instruction, a selector valid
for the vendor and model, and permission (CPL0 or CR4.PCE in protected long
mode). v1, v2 and v3 do not imply counter availability. Bad indices and denied
permission keep the #GP(0). The result is the hardware EDX:EAX value, including
zero extension of unimplemented bits and per-counter user-disable behavior. It
is not a monotonic or serialized measurement API. Performance-metrics reads may
clear hardware state when configured; the barrier effect covers that.

## RFLAGS

### Raw read

| ID | Signature | Status |
|---|---|---|
| 729 | `x86_rflags_read() -> i64` | Supported |
| 730 | `x86_rflags_swap(i64) -> i64` | Rejected as a standalone intrinsic |

A read emits `PUSHFQ; POP rax` followed by flag-neutral MOV publication or
spill. The result is the architectural PUSHFQ image, with RF and VM cleared by
hardware. It observes the flags at that PUSHFQ, after the prologue, argument
transport and earlier lowering, not at function entry. Compiler arithmetic,
comparisons and branches change flags, so successive reads do not show a
continuous value. GCM is disabled in functions that contain either intrinsic,
so pure arithmetic does not move across the observation. Source arithmetic
cannot seed raw flags.

Only the reserved RAX is clobbered. PUSHFQ writes exactly eight bytes at
`[old RSP-8, old RSP)`, POP reads them back, and RSP is restored on success.
The backend has no red-zone locals, so fixed objects are above RSP, even with
dynamic alloca. Functions using these intrinsics are never frameless, so the
`rbp+16` CFA stays valid between the two instructions. After a fault, RSP is
not recovered. The caller supplies an ABI-compatible state and an accessible
stack.

### Why standalone swap is rejected

A swap would capture the old flags with PUSHFQ and POP, then execute
`PUSH r64; POPFQ`. POPFQ applies the real CPL, IOPL, reserved-bit, RF, VM,
VIF/VIP, IF, DF, TF, AC/SMAP and ID rules, including faults, so masking the
input would not implement the API. IR intrinsic metadata carries only purity,
memory effects and trap properties. It has no FLAGS token, CFG region or
restoration proof:

- Pinning the swap does not stop pure arithmetic from moving across it.
- Disabling GCM does not stop later lowering from emitting ADD, CMP or TEST,
  library calls or an epilogue after POPFQ.
- The verifier cannot prove that an arbitrary input restores DF=0 and legal
  TF, AC, IF and IOPL state before calls or return.
- Returning the old image cannot restore RF or VM, which PUSHFQ clears.

Standalone swap therefore fails before allocation, even when its result is
dead. 16-bit and compatibility-mode forms are not implemented.

### Owned FLAGS region

`x86_rflags_region` runs a small closed raw program with exclusive ownership
of RFLAGS. It is the only way to swap flags. `InstData::X86RflagsRegion
{ captures, program }` binds the program to `i64` captures and results, keeps
the signature of intrinsic 730 and adds no intrinsic ID. The `terminal`
keyword adopts the caller and environment contract below; it is a
declaration, not runtime discovery.

```text
func @owned(i64)->i64 {
block0(v0:i64):
 v1,v2 = x86_rflags_region terminal "save; swap 0 0; read 1; reapply; normal"(v0)
 return v2
}
```

Blocks in the quoted program are separated by `|` and operations by
semicolons. `jump N` is a direct edge; `jcc C T F` uses architectural condition
0..15 with taken and not-taken block indices. The argument and result lists
declare the capture and observation counts. Arbitrary assembly is not
accepted.

In the outer CFG the wrapper is one pinned, retained, trapping memory barrier
with one normal successor. GCM is disabled in containing functions, and
inlining a function that contains a region is rejected. There is no invoke,
unwind, call, tail call, return or inline asm inside ownership. AArch64
rejects the wrapper.

Raw CFG rules:

- Block 0 is the only entry and starts with SaveEntry, which captures the
  PUSHFQ image after all captures are staged. SaveEntry creates the only owned
  token. Read and Swap consume and replace it. Jumps forward it, and Jcc
  forwards it along the taken path using the existing hardware flags.
- Joins are implicit token phis, and incoming tokens must be in the same phase.
  Every normal exit defines all results, and no path overwrites one.
- ReapplyEntryImage consumes ownership. Only direct jumps and the Normal exit
  may follow it.
- All edges go to later blocks, and every block is reachable. Limits are 64
  blocks, 256 operations, 16 captures and 16 observations. Ownership cannot
  escape into a block parameter, user memory or an ordinary `i64`.

The operations are SaveEntry, Swap, Read, direct JMP, direct Jcc and
ReapplyEntryImage. SaveEntry and Read expand to `PUSHFQ; POP r64`. Swap
captures the old image with that pair, then runs `PUSH captured-r64; POPFQ`.
Reapply runs `PUSH private-saved-r64; POPFQ`. Each push and pop moves exactly
eight bytes. The final action is named ReapplyEntryImage, not "restore
RFLAGS", because PUSHFQ clears RF and VM and POPFQ does not write every bit.

The expansion reserves an aligned private frame area: one saved image and one
eight-byte home per capture and per observation. All input copies finish before
SaveEntry, so aliases and spilled inputs cannot change a later swap input. The
core expansion uses only RAX, and private homes are frame-relative (RBP or the
reserved frame base R15). Nothing inside ownership allocates, probes, realigns
or spills. After selection, allocation and frame layout, a checker parses the
typed instruction list against the raw CFG. It rejects hidden ADD, CMP or TEST,
string instructions, calls, directives, extra stack operations, wrong scratch
registers, overwritten saved inputs and trailing instructions. The CFA stays
`rbp+16` at every intermediate PC.

The `terminal` wrapper declares these preconditions:

- 64-bit mode, an ordinary ABI frame and stack, and a normal continuation.
- DF=0, TF=0, AC=0 and NT=0 on entry, and no branch-step debug mode.
- An accessible private frame and stack.
- Unchanged CPL and privilege, debug and SMAP environment until normal
  completion.
- An external terminal monitor for every fault or trap.

IF and IOPL may be inherited, including at CPL0; no application-only mask
applies to any input image. The saved image describes the machine after the
prologue and all captures. Reapplying it under an unchanged CPL and
environment restores the writable entry bits: CPL0 can restore IOPL and IF, and
DF, TF, AC and NT return to their entry state. AC also controls SMAP accesses
at CPL0.

Every expanded instruction is an exceptional boundary. TF set by POPFQ traps
after the next instruction, so the PUSH after POPFQ can trap with RSP at -8 and
the restore may never run. AC can cause alignment faults, PUSHFQ, POPFQ and
private storage can fault, and IF changes allow interrupts before restoration.
A fault or trap produces no result, and nothing resets, unwinds, restores or
resumes. The monitor must consume the saved hardware context outside generated
code. C signal handlers, handler return into the region, C cleanup, longjmp and
re-entry are outside the contract.

## Structural inventory

Run the read-only inventory against the generated ledger:

```sh
python3 tools/x86-isa/audit_system.py > target/x86-system-inventory.json
```

The tool joins a manual system roster with imported forms and checks the
ledger input hashes. It reports structural matches, not runtime semantics.

## References

- The combined Intel SDM 325462-093US (September 2026) covers every instruction
  on this page:
  <https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf>.
  Page numbers used above: IN 3-455, INS 3-461, OUT 4-171, OUTS 4-173, POPF
  4-407 (Table 4-20), PUSHF 4-528, RDPMC 4-551, REP 4-563, SGDT 4-629, SIDT
  4-655, SLDT 4-657, SMSW 4-659, STR 4-680. Vol. 1 sections 3.4.3.2, 3.4.3.3
  and 20.5.2; Vol. 3B section 20.3.1.4 (single-step exception).
- Intel [ISA extensions reference 059](https://cdrdv2-public.intel.com/865891/319433-059-architecture-instruction-set-extensions-programming-reference_2025-0930.pdf),
  chapter 15, for per-counter user-disable behavior.
- [AMD APM 24594 revision 3.37](https://docs.amd.com/api/khub/documents/j44LvPXzuuXgM0WyHKQfeQ/content),
  CLFLUSH, INSx, OUTSx and repeat prefixes.
- [x86-64 psABI processor levels](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex).
