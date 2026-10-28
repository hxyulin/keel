# Initial C restrict support

The frontend retains `restrict`, `__restrict`, and `__restrict__` in type
sugar, including pointer typedefs. A function definition emits
`restrict_params(0, 1)` for its own top-level restricted pointer-to-object
parameter objects. Indices include any ABI hidden result parameter. Prototype
qualifiers do not supply facts to a later unqualified definition, declarations,
call signatures, or indirect calls. Local/block restrict, inner pointer layers,
function pointers, and array-bracket restrict currently supply no optimization
facts. This is conservative; it does not diagnose every invalid C qualifier.

`Function::restrict_params` is a definition-only access contract for the whole
invocation. If memory accessed based on a listed parameter is modified during
that invocation, accesses to that memory must be based on that parameter. It
does not assert distinct pointer values: two parameters may read the same
unmodified object. It supplies no nonnull, notrap, alignment, immutable-memory,
or type-based alias fact. The semantics follow the modified-object access rule
in [C11 N1570 section 6.7.3.1](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
and the corresponding parameter access distinction in
[LLVM's noalias documentation](https://llvm.org/docs/LangRef.html#noalias).
C restrict also constrains restricted-pointer assignments; the initial consumer
uses no such assignment to infer aliasing facts.

The verifier checks unique in-range normal-purpose pointer indices; existing
entry/signature verification checks matching entry parameters. A C frontend must establish C pointee eligibility because opaque IR pointers
cannot express it. An LLVM producer may assert the same modified-object access
rule directly from a definition's normal-pointer `noalias` parameter, without
recovering C types. This mapping does not transport declaration, call-site or
return-value noalias, and it adds no pointer inequality, nonnull or readability
promise. Text preserves contract order deterministically. Whole-function
clones retain the contract. Inlining copies instructions and blocks, not the
callee's definition contracts, so callee entry remapping does not give caller
arguments new facts. Existing caller contracts retain their entry indices.
Transformations that change entry arguments must remap or drop these contracts.

The memory-forward pass only retains an available exact-address, same-type load
across a store when both addresses are distinct eligible restricted entry
parameters. Any non-address use of such a root (including escape, comparison,
block argument, call argument, or pointer derivation), volatile access or atomic
access through it disables that root for the entire function. Calls, volatile
operations, atomics and unknown writes clear available memory facts. Unknown
addresses never disambiguate a store. Facts remain local to a basic block.

For `int a = *p; *q = 9; return a + *p;`, with both parameters eligible,
`load p; store q; load p; add` becomes `load p; store q; add` using the first
load twice. The first potentially faulting load remains. Without the contract,
the second load remains. Modified overlapping restricted accesses are invalid
C and are not used as differential correctness tests. The native fixture tests
separate writable objects, legal read-only restricted aliases, ordinary writable
aliases, and volatile reads against Clang. This demonstrates an instruction
change, with no runtime performance claim.
