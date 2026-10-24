# Keel / Clang freestanding Wasm comparison at e3391b54

On MacHost (Apple ARM64), 2026-10-24, the unchanged eight-case integer/pointer
corpus passed 640 independently checked invocations across Keel plain/basic and
Clang `-O0`/`-O2`. No compiler correctness defect was reproduced in this corpus.
The clear observed gaps are emitted code size, local/copy volume and dispatcher
control flow. Timing was deferred because the required quiet gate detected an
active Rust compiler; no runtime or compile-time speed comparison is claimed.
Parent owns independent review, integration and cleanup.

## Frozen inputs and tools

Base is clean `e3391b547f74cf59d9f47939f7d05c5a820d9f14`. Maintained inputs and
runner are in [suite/wasm-comparison](../suite/wasm-comparison/README.md).
The identical `corpus.c` SHA-256 is
`6323eab334c63c49344cf2a792b9a435787d93002a229a7b7abd7ab7c6bd38cf`.
There are eight selected cases and zero excluded unsupported cases. This is a
purpose-built supported-subset corpus, not a claim that arbitrary C is accepted.
FP, SIMD, atomics, imports/WASI/libc, indirect calls and aggregate/pointer/narrow
call boundaries are outside scope. No `-Os` campaign was added.

Release Keel SHA-256 is
`9fb2c1f539d67fd39b1eb78df69a664cb006095bda4dc37ee962c1351e321f5a`.
The CLI has no working `--version` query, so source revision and binary hash pin
it. Apple Clang is 21.0.0 (`clang-2100.3.34.2`), `/usr/bin/clang`, SHA-256
`34129c71a01a74f7f3b2443521519b2e5447553fa187f5fcafaaf8c42cc192e2`.
An existing linker was found in the active Rust sysroot:
`~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/gcc-ld/wasm-ld`,
LLD 23.1.1, SHA-256
`09737dab0707f7a3273a07d112594408880105fb5c81d13340cba241c1daa6df`.
Node is v24.22.1 at
`~/.local/share/vite-plus/js_runtime/node/24.22.1/bin/node`, with no engine flags.
Its hash and all absolute tool/command paths are in the frozen manifest.
No installation, remote host, toolchain switch or cache/global configuration
change was used.

Keel uses `cc --target wasm32-unknown-unknown --emit-wasm`, explicit exports,
one fixed memory page, data floor 1024, stack bottom 32768, stack top 65536.
Basic adds exactly `--wasm-opt basic`; plain runs no optimizer. Clang uses the
same source and Keel portable header directory, with:

```text
--target=wasm32-unknown-unknown -mcpu=mvp -std=c11 -ffreestanding
-nostdlib -nostdinc -fno-builtin -fno-lto -fno-fast-math
-O0 (or -O2), explicit -fuse-ld=<existing wasm-ld>
-Wl,--no-entry -Wl,--export-memory -Wl,--initial-memory=65536
-Wl,--max-memory=65536 -Wl,--stack-first -Wl,--global-base=32768
-Wl,-z,stack-size=32768 -Wl,--strip-all, explicit function exports
```

Clang optimization levels intentionally differ from Keel's four basic passes;
these are available configurations, not equal optimization work. Clang links a
complete module while Keel emits one directly. Both own fixed 64-KiB memory and
a 32-KiB software stack budget, but Clang places its stack below static data;
Keel places static data below its checked stack. Clang stack frames/checks and
Keel allocation guards differ. No stack-exhaustion, memory-placement or general
ABI equivalence follows. The corpus stays within bounds, and its own scalar
address getters normalize object placement.

## Stage outcomes and correctness limits

* Build: locked release Keel build passed with existing Rust 1.99.1, default
  managed cargo cache, `CARGO_CACHE_LINKS=0`, `--offline --locked -j2 -p keel`.
* Emission/link: all four final configurations produced complete modules.
* Static policy: bounded decoder accepted only the integer MVP operators used
  here, integer signatures, no imports/tables/start/elements, and fixed nonshared
  one-page memory. All functions, including the recursive helper, were decoded.
