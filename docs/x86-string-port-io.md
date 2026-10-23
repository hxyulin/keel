# Exact string port I/O: production contract design

Status: source/build stage only; runtime/owned-state acceptance and full profile
completion remain false. Base: main 08fe0acf, ALL743 preserved. No frontend work.

## API and state boundary

Append 24 IDs, 743..766. For each of normal and `_owned_terminal` contexts,
append non-REP IN8/16/32, OUT8/16/32, then REP IN8/16/32, OUT8/16/32.
Normal names: `x86_ins8/16/32`, `x86_outs8/16/32`,
`x86_rep_ins8/16/32`, `x86_rep_outs8/16/32`.

Normal single: `(port:I16, address:Ptr)->Ptr` (hardware next address).
Normal REP: `(port:I16, address:Ptr, count:I64)->(Ptr,I64)` (hardware next
address and remaining RCX). I64 is the complete unsigned RCX bit pattern,
including counts above 2^32/2^63; no truncation, signed check or loop synthesis.
Zero count is passed unchanged to the hardware instruction, including its native
permission/exception ordering; no address preaccess or fabricated early return.

The corresponding `_owned_terminal` name adds a first `image:I64` operand and
adds `(before:I64, after:I64)` FLAGS image results after the pointer/count results.
These are actual PUSHFQ images immediately before/after the principal operation,
with hardware RF/VM image normalization, permitting exact unchanged-FLAGS checks
without pretending POPFQ writes every supplied bit. The unmodified input image
is applied by POPFQ. The string instruction inherits the resulting hardware DF
(and all actual other FLAGS), including DF=0 and DF=1. No CLD/STD, image masking,
numerical direction emulation, scalar-I/O loop or permission helper is emitted.

Normal APIs are ordinary ABI operations: entry/continuation obey DF=0. They emit
actual inherited-DF instructions, but do not authorize invoking a C function
with DF=1. General C-function entry FLAGS equality is not promised. The owned
variants are opaque instruction-scoped terminal regions: capture operands, save
entry image, apply requested image, observe-before, execute, observe-after,
reapply entry image, restore fixed allocated register, then publish SSA results.
No ordinary IR/call/return can enter this sequence. Reapplying PUSHFQ's entry
image uses actual POPFQ semantics, not an assertion of complete FLAGS equality.

The owned entry/environment contract is the existing explicit terminal FLAGS
contract: long mode, stable accessible ABI frame/stack, entry DF/TF/AC/NT=0,
controlled privilege/debug/interrupt/SMAP environment, and an external terminal
monitor for every exception. The full input image and all hardware effects are
retained. Normal completion requires entry-image reapplication and actual ABI
execution state; faults/traps publish no results and permit no automatic cleanup,
C unwind, arbitrary PC resumption or ordinary handler re-entry. No breakpoint or
tracing configuration is added by this family. Standalone FLAGS swap remains
rejected and the general raw FLAGS wrapper body remains closed to string ops.

## Architectural contract

One exact INSB/INSW/INSD or OUTSB/OUTSW/OUTSD, optionally F3 REP, default 64-bit
address size (no 67), fixed DX port; 8/16/32-bit elements only (no 64-bit form).
INS writes RDI-addressed memory from the port; OUTS reads RSI-addressed memory to
the port. Each completed element changes the index by +1/2/4 with DF=0 or
-1/2/4 with DF=1. A single operation does not consume RCX. REP uses full RCX,
decrements after successful elements, and on normal completion returns RCX=0.
Pointers are returned from hardware, not recomputed using a numerical recipe.
Principal instructions affect no FLAGS and leave DX unchanged. Default flat
long-mode DS/ES only; no FS/GS/address-size override extension is offered.

Permission is exact CPL/IOPL/TSS I/O-bitmap permission for every port byte of an
element; memory readability/writability, canonicality, paging and AC checks apply.
The caller owns valid buffers, exclusive device use and device transaction-rate
and access-width contracts, disjoint from compiler private storage/control data.
No automatic permission grant, probing, emulation or host permission promise is
added. The owned form applies the explicit full caller image with actual POPFQ
semantics, including any privileged IOPL effect; it does not manufacture an
IOPL-enabling image or bypass hardware checks.

Faults preserve architectural partial effects: REP may have completed a prefix;
terminal hardware context carries the next index and remaining count at the
interrupted instruction. INS may read the port and then fault/VM-exit writing
memory, so a failed iteration need not be device-effect-free. No transactional
rollback, exact device event count inferred from RCX alone, or arbitrary restart
promise is supplied. Root must qualify native observations against processor /
virtualization context. Zero-count permission ordering is left to actual REP,
never optimized to a software no-op or an added permission check.

