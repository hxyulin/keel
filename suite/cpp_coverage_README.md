# C++ coverage through Clang LLVM IR

`cpp_coverage.py` exercises **Clang C++17 → unmodified LLVM IR → the existing
`llvm_import.py` → Keel native objects**. It is a correctness and rejection
inventory, not a native C++ frontend, application coverage claim, or performance
comparison. Clang has already optimized the IR at the recorded production
level. This does not measure an end-to-end replacement for Clang.

## Reproduce

Use a private build directory and a new artifact directory on an Apple AArch64
host. The current importer requires LLVM 23's shared library. All compiler and
native validation work must use the existing shared suite lock:

```sh
work=$(mktemp -d /tmp/keel-cpp-coverage.XXXXXX)
python3 suite/fast_opt_locked.py "$work/lock.jsonl" \
  cargo build --offline --locked -j2 -p keel --target-dir "$work/target"
python3 suite/fast_opt_locked.py "$work/lock.jsonl" \
  python3 suite/cpp_coverage.py \
    --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
    --keel "$work/target/debug/keel" --abi-rustc rustc --out "$work/run"
python3 -m unittest discover -s suite -p test_cpp_coverage.py -v
```

`--clangxx` and `--clang` select compiler executables; `--target` selects an
explicit Clang target (otherwise `clang++ -dumpmachine` is recorded and used).
`--case NAME` is repeatable for a focused rerun. Omitted cases are not tested.
The optional `--abi-cc gcc-15` compiles a second C boundary with that compiler's
native target. `--abi-rustc rustc` builds the equivalent `no_std` Rust boundary
using Rust 2024, opt-level 2 and panic=abort. Both optional boundaries must
link/run and agree with the Clang boundary; incompatible targets are failures.
Use compiler version evidence to distinguish independent implementations from
aliases such as Apple's `/usr/bin/gcc` (which is Clang). The Rust fixture is
only an ABI peer; it is not input to a Rust frontend or to the LLVM importer.

The runner never builds Keel itself. `--keel` must name the intended frozen
binary. The README commands keep build outputs private and use `cargo -j2`.
The harness does not acquire the lock internally: invoke the wrapper once,
not recursively. No timings or performance results are reported.

## Profiles and oracle

Each C++ module exports `cpp_case(unsigned long long, unsigned long long,
volatile unsigned long long *)`. A Clang-compiled C driver calls a separate
C-ABI boundary object, which calls back into that export. C++ RAII and explicit
callback cases also call `boundary` in that separate object. Linking uses
Clang++, with no LTO. The optional Rust object supplies the same C ABI symbols.
This tests calls in both directions across object/compiler boundaries for this
scalar signature, not the full C++ ABI.

The driver runs 144 pairs from 12 boundary values plus 4,096 deterministic
xorshift pairs. It prints **every** return value and both volatile memory
cells, then a checksum. For each case the native Clang reference object is
compiled from the exact `.ll` submitted to the importer. Every native reference
must build, run successfully, and produce the expected output shape, including
for cases whose import is rejected. Each accepted module is compiled at Keel
`none`, `fast`, and `full`; complete stdout must equal its reference for every
selected boundary object. Checks stop on a failure within a case and continue
to the next case. The JSON stage and command logs identify the failure.

The six required regression-floor profiles are:

- `scalar`: unsigned wrapping arithmetic, shifts and volatile scalar memory.
- `templates`: an inlined scalar template instantiation.
- `constexpr`: recursive constant evaluation forced into a constant expression.
- `overloads`: overload resolution plus emitted mangled scalar definitions.
- `raii`: an inlined destructor with an observable C callback and volatile store.
- `c_abi`: explicit cross-object C callbacks.

The probes are `constexpr_runtime`, `virtual`, `cfg`, `gep`, `alloca`,
`signed_contract`, and `eh`. These may reject with a recognized unsupported
feature diagnostic; acceptance automatically enables the same three-tier
native differential tests. They are not pinned to rejection, so rerun them
after the CFG and GEP/alloca importer tracks are integrated. The report records
the first importer blocker; removing it may expose another. For example,
virtual dispatch currently stops at global data before reaching indirect calls.
The runtime constexpr probe intentionally retains its observed linkage blocker.
Templates, constexpr, overloads and RAII often disappear during Clang's scalar
lowering/inlining; successful import does not imply support for their general
runtime machinery. `ir_indicators` are textual counts for orientation, not a
complete LLVM feature parser.