* Validation: Node `WebAssembly.validate` and module construction passed for all
  four; imports were independently required to be empty before instantiation.
* Execution: each case used seeds 0, 1, 0x80000000, 0xffffffff and lengths
  0, 1, 31, 1024, 16384. BigInt scalar arithmetic supplied independent expected
  results, including unsigned i64 wraparound. Every invocation checked the exact
  1,040-byte object, including both eight-byte canaries. Only pointer mutation
  may alter the data bytes. Unrelated linear-memory/stack bytes were not equated.
* Separate valid-IR controls: 18 existing CLI Wasm tests and five optimizer
  Wasm32 tests passed in release, including division/load traps, volatile order,
  pointer phi edges, retained frames and recursion. Trapped instances are
  discarded. These controls are not undefined-C comparisons with Clang.

## Static results

All custom sections (names, producers, target-features and debug payloads) are
removed uniformly before comparison; raw outputs are retained. Body bytes
include local declarations and exclude each enclosing body-size LEB. Operator
counts include local accesses and structural operators, not immediate bytes.

| Configuration | Whole module bytes | Sum of bodies | Declared locals | Operators |
| --- | ---: | ---: | ---: | ---: |
| Keel plain | 3052 | 2777 | 204 | 1321 |
| Keel basic | 2940 | 2665 | 204 | 1257 |
| Clang -O0 | 1569 | 1347 | 20 | 601 |
| Clang -O2 | 1453 | 1231 | 14 | 609 |

These sums include two address getters and the internal recursive helper.
Keel basic bodies are 2.16 times Clang `-O2` bodies for this module. This is a
byte ratio, not a speed ratio. Basic removes 112 body bytes versus plain while
retaining all 204 declared locals. Clang `-O2` is not uniformly smaller than
`-O0`: its integer loop, checksum, mutation and local-array bodies grow, consistent
with different optimization choices; aggregate counts must not hide that.

| Function | Keel plain | Keel basic | Clang -O0 | Clang -O2 |
| --- | ---: | ---: | ---: | ---: |
| scalar | 114 | 114 | 63 | 26 |
| wide | 131 | 131 | 74 | 37 |
| integer_loop | 238 | 224 | 112 | 176 |
| branch_switch | 510 | 496 | 229 | 108 |
| checksum | 272 | 258 | 114 | 188 |
| pointer_mutate | 367 | 353 | 208 | 209 |
| local_array | 546 | 518 | 259 | 337 |
| recursion (exported loop) | 193 | 179 | 121 | 36 |
| recursive_helper (internal index 10) | 260 | 246 | 146 | 98 |

Largest basic-minus-`-O2` exported-body gaps are branch/switch (+388 bytes),
local array (+181), pointer mutation (+144), and recursion caller (+143).
The caller plus helper together are 425 versus 134 bytes. These rankings are
specific to the frozen module; the helper remains internal in every profile.

Three concrete emission opportunities warrant separate implementation review:

1. Dispatcher cost on nontrivial CFGs. Basic still has 72 `if`, 42 unconditional
   `br`, and zero `br_if` operators, versus Clang `-O2`'s zero `if`, seven `br`
   and 23 `br_if`. The branch/switch case is the largest absolute gap. Structured
   CFG emission or narrow acyclic special cases could reduce dispatch machinery;
   these counts do not establish the safety or runtime benefit of a rewrite.