All APIs are impure pinned memory/fault barriers even if every output is dead.
INS/OUTS memory direction remains part of the architectural contract; generic
memory/integer rules are unchanged, and Barrier is not a new CPU fence. Baseline
NONE/V1/V2/V3; explicit AArch64 rejection.

## Concrete resource and ownership proof before enabling production

Only containing functions reserve an aligned 80-byte private frame area using
existing frame/SlotOffsets helpers. No change to allocator pools, global RDI/RSI
reservation, unrelated function frames or ordinary IR scheduling. Every SSA
operand is copied with reserved RAX to its own immutable private home BEFORE any
implicit register is changed or output is published. This includes aliased SSA
values, register inputs, spilled inputs and temporally reused input/result homes.

Private slots: saved index, port, address, count, requested image, entry image,
next pointer, remaining count, before image, after image (ten exact m64 slots).
Port materialization consumes only its low I16 through an exact MOV DX,m16.
Before destructive index loading, save the allocated RDI (INS) or RSI (OUTS) in
the dedicated slot; restore it before any SSA publication. RAX/RCX/RDX are already
allocator-reserved S0/S1/S2. REP loads RCX with exact m64. Non-REP leaves RCX alone.
No SSA allocation, rematerialization, call, integer arithmetic, frame adjustment
or synthetic loop occurs inside the owned sequence.

A post-frame/pre-emission checker reconstructs the exact typed instruction and
operand/label list independently from the API descriptor, verifies contiguous,
aligned, disjoint private-frame provenance and bounds, and rejects any deletion,
extra instruction, wrong width, lock, changed register/home, altered prefix,
stack imbalance, image overwrite or publication before restoration. All stack
pairs have RSP -8 then 0 under stable rbp+16 CFA; every instruction is a possible
terminal fault boundary, and REP's partial progress is the principal boundary.
Both typed and text output consume this same checked Program. State-owning
variants cannot be inlined or GCM-moved across ordinary IR; existing optimizer
state guards must include the new opaque boundary if not already covered by
impure barriers. No shared raw-plan/parser restriction is relaxed.

## Source review and subsequent proof

Intel SDM 093: INS Vol.2A 3-461..463; OUTS Vol.2B 4-173..176;
REP Vol.2B 4-563..564; Vol.1 20.5.2 (bitmap covers each port byte).
Cached audit primary PDF SHA256:
`a4a62e6a7ba11a76c7753a195b825087306812aac39b930973f9168ee599f321`.
Official pin:
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
AMD APM Vol.3 publication24594 INSx/OUTS/Repeat Prefixes is cross-reviewed;
source retrieval provenance and revision are recorded with the additive packet.

Before a long matrix, send the independently compiling production checkpoint and
an exact proof plan. Separate source-only proof covers raw/opt dead retention,
normal and owned DF contexts, zero/full64 counts, all inputs/results live, dying
home reuse, fixed-register banks, spills/tiny/empty, aliases, memory guards,
FLAGS images and typed/text plus forced Clang/GAS exact encodings. Prepare a
candidate packet that imports the accepted port-I/O and permission guest
infrastructure and its immutable seals, rather than a duplicate planner. Root
owns guest build/enrollment/execution, CPL/IOPL/TSS/partial-fault evidence,
independent review and native acceptance. Child executes no target/guest/remote
code and makes no whole-profile promotion.

Source/proof notes: `suite/evidence/x86-string-port-io-20261023/primary.json`
records exact primary provenance. AMD's official current endpoints failed in this
environment; the AMD-authored mirror actually contains revision3.04, April2003,
despite its r3.08 filename. This is an explicitly older cross-check, with no claim
of current AMD/native qualification. Intel093 supplies the partial-fault caveat.

GAS and Keel emit REP word prefixes as `66 F3`; Clang's ordinary mnemonic emits
`F3 66`. Exact forced cross-oracles use an explicit66 prefix followed by REP's
dword mnemonic in Clang, producing the same word instruction and byte order.
Both forced complete streams and the unconstrained Clang literal form are
checked; no extra prefix or I/O instruction is dropped from a checked stream.
`.p2align` directives alone are removed identically before complete-stream
comparison, avoiding assembler-specific NOP padding choices.

Large matrix captures, observed assembly, logs and generated packet metadata
remain local external payloads under root's a95a9a9c artifact policy. The tracked
manifest gives local paths/hashes for root registration. Packet sources supply
independent references and a finite expected-state model for integration with
existing accepted guest infrastructure, with no new boot planner or runner.
