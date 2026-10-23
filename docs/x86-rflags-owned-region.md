# Owned FLAGS region: structural prerequisite, not production support

Status: fullflags=false, full-profile-complete=false, native-acceptance=false.
Standalone ID 730 remains rejected. Registry count stays 731; signatures and all
IDs are preserved. Parsed `InstData::X86RflagsRegion` now binds the raw program to
I64 captures/results, runs its structural verifier, and expands through an exact
post-allocation/frame resource checker. The explicit `terminal` syntax adopts
the caller/environment contract below; it is not runtime discovery or native proof.

Example (observations named in program order):

```text
func @owned(i64)->i64 {
block0(v0:i64):
 v1,v2 = x86_rflags_region terminal "save; swap 0 0; read 1; reapply; normal"(v0)
 return v2
}
```

Blocks in the quoted raw program are separated by `|`; operations use semicolons.
`jump N` is a direct raw edge; `jcc C T F` uses architectural condition 0..15 and
taken/not-taken block indices. The wrapper's argument/result lists declare capture
and observation counts. No arbitrary source assembly is accepted.

## Findings from existing infrastructure

`x86_x87_region(ptr,i32)->i32` is one pinned memory-barrier intrinsic. Its backend
owns the internal instruction sequence and private scratch registers, takes
snapshots, and explicitly FNINIT/resets its x87 state before normal completion.
The ordinary IR verifier checks its signature; x86 codegen validates constant
controls with `x87_region_control`. There is no reusable CFG token verifier there.
Its terminal fault/no-resume and restricted entry assumptions are specific to
that floating contract. They do not license copying its reset or handler policy
for FLAGS. Inline asm's `~{cc}` is a clobber declaration, not an ownership token,
exact exception boundary, or restore-before-call proof.

Pure IR arithmetic is schedulable independently of pinned operations. GCM is
already disabled for the standalone raw FLAGS intrinsics. That alone does not
exclude constant folding, GVN, tree selection, compare fusion, register allocation,
implicit libc calls, spill transport, or epilogue arithmetic. An ordinary Function
CFG between two swap intrinsics is therefore not the first production candidate.

## Wrapper and CFG/token contract

The dedicated `InstData::X86RflagsRegion { captures, program }` wrapper preserves
the fixed signature of intrinsic 730 and assigns no new intrinsic ID. Its raw
Swap operation implements that existing full hardware contract inside ownership;
standalone 730 stays rejected. The wrapper is one pinned, retained, trapping
memory barrier in the outer Function CFG. GCM is disabled in containing functions,
and inlining a function containing a region is rejected. No optimizer can inspect
or schedule into its closed raw body. Its sole normal successor resumes ordinary
IR after entry-image reapplication under the stated entry/environment contract.
There is no invoke, unwind-to-cleanup, call, tail call, return or inline asm inside
ownership. AArch64 rejects the wrapper before emission.

The wrapper uses this concrete raw CFG:

- Block 0 is the sole entry, beginning with SaveEntry. Captures are bounded,
  typed I64 values staged before it. SaveEntry captures the actual normalized
  PUSHFQ image at that machine boundary in a private immutable capability.
- SaveEntry creates the sole implicit owned token. Read and Swap consume and
  replace it. A direct forward jump forwards it; a Jcc transfers it along exactly
  the taken path. Jcc reads existing hardware flags without CMP/TEST synthesis.
- Joins are implicit token phis. All incoming tokens must have the same owner
  phase. Observations use must-defined and may-defined sets: every normal exit
  needs all results; no path may overwrite one. Distinct alternatives may define
  the same observation, with the selected value published after exit.
- ReapplyEntryImage consumes active ownership and produces an image-reapplied
  token. Only direct jumps and Normal exit may follow. Another swap/read,
  duplicate reapplication, mixed-phase join, or un-restored exit rejects.
- All edges target later in-range blocks; every block is reachable. Limits are
  64 blocks, 256 operations, 16 captured I64 images and 16 observations. Thus
  boundedness does not rely on trusting a loop trip count. Ownership cannot
  escape into a Function block parameter, be stored in user memory or be copied
  as an ordinary I64. This is structural linearity, not machine-state equality.