2. SSA materialization and local assignment. Basic retains 204 locals, 327
   `local.get` and 245 `local.set`; Clang `-O2` has 14, 145 and 31 respectively
   (plus 29 `local.tee`, versus Keel's 11). Even direct scalar/wide bodies remain
   much larger. Local reuse and expression-stack emission are candidates;
   liveness, aliases, trap order and calls need independent validation.
3. Pointer width conversions and frame overhead. Basic has 15 `i32.wrap_i64`
   and five `i64.extend_i32_u` operators, versus zero and one in Clang `-O2`.
   The IR and mutation fixture expose the pointer/cast chains. Some conversion
   instructions are required by Keel's wasm32 normalization contract; raw count
   does not prove redundancy. Keel's checked runtime prologue also applies to
   tiny address getters (65/81 bytes versus Clang `-O2`'s 8/8). Removing guards
   or frames would change documented exhaustion/placement behavior and is not
   justified by this comparison.

No production compiler edit is included. See the independently owned gap audit
for source-stage analysis; this report does not approve a broader optimizer,
stack-slot deletion or relaxed bounds/ABI semantics.

## Timing status and evidence

The single final attempt's preflight saw Rust compiler PID 49129 at 95.9% CPU;
the next gate saw 98.6%. Load was 2.90 on 18 logical CPUs, but the process-level
quiet requirement failed. The report records `deferred: quiet preflight/gate
failed`. There are no runtime samples, compile samples, medians, ranges or speed
ratios to report. Correctness execution was not timed as a benchmark. No retry,
remote run or simultaneous profiler was started.

The maintained runner contains the requested finite timing protocol for a later
authorized quiet run: seven seeded interleaved paired rounds in one Node process,
12 fixed warmup batches, substantial internal Wasm work, separate getter wrapper
samples, raw samples/medians/ranges/ratios, consumed return values outside timed
batches, and preflight/per-round quiet checks. CLI samples include process
startup, preprocessing and Clang linking; diagnostic object emission is not mixed
with end-to-end samples. That timing path was not exercised in this run, fixed
warmup does not prove tier stability, and wrapper/scalar/i64 microcases remain
boundary dominated. No geometric mean or application-performance conclusion is
supported.

Final evidence directory is `/tmp/keel-wasm-comparison-e3391b54-20261024-r3`.
Its frozen `manifest.json` SHA-256 is
`a43403a5ad24b52ea120c11706def605500efda98923166d2a6953463559e01d`.
It contains raw/stripped modules, per-function body hex/operator listings,
Clang assembly/objects, correctness results, exact commands/tool hashes and quiet
failure records. `artifact-index.json` adds hashes of final maintained runner
snapshots and diagnostic IR dumps without mutating that manifest. Stripped hashes:

```text
plain    cea6217038a75089c2ad2916d1103df79be66095f129233514661c4055cd9c72
basic    3163ff56e32f4e229a55f832924474d8a8563754509aaf43692acaf1a5cb3b0b
clang-O0 a78b1d876e2b8ac78f3ed12a9e698c4a419997a64f4bbaf79314da0064356ba1
clang-O2 99eeeefaf1c0c30a850132866ead622a5cd4c49b1221353542a011e8cf52cb3b
```

Early r1 recorded a Clang placement error; an initial handoff incorrectly called
it linked before inspecting failure rows. That handoff was corrected. Frozen r2
linked and passed correctness; its signed-LEB immediate metadata was unsigned.
The peer caught that decoder defect. r3 corrects signed constants (including
five-byte negative i32 and ten-byte negative i64); binary hashes, byte counts and
operator counts are unchanged. Six deterministic decoder tests cover signed
constants, immediate boundaries, custom stripping and rejected features. Node
harness syntax checks, Python compilation and `git diff --check` pass.

Unoptimized/basic IR diagnostics use existing library preprocessing, compilation
and `optimize_wasm32_basic` APIs in an outside-Git helper, not an unsupported CLI
IR flag or production edit. Helper source/lock hashes are indexed. Its first
standalone lock generation selected newer cached dependency patch versions; the
final diagnostic helper was rebuilt with repository-pinned dependencies and
`--offline --locked -j2` before generating the consumed dumps. That initial
helper build is retained in the audit logs and did not produce the measured
Keel binary or final IR evidence. No dependency/toolchain was installed.

Build/test/measurement lock records:
`/tmp/wasm-comparison-{build,tests,final}-e3391b54-20261024.jsonl`.
Pinned helper build record:
`/tmp/wasm-comparison-ir-build-pinned3-e3391b54-20261024.jsonl`;
IR dump record: `/tmp/wasm-comparison-ir-dump-e3391b54-20261024.jsonl`.
All compilation/execution gates used the existing shared suite lock. No build
outputs were deleted, independent checkouts shared no `CARGO_TARGET_DIR`, and
no main integration was performed.
