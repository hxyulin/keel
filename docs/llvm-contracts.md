# LLVM import contracts

Status: memory, richer boundary pointer/capture contracts, defined integer ranges,
scalar load-result facts and fixed-stack lifetimes are represented. New fact
consumers, broader metadata and storage reuse remain deferred.

## Implemented slice

- `FunctionAttrs.contract` and sparse call-site contracts represent three memory
  regions, `nofree`, `nosync`, contextual `nocallback`, explicit accessibility
  preservation, and bounded pointer facts.
  Text, verification, composition and conservative mutation invalidation are
  implemented. These contracts do not change ABI signatures.
- LLVM 23 import transports supported contracts at their original scope.
  Non-null/alignment facts need definedness; readable extents are boundary facts.
  Definition-only normal-pointer noalias maps to `restrict_params`. Declaration,
  call-site and return noalias are not promoted into that list.
- Pointer contracts also carry conditional readable extents, independent address
  and provenance capture permissions at return/other escape locations, and
  allocation-like return noalias. No non-null address, size, writable bytes or
  access stability follows from return noalias.
- Integer `parameter_ranges` and `return_ranges` apply at function/call boundaries
  only. LLVM import requires range and noundef at the same owner and position.
  Poisonable ranges are omitted. No new optimizer consumes these richer facts.
- Sparse `ValueFacts` attach one integer interval or pointer facts to one
  instruction result. LLVM load metadata is retained only with explicit noundef.
  Result-pointer alignment is separate from access alignment; no access becomes
  notrap. Mutating or moving instructions drops their result facts.
- More detailed LLVM memory classes are conservatively coarsened: errno joins
  other accessible memory; target-private state joins inaccessible memory.
  LLVM export uses a widened default compatible with Clang 21, so some region
  precision may be lost. A native readable-without-nonnull fact exports through
  `dereferenceable_or_null`; its unconditional readable extent is not recovered
  by the initial importer.
- LLVM `nocallback` has contextual module semantics. Initial import retains it
  for external declarations and direct external call sites; local definitions
  and unresolved indirect sites omit it. No all-callbacks prohibition is inferred.
- The opt-in `call-load-reuse` pass now consumes scoped contracts alongside its
  existing stable-body proofs. Contract-based reuse requires no accessible
  writes plus `nofree`, `nosync`, `nocallback` and `preserves_access`, and excludes
  returns-twice.
  First reads and calls remain pinned. Default pipelines are unchanged.
  `preserves_access` excludes permission/mapping changes to live objects. It is
  not inferred from LLVM attributes; LLVM export drops this Keel-only promise.
- Whole-fixed-stack `lifetime_start(ptr)` / `lifetime_end(ptr)` intrinsics survive
  LLVM import/export and remain memory barriers. The verifier requires a direct
  fixed-stack address. Native lowering emits no instructions; Cranelift uses
  `sequence_point`. There is no stack-slot reuse or lifetime-based DCE yet.
- Execution regressions cover masked overflow, non-null and oversized-shift
  poison, PHI-edge masking and division overflow totalization. No imported
  poison flag has become an unconditional arithmetic assumption.

The remaining sections state the design and the conditions for further consumers;
they do not claim all listed future analyses or debug/profile transport exist.

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

For ordinary Keel reads, no freeing is not a complete access-validity proof.
A single-threaded call can change page protections without changing bytes or
freeing an allocation. Keel preserves the subsequent read fault. The initial
consumer therefore also requires `preserves_access`: existing live objects keep
their prior readable/writable accessibility on normal return. This is independent
of content effects. It is unknown on LLVM import and has no direct LLVM export
mapping. Stable body proofs remain another way to establish safety. A native
permission-change regression checks that absent this promise, the second read
remains and still faults. Do not silently strengthen LLVM's nofree annotation.

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
- Address/nullness capture and read/write provenance capture, independently for
  returned copies and other escapes. These constrain the particular passed copy,
  not every alias of the original pointer. The existing `nocapture` spelling is
  retained as shorthand for no components captured anywhere.
- Conditional readable extents from `dereferenceable_or_null` with independently
  established definedness. They do not imply a non-null address or unconditional
  readable bytes.

