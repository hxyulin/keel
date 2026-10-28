# LLVM import contracts

Status: design decision for the next IR work. Except for the existing IR fields
and importer preservation fixes described below, the types and consumers in
this document are not implemented.

## Decision

Keep Keel's ordinary operations concrete: wrapping integer arithmetic, masked
shifts, saturating conversions and the documented division traps retain their
current meanings. Add explicit, scoped producer contracts for memory and values.
Do not change an ordinary Keel operation to LLVM's poison-producing semantics.
Do not introduce a global switch that makes imported functions obey different
arithmetic or pointer rules.

A contract restricts valid executions at its stated boundary. It is not a
runtime check, and its presence does not promise a trap when violated. This is
the same producer-obligation approach already used by Keel's definition-only
restrict contracts. Ordinary operation behavior stays unchanged within that
domain. The parser/verifier can check structure, types and scope; the producer
must justify the semantic promise.

LLVM import may add a contract only when every defined source execution reaching
that boundary satisfies it. A poison-producing source annotation is not enough.
The importer continues to lower supported operations conservatively when it
cannot retain an annotation. It must record that loss rather than label the
annotation as preserved.

The existing boolean representation stays unchanged. No new i1 type is needed
for this work.

## What already exists

`MemoryFlags` represents volatile, alignment and per-access fault freedom.
`FunctionAttrs` already represents unwind/return promises and several optimization
policies. `SourceLoc` and the debug structures represent source information.
`Function::restrict_params` represents a bounded definition-only modified-object
access contract. Preserve exact incoming facts in these fields before adding
new representations.

`MemoryEffects` currently classifies instructions, including a conservative
`Barrier` for calls. It is not a function effect summary. Replacing that barrier
with `None` after reading an LLVM attribute would change several optimizer
assumptions at once. Leave the existing classification conservative during the
first implementation.

## Memory representation

Add a typed function contract, separate from `Signature` and ABI classification.
The same contract vocabulary applies to declarations, definitions and individual
call sites, including indirect calls. A fact about one call is never promoted to
all calls of the callee.

Use a compact memory summary with independent read/write access sets for:

- Memory reached through pointer arguments.
- Other memory accessible to the caller, including globals and captured pointers.
- State inaccessible to ordinary program pointers.

An access set is `none`, `read`, `write` or `readwrite`. Defaults allow all
accesses in all regions. These sets describe externally relevant effects, not
literal absence of internal loads and stores. Unsupported LLVM location classes
must conservatively widen the affected summary, or be reported as unrepresented;
never interpret an unknown class as `none`.

LLVM's `memory` attribute is location-sensitive. Its write-only form does not
forbid internal reads after a write, and its no-memory form permits private
local accesses. The adapter must follow the pinned LLVM 23 contract rather than
copy assumptions from an attribute's spelling. See [LLVM memory attributes](https://llvm.org/docs/LangRef.html#memory).

Store non-memory behavior separately: unwind, normal return/divergence,
allocation/free of existing objects, synchronization and pointer capture.
The absence of writes alone establishes none of these properties. Start with
unknown for each unrepresented effect.

Keep summaries small and copyable. Store uncommon per-call/value contracts in
optional side tables keyed by stable IR entities, with absent entries meaning
unknown. Do not enlarge every arithmetic instruction or allocate metadata objects
for defaults. Analyses derive temporary facts lazily and invalidate them after
relevant edits; there is no runtime shadow state in this design.

A contextual resolver computes effective call effects from the call-site
contract and the target's applicable contract. Intersect allowed effects from
both trusted contracts; if one is unknown, retain the other. For indirect calls,
use only the call-site contract unless target resolution proves more. Body-derived
summaries require stable binding before use at external/interposable calls.
Operand bundles remain rejected until their effects have an explicit mapping.

### First consumer

Teach load reuse to keep a completed-read fact across a call only after proving
both that the call cannot modify its bytes and that the object's lifetime stays
valid. Unknown freeing, synchronization or callback behavior remains a barrier
unless the relevant analysis proves the fact survives. Read-only annotations
alone do not discharge every lifetime or control-effect obligation.

The call stays pinned. This change does not authorize call elimination,
common-subexpression elimination, duplication, speculation or motion. Each of
those requires a separate proof for observable effects, exceptional exits,
termination and repeatability. Roll out consumers individually; do not broadly
change `InstData::memory_effects()` as the first step.

## Pointer facts and aliasing

Represent pointer facts independently: alignment, known non-null address,
readable byte extent and capture permissions. Bind facts to the parameter,
return value or SSA value they describe, plus the point at which they hold.
Dereferenceability is not permanent validity after calls or lifetime changes.
Do not infer writable bytes from readable bytes.

