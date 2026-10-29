# Optimizer

`keel-opt` runs function passes (`fn(&mut Function) -> bool`) and module passes
through `Pass::run_module`. This page states each pass's contract: what it
does, which tiers run it, what it preserves and where it stops. The tier
pipelines are defined in code by `OptimizationTier::pipeline`,
`BALANCED_PIPELINE`, `SIZE_PIPELINE`, `FULL_ROUND` and `FULL_EPILOGUE` in
`crates/keel-opt/src/lib.rs`; [optimization levels](optimization-levels.md)
summarizes them. Pass-specific admission rules live in each pass module's
rustdoc.

## Pass manager

The manager runs each pass over the module before moving to the next and
verifies, after every pass, each function the pass reports changed, plus any
function a module pass added. A pass that edits a body must report the change:
the deterministic-pass cache trusts the same flag. Clean-cache entries belong
to pass instances, so a repeated unchanged instance stays skipped until its
function changes.

Ordinary and reported optimization share one execution loop. Without a report
the loop creates no timers, IR counts or observation records. Completed pass
invocations stay in a partial report when their verification fails, and
skipped invocations keep pipeline order with zero timings. All exits release
analysis and scratch caches.

A thread-local cache reuses CFG, dominator and loop-depth analyses while the
function's CFG stamp is unchanged, or when block count, layout and ordered
successors still match. Debug builds check the cached CFG inputs on every hit.
The `keel-opt/audit-analysis` feature also recomputes and compares the
analyses on each hit; use `cargo test -p keel-opt --features audit-analysis`
when changing CFG mutation or analysis code.

## Canonical form

Basic, Balanced, Size and Full run `simplify-cfg` and then `canonicalize`
before scalar consumers. Operand canonicalization does no constant evaluation,
reassociation, instruction creation or removal, or motion. Its form is:

- Every operand names its resolved SSA value. Aliases can still exist in value
  tables and block parameter definitions.
- Operands of commutative integer operations (`iadd`, `imul`, `and`, `or`,
  `xor`, `umulhi`, `smulhi`) and checked add and multiply sort by
  `(is_integer_literal, value_id)`, so a single literal is on the right.
- Integer comparisons sort the same way and reverse the predicate on a swap:
  `icmp.slt 37, x` becomes `icmp.sgt x, 37`.
- Floating-point operand order, noncommutative operand order, select arms,
  edge order, memory effects and trapping behavior are unchanged.

This is a structural form, not a promise that equivalent trees are identical.
Later passes can break it by recording aliases or folding values to literals.
`Pass::requires_canonical_form()` declares an entry requirement: the manager
restores the form right before a requesting pass when an earlier pass reported
a change, unless that pass promised `produces_canonical_form()`. A pass that
reports no change must not mutate the function. This applies to explicit and
custom pipelines too. The guarantee holds on entry to a requesting pass, not
during its rewrites or on optimizer exit. Direct calls to `Pass::run` bypass
it; use `run_pipeline` or `run_passes`.

## Passes