For offset pointers, a consumer must prove the byte range is inside the extent,
check arithmetic without overflow and retain the required alignment. It must
also prove that no lifetime boundary invalidated the fact. One-past pointers
remain useful addresses but are not automatically readable.

Parameter alias contracts use the modified-object access rule already supported
by `restrict_params`, only where the LLVM definition contract implies it for
normal pointer parameters. Prove this bounded mapping with overlapping read-only
and disjoint writable examples before enabling it. Do not copy declaration or
call-site facts into a definition-only list. Return-value noalias needs a
separate allocation-like relation, now represented as `return_noalias`; it is not
a restrict parameter and does not imply non-null or a known allocation size.

Keep these initial contracts at function/call scope. Inlining must drop
callee-scoped alias promises until there is a scoped remapping mechanism.
A whole-function clone can preserve them. Never turn alias contracts into
pointer inequality comparisons.

Defer access-specific alias scopes and TBAA until Keel can remap scopes and
maintain byte-range/type validity through transformations. Do not stash opaque
LLVM metadata and re-emit it as if it remained authoritative.

### Scoped integer ranges

`range_param(index, lower, upper)` and `range_result(index, lower, upper)` describe
nonempty modular half-open intervals at i8/i16/i32/i64 ABI positions. Endpoints
are unsigned bit patterns; text uses hex so wrapped intervals are unambiguous.
For example `[0xfffffff8, 8)` includes negative eight through positive seven for
i32. These do not become facts about the argument on paths that bypass a call,
or about unrelated SSA values. Poison-producing ranges without noundef cannot
be promoted, including on returns later masked by select or freeze.

Simultaneous intervals at one position are a conjunction. Composition retains
every constraint rather than approximating a disconnected intersection with one
interval. The diagnostic LLVM exporter currently rejects multiple conjunctive
intervals at a position; it does not silently drop one. Export of a single
interval emits noundef plus range. Empty LLVM ranges are conservatively omitted;
i1, i128, vectors and assume/guard facts are outside this boundary slice.
Existing i1-to-i8 representation is unchanged. Load metadata is handled separately
below.

The [LLVM C API](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h)
has no ConstantRange-attribute getter. The importer reads only the
eligible position from LLVM's verified canonical signature, skipping quoted
names and nested argument attributes. It never calls the integer-attribute API
on a ConstantRange attribute. The [capture bit encoding](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm/Support/ModRef.h)
is checked against LLVM 23
using every combination of supported components and locations.

Whole-function clones retain contracts; instruction clones drop call-site facts.
Mutation invalidation applies to the complete contract. Entry-parameter removal
clears positional parameter facts, and byval splitting skips signatures with
these facts until it can remap them. Cranelift export does not transport the new
contracts. No alias/value consumer, pointer validity inference, speculation or
dead-load elimination is added by this slice.

### Instruction-local result facts

`Function::inst_facts` and `set_inst_facts` store sparse `ValueFacts` for exactly
one result of an inserted instruction. For example:

```text
v1 = load.ptr align 8 v0 facts(align 16, nonnull, readable 32)
v2 = load.i64 v1 facts(range(0x0, 0x80))
```

The first alignment describes the cell address used by the load. The result
alignment describes the loaded pointer. Neither the cell nor a subsequent read
becomes non-faulting. Facts apply when the instruction executes. They do not
establish that the instruction executes on another path, nor describe a sibling
load or an input parameter. Readable extents hold at this result's production
point; they are not permanent validity after calls or lifetime changes.

LLVM import reads load metadata through the C API. Explicit `!noundef` is required
for this first profile, including pointer extents. A single `!range` interval is
retained for i8/i16/i32/i64. Pointer `!align`, `!nonnull`, `!dereferenceable` and
`!dereferenceable_or_null` are retained within the existing alignment/extent
bounds. Positive unconditional readability also establishes non-null in the
supported LLVM address-space-zero profile. Poisonable metadata without explicit
noundef is omitted, not asserted as a concrete fact. Multi-interval union ranges
are omitted as a whole; keeping just one member would incorrectly strengthen
the source promise. i1 result masking is not annotated as a raw i8 load fact.

