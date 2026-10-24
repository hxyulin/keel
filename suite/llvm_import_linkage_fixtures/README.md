# Bounded LLVM ODR function copies

`suite/llvm_import_linkage_validate.py` validates this feature independently of
the aggregate frontend harness:

```sh
python3 suite/llvm_import_linkage_validate.py \
  --libllvm /path/to/LLVM23/libLLVM.dylib \
  --keel target/debug/keel --out NEW_OUTPUT_DIR
```

The validator saves unchanged Clang C++ LLVM output, commands, checks, native
objects/executables, and source hashes under `--out`.

## Supported contract

Default mode transports exported ODR definitions with coalesced identity.
This document describes the separate legacy `--localize-odr-functions` mode (or Python
`import_module(..., localize_odr=True)`), which explicitly asserts that **no outside
translation unit requires any imported `linkonce_odr` or `available_externally`
definition's symbol**. This is a whole-module contract provided by the caller;
it cannot be inferred from a single LLVM module. It is suitable for a bounded
frontend experiment whose callers only use its ordinary public entry points.
Normal LLVM ODR/equivalence requirements still apply across translation units.

Under that contract the importer retains every function, in source order, under
its original byte name. Eligible definitions become Keel `local` functions. Their
symbols are not exported, supplied to outside callers, or coalesced. This is the
same semantic freedom as making local copies of ODR bodies for direct calls;
it is not weak/linkonce symbol support in the object format or linker.

Every use of a candidate must be a direct call with no candidate address in the
arguments or operand bundles. Recursion, mutually dependent ODR helpers, unused
ODR definitions, and direct calls to defined internal/private helpers are
eligible. The complete helper call closure must contain no named global storage,
function address operands, indirect calls, or calls to public/external functions.
Constant expressions and transitive local-helper dependencies are inspected.
Local stack allocations and pointer arguments retain the existing import rules.
All existing signature, visibility, target, instruction, and semantic checks
still apply. No compiler output is rewritten or filtered; unsupported unused
functions cause whole-module rejection too.

## Evidence and restrictions

The validator generates the actual existing `constexpr_runtime.cpp` and
`virtual.cpp` probes at Clang `-O0 -fno-exceptions -fno-rtti`. The former's
recursive `_Z6factorj` can be retained as a local copy. The latter requires weak
vtable storage and observable method addresses and remains rejected in this
localization mode; the default exported-ODR path supports it.

`left.cpp` and `right.cpp` produce duplicate ODR helper definitions. Original
LLVM/Clang executables and None/Fast/Full Keel objects are checked against
independent arithmetic assertions for two imported TUs and both mixed
Clang/Keel arrangements. Global-symbol inspection proves the local helper was
not strengthened, and an outside caller fails to link without a real provider.
`available.ll` is compared to original LLVM plus its external provider; an unused
ODR definition and its private callee are retained as well.

Original two-TU identity fixtures assert equal function addresses; importing
either address-observable TU is rejected. Original two-TU static-storage
fixtures assert shared state; their imports are rejected. The original weak
interposition fixture asserts a strong override wins; weak linkage is rejected.
All COMDAT selection kinds and a function/data group are rejected. Source bytes,
existing/absent destinations, stdin/stdout, default exported-ODR mode, malformed LLVM,
late semantic failures, and downstream verifier failures are checked.

General non-ODR weak support still needs interposition contracts beyond
the implemented ODR symbols and storage. Broader work includes
external-reference resolution, identity preservation, interposition semantics,
COMDAT group selection, and the associated object/linker implementation. In
particular this option must not be enabled for arbitrary C++ libraries or used
to promote virtual dispatch coverage.
