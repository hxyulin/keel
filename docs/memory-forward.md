# Local memory forwarding

The named `memory-forward` pass removes a same-type ordinary load when an
ordinary load or store in the same block already established its value.
The first potentially faulting access remains. Constant i64 `ptradd` chains
share a canonical root and checked signed byte offset; unknown derivations
remain opaque SSA addresses. Aliases are resolved before analysis.

Stores invalidate overlapping byte footprints even when their types differ.
Adjacent ranges on one unrestricted root can retain facts. Footprint comparison
uses circular modulo-2^64 distances, matching both supported targets: signed
interval ordering alone is unsound when pointer arithmetic wraps. Accumulating
constant offsets uses checked i64 addition; overflow abandons the derivation.
Pointer-valued accesses forward at an equivalent address but conservatively
invalidate facts on stores because their width is target-dependent.

Distinct roots disambiguate a modifying access only when **both** have valid
entry `restrict_params` contracts. Recognized constant derivations are allowed;
unknown derivations, comparisons, block argument passing, escapes, volatile
and atomic uses disqualify the affected root throughout the function. Read-only
restricted pointers may alias and are never equated. No null, pointer inequality,
alignment or `notrap` annotation supplies alias evidence. Calls, observable
accesses, unknown writes and lifetime operations remain memory barriers.

`AliasInfo` is private, function-local optimization analysis. Its `may_alias`
consumer must supply nonzero byte widths and at least one modifying access,
and handle barriers itself. Recompute after function changes. Cloned/inlined
callee parameters do not acquire entry contracts; the analysis does not
invent or persist provenance and changes no IR, ABI or calling convention.

The pass is opt-in; no optimization tier runs it. A suitable placement is
after inlining, address simplification/GVN and stack forwarding, before final
simplification and dead code cleanup. No performance claim is made for it.

Tests: `cargo test -p keel-opt --test memory_forward`.

The opt-in `call-load-reuse` pass also accepts explicit scoped call contracts.
Contract-based reuse requires no accessible writes, no freeing of existing
objects, no synchronization, contextual no-callback and explicit preservation
of existing object accessibility. The latter excludes permission/mapping changes
and is not inferred from LLVM nofree. Unknown and returns-twice
calls remain barriers. The first read and the call remain pinned. Fixed-stack
lifetime events invalidate all available reads, including across restarts at the
same address. See [LLVM contract design and implementation](llvm-contracts.md).
