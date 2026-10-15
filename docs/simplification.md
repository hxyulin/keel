# Simplification coverage and phase ordering

Keel needs both broader rewrite coverage and additional analyses. Repeating the
current passes cannot infer facts they do not represent. Conversely, adding a
rewrite does not help if an earlier/later pass is the only one that exposes its
preconditions.

## Coverage audit

| Area | Current support | Substantial missing work |
|---|---|---|
| Constant/identity simplification | Constant evaluation, integer identities, elementary cast cancellation, constant selects, checked arithmetic, power-of-two reductions | Broader Boolean/algebraic identities, cast chains, integer reassociation and cancellation with canonical forms |
| Integer facts | `known-bits` now proves zero/one bits through bit operations, shifts, casts, selects and block arguments; derives bounds for comparisons | Carry-aware arithmetic facts, richer intervals, relationships between values, facts conditional on a taken edge |
| Constant propagation | SCCP follows executable edges and propagates constants through parameters | Combining executable-edge information with nonconstant bit/range facts |
| CFG | Constant branches/switches, unreachable removal, forwarding and block merging | Branch threading using path facts, richer switch simplification, additional merge/selection forms |
| SSA parameters | Simple uniform/dominating and anchored self-loop parameters via opt-in `phi-simplify` | General mutually recursive redundant parameter components |
| Memory | Exact-address local forwarding, explicit access/effect contracts | Escape-aware stack promotion, aggregate scalar replacement, alias-disambiguated forwarding and dead stores |
| Normal forms | GVN normalizes commutative keys and reversed comparisons | Consistent emitted-IR operand forms that all consumers can rely on |

These are not all profitability transformations. Proving a comparison constant,
removing a redundant mask, or replacing a trivial parameter simplifies the
representation. Inlining/unrolling/code-motion choices need additional
profitability policy. They should not prevent improving scalar reasoning first.

## Known-bit reasoning

The opt-in `known-bits` pass stores disjoint known-zero and known-one masks for
each integer value, from i8 through i128. Unknown is the initial state. A
consumer worklist propagates monotonically until no additional bit is proved.
Incoming facts for a block parameter intersect **all** incoming edges. This
includes distinct edges from one terminator and loop backedges. No unreachable
edge is ignored until CFG cleanup removes it. Entry parameters remain unknown;
SCCP aliases are resolved without treating aliased parameters as definitions.

Supported transfers include AND/OR/XOR/NOT, shifts with known effective counts,
sign/zero extension, truncation, selects, and the 0-or-1 range of comparisons and
overflow flags. Bounds from the masks can prove integer comparisons, including
signed ordering without assuming no overflow. AND/OR identities remove masks or
tags already established by their operands. Fully determined values materialize
only when the IR immediate can represent the result; a general i128 value is
not truncated to i64. Loads, stores and potentially trapping operations are not
removed by this pass. Strict floating-point algebra is unchanged.

The tests exhaust every four-bit abstract comparison against its concrete
values and exercise all i8 inputs/pairs for casts, masks and shifts. They also
cover signed/unsigned predicates, loop merges, multiple edges, reversed layout,
SCCP aliases, i128 representation, memory/trap preservation and 64 subsets of
other passes. A native fixture runs through both selectors and allocators with
restricted registers.

## Why ordering matters

Three independent interactions have concrete regression tests:

1. **GVN then combining:** after GVN merges `x+y` and `y+x`, a comparison of
   their results becomes `a == a`. A combiner that already ran cannot see that
   equivalence. A later combining pass folds it, then CFG cleanup removes the
   branch.
2. **Facts then CFG then combining:** proving `(x & 15) < 16` removes an edge.
   The surviving join argument can then become constant. A comparison using
   that argument needs another scalar cleanup after CFG changes.
3. **DCE then CFG:** deleting dead block parameters can make previously
   different branch arguments identical, enabling forwarding or branch cleanup.

Local worklists inside `instcombine` and `known-bits` already revisit dependent
values. CFG cleanup itself runs to convergence. Re-running these passes globally
is useful only when a different pass changed the facts or structure they see.
The current pass interface reports only a boolean, so it cannot yet pass an
accurate affected-value/edge worklist between analyses.

A natural phase group is scalar reasoning, equivalence discovery, branch/phi
cleanup, and DCE, with another round when one stage exposes useful work for a
previous one. Measure the number of productive rounds and their cost. Future
conditional repetition should preserve verifier checks and use change summaries
or an explicit bound. Do not run the entire optimizer indiscriminately to a
fixed point: GCM deliberately rematerializes constants near uses, while GVN
merges them. Repeating those two can undo useful placement and lengthen live
ranges. Code motion remains at the end of the experiments here.

## Sweep

The [ordering report](../suite/reports/simplification-order-aarch64-2026-10-15.json)
compares the existing Full pipeline with four alternatives on identical
preprocessed Lua, QuickJS and SQLite sources, using Keel's assembler:

- `cleanup`: another instcombine and CFG cleanup after GVN.
- `bits-early`: known-bit reasoning before the existing combining/CFG sequence.
- `bits-late`: known-bit reasoning after GVN, followed by combining/CFG cleanup.
- `bits-repeat`: a second equivalence/known-bits/combining/CFG/DCE round.

All four correctness presets pass. Results relative to the existing Full
pipeline (geometric means for time, aggregate text delta):

| Recipe | Runtime ratio | Compile-time ratio | Text delta | IR instructions removed |
|---|---:|---:|---:|---:|
| cleanup | 1.0037 | 1.0611 | +12,296 bytes | 2,716 |
| bits-early | 1.0042 | 1.0382 | -864 bytes | 151 |
| bits-late | 1.0069 | 1.0932 | +10,492 bytes | 3,045 |
| bits-repeat | 1.0020 | 1.2407 | +9,656 bytes | 3,167 |

The repeated recipe removes 2,737 blocks. A third round yields byte-identical
printed IR on all 44 source files; this is evidence of convergence on this
corpus, not a bound for arbitrary inputs. None of these recipes establishes an
aggregate runtime gain. More simplified IR does not itself prove cheaper
machine code: CFG/layout, placement and allocation still affect the output.
The cause of the text growth has not been isolated. Known-bit reasoning and
these recipes remain opt-in; the default pipeline is unchanged. The next
canonical operand-placement change is measured separately.
