# Owned FLAGS region

`x86_rflags_region` is an x86-64 IR wrapper that runs a small, closed raw
program with exclusive ownership of RFLAGS. It is the only way to perform a
FLAGS swap; standalone intrinsic 730 (`x86_rflags_swap`) stays rejected (see
[x86-raw-rflags.md](x86-raw-rflags.md)).

Status: the compiler side is implemented. `InstData::X86RflagsRegion` binds the
raw program to I64 captures and results, runs a structural verifier, and expands
through an exact post-allocation resource checker. The explicit `terminal`
keyword adopts the caller/environment contract below; it is not runtime
discovery. Native acceptance has not been established, and full raw-FLAGS
support remains incomplete.

## Syntax

Observations are named in program order:

```text
func @owned(i64)->i64 {
block0(v0:i64):
 v1,v2 = x86_rflags_region terminal "save; swap 0 0; read 1; reapply; normal"(v0)
 return v2
}
```

Blocks in the quoted raw program are separated by `|`; operations within a block
are separated by semicolons. `jump N` is a direct raw edge; `jcc C T F` uses
architectural condition 0..15 and taken/not-taken block indices. The wrapper's
argument and result lists declare the capture and observation counts. Arbitrary
source assembly is not accepted.

## Design background

`x86_x87_region(ptr,i32)->i32` is a single pinned memory-barrier intrinsic whose
backend owns the internal sequence and private scratch registers, takes
snapshots, and resets x87 state (FNINIT) before normal completion. It has no
reusable CFG token verifier, and its terminal-fault and entry assumptions are
specific to floating point; its reset or handler policy does not carry over to
FLAGS. Inline asm's `~{cc}` is a clobber declaration, not an ownership token,
exception boundary, or restore-before-call proof.

Pure IR arithmetic is scheduled independently of pinned operations. Disabling
GCM (already done for the standalone raw FLAGS intrinsics) does not exclude
constant folding, GVN, tree selection, compare fusion, register allocation,
implicit libc calls, spill transport, or epilogue arithmetic. An ordinary
function CFG between two swap intrinsics is therefore not a viable design.

## Wrapper and CFG/token contract

`InstData::X86RflagsRegion { captures, program }` keeps the fixed signature of
intrinsic 730 and adds no new intrinsic ID. Its raw Swap operation implements
the full hardware contract of 730 inside ownership.

- The wrapper is one pinned, retained, trapping memory barrier in the outer
  function CFG.
- GCM is disabled in containing functions, and inlining a function that contains
  a region is rejected. No optimizer can inspect or schedule into the raw body.
- Its sole normal successor resumes ordinary IR after the entry image is
  reapplied.
- There is no invoke, unwind-to-cleanup, call, tail call, return, or inline asm
  inside ownership.
- AArch64 rejects the wrapper before emission.

The raw CFG rules:

- Block 0 is the sole entry and begins with SaveEntry. Captures are bounded,
  typed I64 values staged before it. SaveEntry captures the actual PUSHFQ image
  at that machine boundary into a private immutable capability.
- SaveEntry creates the sole implicit owned token. Read and Swap consume and
  replace it. A direct forward jump forwards it; a Jcc transfers it along exactly
  the taken path. Jcc reads the existing hardware flags without synthesizing
  CMP/TEST.
- Joins are implicit token phis, and all incoming tokens must be in the same
  owner phase. Observations use must-defined and may-defined sets: every normal
  exit needs all results, and no path may overwrite one. Distinct alternatives
  may define the same observation; the selected value is published after exit.
- ReapplyEntryImage consumes active ownership and produces an image-reapplied
  token. Only direct jumps and Normal exit may follow. Another swap/read,
  duplicate reapplication, a mixed-phase join, or an un-restored exit is
  rejected.
- All edges target later in-range blocks, and every block is reachable. Limits
  are 64 blocks, 256 operations, 16 captured I64 images, and 16 observations, so
  boundedness does not depend on a loop trip count. Ownership cannot escape into
  a function block parameter, be stored in user memory, or be copied as an
  ordinary I64. This is structural linearity, not machine-state equality.

The saved capability cannot be passed as an ordinary input or overwritten. The
final action is deliberately named **ReapplyEntryImage**, not "restore RFLAGS":
PUSHFQ clears RF/VM in its image, and POPFQ does not write every bit. A
structural Normal exit is not on its own permission to resume the ABI; the exact
machine check and the declared entry/environment preconditions also apply.

## Allowed operations and machine resources

The implemented subset is SaveEntry, exact Swap, exact Read, direct JMP, direct
Jcc, and ReapplyEntryImage. No ordinary integer/SIMD operation, user-memory
load/store, string operation, arbitrary opcode, or native call is accepted. This
restricts compiler lowering; it does not mask the swap image or make arbitrary
architecture state safe. Full-width I64 inputs are unchanged and all hardware
permissions and side effects remain in effect. Adding ordinary integer, SIMD, or
memory operations would require a separate exact lowering proof.

The expansion assigns explicit resources to every emitted instruction: full
FLAGS use/definition (including execution-control bits), the private saved-image
capability, captured inputs, observations, scratch GPRs, RSP, private frame
memory, CFA, and exceptional PC. A checker verifies the typed instruction list
after selection, allocation, and frame layout, before text or typed output
consumes it.

Expansion:

- SaveEntry and Read expand to `PUSHFQ; POP r64`.
- Swap captures the old image with that pair, then `PUSH captured-r64; POPFQ`.
- Reapply uses `PUSH private-saved-r64; POPFQ`.

