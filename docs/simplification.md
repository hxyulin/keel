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
| Normal forms | Explicit initial canonicalization, shared local normalization and declared consumer entry requirements; integer literals last, deterministic interchangeable operand order, reversed comparison predicates | Boolean/cast/associative expression forms, precise invalidation to reduce restoration scans |

These are not all profitability transformations. Proving a comparison constant,
removing a redundant mask, or replacing a trivial parameter simplifies the
representation. Inlining/unrolling/code-motion choices need additional
profitability policy. They should not prevent improving scalar reasoning first.

## Canonicalization contract

Basic and Full begin with `canonicalize`. It performs no constant evaluation,
reassociation, instruction creation/removal, or motion. Its structural form is:

- Every instruction operand directly names its resolved SSA value. Aliases in
  value tables and block parameter definitions can still exist.
- For commutative integer binary operations (`iadd`, `imul`, `and`, `or`, `xor`,
  `umulhi`, `smulhi`) and checked add/multiply (signed and unsigned), operands
  sort by `(is_integer_literal, value_id)`. A single integer literal therefore
  appears on the right. Two nonliterals or two literals sort by SSA value ID;
  separately defined equal literals need not share an ID.
- Integer comparisons use the same ordering and reverse the predicate on a
  swap: for example `icmp.slt 37, x` becomes `icmp.sgt x, 37`. Signedness and
  equality semantics are preserved.
- Floating-point operand order, noncommutative integer operand order, select
  arms, edge order, memory effects and trapping behavior are preserved.

This is a structural contract, not a promise that equivalent arithmetic trees
are identical: `(a+b)+c`, `a+(b+c)`, and duplicated constants still need other
passes. Raw frontend IR remains valid without canonicalization, and the None
optimization tier runs no passes.

A leading pass alone cannot establish a permanent invariant. CFG cleanup,
SCCP, GVN and memory forwarding can record lazy aliases; folding can turn an
existing value into a literal. Either can invalidate the ordering of users
that the transforming pass did not touch.

`Pass::requires_canonical_form()` declares an entry requirement. The manager
tracks whether the form is established and restores it immediately before a
requesting pass when necessary. Any pass reporting a change invalidates the
form conservatively unless `produces_canonical_form()` promises it on exit.
A pass reporting no change must not mutate the function. Restoration is
profiled and verified as canonicalization, including in explicit/custom
pipelines. `instcombine` is the first consumer and also uses the same payload
normalizer within its worklist as new facts appear during its execution.
Direct calls to `Pass::run` bypass manager prerequisites; callers needing the
entry contract should use `run_pipeline` or `run_passes`.

The guarantee is **on entry to a requesting pass**, not at every point during
its rewrites or on exit from the entire optimizer. GVN retains independent
key normalization, including its existing FP equivalence policy. GCM remains
last and can rematerialize constants. Future consumers should declare their
requirement rather than depend on a particular preceding pass name.

Tests cover all integer widths, every signed/unsigned comparison predicate,
both result identities of checked operations, literal precedence over value
IDs, idempotence, preserved instruction counts, memory/FP ordering, and
restoration after a custom pass exposes a literal through an alias. The native
fixture covers signed and unsigned comparisons with literals initially on the
left through both selectors and allocators under restricted registers.

The [stage comparison](../suite/reports/canonical-stage-aarch64-2026-10-15.json)
measures the previous Full compiler against this contract and initial stage on
the same 44 preprocessed files with Keel assembly (7 paired runtime samples,
5 paired compile samples). The candidate release binary after final formatting
and lint cleanup is byte-identical to the measured binary.

| Metric | Before → after / ratio |
|---|---:|
| Literal-left integer comparisons | 297 → 0 |
| Static IR instructions / blocks | unchanged |
| Aggregate text | −1,056 bytes |
| Runtime geometric mean | 0.9923 (−0.77%) |
| Compile-time geometric mean | 1.0768 (+7.68%) |

Seven of eight workload confidence intervals include no runtime change; this
is not an established aggregate application speedup. The stage is enabled as
the requested structural foundation. Conservative whole-function restoration
and verification have a measurable compile cost; attributing that cost to
individual components needs profiling. More precise change information could
avoid restoration when a pass changes unrelated instructions. The final
SQLite IR has one literal-left commutative binary operation, consistent with
the contract applying at consumer entry rather than optimizer exit.

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
these recipes remain opt-in. This sweep predates the canonicalization changes,
which are measured separately below.


## Integer operand canonicalization

The original isolated change made `instcombine` put integer literals on the right of
commutative binary operations, sharing GVN's commutativity classification.
Noncommutative operations and floating-point operand order are unchanged.
That version normalized inside combining without an extra pipeline stage. It avoided
requiring every consumer to handle both `constant + x` and `x + constant`.
For example, the existing AArch64 emitter folds its right operand into an
immediate: canonicalizing `37 + x` removes a materialized constant register.

The [isolated comparison](../suite/reports/canonical-constants-aarch64-2026-10-15.json)
uses saved pre-change and candidate compilers with identical Full pipelines and
Keel assembly. It reduces literal-left cases from 518 to zero across the corpus,
with exactly the same static IR instruction and block counts. Generated text
shrinks by 80 bytes in Lua, 448 in QuickJS and 728 in SQLite (1,256 total).
The runtime ratio is 1.0019 and compile-time ratio 0.9989, both essentially
neutral; no application speedup is claimed. The form now belongs to the shared
normalizer and explicit stage described above. Known-bit analysis and the extra
cleanup recipes remain opt-in.

This illustrates two different needs: fact analysis discovers missing proofs,
while canonicalization makes existing rules and lowering easier to apply.
Repeated passes cannot substitute for either one. The next simplification
families are Boolean absorption/cancellation, broader integer cast chains,
integer algebraic cancellation, and conditional range facts for CFG cleanup.
