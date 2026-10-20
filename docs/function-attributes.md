# Function attributes

`FunctionAttrs` carries `noninterposable`, `inlinehint`, `alwaysinline`,
`noinline`, `noreturn`, and `nounwind` on IR definitions, external declarations, and each direct
callee record. These are separate from `Signature`: they do not participate
in ABI or C function type compatibility. `returns_twice` retains its existing
signature representation and behavior.

## Text and consistency

Attributes follow the signature (including `cc(c)` and `returns_twice`) and
precede `local` and `personality`. For example:

```text
decl @stop() cc(c) noreturn nounwind
func @helper(i32) -> i32 cc(c) inlinehint alwaysinline local {
block0(v0: i32):
    return v0
}
```

The printer uses the order `noninterposable inlinehint alwaysinline noinline noreturn nounwind`.
The parser rejects duplicate and malformed tokens. The verifier rejects
`alwaysinline` with `noinline`, and `noreturn` with `returns_twice`, on declarations,
definitions and callee records. It requires direct callee attributes to agree
with the module declaration/definition. `Module::sync_function_attrs` refreshes
these records after construction or deliberate module contract edits; it does
not resolve conflicting policies. Function imports default to no promises;
programmatic builders must explicitly set/synchronize attributes. Inlining
preserves imported callee contracts.

Verification checks representation and consistency, not arbitrary semantic
contracts. A `return` in a `noreturn` body is accepted, just as an external
contract cannot be proved here. The `call-contracts` optimizer pass consumes
explicit direct contracts as described below. It never infers them from function
bodies. A contradictory body is a producer contract violation,
not a proof result or an excuse to change existing runtime behavior.

## C declarations and contracts

C `inline` adds `inlinehint`. Inline-only emission is resolved after all
file-scope declarations: a later file-scope `extern` or plain non-inline
declaration makes the definition external, just as an earlier one does. Explicit `static`
linkage is tracked independently and remains private. A definition declared only
`inline` retains the existing private-copy emission behavior. Block-scope
function declarations merge visibility/contracts but do not request a public
definition. Inline policy does not change this determination; an externally emitted definition cannot pass the
ordinary inliner's private-only boundary. The explicit binding-aware integration
is described below.

GNU `always_inline`, `noinline`, `noreturn`, `nothrow`
(including double-underscore spellings), and C `_Noreturn` map to their IR
counterparts. `nothrow` means the function promises no exception unwinding;
it supplies no C++ termination semantics. No C++ `noexcept` syntax is added.
Recognized GNU function attributes take no arguments.

Redeclarations and the definition accumulate attributes by boolean union,
independent of their order. Missing attributes never clear earlier ones.
Explicit `always_inline` and `noinline` conflict and produce a diagnostic,
even on separate declarations. Ordinary `inline` with `noinline` is permitted;
`noinline` wins. `noreturn` with `returns_twice` is diagnosed. Existing special
`setjmp`-family/`vfork` handling remains in place.

`noreturn` and `nounwind` are independent promises. Neither implies the other,
and C syntax alone supplies no `nounwind`. A direct C `noreturn` call retains
its existing following `trap`: if the callee unexpectedly returns, native
execution still traps. This includes inlined bodies. Debug locations/scopes
retain the inliner's existing lexical-subtree treatment.

Contracts apply to the named direct function, including declarations without
bodies and named functions returning pointers. Attribute ownership is specific
to the attribute category. Inline policies, `returns_twice`, and GNU `nothrow`
are declaration attributes in the supported placements: they accumulate through
intermediate declarator layers until the complete declarator identifies the
named function. This includes factory declarations and attributes inside an
already supported array suffix. Pointer objects, typedefs and parameters do not
acquire indirect-call contracts.

GNU `noreturn` can instead qualify a function type. For this promise, declarator
parsing keeps the function owner of each layer: an incoming enclosing function
owns it; otherwise its suffix (or the nested declarator of a pure grouping
layer) supplies the owner. Only a promise whose owner is the declared function
survives into named metadata. Function-node identity is independent of ABI/type
equality. Layout attributes and pointer qualifiers retain their handling
independently of either contract category.