Each push/pop accesses exactly eight bytes with a transient RSP delta of -8 then
0. There is no 16/32-bit form and no numeric flag packing. The saved image and
every captured input survive all branches and scratch reuse, and no allocatable
live SSA value is clobbered. Inputs are copied before any state change or output
write, so input/output aliases and spilled inputs cannot alter a later swap
input.

Preflight reserves a dedicated aligned contiguous frame area: one immutable
saved image, one eight-byte home per capture, and one per observation. It
validates the plan, I64 signature, frame bounds/alignment, and the stable
`rbp+16` CFA before emission. All input uses, rematerialization, and copies
finish before SaveEntry. The core expansion uses only the reserved RAX; allocated
GPR and SIMD SSA values remain intact. Private MOV homes are frame-relative (RBP
or the reserved aligned frame base R15), eight-byte aligned, disjoint, and
survive all paths. No dynamic allocation, probing, realignment, or new spill is
inserted inside ownership. Observation stores are explicit recipe instructions;
normal SSA publication and spills occur after the checked done label.

An independent sequence parser checks every typed opcode, exact operand and
width, label and branch destination, private-home provenance, and the complete
PUSHFQ/POP and PUSH/POPFQ pairs against the verified raw CFG. No CFG edge
carries a nonzero RSP delta. It rejects hidden ADD/CMP/TEST, string
instructions, calls, directives, text fragments, extra stack operations, wrong
scratch registers, overwritten saved inputs, and trailing instructions. The same
checked program feeds both text and typed output. Operation labels identify raw
fault sites, and the CFA stays `rbp+16` even at a pushed intermediate PC. After
reapplication, publication uses normal MOV transports and the ordinary
epilogue.

## Entry, permission, and normal completion

The `terminal` wrapper declares these caller/environment preconditions:

- 64-bit mode, a coherent ordinary ABI frame and stack, and a normal
  continuation.
- DF=0, TF=0, AC=0, and NT=0 on entry.
- Controlled debug configuration (no branch-step mode).
- Accessible private frame and stack.
- Unchanged CPL and privilege/debug/SMAP environment through normal completion.
- An external terminal monitor for every fault or trap.

IF and IOPL may be inherited as permitted by the controlled environment,
including at CPL0; the wrapper applies no application-only mask to any input
image. Stack access permissions are architectural and faults are terminal. These
are declared preconditions, like valid memory for a raw operation, not claims
that the compiler proves the external environment dynamically.

The saved image describes the machine boundary after the compiler prologue and
all captures, not function entry or interrupt entry. PUSHFQ clears RF/VM in its
image; there is no promise to recover RF or arbitrary saved interrupt/signal
contexts.

POPFQ applies real CPL/IOPL semantics: IOPL changes only at CPL0, IF is writable
only with sufficient privilege, and protected privilege fields can stay
unchanged without a privilege exception. Reserved bits, RF, VM, and VIF/VIP get
their architectural treatment. DF, TF, AC/SMAP, ID, NT, and the other writable
bits are all in scope. AC is not only CPL3 alignment checking: at CPL0 it also
affects SMAP accesses.

Reapplying the entry image under an unchanged CPL and environment restores the
writable entry execution bits: CPL0 can restore IOPL/IF; at nonzero CPL, IOPL
cannot change, so IF's writability is the same as at entry. DF/TF/AC/NT return
to their required entry state on a normal path. Read-only bits keep their
hardware treatment. An outstanding trap from an instruction that observed TF is
terminal even if its RIP is after reapplication. Result publication, ordinary
arithmetic, calls, and return therefore rely on the normal ABI state only after
actual normal completion under these preconditions.

## Exceptional completion: terminal external monitor

Every expanded instruction is its own exceptional boundary; a balanced normal
sequence does not hide its intermediate states. Intel SDM Vol. 3B section
20.3.1.4 states that TF set by POPF causes a single-step trap after the next
instruction, so the PUSH immediately after POPFQ can trap with RSP at -8 and the
subsequent restore may never run. A trapping restore can also leave a pending
debug event after apparent reapplication. AC can cause alignment faults, and
AC/SMAP changes can alter supervisor access permissions. PUSHFQ/POPFQ and
private storage can page- or stack-fault. IF changes can allow asynchronous
interruption before restoration.

The adopted contract is terminal external-monitor ownership: a fault or trap
produces no normal result and there is no automatic reset, unwind, restoration,
or handler resume. The monitor must consume the exact saved hardware context
outside ordinary generated ABI continuations. No generic C signal handler or
interrupt transparency is supported; returning from a handler into a region PC,
normal C cleanup, longjmp, or re-entry is outside the contract. The structural
report keeps TerminalExceptionBoundary and AsynchronousEnvironment as unresolved
obligations: the wrapper compiles under the declared contract, but this is not
native acceptance. There is no exceptional-success token or generated handler
path.

## Remaining work

The compiler path (parsed wrapper, verifier, optimizer and foreign-target
restrictions, frame preallocation, input capture, exact full-width expansion,
and post-expansion resource checking) is implemented. Native validation still
needs a controlled guest (IDT, frame, fixed continuation) exercising CPL0,
IOPL/IF, DF/TF/AC, next-instruction RSP=-8, restore traps, and exact terminal
context. The raw subset and entry contract do not imply general context or
interrupt transparency.

Tests:

```sh
cargo test -p keel-ir --test rflags_region
cargo test -p keel-codegen --test rflags_region_prerequisite
cargo test -p keel-cranelift --test rflags_region
```

## References

Intel SDM revision 093 (September 2026): Vol. 2B 253667-093US, PUSHFQ/POPFQ and
Table 4-20; Vol. 1 sections 3.4.3.2-3.4.3.3; Vol. 3B 253669-093US section
20.3.1.4 (Single-Step Exception).
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
