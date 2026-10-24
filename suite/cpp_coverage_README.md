# C++ coverage through Clang LLVM IR

`cpp_coverage.py` exercises **Clang C++17 → unmodified LLVM IR → the existing
`llvm_import.py` → Keel native objects**. It is a correctness and rejection
inventory, not a native C++ frontend, application coverage claim, or performance
comparison. Clang has already optimized the IR at the recorded production
level. This does not measure an end-to-end replacement for Clang.

## Reproduce

Run on an Apple AArch64 host with a new artifact directory. The importer
requires LLVM 23's shared library.

```sh
cargo build -p keel
python3 suite/cpp_coverage.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel target/debug/keel --abi-rustc rustc --out target/cpp-coverage-1
python3 -m unittest discover -s suite -p test_cpp_coverage.py -v
```

`cargo suite test-frontends` also runs this as its `cpp-coverage` stage.

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

The runner never builds Keel itself; `--keel` must name the intended binary.
No timings or performance results are reported.

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

All fourteen profiles are required regressions:

- `scalar`: unsigned wrapping arithmetic, shifts and volatile scalar memory.
- `templates`: an inlined scalar template instantiation.
- `constexpr`: recursive constant evaluation forced into a constant expression.
- `overloads`: overload resolution plus emitted mangled scalar definitions.
- `raii`: an inlined destructor with an observable C callback and volatile store.
- `c_abi`: explicit cross-object C callbacks.
- `cfg`: multiple blocks and conditional direct calls.
- `gep`: indexed volatile memory.
- `alloca`: unoptimized stack locals.
- `signed_contract`: compiler no-wrap arithmetic.
- `eh_int`: typed integer throw/catch using external runtime typeinfo.
- `constexpr_runtime`: retained exported ODR helper definitions.
- `virtual`: virtual dispatch through a transported vtable.
- `eh`: user-defined typed throw/catch and defined RTTI.

Rejection of any profile fails the run and records the first importer blocker.
The separate ODR interoperability validator checks mixed-TU symbol identity,
shared storage and throw/catch behavior without helper localization.
Templates, constexpr, overloads and RAII often disappear during Clang's scalar
lowering/inlining; successful import does not imply support for their general
runtime machinery. `ir_indicators` are textual counts for orientation, not a
complete LLVM feature parser.

All fixtures use `-std=c++17`. IR production is `-O2` for the original six profiles,
runtime constexpr and virtual dispatch; `-O1` for CFG, GEP, signed contracts and
both EH profiles; and `-O0` for stack locals. Every case except the two EH
profiles explicitly uses `-fno-exceptions -fno-rtti`. Both EH profiles retain
normal C++ exception/RTTI defaults.
The signed arithmetic profile uses bounded values to avoid source undefined
behavior while retaining real compiler `nuw`/`nsw` contracts. **No attributes,
flags, directives, functions or instructions are stripped from Clang's IR.**

`--localize-odr-functions` explicitly asserts no outside TU requires helper
symbols and enables only the bounded direct-helper localization contract.
The aggregate harness retains that fixture separately as `cpp-odr`, exporting
only `cpp_case` to its C/Rust boundaries. Default coverage now transports true
ODR definitions and preserves their identity. The legacy flag is a separate
bounded optimization experiment, not a substitute for exported linkage.

## JSON, artifacts and failure behavior

`--out` must not exist. Existing directories are rejected before any write.
The artifact directory retains source snapshots and SHA-256 identities,
untouched `.ll` files, imported `.keel`, reference and Keel objects/executables,
and separate stdout/stderr files for every command. `report.json` records exact
argv/cwd/return codes, compiler and LLVM versions, selected and actual IR target,
data layout, source revision, importer/binary hashes, flags, IR optimization
levels, per-case import diagnostics, native reference results, differential
results, and the report exit code. The importer and its globals/linkage helpers are
snapshotted and hashed. Commands
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

## Current status

All **14 required default profiles** pass at None/Fast/Full with both the Clang
C and the Rust C ABI boundaries. No default profile is accepted as an expected
rejection. The separate `cpp-odr` run covers the legacy local-copy helper
localization, and the dedicated mixed-TU ODR validator
(`llvm_import_odr_validate.py`) covers both link orders, shared identities,
virtual dispatch, cleanup counts and cross-module user-defined catches. Support
remains bounded to the documented Apple AArch64 contracts and does not
establish general C++ application support.
