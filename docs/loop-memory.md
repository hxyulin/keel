# Guarded loop memory motion: shape checkpoint

Status: source implementation checkpoint, 2026-10-17. The named
`loop-memory` pass is opt-in; the default pipeline is unchanged. Root
subsequently authorized the bounded initial-guard extension below after
the original design checkpoint found no campaign activation. Build and
native validation await an explicit serial lease. Source matching alone
is not an executed C activation result.

## Evidence inspected

The starting commit is `b0bb75c19e5c6c7c752deb14ff1ea26483386d7f`, tree
`7ed85d711280c9aa870fe1b36bd59cef5f7f2cea`; the isolated checkout was clean.
Read-only inspection covered the root's ignored campaign directory
`suite/reports/frontend-llvm-differential-20261017/`: `report.md`,
`data-review-r1/review.md`, retained `src/*.c`, and `build/*/scalar.keel`.
These are evidence references, not files to commit or regenerate here.

The actual retained scalar pipeline is documented in that report. All its
relevant memory loops are unrotated header/body loops: `hot`, `dependent`,
`bits`, `shortcopy`, `reload`, and `reload_restrict`. `tags` additionally has
conditional body control flow. `bitmix` retains the same header/body form.
`floor` has no relevant invariant-load loop. None demonstrates the approved
dedicated-preheader/single-block-self-loop subset.

In `build/reload_restrict/scalar.keel`, `block0` jumps to `block1` with the
initial induction value and accumulators. `block1` compares the induction
value to the runtime bound and branches to `block2` or `block4`. `block2`
contains the ordinary invariant `load.i64 v0`, a store through `v1`, and the
back edge to `block1`. `block4` returns. A zero bound reaches `block4`
without executing the load. Thus neither entry nor the ordinary preheader
is a legal destination for that potentially faulting load, even with valid
restrict contracts. The raw retained IR has an additional step block;
cleanup removes it but does not rotate the loop.

Source inspection of `crates/keel-c/src/irgen.rs` establishes that C
`DoWhile` emits a body and condition block, whereas `While` and `For`
emit a condition header before the body. In
`crates/keel-opt/src/simplify_cfg.rs`, merging a body and its sole-successor
condition block can produce a do-while self-loop. Jump-only guarded
preheaders can also be bypassed. This is a source-level possibility, not
an executed C-to-IR activation result. The existing C fixture
`constant_propagation.c` has a do-while without a load; `control.c` has
Duff's device, which is outside the proposed natural-loop subset.

## Approved subset and proof obligations

The initial subset requires an existing dedicated preheader that enters a
single-block counted self-loop unconditionally. Reaching that preheader
must guarantee the original candidate load executes. The address must be
defined outside the loop and dominate the insertion point. A precomputed
constant-offset address is acceptable; loop-local address definitions
cannot simply be moved with the load without an additional proof.

Only an ordinary invariant load before every observable or potentially
trapping effect in the first iteration is eligible. The whole loop must
exclude calls, volatile or atomic accesses, fences, lifetime effects,
complex exits, nested loops, and unknown barriers. Every modifying access
must be proved disjoint using the shared function-local alias helper.
Non-trapping annotations do not relax memory or lifetime dependencies.
Restrict proves neither dereferenceability nor nonnull. Debug locations,
operand dominance, PHIs, ABI and calling conventions must be preserved.
No store sinking is proposed.

The shared-helper checkpoint
`9792132a2bce6020b3be8c53045422320bce186a` was picked as `a8fb901`
after root coordination (not built or independently reviewed when
announced). The implementation uses
`AliasInfo::compute(&Function)` and
`may_alias(&self, &Function, Value, u64, Value, u64)` with nonzero widths
and at least one modifying access. Barriers remain the caller's
responsibility. The pass does not duplicate the helper's alias rules.

## Authorized bounded extension

For a realistic guarded C do-while, the smallest structural addition is
to split its existing entered-loop edge when cleanup has bypassed a
jump-only preheader. Reuse the existing guard and preserve its branch
arguments; do not invent or strengthen the guard. This may demonstrate a
useful C positive but does not address the measured for-loop kernels.

To handle `reload_restrict`, a separate narrowly scoped extension would
need to evaluate the first header's pure integer condition outside the
loop, retain the original zero-trip exit with exactly its original
arguments, and route only the entered case through a dedicated load
preheader. The existing loop can remain unrotated. Substitute initial
header arguments simultaneously, prove operand dominance and branch
equivalence, and reject effectful/trapping header prefixes or observable
effects before the first body load. Root authorized exactly this extension, with a bounded return-exit SSA
repair. The implementation accepts only a header with two predecessors
(one unconditional external jump and one body back edge), an integer
comparison involving a unit-step i32/i64 induction variable and an
invariant bound, at most 16 nontrapping integer header instructions, and a
single straight-line body of at most 128 instructions. The only cloned
operations are integer constants, nontrapping integer binary operations,
and integer comparisons. The original header test remains each iteration.
The body must have exactly one ordinary load, whose address already
dominates entry, before any store or other pinned/trapping effect. Later
ordinary stores must be disjoint according to `AliasInfo`; calls,
volatile/atomic/lifetime/barrier effects and other traps reject the loop.

The exit must have exactly one incoming edge from the header, end in a
return, and contain at most 32 instructions. Existing exit parameters and
arguments are retained. Header values directly used by exit instructions
get additional typed exit parameters, in deterministic operand traversal
order. The original header false edge supplies the original values; the
cloned initial false edge supplies initial arguments or cloned results.
Only exit instruction operands are rewritten. No global alias or use
replacement occurs. Header-defined uses elsewhere reject the loop. Exit
instruction order and effects are unchanged. Moving the load preserves
its result identity and location; cloned instructions keep their original
locations/scopes. Functions with stack-variable debug descriptions are
conservatively skipped.

Whole-function analysis is limited to 512 arena blocks and 8192 laid-out
instructions, with at most eight loop transformations. CFG, dominance,
loop depth and alias information are recomputed before each candidate.
Multiple loads, body block parameters, nested loops and complex exits are
outside this initial implementation.

## Remaining acceptance gates

First establish actual activation from a realistic C fixture through the
frontend and the intended cleanup pipeline. Then test explicit invocation
and subsequent default cleanup with verification after every pass; native
Clang comparisons at trips 0, 1, odd and many; zero-trip null/protected
addresses; conditional-body and fault-order fallbacks; aliasing,
restricted readonly aliases, unknown writes, calls, volatile/atomic,
barrier, multiple-exit and nested-loop rejection; and both codegen targets.

The dedicated C fixtures are `crates/keel-opt/tests/loop_memory/reload.c`
and `driver.c`. They cover the audited reload-style recurrence, aliasing
nonrestrict accesses, legal readonly restrict aliasing, conditional loads,
and a store-before-fault fallback. The driver includes zero-trip null and
protected-page inputs, valid trip counts 0/1/7/257/4096 and sixteen seeds.
Fault-order execution deliberately probes native fault behavior and is
not a valid-input C equivalence proof. Focused IR tests cover direct
header-value returns, multiple liveouts, both branch orientations,
locations, dominance, cleanup and conservative rejections.

No build, native execution, or performance measurement was performed for
this source checkpoint. No validation lease was acquired. Root owns independent
correctness review, integration, later serial runtime/code-size/compile-cost
gates, and any decision to enable a pass by default. Audited kernel-route
contrasts are not predicted patch speedups or application gains.
