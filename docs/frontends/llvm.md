# LLVM import semantics

This page states how LLVM's semantics map onto Keel IR contracts. The
[`keel-llvm` README](../../crates/keel-llvm/README.md) owns the supported
instruction and transport mapping, and [PORTING.md](../../crates/keel-llvm/PORTING.md)
the porting policy.

## Concrete operations and producer contracts

Keel's ordinary operations stay concrete: integer arithmetic wraps, shifts are
masked, conversions saturate and the documented division traps stay. No
operation adopts LLVM's poison semantics, and imported functions follow the
same rules as any other IR. Facts that LLVM attaches to values and calls become
explicit, scoped producer contracts instead.

A contract restricts valid executions at its stated boundary. It is not a
runtime check and does not promise a trap when violated. The parser and
verifier check structure, types and scope; the producer must justify the
promise. LLVM import adds a contract only when every defined source execution
reaching that boundary satisfies it. An annotation that can be poison is not
enough. When the importer cannot keep an annotation, it lowers the operation
conservatively and records the loss.

## Memory and call contracts

`FunctionAttrs.contract` and sparse call-site contracts apply to declarations,
definitions and individual call sites, including indirect calls. A fact about
one call is never promoted to all calls of the callee. They do not change ABI
signatures.

- Three memory regions each have an access set of `none`, `read`, `write` or
  `readwrite`: memory reached through pointer arguments, other memory
  accessible to the caller, and state that program pointers cannot reach.
  Defaults allow everything. LLVM's finer classes widen conservatively: errno
  joins accessible memory and target-private state joins inaccessible memory.
  LLVM's `memory` attribute is location-sensitive, so its write-only form does
  not forbid internal reads, as described in
  [LLVM memory attributes](https://llvm.org/docs/LangRef.html#memory).
- `nofree`, `nosync` and contextual `nocallback` are separate promises; the
  absence of writes establishes none of them. Import keeps `nocallback` only for
  external declarations and direct external calls.
- `preserves_access` promises that existing live objects keep their readable
  and writable accessibility on normal return. It is a Keel-only promise, never
  inferred from LLVM attributes and dropped on export, because no freeing does
  not exclude permission or mapping changes.

The effective effects of a call intersect the call-site contract and the
target's contract; an unknown contract adds nothing. Indirect calls use the
call-site contract only. Operand bundles are rejected. `MemoryEffects` still
classifies calls as barriers. The opt-in `call-load-reuse` pass keeps a
completed read across a call only when the contracts promise no accessible
writes, `nofree`, `nosync`, `nocallback` and `preserves_access`, and the call is
not returns-twice. The read and the call stay pinned.

Whole fixed-stack `lifetime_start(ptr)` and `lifetime_end(ptr)` survive import
and export as memory barriers. Native lowering emits nothing, and there is no
stack-slot reuse or lifetime-based DCE.

## Pointer facts

Pointer facts are independent: alignment, known non-null address, readable byte
extent and capture permissions. They bind to a parameter, return value or SSA
value and hold at a stated point. A readable extent is not validity after later
calls or lifetime changes, and readable bytes never imply writable bytes.

LLVM `nonnull` without `noundef` can be poison for a null argument, while
`dereferenceable(N)` implies definedness at its boundary
([parameter attributes](https://llvm.org/docs/LangRef.html#parameter-attributes)).
Import therefore keeps alignment and non-null facts only with matching
definedness, readable extents from dereferenceability, and conditional extents
from `dereferenceable_or_null`, which imply neither non-null nor unconditional
readability. Address and provenance capture are tracked separately for returned
copies and other escapes, and they constrain only the passed copy.

Definition-only normal-pointer `noalias` maps to `restrict_params`.
Declaration, call-site and aggregate positions do not. Return `noalias` becomes
`return_noalias`, which implies no non-null address or allocation size.
Inlining drops callee-scoped alias promises; whole-function clones keep them.
Access alias scopes and TBAA are not imported.

## Integer ranges

`range_param(index, lower, upper)` and `range_result(index, lower, upper)`
describe nonempty modular half-open intervals at i8, i16, i32 and i64 ABI
positions. Endpoints are unsigned bit patterns written in hex, so
`[0xfffffff8, 8)` covers -8 through 7 for i32. Import needs `range` and
`noundef` at the same position; poisonable ranges are omitted. Several
intervals at one position are a conjunction, and the LLVM exporter rejects
positions it cannot express with one range attribute. The importer reads range
attributes from LLVM's canonical signature text because the C API has no
ConstantRange getter. Entry-parameter removal clears positional facts.

<a id="llvm-contracts-instruction-local-result-facts"></a>
## Instruction-local result facts

`Function::inst_facts` and `set_inst_facts` store sparse `ValueFacts` for
exactly one result of an inserted instruction:

```text
v1 = load.ptr align 8 v0 facts(align 16, nonnull, readable 32)
v2 = load.i64 v1 facts(range(0x0, 0x80))
```

The access alignment describes the address the load reads; the fact alignment
describes the loaded pointer. Neither makes a read non-faulting. Facts hold
when the instruction executes and describe only its result. Readable extents
hold at the production point.

LLVM import keeps load metadata only with explicit `!noundef`: one `!range`
interval for i8 to i64, and pointer `!align`, `!nonnull`, `!dereferenceable`
and `!dereferenceable_or_null`. Multi-interval ranges are omitted as a whole,
and i1 masking is not a raw i8 fact. Export emits the metadata on the same
scalar load; a readable extent without non-null exports as a conditional
extent. Facts on other instruction kinds are diagnosed.

Whole-function clones keep the side table. Instruction clones start without
facts, and rewrites, removal, moves and block removal clear affected entries.
No optimizer consumes these facts.

## Definedness

Supported scalar `undef` and `poison` operands become zero (positive zero for
floating point, null for pointers), and scalar `freeze` keeps the concretized
operand. This is a deterministic refinement, not poison transport; see
[the refinement policy](../../crates/keel-llvm/PORTING.md#scalar-deferred-value-refinement).
Differential tests check allowed outcomes rather than LLVM's arbitrary choices.

LLVM defers poison, and `select` and PHIs can mask it
([LLVM's UB manual](https://llvm.org/docs/UndefinedBehavior.html)):

```llvm
%sum = add nsw i32 %a, %b
%result = select i1 %take, i32 %sum, i32 7
ret i32 %result
```

With `%take` false, signed overflow in `%sum` must not become an unconditional
no-overflow assumption or a trap. Imported `nsw`, `nuw`, `exact`, `disjoint`,
`samesign` and poison-producing pointer constraints are therefore not promoted
into proof results. Native analyses may still prove no overflow, bounded shifts
or nonzero divisors from constants, ranges and dominating conditions. Export
keeps Keel's trap obligations: masks, saturation and division checks disappear
only with a proof that valid Keel executions do not need them.

<a id="llvm-contracts-known-llvm-export-loss-ordinary-memory-faults"></a>
## Known LLVM-export loss: ordinary memory faults

Keel keeps potentially faulting accesses on executed paths even when their
results are unused. The LLVM exporter does not preserve every such fault, or
every memory effect before one:

```text
func @fault_probe(ptr) -> i64 {
block0(v0: ptr):
    v1 = load.i64 v0
    v2 = iconst.i64 0
    return v2
}
```

Native Keel faults when this is called with null. The exported module has an
ordinary unused load, which LLVM removes: at `-O0` instruction selection omits
it, and at `-O2` the IR drops it. `null_pointer_is_valid` does not change this.
A `volatile` load keeps the access, but LLVM may still reorder ordinary stores
around it, so the memory state before the fault can differ. Fault parity is
therefore reported separately from defined-memory execution parity, and
Keel→LLVM→Keel round trips are not semantically exact. Adding memory
attributes or treating ordinary accesses as non-faulting is not a fix.

Removing a path that cannot execute does not remove an observable fault, and
symbol emission and linker retention are separate decisions:

| Case | Keel behavior |
| --- | --- |
| Executed ordinary load with unused result | Kept unless a valid `notrap` promise permits deletion. |
| Executed volatile load with unused result | Kept, including with `notrap`. |
| Unreachable block containing a load or trap | Removed when reachability proves it cannot execute. |
| Unreferenced local function | May be removed when no emission root reaches it. |
| Public function with no local callers | Kept; external callers may exist. |
| Local function marked `used` | Emitted; the linker may still discard it. |
| Definition marked `retain` | Emitted with a request to survive linker garbage collection. |
| Definition in a custom section | Placement alone does not imply retention. |

The bridge rejects custom sections, `retain`, lifecycle registration and LLVM's
reserved retention globals. Function `used` exports as `llvm.compiler.used`,
but importing that output is unsupported. See
[section and retention rules](c/extensions.md#c-custom-sections-and-linker-retention).
