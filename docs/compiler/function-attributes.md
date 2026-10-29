# Function attributes

`FunctionAttrs` carries linkage, binding, inline, control-flow and per-function
policy attributes on IR definitions, external declarations and each direct
callee record. These are separate from `Signature`: they do not take part in
ABI or C function type compatibility. `returns_twice` stays part of the
signature.

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

The printer writes them in this order: `weak constructor used retain
destructor noninterposable no_semantic_interposition inlinehint alwaysinline
noinline noreturn nounwind cold hot optnone flatten no_instrument_function
no_profile_instrument_function naked interrupt`. The parser rejects duplicate and malformed tokens. The verifier rejects
`alwaysinline` with `noinline`, and `noreturn` with `returns_twice`, on declarations,
definitions and callee records. It requires direct callee attributes to agree
with the module declaration/definition. `Module::sync_function_attrs` refreshes
these records after construction or deliberate module contract edits; it does
not resolve conflicting policies. Function imports default to no promises;
programmatic builders must explicitly set/synchronize attributes. Inlining
preserves imported callee contracts.

## Function ISA permissions

`target_features("avx2", "-fma,+fma")` retains ordered x86 permission edits
in `FunctionAttrs::target_features`. The shared feature schema validates names;
the backend applies edits to the invocation's baseline and then validates
prerequisites. A positive edit enables its prerequisites. A negative edit clears
the named feature only, so disabling AVX in an AVX2 baseline also requires
disabling AVX2 and other features that depend on AVX. These requests are separate
from optimization settings and do not change the ABI signature.

The x86 backend uses each resolved set for vector lowering, instruction
selection, inline assembly, and emitted-instruction checks. Wide direct calls
require AVX transport at both the caller and callee. Indirect signatures must
fit the caller's enabled transport. AArch64 and Wasm emission and LLVM export
reject these explicit x86 IR requests.

Optional inlining requires the caller's edits to contain the callee's
permissions for every possible module baseline. Explicit feature contracts
have the same restriction during mandatory C inlining. This is conservative:
matching global flags alone do not prove that an explicitly annotated callee
can be inlined into an unannotated caller. Unannotated mandatory wrappers are
lowered in the caller's permission context; copied operations still undergo
the caller's feature checks. Owned regions and frame observers retain their
existing inlining restrictions.