Export emits metadata on the same scalar load, including noundef, with module-wide
metadata node interning. A native readable extent without a non-null promise is
conservatively exported as a conditional extent, as with boundary facts. This
loses unconditional readability precision and does not invent non-nullness.
Facts on other instruction kinds diagnose instead of being silently dropped.
Call attributes remain in their separate call contracts; call-result metadata,
range unions, assume/guard and path-conditioned facts remain deferred.

Whole-function clones retain the side table. Instruction-data clones start without
facts. Instruction rewrites, removal, single/bulk instruction moves and block
removal clear affected entries. Mutable direct-callee access also clears result
facts on the affected calls. The verifier checks types, arity and bounds; the
producer must justify each promise. Consumers must additionally track dominance,
path conditions, memory changes and lifetime invalidation. No optimizer consumes
these result facts yet, and this transport does not repair the LLVM fault gap.

The load-fact harness fixture executes through original LLVM, native Keel,
re-exported LLVM and the Cranelift backend. The LLVM round trip retains the
fact. Completing the CLIF round trip is currently rejected by the scalar CLIF
importer's existing lack of load support; Cranelift does not transport the fact.

## LLVM definedness

Keep the current conservative refinement policy for supported LLVM input:
Keel must preserve allowed observations on LLVM-defined executions. Supported
scalar `undef` and `poison` operands choose zero; scalar `freeze` keeps the
concretized operand. This is a deterministic refinement, not full poison or
nondeterminism transport. Aggregates and globals retain their existing
restrictions. Accepting a source flag
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
The scalar refinement does not preserve the set of all possible source results.
Differential drivers must check allowed outcomes and freeze consistency instead
of comparing arbitrary LLVM choices byte for byte. Concrete inputs to freeze
retain their original bits, including floating-point signed zero and NaN payloads.

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

## Known LLVM-export loss: ordinary memory faults

The current LLVM exporter does not preserve every observable Keel memory fault.
This is distinct from poison handling on LLVM import. For example:

```text
func @fault_probe(ptr) -> i64 {
block0(v0: ptr):
    v1 = load.i64 v0
    v2 = iconst.i64 0
    return v2
}
```

A native Keel object retains the potentially faulting read. The exported LLVM
module contains an ordinary unused load, which LLVM O2 removes. In the observed
Apple arm64 run, a C caller passing null terminated with SIGSEGV through native
Keel and returned 0 through optimized LLVM. This was reproduced during contract
validation. The new native `preserves_access` consumer checks do not repair this
independent bridge loss.

Fault parity must therefore be reported separately from defined-memory execution
parity. Do not label arbitrary Keel→LLVM→Keel round trips semantically exact.
A follow-up needs an explicit fault-preserving export strategy or an enforced
export profile that excludes observable memory faults. Adding memory attributes
or quietly treating ordinary accesses as non-faulting is not a fix. Making all
accesses volatile would need its own semantic and optimization tradeoff review;
it is not part of this contract implementation.

The [fault and dead-code investigation](llvm-faults-and-dead-code.md) reproduces
the loss at LLVM O0 as well as O2, distinguishes instruction deletion from
function/section collection, and shows why volatile access preservation alone
does not preserve the memory state before a fault.

## Validation of the implemented slice

The affected IR, optimizer, LLVM and native-codegen crate suites passed on Apple
arm64 with LLVM 23 supplied through `KEEL_TEST_LIBLLVM` and a rebuilt Keel supplied
through `KEEL_TEST_KEEL`. After adding access-stability handling, the full IR,
optimizer and LLVM suites passed again, including the native page-protection
case. The native lifetime no-instruction check also passed on the final model.
The isolated Cranelift suite, intrinsic registry generation check, registry tests
and parity normalization tests passed. The policy suite passed 94 Rust/Python
comparisons; the base/flags/calls/memory validators passed 164 comparisons.

Targeted LLVM Clippy passes with warnings denied. Broader Clippy is blocked by
existing repository lints, including the existing `almost_swapped` test in
`keel-ir/tests/call_operand_abi.rs`; no lint suppression was added. Documentation
builds complete with existing warnings. These checks establish no runtime speedup
claim and do not cover the memory-fault export gap described above.