The saved capability cannot be passed as an ordinary input or overwritten. The
checker deliberately calls the final action **ReapplyEntryImage**, not restore
RFLAGS: PUSHFQ clears RF/VM in its image, and POPFQ does not write every bit.
A structural Normal exit alone is not permission to resume the ABI: exact machine
checking and the declared entry/terminal-environment preconditions also apply.

## First allowed program and machine resources

The first implemented subset is only SaveEntry, exact Swap, exact Read, direct JMP,
direct Jcc, and ReapplyEntryImage. No ordinary integer/SIMD operation, user-memory
load/store, string operation, arbitrary opcode or native call is accepted by this
plan. That restriction controls compiler lowering; it does not mask the swap
image or claim that arbitrary architecture state becomes safe. Full-width I64
inputs remain unchanged and all hardware permissions/side effects remain in scope.
Future ordinary integer/SIMD/memory work requires a separate exact lowering proof.

The expansion assigns explicit resources to every emitted
instruction: full FLAGS use/definition (including execution-control bits), private
saved-image capability, captured-image inputs, observations, scratch GPRs, RSP,
private frame memory, CFA and exceptional PC. Its checker verifies the typed instruction list AFTER selection/allocation/frame
layout, before either text or typed output consumes that checked list.

SaveEntry and Read expand to PUSHFQ/POP r64. Swap captures the old image with that
pair, then PUSH captured-r64/POPFQ. Reapply uses PUSH private-saved-r64/POPFQ.
Each push/pop accesses exactly eight bytes and has transient RSP delta -8 then 0;
there is no 16/32-bit form or numeric flag packing. The saved image and every
captured input must survive all branches and scratch reuse. No allocatable live
SSA value is clobbered. Inputs are copied before any state change or output write,
so input/output aliases and spilled inputs cannot alter a later swap input.

Preflight reserves a dedicated aligned contiguous frame area: one immutable saved
image, one eight-byte home per capture, one per observation. It validates the plan,
I64 signature, frame bounds/alignment and stable rbp+16 CFA before emission. All
input uses/rematerialization and copies finish before SaveEntry. Core expansion
uses only reserved RAX; allocated GPR/SIMD SSA values remain intact. Private MOV
homes are frame-relative (RBP or reserved aligned frame base R15), eight-byte
aligned/disjoint, and survive all paths. No dynamic allocation, probing, realignment
or new register-allocation spill is inserted inside ownership. Observation stores
are explicit recipe instructions; normal SSA publication/spills occur after the
checked done label. Arbitrary input/output register aliases cannot overwrite a
later swap's already captured input.

The independent sequence parser checks every typed opcode, exact operand/width,
label/branch destination, private-home provenance, and complete PUSHFQ/POP and
PUSH/POPFQ pairs against the verified raw CFG. Every pair has transient RSP=-8
then zero; no CFG edge carries a nonzero delta. It rejects hidden ADD/CMP/TEST,
string instructions, calls, directives, text fragments, extra stack operations,
incorrect scratch registers, overwritten saved inputs, and trailing instructions.
The exact checked Program is consumed by both text and typed output. Operation
labels identify raw fault sites; CFA stays rbp+16 even at an intermediate pushed
stack PC. After image reapplication, publication uses normal MOV transports and
the ordinary epilogue. GCM exclusion alone is not the machine proof.

## Entry, permission and normal completion obligations

The explicit `terminal` wrapper declares these caller/environment preconditions:
64-bit mode; a coherent ordinary ABI frame/stack and normal continuation; DF=0,
TF=0, AC=0 and NT=0 on entry; controlled debug configuration (no branch-step mode);
accessible private frame and stack; unchanged CPL, privilege/debug/SMAP environment
through normal completion; and an external terminal monitor for every fault/trap.
IF and IOPL may be inherited as permitted by that controlled environment, including
CPL0; the wrapper imposes no application-only mask on any input image. Stack
access permissions are architectural and faults are terminal, not emulated away.
These are declared preconditions, like valid memory for a raw operation, not
claims of dynamically proving the entire external environment. Root owns the
independent freestanding guest boundary proof; the old UID/DMI runtime script is
not such a proof and is not used by this wrapper.

