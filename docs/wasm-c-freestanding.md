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
section attributes diagnose before body pruning. Wasm `import_module`,
`import_name` and `export_name` attributes are also explicitly rejected; runtime
exports remain caller-selected and no source import contract is implemented. Unused static definitions
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

## Macro and header policy

`predefined_macros(Wasm32UnknownUnknown, None)` supplies freestanding C11,
ILP32, signed plain char, little endian, four-byte long/pointers/size_t/ptrdiff_t,
eight-byte long long/intmax_t and binary128 long-double declaration limits.
The 64-bit integer constant macros use `LL`/`ULL`; pointer integer types and
limits retain their proper long/unsigned-long identities. GNU `word`/`pointer`
modes are 32-bit, while `DI` stays 64-bit. No LP64, host OS, x86/ARM, WASI,
SIMD, atomic lock-free/CAS or optional Wasm feature claims are added. `__has_*`
capability queries remain conservative (zero); actual `__has_include` queries
use the same target-aware lookup as `#include`.

`preprocess(raw_c, path, &PreprocessOptions { target: Wasm32UnknownUnknown, .. })`
uses the ordinary local directory for quoted includes, explicit `include_dirs`
and `system_include_dirs` in that order, then only embedded `stddef.h`,
`stdint.h`, `limits.h`, `stdbool.h`, `float.h`, `stdalign.h`, `stdarg.h` and
`stdnoreturn.h`. The existing portable headers derive declarations from this
macro inventory; no host include-next fallback is enabled. Vendor intrinsic
headers (including private Keel wrappers), `stdatomic.h` and libc are unavailable
from the embedded inventory. Explicit user directories can supply their own
headers and take precedence. The caller owns those headers' target compatibility.
Absolute include names are rejected for Wasm; ordinary relative source/header
includes remain supported. No implicit `/usr/include`, SDK, framework, CPATH,
C_INCLUDE_PATH, SDKROOT or compiler resource discovery runs on this path.
Both an explicit sysroot and any Darwin deployment setting are rejected.
Native search and embedded-header behavior remain unchanged.

Ordered user macro definitions/undefinitions retain their existing semantics;
they cannot change the parser's explicit target/model or authorize unsupported
operations. `dump_macros` returns this same inventory after overrides.
`preprocess_with_stats`, when built with `profile`, uses exactly the same guarded
preprocessing path. Compile profiling remains unavailable as described above.

## Validation and scope

`wasm_profile` checks public preprocessed-C layout, empty units, declaration-only
extended types, native LP64 controls and rejection of unsupported bodies,
prototypes, externs, debug/profiling and unused section/linkage/lifecycle attrs.
`wasm_freestanding` preprocesses the maintained raw fixture through embedded
headers, verifies actual WASM32 IR, emits the bounded runtime module, and uses
Node's independent validator/runtime for repeated checksum, local array, direct
helper and static pointer execution with memory sentinels. Existing Clang checks
the declaration fixture and the shared target macro inventory independently.
Native macro/header/type/ABI and private Wasm runtime tests remain controls.
Negative tests cover host/vendor/private headers, include capability queries,
sysroot/deployment, ordered overrides and ambient host paths in a subprocess.
Generated evidence remains outside Git; no package/toolchain installation or
performance campaign is part of this checkpoint.

Use the shared suite lock and existing pinned toolchain/features, for example:

```sh
python3 suite/fast_opt_locked.py /tmp/wasm-c-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c --features profile
python3 suite/fast_opt_locked.py /tmp/wasm-c-release-unique.jsonl \
  cargo test --release --offline --locked -j2 -p keel-c \
  --features profile --test wasm_profile --test wasm_freestanding \
  --test wasm_target_unavailable --test environment --test contracts \
  --test gnu_types --test x86_intrinsics
```

This remains a bounded single-translation-unit library profile. No public CLI
Wasm output, optimizer enablement, native bridge, general C ABI, libc/WASI,
imports, dynamic linking, FP execution or performance claim is established.

## Independent C review (2026-10-24)

The independent review reapplied C candidates `756f1611` and `2331a270` over
`05fe6c68` plus the separate guarded Target prerequisites. It audited public
preprocessed compilation, raw preprocessing and profiling, explicit model
selection and verification, pre-pruning storage/signature/attribute gates,
builtin capability ownership, macro identities and suffixes, portable headers,
ambient include isolation, and native controls. Target CLI/link policy remains
owned by the separate Target review; this C review does not approve that policy.

One correctness finding was fixed: `export_name`, `import_module` and
`import_name` fell through the generic ignored-attribute parser, silently losing
Wasm symbol contracts. A public-compile regression reproduced acceptance before
the fix. These attributes now diagnose explicitly on the Wasm target, including
unused definitions; native handling is unchanged. Existing Clang independently
emitted `wasm-export-name` metadata for the renamed-export witness. New negative
coverage also checks opaque `va_list`/binary128 storage and values after raw
header preprocessing, native capability macro families, more ambient include
environment variables, and a fake on-disk `<keel>/include` fallback inventory.

At reviewed C code `38175e39`, the full C debug suite with `profile` passed 321
tests. Focused release passed 41 tests across `wasm_profile`, `wasm_freestanding`,
`wasm_target_unavailable`, `environment`, `contracts`, `gnu_types` and
`x86_intrinsics`. Production `cargo check -p keel-c --features profile` passed.
Public raw-C execution ran with Node's independent Wasm validator/runtime;
Clang declaration checks and the inventory oracle comparing over 250 shared
macros also passed. Private Wasm pointer/data execution and native LP64/ABI/header
controls passed in the full debug run. This remains finite subset validation,
with the compile/runtime/ABI limitations above retained.

All Rust validation used `CARGO_CACHE_LINKS=0`, the shared suite lock, existing
pins/default cargo cache, and `--offline --locked -j2`. Logs and lock metadata are
`/tmp/wasm-c-review-{final-debug,release,prod}-20261024-a60b.{log,jsonl}`;
the metadata regression's before/after logs are
`/tmp/wasm-c-review-metadata{-fixed,}-20261024-a60b.log`.
Generated LLVM evidence is `/tmp/wasm-c-review-metadata-a60b.ll`.
Changed tests pass targeted rustfmt checks and the diff passes whitespace checks.
Workspace-wide `cargo fmt --all -- --check` reports existing unrelated formatting
differences; this review did not reformat those files. No performance, remote
host, installation, main integration or cleanup work was performed.
