# Freestanding wasm32 C profile

`keel-c` can compile one freestanding C translation unit for
`wasm32-unknown-unknown`. The result is unoptimized IR with the WASM32 storage
layout, intended for the `keel-wasm` runtime emitter. Most users reach this through
the [CLI](README.md); this page is the contract for the C library APIs.

Integer constant expressions and literal types use the wasm32 ILP32 model
(`CDataLayout::WASM32`) throughout. With a 32-bit `long`,
`unsigned int + long` is `unsigned long` and `unsigned long + long long` is
signed `long long`, and literal suffix candidates continue through `long long`
and `unsigned long long`. Numeric pointer constants keep their low 32 bits. A
symbolic address cast to an integer is not a constant.

## Compile contract

`Options { target, debug_info }` selects this profile when `target` is
`Target::Wasm32UnknownUnknown`. `compile(preprocessed_c, path, options)` then:

1. Parses with `CDataLayout::WASM32` (ILP32), independent of the host.
2. Checks every retained declaration and every function definition for unsupported
   features **before** the generator prunes unused bodies, so unused code cannot
   hide an unsupported operation.
3. Verifies the generated module with
   `verify_module_with_layout(DataLayout::WASM32)`.

Empty translation units are supported. Native targets keep LP64 and native section
validation.

### Supported

- Integer computation up to 64 bits and ordinary scalar control flow.
- Direct calls with plain i32/i64 arguments and results, or a void return.
- Integer, pointer, array and record storage within the existing size, alignment
  and bounds limits, and defined static data with pointer initializers.
- Pointer arithmetic using i64 IR offsets with wasm32 normalization (see the
  [memory semantics](../../../crates/keel-wasm/README.md#memory-semantics)).

Every function needs a prototype and a definition in the translation unit. A
forward prototype followed by a definition is fine. Unresolved functions and data
are rejected, including unused `extern` declarations. Local initializers are limited
to 64 bytes, and local byte-string initializers are rejected.

### Rejected

FP, extended and aggregate values; function pointers; narrow, pointer and aggregate
call boundaries; variadics; unprototyped calls; atomics; vectors; VLAs; inline
assembly; computed goto; debug output; and native linkage, lifecycle and section
attributes. The Wasm `import_module`, `import_name` and `export_name` attributes are
rejected explicitly rather than silently ignored: exports are chosen by the caller
and there is no source-level import contract. The native-only section validator is
skipped for Wasm only after this rejection gate. `profile::compile` rejects Wasm
before lexing.

### Declaration-only types

- `long double` has declaration-only binary128 storage (size and alignment 16), so
  typedefs, records and `sizeof` work.
- `__builtin_va_list` has opaque size and alignment 4 for declarations and layout
  queries, and cannot be used as a value, parameter or storage. There is no native
  `va_list` fallback and no variadic ABI.

Declaration support never authorizes FP execution or extended values.

### Using the output

A raw `Module` carries no target. Keep the target/layout pairing yourself, use
`keel_wasm::emit_wasm32_runtime_module` (optionally after the WASM32
basic optimization profile), and never pass the
module to native code generation or other optimizer pipelines. Memory placement,
exports, stack limits and trap behavior follow the
[runtime contract](../../../crates/keel-wasm/README.md).

## Macros and headers

`predefined_macros(Wasm32UnknownUnknown, None)` describes a freestanding C11 ILP32
target: signed plain `char`, little endian, four-byte `long`, pointers, `size_t` and
`ptrdiff_t`, eight-byte `long long` and `intmax_t`, and binary128 `long double`
limits. 64-bit constant macros use `LL`/`ULL`, and pointer-sized integer types keep
their `long`/`unsigned long` identities. GNU `word`/`pointer` modes are 32-bit and
`DI` is 64-bit. No LP64, host OS, x86/ARM, WASI, SIMD, lock-free atomic or optional
Wasm feature macros are defined. `__has_*` capability queries conservatively return
zero, while `__has_include` uses the same lookup as `#include`.

`preprocess(raw_c, path, &PreprocessOptions { target: Wasm32UnknownUnknown, .. })`
searches, in order:

1. the including file's directory (quoted includes),
2. explicit `include_dirs`, then `system_include_dirs`,
3. the embedded headers `stddef.h`, `stdint.h`, `limits.h`, `stdbool.h`, `float.h`,
   `stdalign.h`, `stdarg.h` and `stdnoreturn.h`.

Vendor intrinsic headers (including Keel's private wrappers), `stdatomic.h` and libc
headers are not available. Your own include directories can supply headers and take
precedence; you are responsible for their target compatibility. Absolute include
names are rejected. There is no implicit `/usr/include`, SDK, framework, `CPATH`,
`C_INCLUDE_PATH`, `SDKROOT` or compiler resource-directory lookup, and an explicit
sysroot or any Darwin deployment setting is rejected.

User `-D`/`-U` definitions keep their ordered semantics but cannot change the target
model or enable unsupported operations. `dump_macros` reports the inventory after
overrides. `preprocess_with_stats` (with the `profile` feature) uses the same path.

## Testing

- `wasm_profile`: preprocessed-C layout, empty units, declaration-only extended
  types, native LP64 controls, and rejection of unsupported bodies, prototypes,
  externs, debug/profiling and unused attributes.
- `wasm_freestanding`: preprocesses a raw fixture through the embedded headers,
  verifies WASM32 IR, emits a runtime module and runs it under Node (checksum, local
  array, direct helper, static pointers, memory sentinels). Clang independently
  checks the declaration fixture and the shared macro inventory.
- Negative tests cover host, vendor and private headers, include capability
  queries, sysroot/deployment settings, ordered overrides and ambient host paths.

```sh
cargo test -p keel-c --features profile \
  --test wasm_profile --test wasm_freestanding --test wasm_target_unavailable
```

The full `cargo test -p keel-c --features profile` run includes the native
macro, header, type and ABI controls. Tests need `node` and `clang` on `PATH`.

## Limits

This is a single-translation-unit freestanding profile. There is no general C ABI,
libc or WASI, imports, dynamic linking or FP execution.
