# AArch64 code generation

This page states the AArch64 backend's registers, frame, selection rules and
lowering limits, for both `aarch64-apple-darwin` and
`aarch64-unknown-linux-gnu`. The two targets share selection and allocation;
the [AArch64 entry point](README.md) gives the Linux ABI differences. The
backend is `crates/keel-codegen/src/aarch64.rs` and the modules under
`crates/keel-codegen/src/aarch64/`. Shared selection and allocation rules are
in [IR and backend design](../../compiler/design.md#instruction-selection).

## Registers

| Registers | Use |
|---|---|
| x0–x7 | call arguments and results; never allocated |
| x8, x16, x17 | scratch for reloading spilled operands, intermediate values and parallel copies; never allocated. x8 also carries an `sret` address |
| x9–x15 | allocatable, caller-saved |
| x18 | reserved by Apple; never touched |
| x19–x28 | allocatable, callee-saved |
| x29, x30 | frame pointer, return address |
| v0–v7 | float arguments and results; never allocated |
| v8–v15 | allocatable, callee-saved (the ABI preserves only the low 64 bits, which is all a scalar uses) |
| v16–v18 | scratch |
| v19–v31 | allocatable, caller-saved |

Keeping x0–x7 out of the pool means a call's arguments can be moved into
place without moving live values out of the way first. It also gives the
`i128` lowerings free working registers: they copy their operands into x0–x7
as pairs, compute there, and move the result out. The cost is that no value
is ever placed in x0–x7, which the [allocator decisions](#decisions) change.

Each instruction is lowered with its operands in their allocated registers,
or reloaded into x16, x17 or x8 if spilled. It computes straight into its
result's register, or into x16 and a store if the result is spilled. A result
may share a register with an operand that dies at the same instruction, so a
lowering must read every operand before it writes the destination. Values
narrower than 64 bits have unspecified upper bits; comparisons, division,
right shifts and extensions extend their operands first.

## Calls and copies

An interval live across a call takes a callee-saved register or is spilled.
The prologue saves the callee-saved registers the function uses with
`stp`/`ldp` pairs, and the epilogue restores them.

Parallel copies break a cycle by saving one destination in x16; `fmov` moves
a float's bits into x16 and back, so one temporary serves both classes. A
slot-to-slot move goes through x17. Moves are always 64-bit, since narrow
values have unspecified upper bits. A return with several results moves them
as one parallel copy, because a function that makes no calls may allocate
x0–x7. Guaranteed tail calls move their
arguments into registers, run the epilogue and jump with `b` or `br`.
Arguments that would go on the stack, struct arguments and callers that
return a struct are rejected. A function whose only instruction is a direct
`return_call` forwarding its entry parameters in order, with arguments already
placed for the callee, is a single `b` with no frame, under the same rule as
[x86-64](../x86_64/codegen.md#frames). Debug files keep the normal path.

Private Darwin scalar leaves may also use register-only direct Fast tail calls
without a frame. Unused stack declarations and removed vector values do not
exclude the argument register bank; surviving stack addresses, calls, unsupported
instructions, spills, callee saves, debug data and stack arguments still do.
Full allocation places proven terminal trap regions and cold/no-return tail
calls behind ordinary successors before assigning instruction positions.

## Frame

From `sp` up:

```text
sp + 0              outgoing stack arguments (for the call that needs most)
...                 copies of big structs passed by value
...                 a slot keeping a call's sret address across it
...                 incoming structs that arrived in registers, and a struct
                    result returned in registers
...                 spill slots, 8 bytes each (only as many as needed)
...                 IR stack slots, each at its alignment
...                 padding to 16 bytes
...                 callee-saved FP registers used (d8–d15), in pairs
x29 - 16 * pairs    callee-saved general registers used, in pairs
x29                 saved x29, x30
```

Outgoing arguments must be at `sp` when a call is made. Spill slots come right
after them so `ldr x, [sp, #imm]` reaches them (at most 4095 × 8 bytes). The
epilogue resets `sp` from x29 before popping the saved registers, so it does
not depend on the size of the rest of the frame. A function that uses
`stack_alloc` addresses its fixed frame from x19, which dynamic allocation
does not move. A function reads its own stack parameters at `x29 + 16`.

When a used IR stack slot is aligned above 16, the prologue rounds `sp` down
to that alignment after lowering it by the frame size (`mov x16, sp` then
`and sp, x16, #-align`), so every slot offset gives an aligned address. The
frame then has no fixed offset from x29, so debug information addresses its
slots from `sp`, or from x19 with `stack_alloc`.

Both native backends share one IR slot layout. Slots without lifetime markers
come first, in declaration order. Slots with markers follow, largest first,
each at the lowest aligned offset that no slot live at the same time already
uses. Liveness comes from the markers along every CFG edge, including an
invoke's unwind edge. Each slot keeps its own bytes when the module has debug
information, when the function has a returns-twice call, when the slot is an
`AggregateVaArg` home or has a debug variable, or when more than
16 × (marked slots) + 4,096 pairs of slots are live together.
`EmissionOptions::separate_slots` turns sharing off. Lowering never moves an
access across a marker: the loads that fuse into a consumer must sit next to it.

Argument areas may be larger than 4,080 bytes for incoming parameters and for
direct or indirect calls. Loads and stores use scaled immediate offsets when
they fit and form a scratch address otherwise; aggregate copies follow the
same rule for their final narrow pieces. Outgoing argument areas still count
toward the allocator's 32,760-byte spill-area bound and the function's
frame-size bound.

## Selection

Tree selection (`crates/keel-codegen/patterns.txt`) covers 32- and 64-bit
multiply-add and multiply-subtract and shifted arithmetic and logic.
Floating multiply-add is never fused implicitly.

Scalar loads and stores can consume a single-use, same-block `ptradd` with an
i64 register offset. A single-use i64 shift is absorbed when its masked count
is zero or log2 of the access size, giving a register-offset address in the
memory instruction. Accesses keep their position and width, including
volatile accesses; atomics and 128-bit and vector accesses keep their own
lowering. Both address leaves are published before allocation, with separate
scratch registers for spilled bases, indices and store values. Shared shifts
stay materialized; shared or cross-block addresses and distinct debug source
lines prevent the fold. Narrow index extension is not folded. Stack offsets
depend on the spill frame, so stack address folding is finalized after
allocation.

### Covers

Every cover is off at Baseline selection and on at Covers (the `-O0`
default), Trees and Full; selection-stage covers also need tree selection. See
[covers](../../compiler/design.md#covers) for the registry and
`--isel` toggles. `--isel all,-<name>` drops one, and `--codegen-report`
counts the roots each one claimed.

| Cover | Stage | Pattern |
|---|---|---|
| `offset-address`, `indexed-address` | selection | Constant or register offsets in loads and stores |
| `zero-test` | selection | Zero test of an extended compare reuses the compare |
| `aarch64-ubfx` | selection | Shifted low-bit mask as UBFX |
| `aarch64-multiply-shift` | selection | Multiply by a tuned constant as shift and add (see [M5 tuning](#apple-m5-tuning)) |
| `compare-branch` | backend | Single-use compare re-emitted at its branch; a compare-and-swap flag used only by the next `brif` becomes a branch on the flags the `cas` compare or the LL/SC loop leaves |
| `aarch64-select-compare` | backend | Single-use integer compare delayed to its select |
| `aarch64-abs`, `packed-abs` | backend | Scalar CMP and CSNEG; packed absolute value |
| `aarch64-abs-diff` | backend | Widened absolute difference accumulated as SABAL |
| `aarch64-packed-minmax` | backend | Packed compare and select as min or max |
| `aarch64-extended-add` | backend | Pointer plus extended, shifted i32 index as one ADD |
| `aarch64-packed-low-load` | backend | Load used only by a low-half widening, as one 8-byte load |
| `aarch64-replicate-load` | backend | Load splatted to every lane, as LD1R |
| `aarch64-vector-immediate` | backend | Zero splats and shifts by a splatted constant as immediates |
| `aarch64-widening-add` | backend | Low-half widening added to an accumulator, as SADDW or UADDW |

### Apple M5 tuning

`--tune generic` is the default. `--tune apple-m5` selects measured choices
for Apple arm64 without changing the target, ABI, ISA features or IR tier.
With tree selection, i32 and i64 multiplication by 3, 5 or 9 uses
`add r, r, r, lsl #k`; existing multiply-add covers and shared constants are
kept. `tools/generate_m5_tuning.py` emits the Rust cost table from a
calibration report: a rule applies only when the dependency-chain ratio's
upper 95% bootstrap bound is below 1.0 and the parallel-stream median ratio is
at most 1.0. The x86 target rejects M5 tuning.

```sh
keel cc -O --isel-tier full --tune apple-m5 input.i -o output.s
```

`tools/instruction_metrics.py` reports encoding sizes and optional `llvm-mca`
estimates for comparing lowerings; see
[instruction metrics](../../development/testing.md#instruction-metrics). Those
estimates do not drive selection.

## Branches

A `switch` with at least four dense cases, spanning less than three times
their count, becomes a jump table; the rest is a chain of compares.

Conditional branches reach ±1 MiB. A function whose code may be bigger
(interpreter loops get there: QuickJS's is about 4 MB from keel) has each one
rewritten as the opposite branch over a `b`, which reaches ±128 MiB. Only
those functions pay the extra instruction. The size is bounded by counting
instructions, so no branch is measured by default.

`KEEL_AARCH64_PRECISE_BRANCHES=1` selects experimental per-branch relaxation
on both structured and text routes. It computes label distances and repeatedly
expands only branches outside the signed imm19 range until no further branch
needs expansion. Data widths count in bytes. Alignment uses maximum padding;
unknown inline assembly boundaries or directives force crossing branches long.
Inline assembly stays opaque: its branches and compound statements are never
rewritten by this pass.
This can conservatively expand a reachable branch. Unconditional branches still
require the assembler/linker to diagnose distances outside their range.

For an ordinary i32 or i64 `brif` truth test that compare fusion does not
cover, the backend emits CBZ to the false destination instead of
`cmp reg, #0` and B.cond. On i32, CBZ on the W register tests the same 32 bits;
on i64 it tests all 64. Far branch relaxation may invert CBZ to CBNZ over B.
i8 and i16 keep a masked TST because their upper bits are undefined, and i128
keeps ORR of both halves and CMP. A function containing inline assembly keeps
CMP and B.cond for every `brif`, because a template can read NZCV without
declaring it. The proof that no other generated sequence reads the removed
flags is documented on `BrifTest` in `crates/keel-codegen/src/aarch64.rs`.

## Integer, float and atomic lowering

- A single-use i32/i64 add/sub overflow flag feeding a branch, directly or
  through `xor flag, 1`, stays in NZCV and branches with `hs`, `lo` or `vs`.
  Intervening work, extra flag users, debug boundaries, inline assembly and
  custom operand/constraint claims retain materialization. Selected arithmetic
  writes its allocated destination directly. Narrow overflow and multiplication
  retain their separate materialized flag paths.
- Other overflow flags come from the condition flags (`adds` and `cset hs/vs`),
  from the high half of the product for 64-bit multiplication, and from a
  range check for i8 and i16, computed in 32 bits where it cannot overflow.
- `fcopysign` has no scalar instruction, so it splices the sign bit in
  general registers with `bfxil`.
- On Apple arm64, read-modify-writes and `cas` use the LSE instructions
  (`ldadd`, `ldclr`, `swp`, `cas`…, suffixed `a`/`l`/`al` for the ordering),
  which every Apple arm64 CPU has. `sub` adds the negation and `and` clears
  the complement, since LSE has neither. A `cas` takes the suffix of the
  merged success and failure orderings, and never fails spuriously, so a
  weak one is the same. Its flag compares the old value with the expected
  one, extended for i8 and i16. An i128 load or store is `ldp` or `stp`,
  which FEAT_LSE2 makes single-copy atomic when aligned: acquire adds
  `dmb ishld` after the load, release `dmb ish` before the store, and
  seq_cst also `dmb ish` after the store. A seq_cst i128 load is
  `ldar; ldp; dmb ishld`: Clang's `ldp; dmb ish` lets the load pass an
  earlier 64-bit seq_cst `stlr`, which a store-buffering litmus test under
  herd7's AArch64 model shows. An i128 compare-and-swap is `casp`, and a
  read-modify-write a `casp` loop. Ordered loads and stores are
  `ldar`/`stlr`; fences are `dmb ishld` (acquire) or `dmb ish`. AArch64 Linux
  targets baseline ARMv8.0 and uses load-exclusive/store-exclusive loops
  instead, ordered as the [target README](README.md) describes.
- Inline assembly operands go in reserved registers that no value is
  allocated to. Inputs move in before the template and outputs out after it.
  Clobbering an allocatable caller-saved register makes the asm a call for
  the allocator; clobbered callee-saved registers are saved by the prologue.

## Adding an operation

After the IR steps in
[adding an operation](../../compiler/design.md#adding-an-operation):

1. Add the lowering to the family module: `aarch64/lower_scalar.rs`,
   `lower_memory.rs`, `lower_control.rs` or `lower_vector.rs`, or
   `aarch64/intrinsic.rs` for a target-independent intrinsic. The family
   matches end in a wildcard that panics, so the compiler does not point to
   a missing arm.
2. Reject shapes the lowering does not handle in the preflight checks of
   `aarch64/plan.rs`, with a `CodegenError`, rather than panicking later.
3. Add a fused form only as a [cover](#covers) or a `patterns.txt` rule, so
   Baseline keeps the plain lowering as its reference.
4. Add a program under `crates/keel-codegen/tests/programs/` and run
   `cargo test -p keel-codegen --test run` (eight selection and allocator
   configurations) and `--test asm_snapshots` (`BLESS=1` rewrites the
   snapshots). Add a focused `arm_*` target test when the lowering has
   boundaries such as immediate ranges.

## Decisions

The register-assignment design settled the AArch64 policy: x0–x7 become
allocatable and leaf functions frameless on Darwin first, and on Linux after
the ARM container interoperability test runs. Values with no preference take
x0–x7 after x9–x15, once both orders are measured. Before that, the indirect
call lowering must read its target as part of the argument copy.

## Tests

```sh
python3 tools/test.py codegen --quick
cargo test -p keel-codegen --test arm_zero_branches
cargo test -p keel-codegen --test arm_indexed_memory
cargo test -p keel-codegen --test arm_constant_division
cargo test -p keel-codegen --test vector_aarch64
```