All fixtures use `-std=c++17`. IR production is `-O2` for the required profiles,
runtime constexpr and virtual dispatch; `-O1` for CFG, GEP, signed contracts and
EH; and `-O0` for stack locals. Every case except EH explicitly uses
`-fno-exceptions -fno-rtti`. EH retains normal C++ exception/RTTI defaults.
The signed arithmetic probe uses bounded values to avoid source undefined
behavior while retaining real compiler `nuw`/`nsw` contracts. **No attributes,
flags, directives, functions or instructions are stripped from Clang's IR.**

## JSON, artifacts and failure behavior

`--out` must not exist. Existing directories are rejected before any write.
The artifact directory retains source snapshots and SHA-256 identities,
untouched `.ll` files, imported `.keel`, reference and Keel objects/executables,
and separate stdout/stderr files for every command. `report.json` records exact
argv/cwd/return codes, compiler and LLVM versions, selected and actual IR target,
data layout, source revision, importer/binary hashes, flags, IR optimization
levels, per-case import diagnostics, native reference results, differential
results, and the report exit code. The importer is also snapshotted. Commands
use absolute artifact paths. Reproduction after source changes needs a new run.

Before each import the destination contains a sentinel. Rejection must return
1 with an understood `llvm-import:` unsupported diagnostic, empty stdout, and
the exact destination still intact. Success must replace the sentinel and
proceed through native validation. Parser errors, verifier failures, signals,
timeouts, missing prerequisites, reference build/execution errors, partial
output and differential mismatches are failures, not expected unsupported
features. Raw diagnostics are retained even when the feature is expected.

Exit status is 0 for passing required cases plus any expected probe rejections,
1 for case regressions/unexpected failures, and 2 for unavailable/incompatible
prerequisites or CLI errors. In JSON a valid probe rejection is `rejected`, a
required rejection is `required_rejected`, and other case errors are `failed`
with their stage; the import subrecord further distinguishes crashes,
protocol/preservation failures and unexpected importer errors. A zero run with
probes blocked is `passed_with_blockers`, not full C++ coverage. The focused
unit tests exercise classification, missing tools, timeout artifacts, and
existing-destination preservation.

## Observed local milestone

On 2026-10-21, base revision `1ac952c21cf043bf59d79f739644e8c37ce13cc6`,
Apple Clang 21.0.0 (`clang-2100.3.34.2`), LLVM library 23.1.1, and rustc
1.99.0 (`b940084d7`) produced **6 passing profiles and 7 expected probe
rejections**, exit 0 (`passed_with_blockers`). Selected target was
`arm64-apple-darwin27.0.0`; emitted LLVM triple was
`arm64-apple-macosx27.0.0`. Both Clang C and independent Rust C ABI boundaries
were enabled. All 26 native reference executions succeeded. All 36 Keel
executions (6 accepted modules × 3 tiers × 2 boundary implementations) matched
the complete reference output for 4,240 input pairs each. All 7 rejected
imports preserved their sentinel destinations and emitted no stdout.

Observed first blockers:

- `constexpr_runtime`: `_Z6factorj` unsupported linkage/visibility.
- `virtual`: global data, aliases and ifuncs.
- `cfg`: multiple blocks/PHIs.
- `gep`: unsupported opcode 29, actual `getelementptr inbounds nuw`.
- `alloca`: unsupported opcode 26, actual stack allocation.
- `signed_contract`: `nuw`/`nsw` arithmetic.
- `eh`: global data, aliases and ifuncs (EH itself not reached).

The private artifact directory was
`/private/tmp/keel-cpp-t3-56c46fcd/run-2`; its `report.json` retains all 190
commands and their output paths. Build and run locking metadata is in
`/private/tmp/keel-cpp-t3-56c46fcd/lock.jsonl`; the build used private
`target/` under the same parent and `cargo build --offline --locked -j2 -p keel`.
The full native invocation was the reproduction command above with
`--abi-rustc rustc` and this private directory. The five focused Python tests
passed. No GCC boundary was validated on this host; `--abi-cc` remains an
optional path. CFG/GEP integration has not been validated in this checkout.
No importer, optimizer, backend, or native Rust frontend source was changed.
