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
- CLI `asm`, Keel-owned `link` and `archive` reject Wasm before input/output
  processing. Explicit `link --system` selects external Clang functionality and
  is outside Keel's Wasm rejection gate; see the compatibility boundary below.
  Assembler APIs themselves are architecture-specific
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

## System-link compatibility boundary (revised contract)

The user revised the original zero-Clang-spawn requirement after the argument
audit. `keel link --system ...` remains the explicit, opaque Clang pass-through.
Clang owns target selection, repeated options, option operands, response files,
configuration files and environment interpretation. This includes external Clang
Wasm selection: it does not enable Keel's Wasm frontend, backend or native linker.
Keel neither expands nor validates response/config files on this route. Nested,
quoted, escaped, malformed, missing and cyclic response files are forwarded to
Clang, which owns their diagnostics and resource limits. No Clang dependency,
option table or partial driver scanner is added.

Put `--system` immediately after `link` for the full Clang argument surface.
It may also follow a prefix of Keel native arguments: Keel skips the operands of
its own value-taking options while locating the selector. For example,
`link --target x86_64-unknown-linux-gnu --system ...` selects Clang, whereas
`link -o --system input.o` names a native output file. A standalone `--` or an
option outside Keel's native syntax ends selector discovery; a later `--system`
does not switch routes. In particular, put the selector **before** Clang-only
options such as `-Xlinker`, `-Xclang` and `--config=...`. Keel does not guess their
arity. Response-file contents cannot select the Keel route.

Only the selected `--system` token is removed. Every other argument retains its
order and bytes, including subsequent `--system` operands/filenames and tokens
after `--`, except the existing literal-token translation of `--sysroot` and
`-syslibroot` to `-isysroot`. That legacy translation applies to direct argv
tokens even after `--`; it does not inspect response/config contents or translate
joined spellings. The environment is inherited. This preserves the existing
translation rather than attempting Clang option interpretation.

Without an explicit system selection, Keel rejects every genuine occurrence of
`wasm32-unknown-unknown` in the four guarded target spellings (`--target VALUE`,
`-target VALUE`, `--target=VALUE`, `-target=VALUE`) before native input/output
processing or external tool discovery. This is deliberately conservative for
repeated targets: a later native target does not cancel an earlier Wasm request.
Known native option operands and filenames after `--` are excluded. Native target
dispatch otherwise retains its existing syntax; the guard does not add Clang
target aliases or response expansion to the native linker.

The scope decision follows LLVM 21.1.0 primary sources:
[driver entry point](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/tools/driver/driver.cpp)
expands response files and handles argument-rewriting environment variables;
[Driver.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/Driver/Driver.cpp)
loads explicit/default configuration files and computes the target;
[Options.td](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/include/clang/Driver/Options.td)
defines version-specific operand consumption and driver modes; and
[CommandLine.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Support/CommandLine.cpp)
implements response tokenization, encoding and nested inclusion. These establish
why an argv substring scan cannot prove the external driver's effective target.

`keel --test system_link_args` uses an isolated fake Clang recording NUL-separated
arguments. It covers all four target spellings, repeated targets, native controls,
target-looking output names, literal `--system` operands, `--`, native-prefix
selection, opaque Clang options, nested/quoted response paths and malformed,
missing/cyclic response fixtures. These are forwarding checks, not claims that
Clang accepts every fixture. Native Wasm cases assert no driver record and an
unchanged destination; real ELF and Mach-O tests retain native controls. No real
Clang is invoked for the external Wasm forwarding cases.

Fresh focused validation passed 17 tests in each of debug and release on Apple
ARM64: `system_link_args` (6), `wasm_target_unavailable` (2), `elf_link` (7) and
`macho_link` (2). This includes native Mach-O behavior and host-independent ELF
image checks; it does not claim native Linux execution. Reproduce with:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-link-args-unique.jsonl \
  cargo test --offline --locked -j2 -p keel \
  --test system_link_args --test wasm_target_unavailable \
  --test elf_link --test macho_link
```

Add `--release` for release. Final lock records are
`/tmp/keel-wasm-arg-fix-02a18ad6-final-debug.jsonl` and
`/tmp/keel-wasm-arg-fix-02a18ad6-release.jsonl`. Focused rustfmt and
`git diff --check` pass. These checks validate the revised compatibility
boundary; the parent owns new independent review and integration.

## Historical independent review: original system-link guard

The findings below describe the original, now superseded pre-spawn rule. P1 is
resolved by the explicit contract revision above, not by claiming response-file
inspection. P2 is resolved by forwarding system-route option operands unchanged.
The parent must obtain a new independent review before integration.

Review baseline is candidate `acddcff5`, reapplied on `05fe6c68` as `e7bc6a1c`.
The public C rejection checkpoint remains unchanged. The exhaustive and
nonexhaustive target audit found two unresolved issues in
`crates/keel/src/link.rs::run`; this review does not approve integration:

- **P1: response files bypass the pre-spawn rejection.** With a response file
  containing `--target=wasm32-unknown-unknown`, `keel link --system @file` forwards
  the file to Clang without examining its arguments. A marker executable named
  `clang` confirms that a child is spawned; real Apple Clang 21's `-###` dry run
  confirms that this response file selects `wasm32-unknown-unknown`. The external
  driver can therefore enter a Wasm pipeline despite this checkpoint's gate.
  Nested response files and other driver argument indirection need consideration
  when fixing the boundary. No Wasm object or executable was generated for review.
- **P2: option operands are mistaken for target options.**
  `keel link --system -o --target=wasm32-unknown-unknown missing.o` is rejected
  as a Wasm target although the target-looking token is an output filename.
  Clang accepts it as the operand of `-o`; this is a native argument-surface
  regression. The gate must distinguish target options from other option values.

Simply banning response files would remove part of the existing system driver's
native argument surface. A repair must reconcile that compatibility requirement
with rejection before spawning Clang; it must not enable public Wasm output.
The parent owns that resolution and integration. The separate frontend worker
must preserve the CLI and native backend gates when rebasing.

An additional Unix CLI regression uses an isolated marker executable to verify
zero Clang spawns for all four direct target spellings, with a native target
positive control proving the marker detects actual invocation. It does not cover
or claim to fix the response-file bypass. Probe evidence is
`/tmp/keel-wasm-review-9e78570c-probes/{probes.json,clang-response.txt}`.

Independent validation passed 105 tests in each of debug and release: the five
focused groups above, adding the complete C `environment` integration test and
the Clang spawn regression. The host-independent Portable-context unit control,
predefined-macro rejection, and preprocessing rejection with nonexistent include
paths passed. Workspace `check --workspace --all-targets --features keel-c/profile`
also passed, with existing dead-code warnings in unrelated codegen test helpers.
Every compilation used the prescribed suite lock, offline/locked dependencies,
two jobs, existing toolchain and shared cargo cache with `CARGO_CACHE_LINKS=0`.
Logs and lock records use `/tmp/keel-wasm-review-9e78570c-*`.
The changed Rust test passes focused rustfmt checking and `git diff --check`.
Workspace-wide `cargo fmt --all -- --check` reports pre-existing formatting
differences in other files; those files were left unchanged. These checks do not
supersede the unresolved system-link findings or establish Wasm execution support.
