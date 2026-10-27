# Empty aggregate ABI design

Status: implemented for complete, fieldless, alignment-one structs and unions on
x86-64 SysV. Other empty shapes remain diagnosed.

The first target is `x86_64-unknown-linux-gnu` under native LP64. The motivating
Linux input is `mm/vma.i`, which passes `struct vm_userfaultfd_ctx {}` by value.
Other targets retain their diagnostic until independently qualified.

## Contract

GNU C permits an empty structure with size zero ([GCC documentation](https://gcc.gnu.org/onlinedocs/gcc/Empty-Structures.html)).
Its C type and object remain distinct from `void`. For the initial x86-64 ABI
contract, a complete zero-size aggregate contributes no argument or return
payload. It consumes neither a register nor a stack argument slot, and an empty
return introduces no hidden result pointer.

The prior checkpoint's GCC 14.4.0 and Clang 21.1.8 witnesses both pass the two
integers in `f(long, struct Empty, long)` in RDI and RSI. Both preserve an
effectful call producing the empty argument. Those witnesses establish the
simple case, not every aggregate shape, alignment or variadic case below.

C argument expressions still execute. An empty-valued call still executes even
when its result is unused. Returning an empty expression evaluates that
expression before running exit actions and emitting an IR return with no values.
Omission must not impose a new C argument evaluation order.

IR `ByVal` and `Sret` layouts remain nonempty. No IR syntax, verifier exception,
calling convention or machine encoder change is proposed.

## One frontend mapping

Add a private C ABI mapping beside the current signature construction. It owns
the IR signature, one mapping entry per source parameter, and a result mapping.
Use enums to distinguish omitted parameters from parameters with an IR index,
and void results from omitted aggregate, direct and indirect results.

The mapping removes three sources of duplicated arithmetic:

- Hidden result parameters shifting ordinary parameter indices.
- Omitted source parameters shifting later IR parameter indices.
- Distinguishing fixed source arguments from variadic arguments.

Construct it through the same checked classifier for declarations, definitions,
direct calls and indirect calls. Existing scalar and nonempty aggregate
classification stays unchanged. Keep source function types intact; two different
C types do not become compatible merely because their IR signatures match.

An omission candidate must be a complete aggregate of known size zero, not an
incomplete type, VLA, arbitrary zero-length expression or unsupported value type.
Run existing unsupported-value checks before omission. Establish the admitted
shapes with compiler witnesses: empty structs/unions, nested empty members and
zero-length-array members. Explicit alignment and flexible-array-only records
need separate witnesses; reject unqualified shapes rather than infer an ABI
from size alone. Complex values continue through existing nonempty handling.

## Lowering changes

| Consumer | Required behavior |
| --- | --- |
| Function declarations and imports | Use the mapped IR signature, including cached direct imports. |
| Definition entry | Bind ordinary parameters by mapped index. Allocate an ordinary local object for an omitted parameter without reading an incoming value. |
| `restrict` metadata | Record the mapped IR parameter index, including any hidden result parameter. |
| Debug variables | Keep source parameter identity and local storage; omission does not remove the source variable. Current debug metadata uses a parameter flag, not an ABI index. |
| Call arguments | Evaluate every source expression once. Append only mapped payloads. Preserve existing aggregate snapshot barriers around later effects. |
| Variadic boundary checks | Use the source argument index against the source fixed-parameter count, never the number of emitted IR values. |
| Call result | For an omitted aggregate result, return a local object address to expression lowering without passing it to the callee. |
| Explicit return | Evaluate the expression and exit actions, then emit an empty IR return for an omitted result. |
| Implicit return | Keep the existing no-payload aggregate return behavior. |
| Unreachable result | Preserve aggregate address representation without inventing a scalar ABI result. |

The frontend already represents aggregate expression values as addresses and
reserves at least one backing byte for zero-size automatic objects. Reuse that
storage policy for parameter objects and call-result temporaries. Their C size
remains zero. The backing byte must not become an ABI payload or a required
load/store. Do not promise unique addresses between unrelated empty objects.

Zero-byte copies transfer no bytes, but computing their source and destination
must retain effects. Empty expressions must remain usable in assignments,
conditionals, comma expressions and forwarding returns.

## Boundaries

Support prototyped direct and indirect calls, definitions and declarations in
the first increment. Retain the existing rejection of aggregate arguments
without a prototype. Old-style definitions must either receive the same mapping
with qualified tests or diagnose explicitly; do not silently use the old zip.

Support omitted empty values among the fixed parameters of a variadic function.
Qualify that `va_start` locates later scalar varargs correctly. Continue rejecting
aggregate values in the variadic tail, including empty values, and explicitly
reject aggregate `va_arg` until separately designed. A fixed empty argument is
not permission to accept empty values in every varargs position.

Microsoft ABI calls remain unsupported. Non-x86 targets and nonnative data
layouts retain their existing boundary. Records with nonzero size but no
ordinary payload, such as padding-only records, are outside this increment.

## Validation and implementation order

1. Add compiler ABI witnesses for candidate empty shapes, alignment and result
   passing. Use both GCC and Clang; resolve disagreements before admitting a
   shape. Keep a nonempty record and an incomplete record as controls.
2. Introduce the mapping while empty aggregates still reject. Run existing ABI,
   indirect-call, restrict, variadic, debug and mandatory-inlining tests. This
   separates the indexing refactor from the new permission.
3. Admit the qualified empty forms and lower parameters/results. Check generated
   IR has no zero-size `ByVal`/`Sret` and no dummy ABI operand. Keep IR verifier
   rejection tests unchanged.
4. Cross-link Keel callers with GCC/Clang callees and the reverse. Include empty
   arguments before/between/after scalars, register exhaustion, stack arguments,
   scalar and nonempty aggregate returns, empty returns and indirect calls.
5. Check expression effects with counters and independent expected results.
   Include parameter-address use, conditional/assignment results, return cleanup,
   fixed-empty variadics and required inlining. Exercise optimized/unoptimized
   and debug builds. Controls that consume a dummy argument or erase an
   effectful empty producer must fail normally.
6. Rerun the pinned corpus under unchanged feature permissions. Record the
   acceptance delta and new first errors. Run full validation once the increment
   is complete; source acceptance is not kernel link or boot qualification.

The implementation is confined to the C frontend and its tests. If
qualification reveals a target convention that needs machine ABI changes, keep
that form rejected and report it before expanding this design.

## Implemented validation

`crates/keel-c/tests/x86_empty_aggregates.rs` checks mapped signatures, restricted
pointer indices with and without an indirect result, debug parameter storage,
source type compatibility and retained target/shape/old-style boundaries.
Aggregate `va_arg` and empty variadic-tail arguments diagnose explicitly.

The fixture crosses compiler boundaries in both directions. Its 48 object
variants cover all three selectors, both allocators, normal/empty pools,
optimized/unoptimized IR and debug/no-debug compilation. Text, typed and chunked
objects match. GCC and Clang each execute every variant. Both also compile and
run the complete independent reference pair. Two assembly controls deliberately
consume a dummy argument slot or omit effectful empty producers; each must fail
normally with its expected exit code.

The fixture includes register exhaustion and stack arguments, indirect calls,
empty and ordinary aggregate returns, assignments and conditionals, required
inlining, fixed-empty variadic calls, and an empty-returning cleanup callback.
The cleanup observer checks that return-expression effects occur before cleanup.
To export the fixture and all generated objects for native Linux validation:

```sh
KEEL_X86_EMPTY_AGGREGATE_EXPORT=/tmp/empty-abi cargo test -p keel-c --test x86_empty_aggregates
```

For each exported object, compile `main.c` with GCC or Clang and link that object.
For a compiler reference, link `source.c` and `main.c` instead.