| Pass | What it does |
|---|---|
| `inline` | Bounded private direct-call expansion; see [inlining](#inlining) |
| `inline-noninterposable`, `inline-cheap-reserve`, `inline-size` | Binding-aware inlining variants used by Balanced, Full and Size |
| `small-memory` | Expand constant memory intrinsics of 0 to 32 bytes; see [memory passes](#memory-passes) |
| `stack-forward` | Block-local byte forwarding for nonescaping fixed stack objects |
| `stack-forward-cfg` | Opt-in `stack-forward` followed by CFG cleanup when forwarding made a branch operand constant |
| `canonicalize` | Establish the [canonical form](#canonical-form) |
| `simplify-cfg` | Fold constant `brif` and `switch`, compose arguments through jump-only blocks, merge a block into its only predecessor, drop unreachable blocks |
| `const-fold` | Evaluate instructions on constants with the machine's semantics; trapping divisions stay |
| `instcombine` | Worklist combining; see [scalar simplification](#scalar-simplification) |
| `call-contracts` | Apply direct `noreturn` and `nounwind` contracts; see [function attributes](function-attributes.md#optimizer-consumers) |
| `memory-forward` (opt-in) | Block-local exact-address load reuse and store-to-load forwarding |
| `load-reuse` | Bounded cross-block load reuse |
| `call-load-reuse` (opt-in) | Load reuse across calls with explicit scoped call contracts |
| `branch-ranges` (opt-in) | Fold comparisons using intervals from dominating conditional edges |
| `known-bits` | Fold masks and comparisons from known-zero and known-one bits |
| `phi-simplify` | Replace uniform block arguments with a dominating value |
| `sccp` (opt-in) | Sparse conditional constant propagation |
| `gvn` | Scoped hash table over the dominator tree; loads are never merged |
| `dce` | Mark from pinned instructions; dead block parameters die with their arguments |
| `gcm` | Global code motion, the scheduler for floating instructions |
| `stack-promote` | Promote private fixed stack fields to SSA |
| `stack-cache` (opt-in) | Cache fields of an initialized stack object through loops |
| `loop-vectorize`, `loop-reduce`, `loop-reduce-multi` | Vectorize counted maps and wrapping reductions; see [SIMD IR](simd-ir.md) |
| `loop-strength-reduce` | Replace wrapping i32/i64 induction products with recurrences in two-block loops |
| `loop-unroll` | Two-way unrolling of wrapping integer reductions |
| `loop-rotate` | Guarded rotation that copies header tests to the latch and keeps the entry guard |
| `loop-memory` (opt-in) | Hoist an invariant load through a cloned loop guard |
| `adjacent-abs` | Convert private scalar absolute-value triangles before vectorization |
| `adjacent-diff`, `run-balance-predicate` (opt-in) | Ordered integer reduction experiments |
| `sign-extend-idiom` (opt-in) | Recognize a manual signed-byte conversion |
| `if-convert` (opt-in) | Bounded scalar if-conversion |
| `load-narrow` (opt-in) | Narrow fault-free private stack loads; see `load_narrow.rs` |
| `constant-args`, `empty-calls`, `unused-functions` | [Module cleanup](#module-cleanup) of constant arguments, empty calls and unreachable functions |
| `private-aggregate-cleanup` | Pass private two-word by-value aggregates as word pairs |
| `internal-fast-calls` | Final private transport for contained direct calls |
| `tail-calls` | Replace adjacent scalar call/return pairs; see [tail calls](#tail-calls) |
| `fast-cfg`, `fast-local`, `fast-loop-reduce` | The bounded Fast tier; see [Fast](#fast-tier) |

`-p pass,pass,...` selects any pipeline explicitly, overriding the tier.

## Scalar simplification

`instcombine` uses a consumer worklist, so a substitution exposes further folds
regardless of block layout. Integer rules follow wrapping arithmetic. Division
identities need a known nonzero divisor, and floating-point identities are
excluded. Effectful and trapping producers stay pinned even when a consumer
becomes constant. Rules include:

- constant evaluation, shared with `const-fold` and SCCP, including both
  results of checked add, subtract and multiply through i64;
- power-of-two unsigned division, remainder and multiplication as shifts and
  masks;
- Boolean absorption (`x & (x | y) = x`), XOR cancellation and complement
  pairs (`x & ~x = 0`), for all widths;
- integer cast chains: successive truncations or equal extensions, sign
  extension of a zero extension, and truncation of an extension collapse;
  zero extension of a sign extension and extension after truncation stay;
- wrapping cancellation such as `(x+y)-x` and `x+(-x)`, and grouping of two
  constants in an associative chain whose inner result has one use;
- a bounded three-producer possible-bits query that removes redundant masks
  and shifts and decides comparisons against type or mask bounds;
- constant shift pairs, equality through invertible wrapping operations,
  zero-test selects and grouping of single-use offsets.

An AND whose literal covers every source bit of a widening zero extension is
removed, and sign extension of an AND that clears the source sign bit becomes
zero extension. These rules also run in Fast. i128 literals are created only
when the IR's sign-extended i64 immediate can represent them. Combining skips
producers whose result is already aliased.

## Integer facts

`known-bits` stores disjoint known-zero and known-one masks for each integer
value from i8 to i128 and propagates them with a worklist to a fixed point.
Block parameters intersect the facts of all incoming edges, including
backedges; entry parameters stay unknown. Transfers cover bit operations,
shifts with known counts, extensions, truncation, selects, the 0-or-1 range of
comparisons and overflow flags, and carry-aware add, subtract and negate. The
pass removes masks that the operands already establish and proves comparisons
from the resulting bounds, including signed ordering. Values are materialized
only when the immediate can represent them. It does not remove loads, stores or
trapping operations. Balanced, Size and Full run it late.

`branch-ranges` derives intervals from integer comparisons against literals on
conditional edges whose target has one predecessor and differs from the other
target. It walks the dominator tree with scoped facts, refines signed and
unsigned domains separately, and folds later comparisons the interval decides.
Facts do not leak into siblings, joins or loop headers, and the pass does not
map block arguments or relate different values.

## SCCP

`sccp` uses the
[Wegman and Zadeck](https://cs.brown.edu/research/pubs/techreports/reports/CS-89-36.html)
lattice: unknown, a bit-exact constant, or overdefined. Block parameters meet
arguments from executable edges, and two arms from one predecessor stay
distinct. Values left unknown in executable code become overdefined. Analysis
converges before rewriting, and parameter constants are materialized in their
own block. Checked arithmetic tracks the wrapped value and the overflow flag
separately. `simplify-cfg` and `dce` clean up afterwards. No tier runs it.

## Phi cleanup

`phi-simplify` checks every incoming edge of a block parameter. A parameter's
own backedge value is ignored only when another incoming value anchors it. The
replacement must be defined in a strictly dominating block, so values defined
inside the loop and unanchored cycles never become copies. Entry ABI parameters
stay. Balanced, Size and Full run it late; placing it before GVN and GCM
regressed interpreter workloads.

## CFG cleanup

Forwarding composes a jump-only block's incoming argument tuple with its
outgoing jump, including permutations and repeated arguments. A forwarding
block whose parameter is used outside its own jump stays. One forwarding step
per round lets closed forwarding cycles collapse to self-loops without changing
nontermination. Block merging takes one edge census and layout snapshot and
retries each predecessor after a merge; the entry block is never merged away.

## GVN and GCM

GVN sorts commutative operands and reverses predicates for swapped integer
comparisons. It never merges loads.

GCM places each pure instruction between its early block (the deepest block
defining an operand) and its late block (the common dominator of its uses),
choosing the block in the fewest loops and, among those, the latest. That
hoists invariants and sinks code into the branch that needs it, with three
exceptions:

- An instruction that stays in its block keeps its place, so chains of selects
  do not sink past calls and keep their conditions alive.
- Instructions without operands (constants, addresses) are rematerialized in
  each block that uses them, undoing GVN's function-wide merging.
- Addresses of fixed stack or global objects with constant offsets stay at the
  late placement instead of being hoisted out of loops. Arithmetic on incoming
  pointers still gets loop-invariant code motion.

Repeating GVN and GCM to a fixed point would undo this placement, so GCM runs
last in each round.

## Inlining

`inline` expands direct calls to same-module `local` definitions present at
pass entry, from frozen templates; cloned calls wait for the next invocation.
The binding-aware variants also admit public definitions that carry a
`noninterposable` or `no_semantic_interposition` promise. Limits, exclusions
and skip reports are owned by
[function attributes](function-attributes.md#inliner-policy-and-hard-boundaries).

The signature must match exactly. Each site gets fresh blocks, values and
stack slots, remapped symbol and signature tables, and a continuation whose
block parameters carry the return values. Argument evaluation keeps its order.
`byval` arguments up to 128 bytes are copied, padding included, into private
snapshots before callee effects. `sret` keeps the caller's destination. Callee
lines, scopes and stack variable homes move into a fresh scope below the call
site; the IR has no DWARF inline-subroutine representation.

## Tail calls

Balanced, Size and Full run `tail-calls` after private calling-convention
selection. It needs matching nonvariadic signatures with at most six
single-register scalar arguments and one result. The whole direct-call closure
must contain only scalar arithmetic, control flow and calls to strong local
bodies with the same property. Unknown callees, frame and stack intrinsics,
pointers, memory accesses, assembly and implicit FMA helpers block it, so no
callee can inspect a replaced frame. Explicit IR `return_call` covers cases
outside this boundary.

## Memory passes

All memory passes keep the first potentially faulting access and every store
unless a section says otherwise. Volatile and atomic accesses, calls, inline
asm, `debugtrap` and lifetime barriers end their facts through the IR
memory-effects model. Functions with debugger-visible variable homes are
excluded, because a debugger can edit a home between source locations.
Alignment and `notrap` never establish alias freedom.

### stack-forward

`stack-forward` handles constant-offset addresses of fixed stack objects. An
address passed to a call, returned, converted to an integer, stored or used
outside ordinary loads, stores and address construction makes the object
escape. Volatile or atomic access excludes the object; inline assembly, frame
inspection and stack-state barriers exclude the function. Exact typed stores
satisfy later loads in the same block, and integer loads through 64 bits can be
rebuilt from stored bytes in little-endian order. No facts cross blocks.
Stores are removed only when no load from the object remains.

Full also recovers complete `i32x4` constructors from these byte facts. Four
copies of one `i32` become `vsplat`, and four extracts from at most two `i32x4`
snapshots become `vshuffle`, so C literals such as `(U){s,s,s,s}` and
`(U){a[3],b[0],a[1],b[2]}` reach the target shuffle selector. Partial lanes,
other expressions and mixed widths stay in memory.

### small-memory

`small-memory` expands `memcpy`, `memmove` and `memset` intrinsics with constant
i64 lengths from 0 to 32 bytes into at most six exact 8, 4, 2 and 1-byte
accesses with alignment 1. Every source chunk is loaded before any store, so
overlap in either direction works. `memset` multiplies its unsigned-extended
fill by the repeated 0x01 pattern. A zero length removes only the intrinsic.
Variable and larger lengths stay. Balanced and Full run it before
`stack-forward`.

### memory-forward

`memory-forward` removes a same-type ordinary load when a load or store in the
same block already established its value. Constant i64 `ptradd` chains share
a root and a checked signed byte offset; other derivations stay opaque.
Stores invalidate overlapping byte footprints of any type, compared with
circular modulo-2^64 distances because pointer arithmetic can wrap.
Pointer-width stores invalidate all facts. Distinct roots disambiguate a write
only when both have valid entry `restrict_params` contracts; comparisons,
block-argument passing, escapes, volatile and atomic uses disqualify a root.
`AliasInfo` is the private, function-local analysis behind this check.

Tests: `cargo test -p keel-opt --test memory_forward`.

### load-reuse

`load-reuse` makes an ordinary successful load's value available for its exact
type and canonical address along unique-predecessor edges in reverse
postorder. Merges, unprocessed predecessors and backedges inherit nothing, and
a loop header resets availability. Chains stop after eight edges and snapshots
hold at most 128 facts. Stores do not create facts. A later duplicate load
disappears only after the original load succeeded with no possible mutation
or lifetime barrier on the path. Address analysis is shared with
`memory-forward`. Balanced, Size and Full run it.

`call-load-reuse` also reuses loads across calls whose explicit scoped
contracts promise no accessible writes, no freeing, no synchronization, no
callbacks and preserved object accessibility. See
[LLVM import](../frontends/llvm.md).

### stack-promote

`stack-promote` promotes scalar i8 to i64 and pointer fields reached only
through fixed stack addresses with constant, nonnegative, in-bounds offsets.
Accesses to overlapping ranges must agree in offset and type, except that a
covering integer store of a repeated constant byte may initialize a smaller
field. Calls, intrinsics, volatile or atomic accesses, address comparisons and
returned or stored addresses make the slot escape. Inline assembly, frame
observers, stack save and restore, returns-twice calls, invokes and landing
pads exclude the function.

A must-initialization analysis requires an exact or covering store before every
load on every path, including zero-trip paths; the pass never invents a value.
Promoted fields become block parameters, and a store is removed only when no
overlapping read stays in memory. Limits are 256 blocks, 8192 instructions and
64 fields. Full also admits a suffix after the entry block's last memory
barrier when later loads derive from incoming pointer parameters: fresh reads
after the barrier seed the SSA values. Follow the pass with
`phi-simplify,dce`. Balanced, Size and Full run it.

`stack-cache` seeds integer and pointer fields of an object passed to an
initializer from memory after the last entry-block effect and carries them
through bounded loop SSA, keeping every store. Limits are 64 blocks, 2,048
instructions and eight fields. No tier runs it
(`cargo test -p keel-opt --test stack_cache`).

### loop-memory

`loop-memory` hoists one invariant ordinary load out of an unrotated two-block
loop. The header's first test is cloned before the loop, so the zero-trip exit
keeps its original arguments and only the entered path runs the load. The
header compares a unit-step i32 or i64 induction variable against an invariant
bound using at most 16 nontrapping integer instructions. The body is one block
of at most 128 instructions whose only load comes before any store or other
effect; later stores must be disjoint by `AliasInfo`. The exit has one incoming
edge and ends in a return. Functions are limited to 512 blocks and 8192
instructions, with at most eight transformations. No tier runs it.

## Module cleanup

`constant-args` substitutes an integer or floating literal into a local body
only when every direct call passes the same literal and the function cannot
escape through data, addresses, assembly, retention or lifecycle roots. Weak,
public, variadic, returns-twice and explicit aggregate-transport functions are
excluded. The signature and call arguments stay; the body is not inlined or
cloned. `empty-calls` removes calls proven to have no effect, and
`unused-functions` removes functions that optimization made unreachable. Both
treat constructors, destructors, address references, relocations and assembly
references as roots. Balanced, Size and Full run them late.

## Loop passes

### loop-unroll

`loop-unroll` runs in Full after `loop-reduce-multi` and before `loop-rotate`.
It accepts a loop with a two-instruction unsigned header test, one
straight-line body and one external preheader, carrying an induction variable
from zero by one and one wrapping sum `iadd(sum, term)`. The term may use the
induction variable and one or two ordinary loads. Stores, calls, volatile or
atomic accesses, division, floating point, other liveouts, nested loops and
source locations in the loop reject it.

The paired loop carries `(iv, lane0, lane1)` and enters two body copies only
when unsigned `bound - iv >= 2`, which keeps both increments in range without
wrapping. The lanes combine once with wrapping addition, then the original loop
handles the remaining iteration. Loads of the second iteration run only after
both iterations are known to exist.

### Ordered reductions

`adjacent-diff` recognizes a rotated loop summing absolute differences of
consecutive signed i32 elements and loads two new elements per iteration,
guarded against address wrap with the scalar loop as fallback.
`run-balance-predicate` recognizes a credit/debit recurrence with an unsigned
comparison. Both are explicit experiments. Full's `adjacent-abs` converts a
private scalar absolute-value triangle without moving memory.

### sign-extend-idiom

`sign-extend-idiom` rewrites the C shape `b = state & 255; b < 128 ? b : b - 256`
into `sext.i64(trunc.i8(state))` and removes the negative arm. It needs the
exact header, arm and merge shape on i64, and rejects negative-arm source
locations. Run DCE afterwards. Tests:
`cargo test -p keel-opt --test sign_extend_idiom`.

### if-convert

`if-convert` admits a branch whose two single-predecessor arms jump to the same
join, with at most eight speculated instructions and eight join parameters. It
moves the arm instructions before the decision and selects each join argument
with the original condition. Only integer constants, compares, selects,
wrapping arithmetic, logic, masked shifts and rotates, high multiply, bit
counts and integer conversions may be speculated; new opcodes do not enter
implicitly. Full's reduction passes normalize
[conditional multi-reductions](simd-ir.md#conditional-multi-reduction-normalization)
privately instead.

## Fast tier

Fast runs `fast-cfg, fast-local, fast-cfg, dce, fast-loop-reduce`. Each CFG
pass folds branches, forwards one jump-only edge per destination, removes
unreachable blocks and merges straight-line chains up to 64 instructions, with
no fixed-point loop. `fast-local` makes one reverse-postorder pass with the
`instcombine` constant, identity, cast, comparison and power-of-two rules, a
256-entry per-block table of pure expressions, and a 256-entry table that
reuses an executed division's quotient for a matching remainder. Fast builds no
dominator tree or use graph and does no GVN, inlining or loop analysis apart
from the size-capped `fast-loop-reduce` sum transform (at most 256
instructions, 16 blocks, one stream address and eight vector-expression
values). Division by zero and signed MIN/-1 still trap; signed MIN % -1 is zero.

## Apple M5 tuning

`--tune generic` is the default. `--tune apple-m5` selects measured choices for
Apple arm64 without changing the target, ABI, ISA features or IR tier. With
tree selection, i32 and i64 multiplication by 3, 5 or 9 uses
`add r, r, r, lsl #k`; existing multiply-add covers and shared constants are
kept. `tools/generate_m5_tuning.py` emits the Rust cost table from a
calibration report: a rule applies only when the dependency-chain ratio's upper
95% bootstrap bound is below 1.0 and the parallel-stream median ratio is at
most 1.0. The x86 target rejects M5 tuning.

```sh
keel cc -O --isel-tier full --tune apple-m5 input.i -o output.s
```

`tools/instruction_metrics.py` reports encoding sizes and optional
`llvm-mca` estimates for comparing lowerings; see
[testing](../development/testing.md#instruction-metrics). Those estimates do
not drive optimizer decisions.
