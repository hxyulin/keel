# Three-input IR harness

`keel-ir-harness` accepts Keel, LLVM or Cranelift IR. It imports into verified
Keel, exports to each other IR, imports again, and closes the cycle in the
original IR. LLVM and Cranelift input fixtures also run through their original
backends before conversion. Keel's native backend executes every imported
round trip. Each executable must pass the supplied C driver and match the
expected stdout bytes.

This is the initial scalar milestone. It does not establish full feature parity.
The independently written `ir-fixtures/scalar.*` sources cover wrapping integer
arithmetic, unsigned comparison, masked shifts, conditional branches and block
arguments. The C driver checks 81 edge pairs and 10,000 deterministic random
pairs with an unsigned C oracle. It reports the failing input on stderr.

## Build and run

The harness stays in the isolated Cranelift workspace. It does not add
Cranelift dependencies to the main workspace.

```sh
cargo build -p keel
cargo build --manifest-path crates/keel-cranelift/Cargo.toml --bins --locked
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked

crates/keel-cranelift/target/debug/keel-ir-harness \
  --target arm64-apple-macosx11.0.0 \
  --format clif \
  --input crates/keel-cranelift/tools/ir-fixtures/scalar.clif \
  --name probe \
  --driver crates/keel-cranelift/tools/ir-fixtures/scalar.c \
  --expected crates/keel-cranelift/tools/ir-fixtures/scalar.expected \
  --out target/ir-harness-clif \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --clif-import crates/keel-cranelift/target/debug/keel-clif-import
```

For the other inputs, change `--format` and `--input` to `keel`/`scalar.keel`
or `llvm`/`scalar.ll`. Use a fresh output directory for each run. LLVM import
requires the LLVM 23 library accepted by `suite/llvm_import.py`. Executing the
objects requires an Apple arm64 host and Clang. The target is always explicit.
The driver must exercise defined source behavior. A successful run cannot
establish equivalence on LLVM poison or undefined executions.

## Evidence and failure behavior

`report.json` records arguments, path outcomes, diagnostics, command outputs,
and elapsed nanoseconds. Intermediate IR, objects, executables and execution
stdout remain in the output directory. Paths run independently after the
initial import; a failed path does not hide the others. Any failed path or
initial import makes the command exit unsuccessfully. A failed initial import
is recorded as `fatal`, with no invented downstream results.

Timings describe the diagnostic run. They include compilation, linking and
process startup, and are not throughput benchmarks. Keel parsing, verification,
LLVM export, CLIF import/export and native/Cranelift compilation run in Rust.
LLVM import uses `keel_llvm::import::Importer` in-process. The loaded LLVM 23
library is reused across conversions; each translation owns an independent
LLVM context. Python and the external Keel executable are not used for import.

## CLIF import contract

`keel-clif-import` uses Cranelift's parser and verifier and verifies the resulting
Keel module. It requires one function, an explicit target and an explicit
external symbol name because CLIF text has no module linkage contract.
Only `apple_aarch64`, normal scalar integer ABI arguments and zero/one return
are accepted. Supported operations are integer constants, wrapping add/sub/mul,
bitwise operations, shifts/rotates, comparisons, select, integer width
conversions, branches, block arguments and return. Other operations fail with
function, block and instruction context. Calls, stack slots and global values
are rejected. Source locations are omitted. This importer is narrower than
Keel's existing CLIF exporter. Rejection identifies a converter gap and does
not imply Keel or Cranelift lacks the feature.

`--object` compiles the original verified CLIF directly to an object on stdout.
It does not pass through Keel, so the input oracle can expose importer errors.

## Feature loss work

The report separates execution success from fact preservation. The latter is
currently `not_measured`. Its `loss_contracts` document known adapter behavior,
not detected losses in a particular input. Consult the existing importer and
exporter READMEs for exact supported features.

Next fixtures should isolate ABI extension facts, floating-point NaNs and signed
zero, division traps, memory effects, atomics, vectors and module metadata.
Each needs an explicit observation: result bits, memory changes, termination,
or retained IR facts. Trap fixtures need subprocess termination expectations;
the current successful-execution driver contract does not cover them.

For each gap, establish which layer owns it before changing Keel: a missing
converter mapping, a Keel representation gap, or an intentional semantic
refinement. Add source fixtures and fact assertions before implementing a fix.
Execution equality alone does not prove retention of overflow promises,
`notrap`, pointer constraints, debug information or optimization metadata.