The saved image describes the current machine boundary after compiler prologue
and all captures, not function entry or interrupt entry. PUSHFQ clears RF/VM in
its image; normal long mode and ordinary ABI/debug entry are the contract, with
no promise to recover RF or arbitrary saved interrupt/signal contexts.

POPFQ must apply actual CPL/IOPL semantics: IOPL changes only at CPL0, IF is
writable only with sufficient privilege, and protected privilege fields can remain
unchanged without a privilege exception. Reserved bits, RF, VM, VIF/VIP have their
architectural treatment. DF, TF, AC/SMAP, ID, NT and the other writable bits cannot
be omitted as merely non-application state. Do not model AC solely as CPL3
alignment checking: at CPL0 it also affects SMAP accesses. Reapplying the entry
image under unchanged CPL/environment restores the writable entry execution bits:
CPL0 can restore IOPL/IF; nonzero CPL cannot change IOPL, so IF's writability remains
that of entry. DF/TF/AC/NT return to their stipulated legal entry state on a normal
path. Read-only/non-writable bits keep their hardware treatment. An outstanding
trap from an instruction that observed TF is terminal even if its RIP is after
reapplication; it is not a normal completion certificate. Thus result publication,
ordinary compiler arithmetic/calls/return use the declared normal ABI state only
on actual normal completion under those preconditions. No input mask
or silently restricted application-flag swap can substitute for that proof.

## Exceptional completion: explicit terminal external monitor contract

Every expanded instruction has its own exceptional boundary; a balanced normal
sequence does not make its intermediate states disappear. Intel 093 Volume 3B
section 20.3.1.4 says that TF set by POPF causes a single-step trap after the next
instruction. Thus PUSH old-image immediately after POPFQ can trap with RSP=-8;
the subsequent POPFQ restore may not execute. A trapping restore instruction can
also leave a pending debug event after apparent reapplication. Branch-step debug
configuration and other breakpoints require their own environment treatment.
AC can cause alignment faults; AC/SMAP changes can change supervisor access
permissions. PUSHFQ/POPFQ and private storage can page/stack fault. IF changes can
allow asynchronous interruption before restoration. These are real architectural
effects, not hidden compiler exceptions or permission checks to emulate away.

The adopted exception contract is terminal external monitor ownership:
a fault/trap supplies no normal result and promises no automatic reset, unwind,
restoration or handler resume. The monitor must consume the exact saved hardware
context outside ordinary generated ABI continuations. No generic C signal handler
or arbitrary interrupt transparency is claimed. An existing handler's return to a
region PC, normal C cleanup, longjmp, or re-entry is outside this contract.
This policy is a caller/environment precondition, not a boolean claiming dynamic
verification. The structural report retains TerminalExceptionBoundary and
AsynchronousEnvironment as unresolved proof obligations; exact wrapper expansion
can compile under the declared contract while independent native acceptance stays
false. There is no exceptional-success token or generated handler/cleanup path.
Unwrapped POPFQ swap remains blocked.

## Compiling stage and required next stages

Stage 1 introduced the bounded structural analyzer. Stage 2 connects the parsed
wrapper, verifier, optimizer/foreign-target restrictions, frame preallocation,
input capture, exact full-width expansion and post-expansion resource checking.
These compiler checks permit the explicit terminal contract; they do not establish
native acceptance. Root's controlled guest IDT/frame/fixed-continuation proof must
exercise CPL0, IOPL/IF, DF/TF/AC, next-instruction RSP=-8, restore traps and exact
terminal context. No child target execution is performed. The raw subset and
entry contract do not imply universal context or interrupt transparency.

Primary review uses cached official Intel September 2026 revision 093:
Volume 2B 253667-093US, PUSHFQ/POPFQ entries and Table 4-20; Volume 1 sections
3.4.3.2–3.4.3.3; Volume 3B 253669-093US section 20.3.1.4 (Single-Step Exception).
The official combined document is
https://cdrdv2-public.intel.com/929349/325462-093-sdm-vol-1-2abcd-3abcd-4.pdf
No target code, native/guest/QEMU process, SSH, remote or delegated task is run
for this prerequisite. Full flags and the full profile remain incomplete.
