# Bounded public freestanding wasm32 C APIs

This checkpoint extends the reviewed private [integer lowering](wasm-c-scalar-lowering.md)
and [pointer/data profile](wasm-c-pointer-data.md), using the recognized
[target foundation](wasm-target-foundation.md). It enables only the `keel-c`
library APIs. CLI, optimizer, artifact transport and native output remain guarded.
Native [ISA](userspace-isa-status.md), [intrinsic](c-x86-intrinsics.md),
[GNU vector/mode](gnu-vector-mode.md), ABI, FP and memory contracts are unchanged.
Parent integration and independent review remain separate.

## Compile contract

The unchanged public `Options { target, debug_info }` selects this profile only
with `Target::Wasm32UnknownUnknown`. `compile(preprocessed_c, path, options)`
constructs its parser with `CDataLayout::WASM32`, independently of the host.
It checks every retained declaration and every definition before the generator's
worklist/body pruning, then uses `verify_module_with_layout(DataLayout::WASM32)`
on the generated module. Empty translation units are supported. Native targets
continue selecting LP64 and native section validation.

This profile accepts integer computations through 64 bits, ordinary scalar
control flow, direct calls with plain i32/i64 arguments/results (or void return),
bounded integer/pointer/array/record storage and defined static data pointer
initializers. All function declarations require a prototype and a definition in
the translation unit; unresolved functions/data are rejected, including unused
extern declarations. A forward prototype followed by a definition is allowed.
Pointer arithmetic retains the reviewed i64 IR offsets/conversions with wasm32
normalization. No host import manifest or C pointer/aggregate call ABI is implied.
Local initializers remain limited to 64 bytes; local byte-string initializers
remain rejected. The existing size, alignment, pointer initializer, record and
array bounds still apply.

Unsupported FP/extended/aggregate values, function pointers, narrow/pointer/
aggregate call boundaries, variadics, unprototyped calls, atomics, vectors,
VLAs, inline assembly, computed goto, debug output and native linkage/lifecycle/
section attributes diagnose before body pruning. Unused static definitions
cannot hide these operations. Wasm modules skip the native-only section validator
only after this full rejection gate. `profile::compile` explicitly rejects Wasm
before lexing; it cannot bypass the profile through native defaults.

`long double` has declaration-only binary128 storage (size/alignment 16), including
typedefs, records and unevaluated layout queries. `__builtin_va_list` has opaque
size/alignment 4 for declaration/layout queries; it deliberately cannot participate
in value/storage/parameter use. This is no native va_list fallback or variadic ABI.
`sizeof` and declaration support never authorize FP execution or extended values.

A raw `Module` carries no target. Callers must retain the target/layout pairing,
use the explicit Wasm runtime emitter and avoid native code generation and
unaudited target-sensitive optimizer passes. Backend memory placement, exports,
stack limits and trap lifetime follow [the runtime contract](wasm-memory-backend.md).

