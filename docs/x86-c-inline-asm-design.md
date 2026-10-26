# x86-64 C inline assembly design

Status: scalar fallthrough milestone implemented, 2026-10-26. See the
[current contract](x86-c-inline-asm.md). The remaining milestones below are
implementation plans. The [AArch64 subset](c-inline-asm.md) remains supported.

## Current blockers

The [configured Linux audit](../suite/linux-kernel/audit-followup.json) accepts
326 of 506 assembly inputs. Of 484 C inputs, 476 first fail on x86 inline asm,
five on file-scope asm and three are accepted. These are first errors, not a
count of every unsupported construct in those files.

The remaining assembly first errors include pushf aliases (100 inputs),
standalone prefixes (42), whitespace .type syntax (15), RDTSCP (five), and
smaller instruction-binding, register-alias, expression, .incbin and feature
selection gaps. Accepted assembly also has unresolved object differences:
none of the 326 objects strictly matches the independent GNU whole-object
comparison. Shared-section comparisons identify narrower successes, not kernel
link qualification.

Inline asm will expose further C errors. Kernel compiler options, ABI policy
(including no red zone and general-register-only code), sections, relocations,
linking and execution still need separate qualification. Supporting privileged
instruction bytes does not establish these contracts.

## Starting architecture and planned changes

Reuse the C operand AST and its evaluate-once lvalue capture, the InlineAsm IR
entry point, generated x86 register/condition mappings and existing GNU assembly
parser/encoder. Do not introduce another instruction encoder or shell out to
an assembler for production compilation.

At the start of this work the IR supported register outputs, inputs, ties and clobbers, but its
physical-register representation and parser were AArch64-specific. Memory was
input-address-only. Early-clobber information was discarded because that backend
assigns distinct registers. The x86 backend rejected InlineAsm. Removing the
frontend target check alone would be incorrect.

Extend the contract to retain operand access (read, write, read/write), value
type, memory extent, constraints, ties and early-clobber timing. Keep SSA results
for register and flag outputs; represent memory writes through address operands
and explicit effects, without a dummy loaded result. Replace the AArch64-only
physical-register assumption with target-tagged register identities. Existing
AArch64 syntax must continue to round-trip and pass its execution tests.

Parse constraints into alternatives of allowed locations, not a single enum
letter. Preserve column alignment across operands: choose one legal alternative
for the whole statement. A union such as rm permits either location within one
alternative; comma-separated alternatives are a separate concept. Preserve
immediate predicates and relocatable symbol/addend operands independently of
register values. Source diagnostics name the operand and failed requirement.

## First milestone: scalar fallthrough asm

Implement x86-64 GNU extended asm with integer/pointer operands through 64 bits:

- Register inputs and outputs: r, =r, +r, early-clobber &, numeric/named ties,
  and fixed a/b/c/d/S/D constraints.
- Numeric i/n immediates, cc and memory clobbers, and GPR clobber aliases.
- AT&T templates with numeric/named operands, literal %%, unique %= labels,
  register views b/w/k/q and immediate c formatting.
- Empty compiler barriers and explicit volatile. Initially retain every asm
  statement and conservatively invalidate memory knowledge, as AArch64 does.
  This is an optimization limit, not a claim that volatile is a hardware fence.

Keep memory operands, unions/alternatives, flag outputs and asm goto diagnosed
until their respective milestones. Do not accept them and approximate their
meaning. Reject RSP clobbers and reserve a live frame pointer. Diagnose register
exhaustion. Fixed input/output overlap is legal only through a declared tie;
clobbers cannot overlap operand registers. The stack pointer and direction flag
must satisfy the function ABI on normal fallthrough. Calls and terminal
transfers inside a template need separate contracts.

Start with a conservative asm-boundary plan: snapshot input sources before
shuffling, assign operand registers, preserve unrelated live values in frame
slots, initialize tied outputs, emit the complete template, capture results,
then restore preserved values. Use parallel-copy machinery or staged slots so
fixed registers and rematerialization cannot overwrite another pending input.
Save/restore explicitly clobbered callee-saved registers through the function
frame. Preserve output values before restoring any overlapping old homes.
Do not treat a generic caller-saved call barrier as sufficient for arbitrary
callee-saved clobbers.

This avoids an immediate general allocator rewrite. The current allocator
models sequence uses before definitions; arbitrary early-clobber overlap needs
more timing detail. Initially keep all untied inputs and outputs distinct,
including fixed registers, and later permit proven-safe reuse. Snapshotting all
live-through values costs code but gives a reviewable correctness baseline.
Allocation validation must check the plan even for custom allocators and empty
register pools.

Substitute operands only after allocation. Parse the resulting AT&T template
into the same object program as compiler output, preserving directives,
relocations, sections and source locations. Keep macro definitions and section
state in translation-unit order; do not assemble isolated blobs and concatenate
bytes. Text, typed and chunked object emission must describe the same program.

## Memory and kernel constraints

Next add m/=m/+m, unions rm/rmi, immediate ranges used by captured sources,
fixed local register variables and the required address/modifier forms. Generate
register masks, condition aliases, immediate predicates and positive/negative
constraint witnesses from a small reviewed constraint schema. The ISA catalogue
does not define GCC constraints, C effects or allocation timing; keep those
semantics explicit.

