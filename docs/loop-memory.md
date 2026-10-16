# Guarded loop memory motion: shape checkpoint

Status: design checkpoint only, 2026-10-17. No `loop-memory` pass is
implemented or registered, and the default pipeline is unchanged. The
approved initial subset has no demonstrated activation on the inspected
retained C-generated IR. Building an inactive pass would not satisfy the
required realistic C activation gate.

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

The committed shared-helper checkpoint is
`9792132a2bce6020b3be8c53045422320bce186a` (not built or independently
reviewed when announced). Before implementation depends on it, coordinate
pickup with root and cherry-pick that committed checkpoint. Use
`AliasInfo::compute(&Function)` and
`may_alias(&self, &Function, Value, u64, Value, u64)` with nonzero widths
and at least one modifying access. Barriers remain the caller's
responsibility. This design checkpoint neither copies nor duplicates the
helper's alias rules.

## Bounded extensions for root to consider

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
effects before the first body load. This is a recommendation requiring
root scope coordination, not authorization to broaden the initial pass.

## Remaining acceptance gates

First establish actual activation from a realistic C fixture through the
frontend and the intended cleanup pipeline. Then test explicit invocation
and subsequent default cleanup with verification after every pass; native
Clang comparisons at trips 0, 1, odd and many; zero-trip null/protected
addresses; conditional-body and fault-order fallbacks; aliasing,
restricted readonly aliases, unknown writes, calls, volatile/atomic,
barrier, multiple-exit and nested-loop rejection; and both codegen targets.

No build, native execution, or performance measurement was performed for
this checkpoint. No validation lease was acquired. Root owns independent
correctness review, integration, later serial runtime/code-size/compile-cost
gates, and any decision to enable a pass by default. Audited kernel-route
contrasts are not predicted patch speedups or application gains.
