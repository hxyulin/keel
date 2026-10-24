# Bounded freestanding wasm32 CLI output

The `keel` driver can emit a complete core MVP `.wasm` from one freestanding C
translation unit or explicitly supplied raw wasm32 Keel IR:

```sh
keel cc --target wasm32-unknown-unknown --emit-wasm \
  --export checksum --export local_array input.c -o output.wasm
keel compile --target wasm32-unknown-unknown --emit-wasm \
  --export entry input.keel -o output.wasm
```

`--export name` is repeatable and exports that exact function name without
renaming or native underscore decoration. There are no implicit function exports;
an empty export list is accepted. The owned linear memory is always exported as
`memory`. Missing or duplicate function exports, empty names and collisions with
`memory` are errors from the existing backend. C source `export_name` and import
attributes remain unsupported rather than silently changing these names.
`-o` is required and cannot name stdout (`-`). The suffix does not select the
format; `--emit-wasm` and the exact target triple are both required. Wasm flags
with a native/default target, or without `--emit-wasm`, are errors. Selecting the
Wasm target without `--emit-wasm` still rejects before native processing.

## Input and execution contract

Raw C uses the reviewed [freestanding public preprocessing and compilation
APIs](wasm-c-freestanding.md), including embedded portable headers, explicit
WASM32 layout, and whole-translation-unit unsupported-feature preflight. Files
ending in `.i` are already preprocessed and go directly to that compiler.
`-I dir`, `-isystem dir`, `-D name[=value]` and `-U name` retain their ordinary
ordered semantics for raw C. Explicit header directories are caller-owned target
inputs. No implicit host include path, SDK/environment discovery, libc, native
frontend fallback or external compiler is used. Ambient `SDKROOT` and Darwin
deployment variables do not select a Wasm sysroot or deployment target.

Raw IR has no embedded target identity. `compile --target
wasm32-unknown-unknown --emit-wasm` explicitly asserts that the user supplied
unoptimized wasm32 storage layout; the driver cannot infer producer provenance
or translate native C IR into wasm32 layout. Target-tagged `KEEL-IR-UNIT` artifacts
are rejected, including native units: this interface cannot relabel them. Raw
target declarations are not added to the IR syntax. Both input routes call
`keel_wasm::emit_wasm32_runtime_module`, whose explicit WASM32 verifier, complete
module preflight and independent MVP byte validation run before output is opened.

The accepted C/IR subset and scalar call boundary are exactly those in the
[public C contract](wasm-c-freestanding.md) and
[runtime backend README](../crates/keel-wasm/README.md). This includes integer
computations through i64, ordinary control flow, direct i32/i64 calls, bounded
local frames and recursion, and defined static data with data-pointer fixups.
It does not add a general C ABI or accept narrow/pointer/aggregate call boundaries.
FP, SIMD, atomics, function pointers, imports, unresolved externs, WASI/libc,
native objects/assembly, relocatable Wasm objects and object linking remain
unsupported. There is no start function or implicit call to C `main`.

No optimizer runs, including a nominal level-zero pipeline or native canonical
restoration. All explicit optimization/pass/verification/report flags reject:
`-O`, `-p`, `--opt-tier` (even `none`), `--verify-each`, `--no-verify-each` and
`--opt-report`. `-O0` is unsupported syntax and also rejects. Native CPU tuning,
features, register allocation, instruction selection, rematerialization, debug
`-g`, `--emit-object`, `--emit-ir`, preprocessing-only `-E`/`-dM`, explicit sysroot
and Darwin deployment controls also reject before input loading. Last-option
overrides cannot hide a forbidden flag's presence. Unimplemented import/link
flags are errors. This route never calls a linker, including `link --system`;
the separately reviewed explicit system-link pass-through policy is unchanged.
Native commands and target-free Portable `opt` keep their existing behavior.

## Fixed owned memory

The memory has equal initial and maximum page counts, no sharing or growth, and
the existing checked software stack. These optional flags accept only unsigned
decimal u32 values (no sign or hexadecimal notation); repeated values use the
last occurrence:

| Flag | Default | Meaning |
| --- | ---: | --- |
| `--wasm-memory-pages` | 1 | Fixed number of 65536-byte pages, at most 65536 |
| `--wasm-data-floor` | 1024 | First eligible byte for static data, nonzero |
| `--wasm-stack-bottom` | 32768 | Lowest stack byte; static data must end here or below |
| `--wasm-stack-top` | 65536 | Initial stack pointer, within configured memory |

The data floor must be at or below stack bottom, bottom must be at or below top,
and both stack bounds must be multiples of 16. Changing page count does not
automatically move the other bounds. Data placement and recursive frame sizes
must fit the configured regions. These bounds govern placement and stack checks;
they do not protect a null page or separate objects within linear memory.
The [runtime contract](wasm-memory-backend.md) governs exact access widths,
pointer normalization, static data placement, constant-data obligations and
software-stack behavior. Discard an instance after any trap: saved software
stack pointers are restored only on normal returns.

## Publication and validation

Frontend, verifier, unsupported-subset, export, and memory errors preserve an
existing destination. After complete validated bytes exist, the driver creates
a unique exclusive temporary file in the destination directory, writes and
syncs it, closes it, then renames it over the destination. Write/sync/rename
failures remove the temporary and leave the previous destination in place.
Inputs are unmapped before publication, permitting an input path as output.
This is atomic replacement, not a crash-durable directory-sync guarantee;
process termination or filesystem failure can prevent temporary cleanup.

The `keel` integration test `wasm_output` exercises actual CLI C/preprocessed-C/raw-IR output
with Node's independent validator/runtime: checksum, local arrays, direct helper,
recursive frames, static data pointers, byte sentinels, explicit exports, fixed
memory and deterministic equivalent raw-IR output. Tests also cover duplicate,
missing, empty and memory-colliding exports, unsupported verified IR, malformed
memory options, frontend/preprocessing failures, artifact relabelling, in-place
outputs, create/rename failures, unchanged sentinel destinations, temporary
cleanup, explicit headers/macros and ambient host isolation. Isolated marker
executables establish no native compiler/SDK/linker discovery or spawn; the
marker itself has a positive control. Existing CLI binary tests and unchanged
`wasm_target_unavailable`/`system_link_args` retain native and Portable controls.

Validation on the reviewed combined target/C base `e24cba6d` uses the existing
shared lock and pinned toolchain with the shared cargo cache, for example:

```sh
python3 suite/fast_opt_locked.py /tmp/wasm-cli-unique.jsonl \
  cargo test --offline --locked -j2 -p keel --bin keel \
  --test wasm_output --test wasm_target_unavailable --test system_link_args
```

Use `--release` for release validation. No toolchain/dependency installation,
cache policy change, remote host or performance campaign is needed. Only a
path dependency on the existing `keel-wasm` crate and its lockfile edge are added.
CLI output ownership does not change any frontend, IR, optimizer, backend, native
ABI, FP or memory implementation. Parent integration and new independent CLI
review remain separate.

On 2026-10-24, all 56 focused tests passed in both debug and release on Apple
ARM64: 38 binary unit tests, 9 `wasm_output`, 2 `wasm_target_unavailable`,
and 7 `system_link_args`. Both profiles executed the Node witnesses. Locked
`cargo check -p keel` and final `cargo check -p keel --all-targets` also passed
without warnings. Evidence is
`/tmp/keel-wasm-cli-final-debug-fixed-82cc536f.jsonl`,
`/tmp/keel-wasm-cli-release-82cc536f.{jsonl,log}` and
`/tmp/keel-wasm-cli-check-82cc536f.jsonl` and
`/tmp/keel-wasm-cli-final-check-82cc536f.{jsonl,log}`. A first tool-marker test used a
nonexistent macOS `/bin/touch` path; its positive control caught this test-helper
error. The final helper uses shell redirection and passes. No production semantic
correction was needed during these runs. Changed Rust files pass rustfmt checks
and the diff passes whitespace checks. These are finite correctness checks, with
no performance or cross-host native-execution claim.