For example, `int * __attribute__((noinline)) h(void)` and
`void (* __attribute__((noinline)) h(void))(void)` both retain `noinline` on `h`.
In contrast, `void (* __attribute__((noreturn)) get(void))(void)` and
`void (__attribute__((noreturn)) (*get(void)))(void)` assign `noreturn` to the
returned function, so neither promises that `get` cannot return. If that same
attribute list contains `returns_twice`, that declaration attribute still applies
to `get`; the two promises have different owners. Unsupported pointer-type
promises remain unrepresented. Declaration-specifier and final named-declarator
attributes retain their direct-function handling, and conflicting named policies
are diagnosed across placements and redeclarations.

Declarator-position contracts on a function-pointer typedef base remain diagnosed
when the hidden function makes ownership ambiguous; declaration-specifier/final
named-declarator placements and unambiguous grouping remain available. This is
bounded ownership support, not full GNU attribute-placement or C++ grammar support.
The classification follows [Clang's attribute definitions](https://github.com/llvm/llvm-project/blob/main/clang/include/clang/Basic/Attr.td)
and retained per-attribute Clang AST evidence, detailed in
the ROUND3 correction record.

Indirect call signatures carry none of these new attributes. Function
pointer/declarator attributes do not establish indirect-call promises; no
contract propagates through pointer types, casts, matching ABI types, or a
possible target's name. Indirect calls keep their continuations. A direct
`nounwind` invoke may be lowered to a call and its normal branch,
subject to the representation safeguards below.

## Optimizer consumers

`call-contracts` runs in Basic and Full, and is available as an explicit pass.
None remains unchanged. Basic and Full run it before their second CFG cleanup,
which removes newly redundant joins. This inexpensive contract/CFG cleanup does
not require the Full tier's analyses. In Full this is after inlining, so imported
direct calls receive the same treatment. The existing private-only inliner, its
input pipeline, policy and tier placement are unchanged. The pass manager still
verifies after every pass.

A direct `noreturn` **call** retains the call and terminates its normal continuation
with a defensive `trap`. An existing following trap and its debug location are
preserved. Otherwise the new trap receives the call location. The suffix and
blocks made unreachable are removed together only if no surviving operand loses
its resolved SSA definition. Effects before the call, the call itself, and its
implicit unwinding behavior remain. A `noreturn` **invoke** alone is unchanged:
there is no new terminator or representation for retaining only its unwind edge.

A direct `nounwind` **invoke** becomes a call with the same instruction handle,
results, arguments and debug location, followed by a jump to the original normal
destination with the invoke's location. Shared landing pads needed by other
invokes remain. `nounwind` alone preserves normal return, including
`returns_twice`; with `noreturn` the resulting call also receives the defensive
trap treatment. The caller's personality, ABI and debug scopes/variables remain.
No indirect-call promises or memory/alias promises are introduced.

Conservative representation limits: the verifier currently permits an invoke
result in a shared normal destination that is not dominated by the invoke's
source block. Such a used result cannot become a call result, so that lowering
is skipped. A cut is also skipped when it would delete an SSA definition used
on a surviving path, or the last matching catch clause needed by a surviving
`eh_typeid`. The pass does not synthesize replacement values, move effects or
invent catch clauses. Preexisting unreachable sites are not independently rewritten.
The frontend's declarator ownership field for GNU `noreturn` remains separate
until ownership is resolved; named globals and trap emission use `FunctionAttrs`
as their single source of truth.

## Inliner policy and hard boundaries

The ordinary `inline` pass expands only private, nonrecursive definitions and
supported direct `call` sites. Public/interposable definitions, missing
definitions, variadics,
`returns_twice`, exception-handling bodies/callers, dynamic-frame intrinsics,
inline assembly, tail-call sites, and other preexisting unsupported operations
remain excluded. By-value parameters larger than 128 bytes remain unsupported.
`noinline` prohibits expanding a function; it does not prevent optimizing calls
inside that function. Self and mutual direct recursion are excluded.

Ordinary profitability limits are 64 instructions, 8 blocks and 512 bytes of
callee stack slots. `inlinehint` doubles each threshold. `alwaysinline` bypasses
these three soft thresholds, but cannot bypass legality or the hard safeguards:

- At most 128 expanded original call sites per caller per invocation.
- At most 2,048 estimated added IR instructions per caller per invocation.
- At most 16,384 estimated added IR instructions across the module per invocation.
- At most 65,536 bytes of callee stack slots, and at most 65,536 bytes of added
  summed slot/snapshot sizes per caller per invocation (excluding backend frame
  alignment and spills).

The IR estimate includes two glue instructions and a conservative byte-based
by-value copy allowance. Bounds cap expansion, not the original module's size
or existing caller frame. Templates and callsite lists are frozen at pass entry;
cloned calls are not revisited in the same invocation. Budgets are consumed in
module/function layout order, deterministically for a given input. Repeated
explicit invocations each receive a fresh bounded budget.

`keel_opt::inline_with_report` returns `InlineReport { changed, skipped }` with
caller, callee, original instruction handle and reason for skipped explicit `alwaysinline` sites (unsupported
callee/caller/site, recursion, unavailable private definition, signature mismatch,
or hard safeguard). The normal pass interface preserves its boolean API and
discards this report; no main CLI change is made. Indirect calls have no explicit
request to report. Basic/none optimization tiers retain their existing pipelines:
`alwaysinline` is acted on when the inliner runs, not by adding new tier passes.

Deferred: assume, distinct unreachable, hot/cold and branch weights, memory/alias
contracts, and public interposable inlining. Suggested umbrella documentation
follow-up: link this contract from `docs/design.md`; integration/suite-wide evidence
belongs to coordinator review rather than this focused worker session.

## Explicit non-interposable binding

`noninterposable` is an opt-in producer promise on a named function. Direct
references to that name must resolve to the promised definition; a different
implementation cannot replace it at static or dynamic link time. The producer
must arrange a compatible final link/load environment. Keel does not infer this
promise from a definition, public linkage, target triple, or `alwaysinline`.
Declarations may carry the promise before a definition is available. The
verifier checks callee/module agreement, not the eventual link/load environment.

The C spelling is `__attribute__((keel_noninterposable))`, advertised by
`__has_attribute(keel_noninterposable)`. Its ownership and boolean-union
redeclaration rules match the named inline policies. It survives IR printing,
parsing, cloning and `Module::sync_function_attrs`. The printer places it before
inline policies. It is independent of `Signature`, narrow argument/result
extension purposes, `byval`, `sret`, pointer capture/alias metadata, `noinline`,
and `alwaysinline`. It supplies no memory, trap, unwind or arithmetic promises.

This attribute does **not** set `Function::local`, hide a symbol, change its ABI,
add linker options, enforce local relocations, or make a declaration a definition.
Public definitions remain exported and are not deleted by inlining. Unexpanded
calls retain normal target call emission. A producer relying on ELF shared-library
preemption, Darwin flat namespaces/interposable exports, symbol wrapping, or
runtime replacement must leave the attribute absent unless it can ensure this
specific binding. This is a source/IR semantic assertion, not a linker binding
implementation or an alias for LLVM's relocation-oriented `dso_local`.

Target investigation: ELF supports runtime symbol preemption; LLVM represents
that separately from linkage in its [runtime preemption specifiers](https://llvm.org/docs/LangRef.html#runtime-preemption-specifiers).
Darwin's linker normally uses a two-level namespace, but supports
`-flat_namespace`, `-interposable`, and `-interposable_list` (see Apple's
[ld64 manual source](https://github.com/apple-oss-distributions/ld64/blob/main/doc/man/man1/ld-classic.1)).
Those different defaults do not establish a universal Keel frontend promise.
Existing public definitions remain conservatively interposable on both targets;
no new target configuration or CLI/library option is necessary.

### Named binding-aware inliner integration

`inline-noninterposable` admits private definitions and explicitly
`noninterposable` public definitions. It uses the same cloning implementation,
recursion rejection, ABI signature equality, `noinline` precedence, supported
operations, byval snapshots, debug subtree handling, soft limits and hard budgets
as `inline`. `alwaysinline` never supplies a binding proof or bypasses legality.
The public body and its linkage remain intact, including when every direct call
has been expanded. Calls and function addresses from other translation units can
still reach the exported ABI entry. No alias/capture metadata is inferred or
transferred onto the caller's parameters.

This pass is registered by name and supports the normal optimization report's
inline-site outcomes. It is absent from every tier preset, including Fast and
Full. Neither `inline` nor `inline-aggregate-reserve` changes its private-only
admission. A caller can explicitly schedule it before an existing tier pipeline;
for a small CLI example:

```sh
keel cc -p inline-noninterposable,fast-cfg,fast-local,fast-cfg,dce,fast-loop-reduce source.c -o source.s
```

This removes the two per-element helper calls in `k_narrow` when its two exported
helpers are explicitly marked. It does not provide mixed-width SIMD conversion
support and does not establish any runtime speedup. Ordinary unmarked kernel
sources retain their prior semantics and calls.
