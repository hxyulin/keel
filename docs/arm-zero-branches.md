# AArch64 ordinary zero branches

For an ordinary i32/i64 `brif` truth test, the AArch64 backend emits a single
CBZ instead of `cmp reg, #0` followed by a B.cond. This saves one instruction
(four bytes before layout effects). No runtime speedup is claimed. No new pass,
selector rule, IR operation, ISA feature, assembler API or tuning option is
involved.

## Eligibility and edges

`FunctionCompiler` tries compare fusion first: a fused integer or floating
comparison still uses `compare` and `Symbolic::Conditional`. Otherwise, an
i32/i64 condition in a function without inline assembly uses `use_reg` and
`Symbolic::Compare` through the existing `compare_branch` helper. The branch is
CBZ to the **same false destination** (or false-edge argument landing pad). The
true edge, allocator edge copies and `branch_unless_next` are unchanged. Far
branch relaxation may still invert CBZ to CBNZ over an unconditional branch.
EQ/NE expressions and fallthrough selection are not specially optimized.

Width semantics:

- **i32:** CBZ on the W register tests exactly the same 32 bits as the old
  `cmp wN, #0`, ignoring undefined bits 63:32.
- **i64:** both test all 64 bits.
- Zero is false; every nonzero pattern (including 2, the sign bit and all ones)
  is true. No bit-zero or normalized-boolean assumption is made.

`use_reg` loads or rematerializes the operand at the same point as before, ahead
of any edge copies that could overwrite it. Allocation inputs, scratch
registers, frame layout and calling convention are unchanged.

Not affected:

- **i8/i16** keep masked TST because their upper register bits are undefined.
  Scalar comparisons produce i8 and therefore stay on this path, even when a
  CSET producer would make the upper bits zero. There is no i1/bool IR type.
- **i128** keeps ORR of both halves followed by CMP; testing one half would be
  wrong.
- Floating and pointer `brif` operands are rejected by the verifier. Floating
  comparison results go through comparison fusion or i8 materialization,
  preserving ordered/unordered semantics and negative zero.

## NZCV proof boundary

CMP writes NZCV and CBZ does not, so dropping the CMP is only safe if no later
instruction (including across joins and loops) reads the flags it used to set.
There is no IR flag SSA value or flag block argument, and in compiler-generated
AArch64 code flag use is local to a single lowering sequence:

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

`aarch64/vector.rs` has no NZCV consumers. Allocator copies, address
construction, reloads, rematerialization, prologue and epilogue do not consume
flags. Every generated consumer is preceded by its own defining instruction on
every path through its sequence, and no sequence reads predecessor flags. That
property holds under concatenation, block edges, joins and backedges, so
differing incoming flags are never read. Calls and returns carry no NZCV value
under the ABI.

This is an argument over the current lowering inventory, not a machine flag
liveness analysis. Any future flag-consuming lowering must keep the same
boundary or extend the fallback below.

**Inline assembly fallback.** `AsmClobber::Flags` describes writes, but
constraints do not describe reads: a template can read NZCV in another block
without `~{cc}`. If **any laid-out instruction** in a function is InlineAsm
(even in an unreachable block), every ordinary Brif in that function keeps the
CMP + B.cond form. Template contents and clobbers are not inspected.

## Layout, faults and debug information

B.cond and CBZ/CBNZ both use a signed imm19 word-scaled displacement, and text
and structured relaxation handle both, including inversion over B. Output is
not byte-for-byte identical to the old form: removing instructions can change
PC-relative offsets, alignment, branch relaxation, unwind ranges, line-table
addresses and executable metadata.

The removed CMP is nonfaulting; no load, store, call, trap or volatile effect
moves. IR source locations and the location-emission boundary around
`allocated_inst` are unchanged, and CBZ is emitted at the Brif's location. CFI
directives, stack layout and unwind rules are unchanged. Debugger-visible NZCV
contents and breakpoint PC addresses may differ.

## Testing

```sh
cargo test -p keel-codegen --test arm_zero_branches
```

The test and its header-free C oracle (`tests/fixtures/arm_zero_branches.c`)
cover all three selection tiers and both allocators with normal and empty
register pools: 32/64-bit truth, reversed targets, edge arguments, spills, raw
aliases, call results, locations/CFI, i8/i16/i128 fallbacks, comparison and
float paths, and an inline-assembly flags witness in a successor block. Each
compilation checks that structured and text objects match. On Apple ARM64, a
native test compares all 16 width/layout/edge combinations against Clang C
conditionals for zero, high-bit, nonboolean and negative patterns, W/X width
counterexamples and all 256 byte values.