LLVM `nonnull` without `noundef` can yield poison for a null argument. In contrast,
`dereferenceable(N)` implies definedness and only establishes accessibility at
the attribute boundary. This distinction controls import eligibility; see
[LLVM pointer attributes](https://llvm.org/docs/LangRef.html#parameter-attributes).

The first import subset is:

- Argument alignment/non-null facts when a matching definedness precondition
  establishes that they hold on every defined call.
- Positive, bounded readable extents from a supported dereferenceability contract,
  at entry or the call return point. No inferred function-wide `notrap` flag.
- The complete no-capture case. More detailed capture components remain unknown
  until the representation can distinguish address capture from access capability.

For offset pointers, a consumer must prove the byte range is inside the extent,
check arithmetic without overflow and retain the required alignment. It must
also prove that no lifetime boundary invalidated the fact. One-past pointers
remain useful addresses but are not automatically readable.

Parameter alias contracts use the modified-object access rule already supported
by `restrict_params`, only where the LLVM definition contract implies it for
normal pointer parameters. Prove this bounded mapping with overlapping read-only
and disjoint writable examples before enabling it. Do not copy declaration or
call-site facts into a definition-only list. Return-value noalias needs a
separate fresh-allocation relation; it is not a restrict parameter and does not
imply non-null or a known allocation size.

Keep these initial contracts at function/call scope. Inlining must drop
callee-scoped alias promises until there is a scoped remapping mechanism.
A whole-function clone can preserve them. Never turn alias contracts into
pointer inequality comparisons.

Defer access-specific alias scopes and TBAA until Keel can remap scopes and
maintain byte-range/type validity through transformations. Do not stash opaque
LLVM metadata and re-emit it as if it remained authoritative.

## LLVM definedness

Keep the current conservative refinement policy for supported LLVM input:
Keel must preserve allowed observations on LLVM-defined executions. Unsupported
explicit `undef`, `poison` and `freeze` remain rejected. Accepting a source flag
while omitting it is a recorded information loss, not proof that the value is
always defined.

LLVM has deferred poison in addition to immediate undefined behavior. `select`
and PHIs can mask a poisoned value. See [LLVM's UB manual](https://llvm.org/docs/UndefinedBehavior.html).
For example:

```llvm
%sum = add nsw i32 %a, %b
%result = select i1 %take, i32 %sum, i32 7
ret i32 %result
```

With `%take` false, signed overflow in `%sum` must not become an unconditional
Keel no-overflow assumption or an inserted trap. A plain return without a
`noundef` return contract is not itself a universal definedness boundary either.
Replacing `nsw` with wrapping arithmetic refines this example; treating `nsw`
as a function-wide input restriction does not.

### Which arithmetic facts can be used

Native Keel analyses may prove no overflow, bounded shifts, nonzero divisors and
other facts from constants, ranges and dominating conditions. Those are facts
about concrete Keel values and can be consumed and exported when their scope
matches the transformed operation.

Imported `nsw`/`nuw`, `exact`, `disjoint`, `samesign`, range annotations and
poison-producing pointer constraints are not promoted directly into these proof
results. A future importer definedness analysis must account for propagation,
selected arms, PHI edges, path reachability and source definedness boundaries.
It may promote a fact only at a point where a defined execution establishes it.
A check of an overflow flag establishes a useful fact only on the appropriate
success edge, not on all executions of the arithmetic instruction.

Do not implement runtime validity shadows, automatic guards or a generic LLVM
`assume` import in the first memory-contract change. They would add runtime work
or new semantic obligations without being needed for the initial memory wins.
If later coverage requires full poison transport, design it as explicit IR
semantics and audit every transform before enabling it. Do not encode it as a
metadata bit that arithmetic passes can ignore.

Preserve Keel's existing trap obligations on export. Extra masks, saturation
intrinsics and future division checks may disappear only with a concrete proof
that their behavior is unnecessary on valid Keel executions. An imported LLVM
flag that was dropped is not such a proof.

## Lifetime and debug follow-ups

Add explicit lifetime start/end events for fixed stack-object regions. Events
invalidate overlapping memory facts and separate restarts into distinct lifetime
epochs even when the numeric address is unchanged. Stack allocation/frame
existence is separate from object accessibility. Keep end events conservative
until the optimizer can prove slot reuse does not affect escaping references.
Do not hoist reads across start, end, stack restore or unknown freeing calls.

The initial LLVM adapter remains limited to recognized direct fixed-slot
lifetime markers. Importing arbitrary derived-pointer markers needs a separate
region proof. Dead-lifetime loads can be refined conservatively without
pretending their old contents are valid facts. No instruction should be removed
merely because it is near a lifetime marker.

Preserve source locations and inline ancestry using Keel's debug structures.
Treat full variable-location/debug-expression transport as a later coverage
item. Preserve branch weights, loop hints and function optimization policy as
hints with explicit clone/merge/drop rules. Hints never create memory or
arithmetic proofs. Debug locations are not alias scopes, and a source variable
lifetime is not a storage accessibility contract.

## Implementation order and acceptance tests

1. Preserve existing access alignment and exact function attributes. Check parsed
   Keel fields and round-trip output; ensure no fault-freedom or call-site-wide
   facts are invented.
2. Add function/call memory summaries and bounded pointer contracts, including
   parser/printer/verifier coverage. Keep all optimizer behavior conservative.
3. Enable load reuse across proven non-writing, lifetime-preserving calls. Test
   unknown and indirect calls, aliasing writes, callbacks, exceptional paths,
   freeing and legal read-only aliases as negative controls. The call must remain.
4. Add fixed-stack lifetime events and test restarts, overlap and escaped pointers
   before enabling stack-slot reuse.
5. Add concrete range/definedness proof consumers. Test masked overflow, both
   select arms, edge-sensitive PHIs, zero/oversized shifts, division overflow and
   noundef/non-null boundaries. An unused poisoned computation must not add a trap.
6. Extend debug/profile transport independently of semantic contract work.

For each feature, the conversion report should distinguish an imported semantic
contract, a proved fact, a retained hint, a dropped annotation and a rejection.
An execution match alone does not establish fact preservation. Add both a
structural assertion and a defined-domain execution test, plus a case that
would expose an unjustified stronger fact.
