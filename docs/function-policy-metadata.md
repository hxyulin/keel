# Function policy metadata

The C frontend retains the following attributes on Keel IR declarations,
definitions and direct callee records. This increment stores requests for later
policy consumers; it does not select a per-function optimization pipeline.

| C attribute | Keel IR representation | Current execution policy |
|---|---|---|
| `cold`, `hot` | `FunctionAttrs::{cold, hot}` | Retained hint. No block weights or layout change. |
| `optimize(...)` | Ordered `FunctionOptimization` options | Retained request. No pass, feature or backend override. |
| `optnone` | `FunctionAttrs::optnone` | Retained request. Optional optimization is not yet disabled by this field. |
| `flatten` | `FunctionAttrs::flatten` | Retained request. Does not alter inlining decisions yet. |
| `no_instrument_function` | Matching boolean | Retained suppression request for future instrumentation. |
| `no_profile_instrument_function` | Matching boolean | Retained suppression request for future profiling. |
| `naked` | `FunctionAttrs::naked` | Requires special entry/return lowering. Executable consumers reject it. |
| `interrupt` | `FunctionAttrs::interrupt` | Requires special entry/return lowering. Executable consumers reject it. |

Existing `always_inline`, `noinline`, return/effect contracts, linkage, section
and alignment behavior remains active. These new capability queries stay false:
accepting metadata does not establish an implemented function policy.
Other attributes still follow the existing [frontend coverage](c-frontend-coverage.md)
rules; this is not a catch-all attribute store.

## Parsing, merging and text

Both ordinary and double-underscore C spellings are accepted. Attributes on
later declarations update definitions and direct callee records throughout the
translation unit. Applying these function policies to objects, parameters,
typedefs or members is diagnosed.

`optimize` accepts a nonempty argument list of narrow strings and nonnegative
integer constants that fit `u32`. Strings must contain printable ASCII. A string
may itself contain a comma-separated option list. Argument boundaries, repeated
options and declaration order are preserved. Integers stay numeric rather than
being rewritten to a particular compiler's flag spelling. Unknown optimization
option strings are retained without promising support for their effects.
There is no fixed byte limit on the stored option list. GCC's
[function attribute documentation](https://gcc.gnu.org/onlinedocs/gcc/Common-Function-Attributes.html)
defines the source extension that this metadata records.

For example:

```c
long work(long) __attribute__((optimize(2, "no-unroll-loops")));
long work(long x) { return x + 3; }
long work(long) __attribute__((cold, optimize("Os")));
```

The function carries this Keel IR policy:

```text
cold optimize(2, "no-unroll-loops", "Os")
```

Repeated C declarations merge boolean requests and append optimization options.
IR text permits repeated `optimize(...)` groups and appends them in order;
repeated standalone IR flags are diagnosed. `hot` with `cold`, `naked` with
`interrupt`, and special entry contracts with `alwaysinline` are rejected.
The verifier checks new metadata and direct-callee agreement with the module.

`FunctionAttrs` is now `Clone` rather than `Copy`. Optimization lists share
immutable storage, so cloning a declaration does not duplicate option strings.
`Module::function_attrs` now returns a borrowed attribute record. Attribute
synchronization and inliner imports preserve the canonical module contract;
importing an existing callee does not append its options again.

## Special entry contracts

Only argument-free `interrupt` is represented in this increment. Target-specific
forms such as `interrupt("IRQ")` remain rejected. Interrupt signature validation,
register preservation, interrupt/error-code frames and return instructions are
not implemented. Naked functions likewise have no implemented body, stack or
unwind contract for executable emission.

Modules containing special entry contracts remain opaque to the ordinary
optimizer pipeline. Native text, typed and chunked emission, LLVM export,
Cranelift translation and Wasm emission reject them before publishing output.
The check includes definitions, declarations and callee tables. This prevents
inlining or dead-function cleanup from silently converting an unsupported entry
contract into an ordinary C function.

These records allow future target-specific implementations without dropping
source intent. They do not qualify interrupt handlers or naked functions for
kernel execution. Existing assembly-owned transition adapters remain separate.

## Validation

The workspace run passed 3,018 tests with zero failures and 15 ignored.
Workspace all-target/all-feature checking passed. The standalone Cranelift
workspace passed its locked all-target check and special-entry regression.
IR, C and codegen documentation builds passed with existing codegen warnings.
Focused Clippy checks passed; workspace strict Clippy remains blocked by
pre-existing diagnostics, and ordinary all-target Clippy also encounters the
existing `almost_swapped` test in `call_operand_abi.rs`.

Regressions cover late declarations, ordered/repeated options, pointer-returning
functions, function-only placement, conflicts, conservative capability queries,
IR text, direct-callee synchronization, native emission invariance and all
executable-consumer rejection boundaries. An immediate post-inlining check
reproduced duplicated options with the old import merge and passes with the
canonical contract copy. No Linux corpus rebuild, kernel boot or timing claim
is made for this metadata increment.
