# Optimizer

`keel-opt` runs function passes through `Pass::run` and module passes through
`Pass::run_module`. This page states each pass's contract: what it does, which
tiers run it, what it preserves and where it stops. The tier pipelines are
defined in code by `OptimizationTier::pipeline`, `BALANCED_PIPELINE`,
`SIZE_PIPELINE` and `DEFAULT_PIPELINE` in `crates/keel-opt/src/lib.rs`. Balanced
and Size join shared round arrays, and Full repeats `FULL_ROUND` three times
before `FULL_EPILOGUE`. [Optimization levels](optimization-levels.md)
summarizes the tiers. Pass-specific admission rules live in each pass module's
rustdoc.

## Pass manager

`Pipeline` is the one entry point: it takes the passes, an
`OptimizationContext`, `OptimizeOptions` and an optional report.
`optimize`, `optimize_with_tier`, `run_passes`, `run_passes_for_target`,
`run_passes_for_target_with_report` and `run_pipeline` are shorthands for it.
`PASSES` lists the registered passes, each a named static in `lib.rs`.

A function pass receives the `OptimizationContext`: the vector target and
capabilities, the pointer `DataLayout` and whether the function is canonical on
entry. `OptimizationContext::new` gives the native vector targets
`DataLayout::NATIVE64` and the portable target no layout. A module pass
receives a `ModuleContext`. It names the functions the pass may change, collects
inline sites for a report and records which functions the pass changed.
Behavior comes from pass properties, never from pass names:
`requires_canonical_form`, `produces_canonical_form`, `deterministic`,
`uses_inline_budget` (Full's shared inlining allowance) and `family`, which
names the function policy (inlining, unrolling or vectorization) that adds the
pass to or removes it from a function's schedule.

The manager runs each pass over the module before moving to the next and
verifies, after every pass, each function the pass reports changed, plus any
function a module pass added. A pass that edits a body must report the change:
the deterministic-pass cache trusts the same flag. With debug assertions or
verification on, the manager fingerprints each function before a pass and
panics, naming the pass, when a function it reported unchanged differs
afterwards. It keeps a function's fingerprint while passes report it
unchanged, so it hashes a function again only after a reported change.
`Function::fingerprint` lists the state it covers. Clean-cache
entries belong to pass instances, so a repeated unchanged instance stays
skipped until its function changes.

`OptimizeOptions` also carries debugging controls. `disabled` skips passes by
name wherever they appear, including function-attribute schedules.
`bisect_limit` runs only the first N invocations; an invocation is one
function pass run on one function or one module pass run, and runs the clean
cache skips are not counted. The smallest limit that shows a miscompile names
the invocation that caused it, in about log2(invocations) runs.
`bisect_verbose` traces each invocation that runs as `bisect: <n> <pass>
@<function>` (or `(module)`). `print_changed` traces each function after an
invocation that changed it, under `; after <pass>, invocation <n>`, and
`print_function` limits that to one function. Traces go to standard error
unless `Pipeline::trace` names a writer. The driver exposes these controls as
`--opt-disable <pass,..>`, `--opt-bisect-limit <n>`, `--opt-bisect-verbose`,
`--print-changed` and `--opt-function <name>` on `cc`, `compile` and `opt`.

Ordinary and reported optimization share one execution loop. Without a report
the loop creates no timers, IR counts or observation records. Completed pass
invocations stay in a partial report when their verification fails, and
skipped invocations keep pipeline order with zero timings. All exits release
analysis and scratch caches.

A thread-local cache reuses CFG, dominator and loop-depth analyses while the
function's CFG stamp is unchanged, or when block count, layout and ordered
successors still match. Debug builds check, after each pass run, that a cached
entry carrying the function's stamp still matches its CFG.
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
- Complementary left/logical-right shifts of the same i32/i64 source become
  rotates. Variable counts may pass through masks and integer casts that
  preserve their low count bits; constant pairs require nonzero in-range counts.
- Floating-point operand order, noncommutative operand order, select arms,
  edge order, memory effects and trapping behavior are unchanged.

This is a structural form, not a promise that equivalent trees are identical.
Later passes can break it by recording aliases or folding values to literals.
`Pass::requires_canonical_form()` declares an entry requirement: the manager
restores the form right before a requesting pass when an earlier pass reported
a change, unless that pass promised `produces_canonical_form()`, and sets
`OptimizationContext::canonical`. A pass that reports no change must not mutate
the function. This applies to explicit and custom pipelines too. The guarantee
holds on entry to a requesting pass, not during its rewrites or on optimizer
exit. Direct calls to `Pass::run` bypass it, and a caller-built context leaves
`canonical` false; use `Pipeline` or `run_passes`.

## Passes

| Pass | What it does |
|---|---|
| `inline` (opt-in) | Bounded private direct-call expansion; see [inlining](#inlining) |
| `inline-noninterposable`, `inline-cheap-reserve`, `inline-size` | Binding-aware inlining variants used by Balanced, Full and Size |
| `small-memory` | Expand constant memory intrinsics of 0 to 32 bytes; see [memory passes](#memory-passes) |
| `memory-expand` | Expand constant memory intrinsics of 0 to 128 bytes in Fast and Basic; see [memory passes](#memory-passes) |
| `stack-forward` | Block-local byte forwarding for nonescaping fixed stack objects |
| `stack-notrap` | Mark loads and stores proven inside a fixed stack slot `notrap`; see [memory passes](#memory-passes) |
| `canonicalize` | Establish the [canonical form](#canonical-form) |
| `simplify-cfg` | Fold constant `brif` and `switch`, compose arguments through jump-only blocks, merge a block into its only predecessor, drop unreachable blocks |
| `const-fold` | Evaluate instructions on constants with the machine's semantics; trapping divisions stay. Balanced and Full run it once, before the first inliner, whose admission counts instructions |
| `instcombine` | Worklist combining; see [scalar simplification](#scalar-simplification) |
| `call-contracts` | Apply direct `noreturn` and `nounwind` contracts; see [function attributes](function-attributes.md#optimizer-consumers) |
| `memory-forward` (opt-in) | Block-local exact-address load reuse and store-to-load forwarding |
| `load-reuse` | Bounded cross-block load reuse |
| `call-load-reuse` (opt-in) | Load reuse across calls with explicit scoped call contracts |
| `known-bits` | Fold masks and comparisons from known-zero and known-one bits |
| `phi-simplify` | Replace uniform block arguments with a dominating value |
| `gvn` | Scoped hash table over the dominator tree; loads are never merged |
| `dce` | Mark from pinned instructions; dead block parameters die with their arguments; lifetime markers stay only while another needed instruction addresses their slot |
| `gcm` | Global code motion, the scheduler for floating instructions |
| `stack-promote` | Promote private fixed stack fields to SSA |
| `loop-vectorize`, `loop-reduce`, `loop-reduce-multi` | Vectorize counted maps and wrapping reductions; see [SIMD IR](simd-ir.md) |
| `fast-math` | Finite scalar/packed algebra, AArch64 contraction and refined f32 reciprocal roots, only in `fast_math` functions |
| `fast-loop-dot` | Full-tier SIMD dot, sum-of-squares and squared-distance accumulation, only in `fast_math` functions |
| `loop-dot` (opt-in) | Batch floating dot products while preserving scalar addition order; see [SIMD IR](simd-ir.md) |
| `loop-strength-reduce` | Replace wrapping i32/i64 induction products with recurrences in two-block loops |
| `loop-unroll` | Two-way unrolling of wrapping integer reductions |
| `loop-unroll-exact` | Fully unroll tiny literal-trip loops in scalar operation order |
| `loop-rotate` | Guarded rotation that copies header tests to the latch and keeps the entry guard |
| `adjacent-abs` | Convert private scalar absolute-value triangles before vectorization |
| `if-convert` (opt-in) | Bounded scalar if-conversion |
| `loop-matmul` | Guarded four-column tiling of bounded wrapping i32 matrix products on AArch64 |
| `devirtualize-const-table` | Version bounded local constant-table calls before Full inlining |
| `constant-args`, `empty-calls`, `unused-functions`, `compact-unused-functions` | [Module cleanup](#module-cleanup) of constant arguments, empty calls and unreachable functions |
| `private-aggregate-cleanup` | Pass private two-word by-value aggregates as word pairs |
| `internal-fast-calls` | Final private transport for contained direct calls; see [internal fast calls](#internal-fast-calls) |
| `tail-calls` | Replace adjacent scalar call/return pairs; see [tail calls](#tail-calls) |
| `branch-facts` | Fold repeated or complementary integer comparisons established by dominating edges |
| `range-facts` (opt-in) | Extend branch facts with scoped constant bounds and bounded integer arithmetic |
| `outline-cold` | Tail-outline bounded terminal panic records into private cold helpers |
| `fast-cfg`, `fast-local`, `fast-loop-reduce` | The bounded Fast tier; see [Fast](#fast-tier) |

`-p pass,pass,...` selects any pipeline explicitly, overriding the tier.

No tier runs the opt-in passes. `memory-forward` applies `restrict` parameter
contracts within a block, and `call-load-reuse` keeps loads across calls whose
scoped contracts LLVM import supplies. The other three are test entry points
for code that tiers run inside other passes:

- The binding-aware inliners share `inline`'s legality checks, cloning and
  growth budgets.
- `instcombine`, the Wasm basic pipeline and required inline-assembly cleanup
  use the `const-fold` kernel.
- `loop-reduce` and `loop-reduce-multi` run `if-convert` on a trial copy of a
  conditional reduction.

## Adding a pass

1. Write the pass in its own module under `crates/keel-opt/src/`. A function
   pass with a plain kernel is a `BuiltinPass::new(name, description, run)`;
   other passes implement `Pass`, and module passes return `Some(changed)`
   from `run_module`. Return true whenever the pass mutates a function. With
   debug assertions the manager fingerprints each function and panics, naming
   the pass, when a function it reported unchanged differs afterwards.
2. Declare the pass properties: `requires_canonical_form` and
   `produces_canonical_form` for the [canonical form](#canonical-form),
   `deterministic` only if the result depends on nothing but the function and
   the target, and `family` if a function policy turns it off. Put admission
   rules in the module's rustdoc.
3. Register a `pub(crate) static` in `lib.rs` and append it to `PASSES`, the
   `keel passes` order. Add its row to the [pass table](#passes). The
   `registry` test in `cargo test -p keel-opt --test tiers` fails until the
   row exists, and fails for a row that names no pass.
4. Add `crates/keel-opt/tests/golden/<pass>.keel`, whose first line is
   `; passes: <pass>`, and its `.out`; `BLESS=1` writes the `.out`. Put focused
   cases in `crates/keel-opt/tests/<pass>.rs`, with submodules as
   `#[path]` files beside it. The `golden` target's
   [differential tests](../development/testing.md#differential-optimizer-tests)
   already run every registered pass alone on every fixture under the
   reference interpreter. They skip functions that use an operation the
   interpreter does not model, so check that the new fixtures run.
5. Run the pass alone with `keel opt -p <pass>` while developing. To place it
   in a tier, add its name to `FULL_ROUND`, `FULL_EPILOGUE`, the Balanced and
   Size round arrays or `OptimizationTier::pipeline` in `lib.rs`. A pass that
   no tier runs is opt-in and needs a reason in the list above.
6. Run `python3 tools/test.py optimizer --quick`, which adds the C and
   codegen program oracles. If a program miscompiles,
   `--opt-bisect-limit` finds the invocation, `--opt-disable <pass>` confirms
   it, and `--print-changed --opt-function <name>` shows the IR it produced.
7. Before timing, check which functions change with `cargo suite stats` and
   `cargo suite codegen-diff`. A tier change is accepted by the
   [performance guide](../development/performance.md).

## Scalar simplification

`instcombine` uses a consumer worklist, so a substitution exposes further folds
regardless of block layout. Integer rules follow wrapping arithmetic. Division
identities need a known nonzero divisor, and floating-point identities are
excluded. Effectful and trapping producers stay pinned even when a consumer
becomes constant. Rules include:

- constant evaluation, shared with `const-fold`, including both
  results of checked add, subtract and multiply through i64;
- power-of-two unsigned division, remainder and multiplication as shifts and
  masks;
- Boolean absorption (`x & (x | y) = x`), XOR cancellation and complement
  pairs (`x & ~x = 0`), for all widths;
- integer cast chains: successive truncations or equal extensions, sign
  extension of a zero extension, and truncation of an extension collapse;
  zero extension of a sign extension and extension after truncation stay unless
  possible-bits facts prove the truncated value fits the retained range;
- the wrapping identity `(x & y) + (x | y) = x + y`, including a pair
  separated by one single-use addition, without expanding shared computations;
- wrapping cancellation such as `(x+y)-x` and `x+(-x)`, and grouping of two
  constants in an associative chain whose inner result has one use;
- a bounded three-producer possible-bits query that removes redundant masks
  and shifts and decides comparisons against type or mask bounds;
- constant shift pairs, equality through invertible wrapping operations,
  zero-test selects and grouping of single-use offsets.

`simplify-cfg` folds a `brif` whose two edges reach the same non-self block
into a jump with one select per differing argument. This does not move or
speculate instructions. It accepts at most eight canonical scalar integer or
pointer parameters; identical arguments need no select.

`simplify-cfg` also removes private `x ? clz(x) : width` and
`x ? ctz(x) : width` diamonds. Both counts already return the type width at
zero. Each arm must contain only the matching count or literal and its jump;
this fold does not enable general if-conversion.

An AND whose literal covers every source bit of a widening zero extension is
removed, and sign extension of an AND that clears the source sign bit becomes
zero extension. These rules also run in Fast. i128 literals are created only
when the IR's sign-extended i64 immediate can represent them. Combining skips
producers whose result is already aliased.

Each rule has a name, such as `identity-zero`, `power-of-two` or
`compare-range`, declared in the `rules!` list in `instcombine.rs`. The
optimization report counts a rule each time its rewrite is applied, per
invocation, in the invocation's `rules` object. Fast and the other passes that
call the `instcombine` kernel count under their own invocations.

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
hoists invariants and sinks code into the branch that needs it, with these
exceptions:

- An instruction that stays in its block keeps its place, so chains of selects
  do not sink past calls and keep their conditions alive.
- Instructions without operands (constants, addresses) are rematerialized in
  each block that uses them, undoing GVN's function-wide merging.
- A `ptradd` of a constant offset stays at the late placement instead of being
  hoisted out of loops, whatever its base. Selection folds the offset into
  loads and stores, while a hoisted address is one more pointer live across
  the loop, often spilled. Other pointer arithmetic still gets loop-invariant
  code motion.
- When that base is a parameter or a pinned result, each load or store of at
  most 8 bytes at the address gets its own copy of the `ptradd` and constant,
  just before it, so selection can fold the offset there. The offset must be
  one that every backend folds: a non-negative multiple of the access size,
  below 4096 times it. Accesses in a block where another instruction uses the
  address keep the original, which is computed there anyway.

Repeating GVN and GCM to a fixed point would undo this placement, so GCM runs
last in each round.

## Inlining

`inline` expands direct calls to same-module `local` definitions present at
pass entry; cloned calls wait for the next invocation. The binding-aware
variants inline `alwaysinline` callees before their callers, so a cloned
wrapper already contains them, and freeze templates before the ordinary sites.
Template instruction and stack costs are cached within that invocation.
Caller stack checks and growth ledgers still use the current caller state.
Full fixes its module allowance from pipeline-entry IR, half of its
instructions with a floor of 196,608, and shares conservative clone charges
across rounds. The floor preserves the old three-round aggregate allowance; it
does not reduce the whole pipeline to one round's allowance. A module merged
from several inputs gets at least the sum of their allowances
(`Pipeline::inline_allowance`).
It schedules original call sites in caller sweeps within each priority phase. Reports include the allowance, cumulative ordinary
and reserve charges, and each site's admission or first rejection reason.
These are bounds on expansion work, not predictions of final machine-code size.
The binding-aware variants also admit public definitions that carry a
`noninterposable` or `no_semantic_interposition` promise. Limits, exclusions
and skip reports are owned by
[function attributes](function-attributes.md#inliner-policy-and-hard-boundaries).

Full can reconsider a legal callee rejected for its block or instruction count
when a call supplies integer literals. It simplifies an acyclic private template
copy before admission. The selected body must fit the existing five-instruction
call-cost allowance; larger selected paths retain the call. The original
definition and ABI stay unchanged. Existing
binding, math-mode, target-feature and function-policy checks still apply.
Costing uses the simplified template while preserving the existing cheap-call
exemption and pipeline-wide growth budgets. No
specialized module definitions are created. Balanced and Size retain their
existing admission rules.

This speculative work is bounded per inliner invocation: 64 distinct contexts,
at most four per callee, 64 laid-out blocks and 256 instructions per input body,
with separate arena and metadata limits. Templates with source locations,
debugger homes or explicit aggregate parameter transport decline. Failed
contexts are cached too. Temporary templates release their address-keyed
analyses before their storage dies. Cloned calls still wait for the next round.

The signature must match exactly. Each site gets fresh blocks, values and
stack slots, remapped symbol and signature tables, and a continuation whose
block parameters carry the return values. Argument evaluation keeps its order.
Only callee slots referenced by surviving instructions or debug variable homes
are charged and cloned. Implicit `aggregate_va_arg` homes count as references;
unused slot declarations do not. Existing caller stack accounting stays
conservative.
`byval` arguments up to 128 bytes are copied, padding included, into private
snapshots before callee effects. `sret` keeps the caller's destination. Callee
lines, scopes and stack variable homes move into a fresh scope below the call
site; the IR has no DWARF inline-subroutine representation.
Each cloned slot and each snapshot gets one lifetime per inlined call, so frame
layout can share its bytes with other slots. It starts before the slot's first
write: before the snapshot copy, or after the callee's entry-block `stack_addr`
of the slot. Without one, the inliner adds a `stack_addr` before the jump into
the copy and charges it. It ends at the head of the continuation. A slot with
markers of its own keeps them, and debug and `aggregate_va_arg` homes get none.
Inlining decisions do not count markers.

## Internal fast calls

Balanced, Size and Full run `internal-fast-calls` once, after the last inlining
and before `tail-calls`. It gives a `local` C function `cc(fast)` and drops its
`sext`/`uext` attributes, then updates every direct callee entry. It skips
constructors, destructors, `used`, `retain`, sectioned, weak and `weak_odr`
functions, variadic and `returns_twice` signatures, `sret` and `byval`
parameters, and functions with more than one result. It also skips a function
that is an exception personality, the callee of a call with explicit ABI
operands, or either end of a `return_call`. Inline assembly anywhere in the
module disables the pass.

`cc(fast)` makes no promise to stay callable from C, so a fast function's
address never escapes. A function whose address is taken by `symbol_addr` or by
a data `ptr` with no addend becomes fast only when the module also calls it
directly. Its signature needs at most six scalar or pointer parameters and
results without extension attributes. The pass then adds a local C thunk named
`<name>.c` (with a numeric suffix if that name is taken), whose body is one
`return_call` to the fast function. It redirects every `symbol_addr` and data
pointer to the thunk, so addresses still compare equal and every indirect
caller uses the C convention. While fast and C place these arguments alike,
both backends emit the thunk as a single jump. Other address-taken functions,
data pointers with an addend, Apple RTTI names, and any address taken in a
module with module assembly keep the C convention.

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
asm and `debugtrap` end their facts through the IR memory-effects model. A
lifetime start is a write of indeterminate bytes to its whole slot:
`load-reuse` and `memory-forward` drop only the facts that may overlap that
slot, and keep facts on accesses that lie inside another fixed slot. Facts
cross an end, since an access after it is undefined. Passes that do not
handle markers see the registry's `Barrier` effect. Functions with debugger-visible variable homes are
excluded, because a debugger can edit a home between source locations.
Alignment, `notrap` and `trap_ub` never establish alias freedom.
`loop-vectorize` is the one pass that uses `trap_ub`: it may reorder accesses
whose fault is undefined behavior.

### stack-forward

`stack-forward` handles constant-offset addresses of fixed stack objects. An
address passed to a call, returned, converted to an integer, stored or used
outside ordinary loads, stores and address construction makes the object
escape. Volatile or atomic access excludes the object; inline assembly, frame
inspection and stack-state barriers exclude the function. Exact typed stores
satisfy later loads in the same block, and integer loads through 64 bits can be
rebuilt from stored bytes in little-endian order. Across blocks, a store must
dominate the load and be the only writer of every byte in the forwarded range.
A lifetime marker is not an escape. No fact on its slot crosses a start, and
facts cross an end. Across blocks, a start blocks the fact unless it comes
before the store in the store's block or sits in a block that strictly
dominates the store's: every path from such a start to the load runs the
store again.
Stores are removed only when no load from the object remains.

With `DataLayout::NATIVE64`, which the native x86-64 and AArch64 contexts
carry, `stack-forward` also forwards an exact eight-byte `i64` store into a
pointer load using `inttoptr`. This preserves all address bits, including null,
and neither moves nor widens a memory access. Partial stores, overlapping
writers and over-aligned pointer loads do not qualify. Contexts without a
layout, including the portable and Wasm pipelines, do not apply this
reinterpretation.

Full also recovers complete `i32x4` constructors from these byte facts. Four
copies of one `i32` become `vsplat`, and four extracts from at most two `i32x4`
snapshots become `vshuffle`, so C literals such as `(U){s,s,s,s}` and
`(U){a[3],b[0],a[1],b[2]}` reach the target shuffle selector. Partial lanes,
other expressions and mixed widths stay in memory.

### small-memory

`small-memory` expands `memcpy`, `memmove` and `memset` intrinsics with constant
i64 lengths from 0 to 32 bytes into exact accesses with alignment 1. `memcpy`
uses 16-byte `i8x16` chunks, then 8, 4, 2 and 1-byte chunks. `memmove` and
`memset` use only the scalar chunks, because `stack-forward` and
`stack-promote` see through scalar stores only. Every source chunk is loaded
before any store, so overlap in either direction works. `memset` multiplies
its unsigned-extended fill by the repeated 0x01 pattern. A zero
length removes only the intrinsic. Each operation is expanded alone, never
merged with a neighbouring one. Variable and larger lengths stay. It also
replaces a `memcmp` of at most 32 bytes whose result is only compared with
zero by chunked loads and compares. Balanced and Full run it before
`stack-forward` in the inlining rounds, and the inliner charges an expandable
copy as its loads and stores.

### memory-expand

`memory-expand` is `small-memory` with a 128-byte limit for `memcpy`,
`memmove` and `memset`; the `memcmp` limit stays 32. A `memmove` longer than
32 bytes uses 16-byte chunks, so its snapshot needs at most 11 registers.
Fast and Basic run it before `stack-notrap`. They do not inline, so no inliner
prices the larger copies. Balanced and Full keep `small-memory`: the 128-byte
limit after their last inlining round gave no gain on the acceptance hosts and
slowed x86-64 by 0.17%. Size and None run neither pass, and longer or variable
copies call the C library.

### stack-notrap

`stack-notrap` sets `notrap` on a non-volatile load or store whose address is a
slot's `stack_addr`, or a chain of constant non-negative `ptradd`s from it, when
the access ends within the slot. A fixed slot stays mapped for the whole frame,
so this holds even after its address escapes. Pointer accesses count as 8
bytes. Slots with lifetime markers qualify too: their bytes stay mapped, and an
access outside a lifetime is undefined whatever its flags say. The pass infers
no alignment. `dce` can then delete
such loads when their results are unused. Every tier except None runs it before
a `dce`: Fast and Basic once, Balanced after each round's `stack-forward`, and
Full after each `FULL_ROUND` `stack-forward`.

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
type and canonical address in reverse postorder. A merge retains only identical
values available from every predecessor. Unprocessed predecessors supply no
facts. Chains stop after eight edges and snapshots hold at most 128 facts.
Exact stores publish their value, but forwarding a later load also requires a
prior successful read or an explicit `notrap` promise on that load. Writes
invalidate possibly overlapping facts. Address analysis is shared with
`memory-forward`. Balanced, Size and Full run it. When reuse changes a function,
it also numbers equivalent pure expressions that depend on the reads removed
by that invocation. This cleanup preserves GCM's local copies of operandless
instructions and address calculations. It leaves unrelated expressions and
branches alone rather than creating new long live ranges after scheduling.
This late numbering is limited to 2,048 layout instructions and 8,192 value
arena entries per function; memory forwarding itself keeps its existing bounds.

A single-block loop with one ordinary store may carry that stored value on its
backedge. Its entry predecessor must already supply a successfully read value
for the exact field. The pass adds a block parameter and removes the repeated
read. It adds no loads and keeps every write and other read, including input
bytes that alias the field. Other writes, volatile accesses and source locations
prevent this loop transform. No fixed-point memory analysis runs.

`call-load-reuse` also reuses loads across calls whose explicit scoped
contracts promise no accessible writes, no freeing, no synchronization, no
callbacks and preserved object accessibility. See
[LLVM import](../frontends/llvm.md).

Block snapshots are released after their last successor is visited. Local
cleanup keeps one reusable fact table and does not allocate block snapshots.

### stack-promote

`stack-promote` promotes scalar i8 to i64 and pointer fields reached only
through fixed stack addresses with constant, nonnegative, in-bounds offsets.
Accesses to overlapping ranges must agree in offset and type, except that a
covering integer store of a repeated constant byte may initialize a smaller
field. Calls, intrinsics, volatile or atomic accesses, address comparisons and
returned or stored addresses make the slot escape. Inline assembly, frame
observers, stack save and restore, returns-twice calls, invokes and landing
pads exclude the function. A lifetime marker is not an escape.

A must-initialization analysis requires an exact or covering store before every
load on every path, including zero-trip paths; the pass never invents a value.
A start of the slot uninitializes it, so a load after a start needs a store
after that start. An end changes nothing.
Promoted fields become block parameters, and a store is removed only when no
overlapping read stays in memory. Limits are 256 blocks, 8192 instructions and
64 fields. Full also admits a suffix after the entry block's last memory
barrier when later loads derive from incoming pointer parameters: fresh reads
after the barrier seed the SSA values. Follow the pass with
`phi-simplify,dce`. Balanced, Size and Full run it.

`stack-promote` also promotes integer and pointer fields of an object passed
to an entry-block initializer call. It seeds them from memory after the last
entry-block effect and carries them through bounded loop SSA. Every later load
must read an exact field or a pointer derived from an entry parameter, so the
pass removes the later stores. A slot with lifetime markers is not promoted
this way, because the seeding loads can come before its start; its markers do
not stop other slots. Limits are 64 blocks, 2,048 instructions and
eight fields (`cargo test -p keel-opt --test stack_cache`).

## Constant-table call versioning

Full runs `devirtualize-const-table` before its first inlining round. It turns a
call through a local constant function-pointer table into a switch of direct
calls. The table has at most eight entries, each a zero-addend pointer to a
local, nonweak, single-block leaf of at most twelve instructions. Targets have
matching scalar signatures, no explicit aggregate transport, no variadics or
returns-twice behavior, and no noinline/optnone or special frame attributes.
Subsequent ordinary inlining exposes the selected body; this pass does not
speculate target instructions.

The native eight-byte pointer layout must be explicit. An index AND mask
proves every table access in bounds; the pointer load's width and alignment are
also proved from the defined object. That load can then be marked `notrap` and
removed by DCE. Mutable, interposable, TLS, weak or underaligned tables,
nonzero-addend entries, unsupported targets, volatile loads and modules with
assembly decline. At most eight sites per function are versioned. Call-site
contracts and function mutation policies are preserved. Balanced and Size do
not run this pass.

## Guarded matrix tiling

Full runs `loop-matmul` before its first inlining round on AArch64 with the
native 64-bit data layout. It recognizes a complete three-loop, square i32
matrix product with zero-based unit induction, a zero-seeded wrapping sum,
and row-major `row*n + column` addresses. Every normalized instruction must
belong to that recipe, apart from integer constants. Loads and stores must
be nonvolatile and have no observable fault. No memory or arithmetic operation
from an unmatched body is moved.

The original graph remains as the fallback. The vector path requires
`4 <= n <= 46340`, nonwrapping pointer ranges, and an output range disjoint
from both input ranges. The dimension cap keeps the original signed i32
indices nonnegative without overflow. The inputs may alias each other.
A private helper accumulates four output columns in i32x4 registers, then
handles remaining columns with scalar products. It never reads the output
buffer. Small sizes, overlapping output, and failed range checks execute
the original loop in its original store order.

Recognition is limited to four ordinary parameters (three pointers and an
i32 dimension), at most 96 input instructions and 16 blocks. At most four
functions per module are tiled. Functions with stack slots, source locations,
special frame attributes, or a disabled vectorization policy decline.
Modules with assembly decline. Other targets, Balanced, and Size do not run
this transform. This is a bounded loop-nest recipe, not a general tiler.

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
The opt-in `compact-unused-functions` pass first compacts callee declarations that no
layout instruction references. This lets it discard inlined private Fast bodies
without leaving imports that require their local definitions. Surviving calls
keep their declaration attributes and call-site facts. Standard cleanup retains
its Fast-definition policy; frontends choose compacting cleanup explicitly.

## Loop passes

### loop-unroll

`loop-unroll` runs in Full after `loop-reduce-multi` and before `loop-rotate`.
It accepts a zero-based unit-step counted innermost loop, including one
nested inside another loop. A two-instruction signed or unsigned header leads
to a straight-line body with one wrapping i32/i64 reduction and up to two
independent additive auxiliary recurrences. Stores, calls, volatile or atomic
accesses, division, floating point, other liveouts and source locations in the
loop reject it. Budgets limit the original body to twenty instructions, two
loads and sixteen live scalar values.

An additive update `sum + term` uses independent even and odd accumulators.
For a constant-affine update `k*sum + term`, each parity stream uses coefficient
`k*k`; their final combination is `k*even + odd`. The streams start with zero
and the original seed respectively, preserving arbitrary seeds, zero trips and
wrapping coefficients without a modular inverse. Byte and halfword loads are
admitted for affine updates; additive updates retain their i32/i64 load policy.

The pair guard checks `bound - iv >= 2` in the original signedness, keeping
increments in range. Both complete body copies run in their original order,
including loads. The original loop handles the final odd iteration. No
loop-carried floating-point operation is reassociated.

### loop-rotate

`loop-rotate` copies a small header test to the unique latch while retaining the
entry guard. A natural loop may contain up to four body blocks and distinct
early exits. Header operations are restricted to integer constants, compares,
and wrapping addition/subtraction. Every memory access stays in its original
block, so an empty loop or an early mismatch performs no extra loads.

The pass forwards current header values to body and exit parameters. A shared
tail keeps its existing edge arguments; direct header-value uses outside the
loop must be in an immediate exit reached only from the loop. Uses in a later
descendant require SSA repair that this bounded pass does not perform. Calls, source locations,
debugger variable homes and cyclic bodies reject rotation. Empty lexical
scopes alone do not prevent it.

### loop-unroll-exact

Full runs `loop-unroll-exact` after loop rotation and dead-code cleanup. It
accepts counted header/body loops and rotated one-block loops with at most eight
literal iterations. Each loop may clone at most 128 instructions, within a
512-instruction function budget. Integer operations and nonvolatile scalar
loads and stores stay in iteration order; the pass does not reassociate
recurrences. Local forwarding then removes redundant reads using the same
readability proofs as `load-reuse`; a store alone is not such a proof. Successive stores
to the exact same scalar address collapse only when no intervening access,
trap or source location can observe the earlier store. Input reads that may
alias the field keep the writes separate. Calls, volatile accesses, floating
operations, unsupported control flow and debugger locations reject the
candidate. Empty lexical scopes alone do not prevent unrolling. The final induction update must not wrap in its comparison domain.

### if-convert

`if-convert` admits a branch whose two single-predecessor arms jump to the same
join, with at most eight speculated instructions and eight join parameters. It
moves the arm instructions before the decision and selects each join argument
with the original condition. Only integer constants, compares, selects,
wrapping arithmetic, logic, masked shifts and rotates, high multiply, bit
counts and integer conversions may be speculated; new opcodes do not enter
implicitly. Full's reduction passes run it on a private copy to normalize
[conditional multi-reductions](simd-ir.md#conditional-multi-reduction-normalization).

## Fast tier

Fast runs `fast-cfg, fast-local, fast-cfg, memory-expand, stack-notrap, dce,
fast-loop-reduce`. Each CFG
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

## Terminal panic records

Full runs `branch-facts` and `outline-cold` after tail-call cleanup. Branch facts
use canonical integer comparisons and their complements. The opt-in
`range-facts` pass also propagates scoped signed/unsigned constant bounds for
i8 through i64. They also track zero/nonzero conditions and
bounded add/subtract expressions when their endpoints stay in the relevant
integer domain. Bitwise AND supplies an unsigned upper bound. Signed and
unsigned intervals only constrain each other when their sign region is known;
a wrapped endpoint never becomes a nonwrapping bound. Each expression query
visits at most sixteen definitions. No default tier schedules `range-facts`.

Only a unique incoming conditional edge establishes a new fact; joins, loops
with another predecessor, and duplicate branch destinations do not. Facts are
restored on leaving each dominator subtree. This is neither a loop fixed point
nor a general relational range solver. A fold runs CFG cleanup and DCE so
unreachable panic records stop consuming storage.

Cold outlining accepts a private Fast function with at most 256 laid-out
instructions and 4096 instruction/value arena entries. It moves a terminal trap
block containing stack records into a cold, noinline, noreturn helper with the
same return convention, then tail-calls that helper. Constants and symbol
addresses are cloned into the helper; at most six scalar live-ins are passed.
The trap block is limited to 128 instructions. Debug locations, exception edges,
inline assembly, intrinsics, returns-twice behavior and alwaysinline functions
prevent outlining. Stack addresses and opaque calls outside the selected block
also prevent it: tail-calling must not invalidate a captured caller-frame address.
The helper retains target feature permissions and call declarations, but starts
with conservative semantic contracts. New helpers are not reconsidered in the
same invocation, and the pass is idempotent.

This isolates cold records rather than implementing general shrink wrapping.
The ordinary frame and unwind machinery handles each helper independently.
Custom pass lists need these passes explicitly; choosing Full includes them.
