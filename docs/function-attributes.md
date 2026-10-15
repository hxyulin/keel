# Function attributes

`FunctionAttrs` carries `inlinehint`, `alwaysinline`, `noinline`, `noreturn`,
and `nounwind` on IR definitions, external declarations, and each direct
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

The printer uses the order `inlinehint alwaysinline noinline noreturn nounwind`.
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
contract cannot be proved here. No contract-based dead-code or unwinding
rewrite is introduced. A contradictory body is a producer contract violation,
not a proof result or an excuse to change existing runtime behavior.

## C declarations and contracts

C `inline` adds `inlinehint`. Inline-only emission is resolved after all
translation-unit declarations: a later `extern` or plain non-inline declaration
makes the definition external, just as an earlier one does. Explicit `static`
linkage is tracked independently and remains private. A definition declared only
`inline` retains the existing private-copy emission behavior. Inline policy does
not change this determination; an externally emitted definition cannot pass the
inliner's private-only boundary.

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
bodies. Contracts written in a pointer qualifier list (after `*` or `^`) are
conservatively discarded, including when that pointer is the named function's
return type. They cannot become promises on the factory. This includes
`returns_twice`; noncontract layout attributes retain their existing handling.
Declaration-specifier and final named-declarator attributes still apply to the
named function. This is a bounded ownership correction, not a complete GNU
attribute-placement compatibility implementation.

Indirect call signatures carry none of these new attributes. Function
pointer/declarator attributes do not establish indirect-call promises; no
contract propagates through pointer types, casts, matching ABI types, or a
possible target's name. Indirect calls keep their continuations. Calls and
invokes are not rewritten based on `nounwind`.

## Inliner policy and hard boundaries

Only private, nonrecursive definitions and supported direct `call` sites are
expanded. Public/interposable definitions, missing definitions, variadics,
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
