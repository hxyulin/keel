# Exact string port I/O

Keel exposes x86-64 string port I/O (INS/OUTS, optionally with REP) as IR
intrinsics. Status: implemented and checked at the compile/encoding level;
runtime validation of the owned-state variants is still outstanding. There is no
frontend syntax.

## API

24 intrinsic IDs, 743..766. For each of the normal and `_owned_terminal`
contexts, in order: non-REP IN8/16/32, OUT8/16/32, then REP IN8/16/32,
OUT8/16/32.

Normal names: `x86_ins8/16/32`, `x86_outs8/16/32`, `x86_rep_ins8/16/32`,
`x86_rep_outs8/16/32`.

| Form | Signature | Results |
|---|---|---|
| Single | `(port:I16, address:Ptr) -> Ptr` | Hardware next address |
| REP | `(port:I16, address:Ptr, count:I64) -> (Ptr, I64)` | Hardware next address and remaining RCX |
| `_owned_terminal` | Adds a leading `image:I64` operand | Adds `(before:I64, after:I64)` FLAGS images after the pointer/count results |

The REP count is the complete unsigned RCX bit pattern, including counts above
2^32 and 2^63; there is no truncation, signed check, or loop synthesis. A zero
count is passed unchanged to the hardware instruction, keeping its native
permission and exception ordering; there is no address pre-access or early
return.

### Normal variants

Normal APIs are ordinary ABI operations: entry and continuation obey DF=0. They
emit the real instruction, which uses the inherited DF, but do not authorize
calling a C function with DF=1. Equality of FLAGS with the C-function entry
state is not promised.

### Owned variants

The `_owned_terminal` variants are opaque instruction-scoped terminal regions.
The sequence is: capture operands, save the entry image, apply the requested
image with POPFQ, observe before, execute, observe after, reapply the entry
image, restore the fixed allocated register, then publish SSA results. No
ordinary IR, call, or return can enter this sequence.

The `before`/`after` results are the actual PUSHFQ images immediately around the
principal instruction (with hardware RF/VM normalization), which allows exact
unchanged-FLAGS checks without pretending POPFQ writes every supplied bit. The
input image is applied unmodified. The string instruction uses the resulting
hardware DF (0 or 1) and all other actual flags. No CLD/STD, image masking,
direction emulation, scalar I/O loop, or permission helper is emitted.
Reapplying the entry image uses real POPFQ semantics; it does not assert full
FLAGS equality.

The owned entry/environment contract is the terminal FLAGS contract from
[x86-rflags-owned-region.md](rflags-owned-region.md): long mode, a stable
accessible ABI frame and stack, DF/TF/AC/NT=0 on entry, a controlled
privilege/debug/interrupt/SMAP environment, and an external terminal monitor for
every exception. Normal completion requires entry-image reapplication and a
real ABI execution state. Faults and traps publish no results and permit no
automatic cleanup, C unwind, arbitrary PC resumption, or handler re-entry. This
family adds no breakpoint or tracing configuration. Standalone FLAGS swap stays
rejected, and the general raw FLAGS region body does not accept string
operations.

## Architectural contract

- Exactly one INSB/INSW/INSD or OUTSB/OUTSW/OUTSD, optionally with an F3 REP
  prefix, default 64-bit address size (no 67 prefix), and the port in DX.
- 8/16/32-bit elements only; there is no 64-bit form.
- INS writes RDI-addressed memory from the port; OUTS reads RSI-addressed memory
  to the port.
- Each completed element moves the index by +1/2/4 with DF=0 or -1/2/4 with
  DF=1. A single operation does not use RCX. REP uses the full RCX, decrements
  it after each successful element, and returns RCX=0 on normal completion.
- Returned pointers come from the hardware, not a numerical recomputation.
- The principal instruction affects no FLAGS and leaves DX unchanged.
- Default flat long-mode DS/ES only; no FS/GS or address-size override.

Permission is the exact CPL/IOPL/TSS I/O-bitmap check for every port byte of an
element, and memory readability/writability, canonicality, paging, and AC
checks apply. The caller owns valid buffers, exclusive device use, and the
device's transaction-rate and access-width requirements; buffers must be
disjoint from compiler private storage and control data. No permission grant,
probing, emulation, or host permission is added. The owned form applies the
caller's full image with real POPFQ semantics, including any privileged IOPL
effect; it does not manufacture an IOPL-enabling image or bypass hardware
checks.

Faults preserve architectural partial effects: REP may have completed a prefix
of the elements, and the faulting context carries the next index and remaining
count. INS may read the port and then fault (or VM-exit) writing memory, so a
failed iteration need not be free of device effects. There is no transactional
rollback, no exact device event count inferred from RCX alone, and no restart
guarantee. Zero-count permission ordering is left to the real REP instruction,
never optimized into a software no-op or an added check.

All APIs are impure, pinned memory/fault barriers even when every output is
dead. Barrier here is a compiler ordering constraint, not a CPU fence. Available
at baseline NONE/V1/V2/V3; AArch64 rejects them.

## Resources and checking

Only functions containing these intrinsics reserve an aligned 80-byte private
frame area using the existing frame/SlotOffsets helpers. Allocator pools,
global RDI/RSI reservation, unrelated frames, and ordinary IR scheduling are
unchanged. Every SSA operand is copied through the reserved RAX into its own
immutable private home before any implicit register changes or output is
published. This covers aliased values, register inputs, spilled inputs, and
reused input/result homes.

The ten exact m64 private slots are: saved index register, port, address, count,
requested image, entry image, next pointer, remaining count, before image, and
after image. The port is loaded with an exact `MOV DX, m16` from its low 16 bits.
Before the index register is overwritten, the allocated RDI (INS) or RSI (OUTS)
is saved and then restored before any SSA publication. RAX/RCX/RDX are already
allocator-reserved scratch registers. REP loads RCX from its m64 slot; non-REP
leaves RCX alone. No SSA allocation, rematerialization, call, integer
arithmetic, frame adjustment, or synthetic loop occurs inside the owned
sequence.

A post-frame, pre-emission checker independently reconstructs the exact typed
instruction and operand/label list from the API descriptor, verifies that the
private-frame homes are contiguous, aligned, disjoint, and in bounds, and
rejects any deletion, extra instruction, wrong width, LOCK, changed register or
home, altered prefix, stack imbalance, image overwrite, or publication before
restoration. All stack pairs move RSP by -8 then 0 under a stable `rbp+16` CFA.
Every instruction is a possible terminal fault boundary, and REP's partial
progress is the principal one. Typed and text output consume the same checked
program. State-owning variants cannot be inlined or moved by GCM across
ordinary IR.

## Encoding notes

GAS and Keel emit the REP word prefixes as `66 F3`; Clang's ordinary mnemonic
emits `F3 66`. Cross-assembler comparisons force Clang to use an explicit `66`
prefix followed by the REP dword mnemonic, which yields the same instruction
and byte order. `.p2align` directives are stripped identically before
complete-stream comparison to avoid assembler-specific NOP padding.

```sh
cargo test -p keel-codegen --test x86_string_port_io
```

## References

- Intel SDM revision 093: INS Vol. 2A 3-461..463; OUTS Vol. 2B 4-173..176; REP
  Vol. 2B 4-563..564; Vol. 1 20.5.2 (the I/O bitmap covers each port byte).
  https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
- AMD APM Vol. 3 (publication 24594), INSx/OUTSx and repeat prefixes, used as an
  older cross-check (revision 3.04).
