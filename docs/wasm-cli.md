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

Without `--wasm-opt basic`, no optimizer runs, including a nominal level-zero
pipeline or native canonical restoration. The opt-in basic profile runs exactly
one fixed sequence: `const-fold`, `simplify-cfg`, `phi-simplify`, `dce`. It calls
the builtins directly, with no native tiers, canonical restoration, host tuning,
vectorization, module passes, custom pass selection or pipeline convergence
loop. Every pass is followed by whole-module `DataLayout::WASM32` verification in both debug
and release. Pointer conversions, normalization and storage layout remain owned
by the Wasm frontend/backend. Ordinary potentially faulting loads, volatile loads
(even `volatile notrap`),
divisions and other pinned effects remain in order with exact access widths.
DCE may remove an unused nonvolatile `notrap` load under the existing producer
promise that it cannot trap and its read has no observable effect. This explicit
exception does not authorize marking any new load `notrap`; raw IR producers
remain responsible for that promise. Native DCE is unchanged.

The CLI non-emitting runtime preflight checks the complete ORIGINAL module
before any optimizer runs, including unexported functions, signatures and dead
instructions. Unsupported input therefore cannot become accepted after constant
branch pruning or DCE. The final ordinary emitter independently repeats runtime
legality checks and validates complete MVP bytes before publication. This adds a
preflight and a module clone to the opt-in route; no compile-time or runtime gain
is claimed. The no-option route still calls the emitter directly.

`--wasm-opt` accepts only the separate argument `basic`; repeated valid requests
are idempotent. Native `-O` still means native Full plus instruction-selection
presets, so it is deliberately not reinterpreted for Wasm. All native
optimization/pass/verification/report flags continue to reject:
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
The original CLI checkpoint did not change any frontend, IR, optimizer, backend, native
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

## Independent CLI review (2026-10-24)

Candidate `3dd569dc` (`ea4680ee`, `788cd8be`, `3dd569dc`) was reapplied onto
`e24cba6d` as `2a138c59`, `9f68b6e0`, `b4ddcb8c`. No blocking production defect
was found. The audit checked flag presence rather than final default values,
dedicated routing before native input/optimization/output handling, raw C versus
preprocessed `.i` versus raw IR, target-tagged artifact rejection, actual WASM32
runtime verification, exact backend export options, u32 memory bounds, and
exclusive same-directory publication with failure cleanup. The existing backend
gates and native/Portable CLI behavior remain intact; no production change was
needed.

Review commit `16a0395b` adds three maintained integration tests. They check that
overriding forbidden options cannot hide their presence, extra inputs reject,
and unimplemented joined/equals spellings and `--` reject before input loading.
This driver requires separate `-o path`, `--export name`, and Wasm numeric flag
arguments; `-opath` and `--flag=value` are not added by this checkpoint. Duplicate
output and memory options retain last-value semantics. All four numeric flags
reject signs, whitespace, hexadecimal, fractional and overflowing values.
Node validates and compiles maximum 65536-page memory with high unsigned stack
bounds without instantiating it. Symlink and hard-link output aliases preserve
their targets on both failed compilation and successful atomic replacement.

Reviewed source `16a0395b` passed 62 tests in each of debug and release: 38 binary,
12 `wasm_output`, 2 `wasm_target_unavailable`, 7 `system_link_args` and 3 existing
`x86_cpu_level` controls. Both profiles execute the Node witnesses. Locked cargo
check with `--all-targets` and build passed without compiler warnings. Every Rust
compilation used `python3 suite/fast_opt_locked.py`, the existing
shared lock, `cargo`, `--offline --locked -j2`, existing pins/features
and default managed cache/storage. Evidence stays outside Git:
`/tmp/wasm-cli-review-{final-debug,release,check,build}-dcabf804.{jsonl,log}`.

Targeted rustfmt checks of all changed Rust files and `git diff --check` pass.
`cargo fmt -p keel -- --check` reports an existing unrelated formatting difference
in `crates/keel/tests/hidden_visibility.rs`; that file is unchanged. Publication
create/rename failure paths are exercised; write/sync/close failures and process
termination are not fault-injected. Maximum-memory validation is not a 4 GiB
allocation/execution claim. This finite review adds no imports, FP, SIMD,
optimizer, native ABI or memory semantics, performance claim, remote access or
main integration. Parent owns integration and cleanup.

