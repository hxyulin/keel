# AArch64 ordinary zero branches: source prototype

This prototype removes one instruction (four bytes before layout effects)
from ordinary i32/i64 `brif` truth tests. It makes no runtime-profit claim.
It is a source checkpoint, pending independent review and an explicit native
correctness/build grant. No new pass, selector rule, IR operation, ISA feature,
assembler API or tuning option is introduced.

## Eligibility and edges

`FunctionCompiler` keeps compare fusion first. A fused integer or floating
comparison still uses `compare` and `Symbolic::Conditional`. Otherwise, an
i32/i64 condition in a function without inline assembly uses `use_reg` and
`Symbolic::Compare` through the existing `compare_branch` helper. The branch
is CBZ to the **same false destination** (or false-edge argument landing pad).
The true edge, allocator edge copies and `branch_unless_next` are unchanged.
The existing far-branch relaxation may invert CBZ to CBNZ over an unconditional
branch. This prototype does not separately optimize EQ/NE expressions or
change fallthrough selection.

For i32, the old `cmp wN, #0` and CBZ W test exactly the same 32 bits, ignoring
undefined bits 63:32. For i64 they test all 64 bits. Zero is false; every
nonzero pattern, including 2, the sign bit and all ones, is true. No bit-zero
or normalized-boolean assumption is used. `use_reg` loads/rematerializes the
same operand at the same instruction as before, before any edge copies can
overwrite it. Allocation inputs, scratch registers, frame and calling
convention are unchanged.

`i8` and `i16` retain masked TST because their upper register bits are not
defined. Scalar comparisons produce i8, and stay on the old path even when a
CSET producer could prove upper bits zero. There is no i1/bool IR type.
`i128` retains ORR of both halves followed by CMP; checking one half would be
wrong. The verifier rejects floating and pointer `brif` operands. Floating
comparison results are handled through existing comparison fusion or i8
materialization, including ordered/unordered semantics and negative zero.

## NZCV proof boundary

CBZ does not write NZCV, while the removed CMP does. Therefore the proof must
cover successor instructions, including joins and loops, rather than merely
the two instructions at the branch. There is no IR flag SSA value or flag
block argument. In compiler-generated AArch64 code, flag use is local to a
lowering sequence:

| Consumers | Defining sequence in `aarch64.rs` |
| --- | --- |
| Fused Brif / materialized Icmp and Fcmp | `compare` emits CMP/CMN/FCMP before B.cond/CSET |
| Scalar, float, vector and wide Select | `test_value`/`test_nonzero` or explicit CMP before conditional branch/CSEL/FCSEL |
| i128 Icmp | low CMP before CCMP/SBCS, then CSET |
| i128 add/sub/negate and overflow | ADDS/SUBS/NEGS before ADC/SBC/NGC or ADCS/SBCS and CSET |
| i128 shifts/counts | TST/CMP before CSEL, all in the same helper invocation |
| Narrow/full-width overflow | ADDS/SUBS/CMP/TST before CSET |
| Signed division guards | CMN then CCMP/B.cond, or CMN/CMP before B.cond |
| Switch | CMP before conditional table/range/case decisions |

`aarch64/vector.rs` has no NZCV consumers; `selected` emits arithmetic/logic
patterns without flag inputs. Allocator copies, address construction,
reloads, rematerialization, prologue and epilogue do not introduce flag
consumers. Every generated consumer is preceded by its own defining
instruction on every executable path through that sequence. No sequence
imports predecessor flags. This property is closed under concatenation,
block edges, joins and backedges: differing incoming flags cannot be read
before being overwritten. Calls and returns carry no NZCV value under the
existing ABI. This is a source proof of the current lowering inventory, not
a newly implemented machine liveness analysis; future flag-consuming
lowerings must maintain the boundary or extend the fallback.

Inline assembly is the exception. `AsmClobber::Flags` describes writes;
constraints do not describe reads. An arbitrary template can read NZCV in
another block without `~{cc}`. The existing validation loop now records
whether **any laid-out instruction** is InlineAsm (even in unreachable
blocks). Every ordinary Brif in that function retains the old test and
B.cond. No template parsing or assumption about a clobber proves safety.
The cross-block CSET witness is a regression fixture for this fallback.

## Layout, faults and debug information

Both B.cond and CBZ/CBNZ have signed imm19 word-scaled displacement. Existing
text and structured relaxation already support both families, including
inversion over B; labels remain symbolic through final assembly. This does
not promise byte-for-byte identity or fixed addresses: removing instructions
can change PC-relative words, alignment, branch relaxation, unwind ranges,
line-table addresses and final executable metadata. Full combined output
must be attributed after the native grant.

The change removes a nonfaulting integer CMP and does not move a load,
store, call, trap or volatile effect. It neither changes IR source locations
nor the location-emission boundary around `allocated_inst`. CBZ is emitted
at the Brif's existing location. CFI directives, stack layout and unwind
rules are unchanged; address/range changes must be checked in generated
artifacts. There is no claim that arbitrary debugger-visible NZCV contents
or breakpoint PC addresses are identical.

## Validation status and remaining gates

`arm_zero_branches.rs` and its header-free C oracle are **authored, unrun**.
They cover all three selection tiers and both allocation tiers with normal
and empty register pools; 32/64-bit truth, reversed targets, edge arguments,
spills, raw aliases, call results, locations/CFI, narrow/i128 fallbacks,
comparison/float paths and a successor inline-assembly flags witness.
Every compilation checks structured/text object equality and absence of
opaque text for compiler-generated code. The Apple ARM native test compares
all 16 width/layout/edge combinations with independent Clang C conditional
expressions on zero, high-bit, nonboolean, negative bit patterns, W/X width
counterexamples and all 256 byte values. Both true and false outcomes are
required. Source-only model checks are not compiler or native test results.

After independent source review and a new lease, required gates are: build
and dedicated tests; unchanged comparison-span/snapshot, inline-assembly,
location/CFI and both-backend regression suites; structured/text/native
instruction and far-range checks; selected retained QuickJS call-result
site activation before emission and its actual CBZ encoding after emission;
full QuickJS public interrupt normal/cancel/recovery correctness and original
eight workload output/exit oracles; all changed TU/function/frame/code/data
and relocation/unwind deltas in the combined applications; and a separately
approved CLI compile-cost comparison. Existing MachO/TLS failures remain
explicit failures/limitations, never reasons to weaken fixtures.

The previous one-site manual experiment does not establish a general speed
benefit. Its compact/padded contrast leaves layout confounding unresolved.
There is no integration, default change or new timing authorization here.