Object encoders use `keel_codegen::x86_features::module_features`; see
[x86 intrinsics](../targets/x86_64/intrinsics.md#function-permissions-and-runtime-cpu-queries).
Target-aware optimizer passes also receive each function's resolved
vector capabilities, so a module-wide AVX2 policy cannot widen a function that
disables AVX.

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
and per-attribute Clang AST behavior.

Indirect call signatures carry none of these new attributes. Function
pointer/declarator attributes do not establish indirect-call promises; no
contract propagates through pointer types, casts, matching ABI types, or a
possible target's name. Indirect calls keep their continuations. A direct
`nounwind` invoke may be lowered to a call and its normal branch,
subject to the representation safeguards below.

## Optimizer consumers

`call-contracts` runs in Basic, Balanced, Size and Full, before their second
CFG cleanup, which removes newly redundant joins. In the tiers that inline, it
runs after inlining, so inlined direct calls get the same treatment.

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
tail-call sites, and other preexisting unsupported operations
remain excluded. By-value parameters larger than 128 bytes remain unsupported.

A callee with inline assembly follows the ordinary cost rules when every copy
of the template assembles and allocates as it does in the callee. Each copy
repeats the labels its template defines, so the template may define only
numeric local labels (`1:`) and labels made unique with `%=`, which includes
the restoration labels of asm goto. Named labels, `name = value` and the
`.set`, `.equ`, `.equiv`, `.eqv`, `.macro`, `.comm`, `.lcomm` and `.symver`
directives keep the callee out of line. So do operands or clobbers that the
caller's frame can reserve: the `rsp` operand, AArch64 x19, x86 r15, and more
than 13 operands and register clobbers in total.
`noinline` prohibits expanding a function; it does not prevent optimizing calls
inside that function. Self and mutual direct recursion are excluded.

Ordinary profitability limits are 64 instructions, 8 blocks and 512 bytes of
callee stack slots. `inlinehint` doubles each threshold. `alwaysinline` bypasses
these three soft thresholds, but cannot bypass legality or the hard safeguards:

- At most 128 expanded original call sites per caller per invocation.
- At most 2,048 estimated added IR instructions per caller per invocation.
- At most 16,384 estimated added IR instructions across the module per invocation,
  or 65,536 in Full's `inline-cheap-reserve` policy.
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
caller, callee, original instruction handle and reason for skipped explicit
`alwaysinline` sites (unsupported callee/caller/site, recursion, unavailable
private definition, signature mismatch, or hard safeguard). The normal pass
interface discards this report. `alwaysinline` acts only in tiers that run an
inliner, so None and Basic do not inline. Hot, cold and per-function
optimization requests select local pipelines and optional selection; `flatten`
requests bounded nested inlining.

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

### Named binding-aware inliner integration

Balanced runs `inline-noninterposable` in its three bounded inlining rounds.
Full's `inline-cheap-reserve` and Size's `inline-size` apply the same binding
rules with their own budgets. `inline-noninterposable` admits private
definitions and explicitly
`noninterposable` public definitions, as well as definitions carrying the
`no_semantic_interposition` equivalent-body promise described below. It uses
the same cloning implementation,
recursion rejection, ABI signature equality, `noinline` precedence, supported
operations, byval snapshots, debug subtree handling, soft limits and hard budgets
as `inline`. `alwaysinline` never supplies a binding proof or bypasses legality.
The public body and its linkage remain intact, including when every direct call
has been expanded. Calls and function addresses from other translation units can
still reach the exported ABI entry. No alias/capture metadata is inferred or
transferred onto the caller's parameters.

The pass reports inline-site outcomes in the normal optimization report. The
private-only `inline` pass keeps its admission. A pipeline can schedule the
binding-aware pass explicitly:

```sh
keel cc -p inline-noninterposable,fast-cfg,fast-local,fast-cfg,dce,fast-loop-reduce source.c -o source.s
```

## C semantic interposition mode

`keel cc -fno-semantic-interposition` permits substituting a strong definition's
body at direct calls. Any replacement must preserve that definition's semantics
and side effects. The frontend emits the IR attribute
`no_semantic_interposition` on strong public definitions in that translation unit.
It does not annotate undefined declarations, weak definitions or indirect function
types. `-fsemantic-interposition` is the default for ordinary public definitions;
when both flags appear, the last wins. These flags belong to native C compilation,
including preprocessed `.i` inputs, rather than IR compilation or Wasm output.
The library API is `compile_with_binding(..., FunctionBinding::NoSemanticInterposition)`;
`compile` retains its existing defaults.

Strong hidden/internal C definitions on ELF also receive this attribute, even
with `-fsemantic-interposition`: ELF visibility prevents runtime preemption of
those definitions. Darwin hidden definitions remain conservative unless the
explicit mode is selected. Target-free IR `hidden` alone supplies no binding proof.

This promise is weaker than `noninterposable`. It does not promise which address
or definition a symbol reference resolves to. Exported entry points, symbol
visibility, function addresses, unexpanded direct calls and indirect calls retain
normal emission and binding. No local aliases or relocation changes are requested.
The binding-aware inliners consume either promise, preserving the existing ABI,
legality, recursion, `noinline` and growth checks. The private-only `inline` pass
remains private-only. Weak bodies remain ineligible even if IR carries the
attribute. A weak-ODR body with either promise is eligible, because every copy
has an equivalent body. Only the caller changes: the definition keeps its
linkage, signature and calling convention, and no memory, capture or return
facts are derived from it, since the linker may keep another object's copy.
The attribute round-trips through IR and synchronizes with named
callee declarations. It does not enable exact-binding call summaries, ABI
privatization or inference of alias, capture or memory contracts.

### Intended final link output

`keel cc --link-intent executable` declares that the compiled code will go into
an executable. It selects `-fno-semantic-interposition` behavior by default.
`--link-intent shared` retains semantic interposition for public definitions.
Without either intent, reusable C outputs retain the conservative default.
An explicit `-fsemantic-interposition` or `-fno-semantic-interposition` overrides
the intent's default regardless of argument order. Repeated intent flags use
the last value. Weak definitions and imported functions retain their guards.

Intent describes the eventual link destination, not the immediate output format.
The command still emits assembly, IR or an object as requested; it does not link
an executable or implement PIC/PIE relocation modes. These intent flags are native
C compilation options and are rejected for Wasm and other subcommands.