## Opt-in basic profile validation (2026-10-24)

Candidate CLI `9bccf212` uses `optimize_wasm32_basic` from `b9059e44` plus
`546c0dab`, with the separate runtime admission prerequisite `82d3244e`
(locally cherry-picked as `e2989057`). The completed safety audit is
`49cd1747`, [audit contract](wasm-basic-optimization-audit.md). Parent owns
independent implementation review and integration; these tests do not replace it.
The optimizer crate does not depend on the backend: its entry point documents
that backend admission of the complete original input is the caller's obligation.

On Apple ARM64 with Node v24.22.1, 89 focused tests passed in each of debug and
release: 38 CLI binary, 17 `wasm_output`, 2 `wasm_target_unavailable`, 7
`system_link_args`, 3 `x86_cpu_level`, and 22 optimizer profile/DCE/CFG/phi/golden
tests. The maintained C and raw-IR corpus in `suite/wasm-basic` exercises C ILP32
static pointers, checksum/local arrays/direct calls/recursive normal returns,
i64 BigInt, dynamic and constant division traps, unused faulting loads,
volatile-plus-notrap loads and store ordering, one-byte canaries, high-bit pointer
round trips, wrapping pointer addition followed by an actual load, dead stack
addresses with retained frame allocation, and recursive stack exhaustion.
A deliberately false raw-IR `notrap` promise demonstrates the documented DCE
exception; it is not a defined-program equivalence claim. Unexported unsupported
FP instructions/signatures, constant-dead FP blocks and unused external function
declarations reject identically before and after requesting the profile. A
malformed global initializer test establishes verification before transformation,
and align-4 pointer data is accepted only with the explicit WASM32 verifier.
Native CLI and Portable optimizer controls pass unchanged.

All Rust compilation used `python3 suite/fast_opt_locked.py`,
existing pins/shared cache, `cargo`, `--offline --locked -j2`, and no
feature changes. Evidence is `/tmp/wasm-basic-final-{debug,release,check,build}-20261024.jsonl`;
debug output is `/tmp/wasm-basic-final-debug-20261024.log`. The all-target check
for `keel` and `keel-opt`, release CLI build, targeted rustfmt checks and
`git diff --check` pass without warnings. No dependencies, toolchains, cache
policies, native behavior or backend emission changes belong to this candidate.

The finite size/count pilot uses the existing dispatcher emitter, holding backend
emission constant between unoptimized and basic requests. C module bytes are
1546/1490, summed function-body bytes 1406/1350, declared locals 117/117, and
encoded instruction counts 652/620 (plain/basic). Raw-IR values are 1292/1198,
1058/965, 74/67 and 536/492 respectively. Body bytes include local declarations
and exclude the enclosing body-length LEB; instruction counts include structural
operators and local accesses. `measure.py` fails on unknown opcodes, validates
modules in Node, and checks equal result sinks. These counts describe this corpus
only; they are not results for the separate straight-line emitter candidate.

Five alternating serialized CLI compile samples and five warmed Node batches per
profile were captured with the shared suite lock. Node batches include JavaScript
and BigInt overhead; CLI times include process startup, preprocessing where
applicable, both legality gates, and the optimizer's module clone. The host had
one-minute load 2.12 on 18 logical CPUs. Full raw samples, binary/corpus hashes,
bytes and counts are in `/tmp/wasm-basic-evidence-20261024-r2/report.json`;
the lock record is `/tmp/wasm-basic-measure-final-20261024.jsonl`. This small
pilot does not isolate the cost of original admission or establish application,
compile-time, runtime or cross-host gains. To reproduce with a built release CLI:

```sh
python3 suite/fast_opt_locked.py /tmp/wasm-basic-build-unique.jsonl \
  cargo build --release --offline --locked -j2 -p keel
python3 suite/fast_opt_locked.py /tmp/wasm-basic-measure-unique.jsonl \
  python3 suite/wasm-basic/measure.py target/release/keel /tmp/wasm-basic-unique
```

The destination directory must be new. The script declines sampling when the
one-minute host load exceeds half the logical CPU count; all generated modules
and reports stay under the supplied temporary directory.
