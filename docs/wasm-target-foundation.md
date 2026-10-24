# Recognized wasm32 target, public pipelines unavailable

This checkpoint builds on main `05fe6c68` and the
[layout migration gates](wasm32-layout-plan.md#variant-and-cli-migration-gates).
It exposes target identity and storage layout with explicit rejection gates;
it does not enable the planned public C profile. The private
[C pointer/data](wasm-c-pointer-data.md) and [memory backend](wasm-memory-backend.md)
checkpoints keep their existing boundaries. Parent integration and independent
review are separate.

## Public contract

`keel_ir::Target::Wasm32UnknownUnknown` recognizes exactly
`wasm32-unknown-unknown`. `Target::data_layout()` is a const storage query:
this variant returns `DataLayout::WASM32`, and all three native variants return
`DataLayout::NATIVE64`. It is not a C data model, ABI classifier, optimizer
context, native backend selection, or runtime configuration. No target is added
to `Module`, and existing native layout wrappers retain their old semantics.
`Target::host()` and `Default` are unchanged.

`Target::ALL` now enumerates four **recognized** targets, retaining the original
three entries in order and appending Wasm. It is not a list of usable C/codegen
targets. The additive `Target::NATIVE` enumerates the original three targets for
native success inventories and their existing controls. Wasm has separate public
API and CLI rejection tests; changing native loops does not establish Wasm support.
No WASI, wasm64, shortened Wasm triple, C ABI, object format, native bridge, or
Wasm output mode is introduced.

## Routing audit and gates

The workspace audit covered exhaustive matches and equality/default fallbacks,
including optional profiling entry points and paths with empty modules:

- `keel_c::{compile,profile::compile}` reject before lexing/parser construction.
  `preprocess` and `preprocess_with_stats` reject before macros, include discovery,
  source scanning or file access. `predefined_macros` already returns `Result`;
  it rejects without constructing a native macro inventory, even with a Darwin
  deployment argument. Internal capability queries conservatively return zero.
  Public options/signatures and production C model mappings are unchanged.
- Native C parser/IR generation fallbacks for va_list, char/long-double policy,
  aggregate classification, debug types, symbols and headers remain behind the
  public frontend gate. New exhaustive parser arms diagnose unsupported target
  operations; they do not implement Wasm declaration semantics. Private Wasm
  layout/lowering continues using its existing test-only entry points. Its native
  control loop now uses `NATIVE`; lowering/runtime code is unchanged.
- `keel_codegen` text, options, pipeline, typed-program and chunked-program APIs
  return `CodegenError` before selection or chunk delivery. Generic CPU tuning,
  selector defaults, pipeline validation and optimizer capability projection also
  reject Wasm. A custom selector that accepts every target cannot bypass the
  public codegen/capability gates. Native custom-selector dispatch is preserved.
- IR native section and full-module section validation reject Wasm, including an
  empty module. Toolchain artifact encode/decode reject Wasm before verification
  or parsing; the existing IR-unit format/version is unchanged. A target-tagged
  artifact is not a Wasm object, nor proof of the C layout that produced raw IR.
- CLI `cc` (including `-E`, `-dM`, `.i`, `--emit-ir`), `compile` and `opt` reject
  explicit Wasm after option parsing and before native tuning, input loading,
  include discovery, optimization, reporting or output. Native object sinks have
  explicit unsupported-target handling. Native CPU/features, optimization,
  selection and allocation flags cannot authorize Wasm. Target-free `opt` retains
  its Portable context without resolving a host target; no optimizer pass or
  canonical/custom context dispatch is changed.
- CLI `asm`, `link` and `archive` reject Wasm before input/output processing.
  `link --system` is guarded before launching Clang, including Clang's `-target`
  and target-equals spellings. Assembler APIs themselves are architecture-specific
  (AArch64 or x86), with no `keel_ir::Target` argument. Link/archive APIs use
  separate closed native machine/format enums; no Wasm-to-native conversion is
  added. LLVM and the excluded experimental Cranelift workspace likewise retain
  their separate closed native `TargetProfile` APIs.
- `keel-opt` has a SIMD `VectorTarget`, not a memory-layout target. Its targetless
  APIs cannot validate a caller-owned target/layout pairing. CLI rejection prevents
  Wasm modules entering its default or requested pipelines; direct private Wasm
  backend use still requires unoptimized input under its documented contract.
  Toolchain memory instrumentation also remains a native-only, targetless API.
  Assembly IR, architecture-specific selection/register allocation, frontend SSA
  builders and suite host defaults do not acquire Wasm semantics.

There are no ABI, FP, memory-operation, dependency, toolchain, cache-policy or
performance changes. Existing native x86/NEON contracts in
[userspace ISA status](userspace-isa-status.md),
[C intrinsics](c-x86-intrinsics.md) and [GNU modes](gnu-vector-mode.md) remain
native contracts. Unsupported public routes return errors instead of silently
selecting native behavior or panicking. Rejections preserve caller-owned outputs
and do not create optimization reports or batch assembly output directories.

## Focused validation

Local Apple ARM64 validation uses the existing shared suite lock, toolchain pins,
offline/locked dependencies, two jobs and cargo's shared default cache with
`CARGO_CACHE_LINKS=0`. Generated evidence stays under `/tmp`.

The focused debug and release groups each passed 97 tests on 2026-10-24.
Two additional native va_list/header controls passed in each profile.
The final workspace check also includes all targets and the existing C `profile`
feature so both public profiling guards compile. The groups are:

- `keel-ir`: `targets`, `data_layout`, `sections` (enumeration, exact triple,
  const layouts, host/default controls and native section rejection).
- `keel-codegen`: `wasm_target_unavailable`, `selection`, `tuning`, `x86_features`
  (all public text/program/chunk paths, permissive custom selector, zero sink
  deliveries, capabilities and existing native selection/feature controls).
- `keel-c --features profile`: `wasm_target_unavailable`, `gnu_types`,
  `gnu_vector_broadcast` (ordinary/profiling APIs, macros, preprocessing and native
  type/vector controls).
- `keel-toolchain`: `foundations` (Wasm transport rejection and native controls).
- `keel`: binary unit tests and `wasm_target_unavailable` (negative output routes,
  CPU/tuning/optimization flags in both orders, output sentinels, reports,
  target-free Portable optimization and native text/direct object controls).

Reproduce each group with this wrapper, adding `--release` for release:

```sh
python3 suite/fast_opt_locked.py /tmp/wasm-target-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-ir \
  --test targets --test data_layout --test sections
python3 suite/fast_opt_locked.py /tmp/wasm-target-check-unique.jsonl \
  cargo check --offline --locked -j2 --workspace --all-targets
```

This is bounded routing validation. Local x86/AArch64 Linux objects are cross
outputs, not native execution on those machines. No Wasm execution or public C
acceptance is claimed here. The next guarded C profile still needs the complete
macro inventory, freestanding header/include policy, declaration/type model,
unsupported-feature diagnostics and the remaining frontend/runtime acceptance
gates in the layout plan before public C/preprocessing can be enabled.

Evidence records are `/tmp/keel-wasm-target-{debug,release}-20261024.jsonl` and
`/tmp/keel-wasm-target-final-20261024.jsonl`. Group logs use
`/tmp/keel-wasm-target-{debug,release}-{ir,codegen,c,toolchain,cli}-20261024.log`;
final changed-codegen/CLI runs, native `contracts`/`environment` controls and
workspace check use `/tmp/keel-wasm-target-final-*-20261024.log`.
The workspace check reports existing dead-code warnings in other test helpers;
there are no new warnings in the target foundation/rejection tests.