A memory operand evaluates its address once and keeps its extent and access
mode. Address registers are inputs for interference, including when an
otherwise write-only output uses them. Early-clobber outputs must not destroy
any untied input address. Materialize addressed C objects, invalidate cached
loads after writes and model unknown memory clobbers. Do not implement memory
outputs as a private temporary followed by copyback: aliases within the
statement must observe the original object. Segment overrides, symbols and
address spaces must survive substitution.

Add relocatable symbolic immediates, x86 modifier variants and =@cc condition
outputs as measured by the pinned corpus. Capture flag outputs immediately
after the template, before shuffles or restores that can alter flags. GCC's
multi-register A constraint must not be treated as an unconditional RDX:RAX
pair in 64-bit mode. SIMD/x87 operands remain separate from the kernel scalar
milestone; no implicit use of vector scratch registers in kernel mode.

## File-scope asm and asm goto

File-scope basic asm is a translation-unit item, not an executable IR node.
Preserve source order with emitted declarations/data/functions and route it
through the shared assembler context. Basic asm preserves literal register
percent signs; do not apply extended-asm operand substitution. This covers
metadata and symbol directives but does not promise arbitrary compiler-owned
function prologues can be replaced by basic asm.

Asm goto requires a new terminator with explicit fallthrough and listed label
successors. Resolve labels using the C function's existing scope rules; retain
all possible edges through optimization, liveness and block layout. Substitute
%l references using compiler-owned labels. Do not hide branches in an ordinary
fallthrough InlineAsm instruction.

First support output-free asm goto. Add output-bearing asm goto only with
edge-defined SSA values and per-edge writeback blocks: outputs promised by the
GNU contract must be available on each successor, read/write inputs initialize
the tied result, and cleanup/stack scope transitions follow the chosen edge.
Test loop backedges, unreachable fallthrough and joins. Block addresses in
alternative/exception tables and linker-visible labels need object witnesses
as well as control-flow tests.

## Validation and delivery gates

1. Add constraint parsing/verifier and template diagnostics before enabling the
   x86 frontend path. Cover impossible ties, clobber aliases, high-byte/REX
   conflicts and unsupported modifiers. Retain AArch64 regression tests.
2. Compile scalar functions through every supported selector/allocator and
   ordinary/empty pools, optimized and unoptimized. Check text/typed/chunked
   object parity. Execute userspace witnesses against independently compiled
   GCC/Clang references, with live values, rematerialization and canaries.
3. Add direct-memory alias tests, side-effecting addresses, register-pressure
   cases, symbolic relocations, flag outputs and constraint alternatives.
   Compare instruction/relocation witnesses without claiming whole-object
   identity where existing assembler differences remain.
4. Add translation-unit ordering and asm-goto execution/CFG witnesses. Use
   bounded CPL0 guests only for witnesses that require privilege. Keep invalid
   constraints as negative controls; crashes/timeouts never count as success.
5. Rerun the unchanged pinned C/assembly corpus after each milestone. Report
   first-error categories and acceptance separately from link/run evidence.
   Extract actual constraint/template coverage before promising kernel closure.

The first implementation should stop after scalar fallthrough asm and its
independent execution tests. File-scope asm can follow without waiting for the
full constraint solver. Memory constraints and asm goto are required follow-ups
before attempting the configured kernel C build.

## References

GNU [extended asm](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html) defines
operand, clobber, template and goto contracts. GNU
[machine constraints](https://gcc.gnu.org/onlinedocs/gcc/Machine-Constraints.html)
identifies target constraints and points to their compiler definitions. Pin the
relevant x86 definitions and compatibility witnesses when implementing the
constraint schema; generated ISA operand kinds are not a substitute.

## Scalar milestone kernel follow-up

The unchanged configured corpus now accepts four of 484 C inputs and 326 of
506 assembly inputs. All 326 previously accepted assembly objects are unchanged.
The [new report](../suite/linux-kernel/audit-inline-asm.json) records first
errors without claiming a kernel code-generation contract or link/run evidence.

450 C inputs now first stop at `rip_rel_ptr(void *p)` in `asm/asm.h`, whose
`"i"(p)` operand becomes a symbolic constant only after its always-inline call
is expanded. The scalar milestone requires immediate integer constant
expressions during parsing. The existing inliner also excludes asm-bearing
callees. Accepting symbolic addresses alone will not resolve this helper.

The next implementation needs to retain such expressions as typed IR operands,
carry asm contracts through required inlining, remap operands and unique labels,
and validate immediate legality after inlining/constant propagation. Required
inlining must also run when optional optimization is disabled. Diagnose
any unresolved constraint before code emission. Distinguish mandatory inlining
failure from a normal profitability decision. Cover unused inline definitions,
call-site constants, symbolic addresses, nonconstant calls and duplicated asm
instances with independent GCC/Clang witnesses. Memory constraints, unions,
file-scope asm and asm goto remain separate milestones; later errors are still
masked by these shared headers.
