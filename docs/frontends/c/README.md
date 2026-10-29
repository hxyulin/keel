# C frontend support

Keel compiles C11 with selected GNU extensions. Native targets are AArch64
Darwin, AArch64 Linux and x86-64 Linux; WebAssembly has a narrower frontend
and backend subset. This is not full GCC or Clang language compatibility.

## Supported areas

- Native preprocessing, declarations, expressions, control flow and constant
  evaluation, including the GNU forms needed by the pinned kernel fixture.
- Structures, unions, packed records and enums, bit-fields, aggregate values,
  initializers and target-dependent layout.
- Native calls, varargs, aggregate ABI transport, TLS and executable lifecycle
  metadata, within the target's documented ABI subset.
- Bounded VLAs, cleanup attributes, GNU case ranges, union casts, statement
  expressions, type queries and overflow builtins.
- Scalar atomics and selected generic atomic builtins.
- GNU vector types, vector expressions and selected x86 intrinsic headers.
- Symbol visibility, weak definitions, checked function aliases, sections,
  retention and function optimization attributes.
- File-scope assembly and checked extended inline assembly, with a broader
  x86-64 constraint set than AArch64.

## Boundaries

Support for a syntax or attribute does not imply every GCC extension or every
placement is supported. Unknown attributes can still be accepted without
semantic effect; code must not depend on that behavior. `naked` and `interrupt`
metadata do not establish executable entry/return support.

Keel does not ship a complete system SDK, libc or compiler runtime. Vendor
intrinsic headers are subsets. Kernel acceptance uses pinned, preprocessed
64-bit inputs and an explicit feature policy; it is not a general Kbuild driver.

## References

- [Language extensions and layout](extensions.md), including [memory builtin policy](extensions.md#c-library-memory-builtins).
- [Function attributes and optimization policies](../../compiler/function-attributes.md).
- [Inline assembly](inline-asm.md) and [x86 constraints](../../targets/x86_64/c-inline-asm.md).
- [x86 intrinsics](../../targets/x86_64/intrinsics.md) and [Wasm frontend](../../targets/wasm/c-frontend.md).
- [Linux integration](../../integrations/linux-kernel.md) and [application tests](../../../suite/README.md).

Tests in `crates/keel-c/tests/` define the accepted and diagnosed cases.
This list describes supported areas, not an exhaustive conformance claim.
