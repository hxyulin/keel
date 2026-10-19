# Diagnostic LLVM → Keel importer

`llvm_import.py` imports a deliberately small scalar LLVM IR subset into textual
Keel IR. It uses LLVM's real parser and verifier through Python `ctypes`, then
runs the supplied Keel executable to parse and verify its buffered output.
It adds no Cargo dependencies and does not run LLVM optimization passes.

The currently validated library is LLVM **23.1.1**, shipped with the installed
Rust 1.99 toolchain. Other LLVM major versions are rejected. The library must
export the required C API symbols; a version number alone does not establish
compatibility. Only Apple AArch64, little-endian, 64-bit pointers are supported.
The input module must carry its target and data layout. There is no host-target
inference or implicit retargeting.

```sh
cargo build --offline --locked -p keel
rustc --edition 2024 --crate-type lib -C opt-level=2 -C panic=abort \
  -C overflow-checks=off --emit=llvm-ir=scalar.ll suite/llvm_import_fixtures/scalar.rs
python3 suite/llvm_import.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel "$PWD/target/debug/keel" scalar.ll -o scalar.keel
target/debug/keel compile --opt-tier fast --emit-object scalar.keel -o scalar.o
```

For the C++ fixture, generate LLVM IR with
`clang++ -O2 -fno-exceptions -fno-rtti -S -emit-llvm suite/llvm_import_fixtures/scalar.cpp -o scalar.ll`.
These are actual compiler outputs, with no directives or attributes stripped.
`-` reads stdin or writes stdout. Translation and both verifiers must succeed
before output is published. File output uses a sibling temporary and atomic
rename; unsupported input leaves an existing destination intact.

## Initial supported subset

- Single-block functions with the ordinary C calling convention, scalar
  i8/i16/i32/i64 parameters and returns, or address-space-zero pointers.
  `signext`/`zeroext` ABI attributes are preserved. Internal/private linkage
  remains local; other unsupported linkage and visibility are rejected.
  Definitions returning i8/i16 require `signext` or `zeroext`; no extension is
  guessed. Function COMDAT membership is rejected.
- Wrapping integer arithmetic, division/remainder, bit operations and shifts;
  integer comparisons and selects; truncation, sign/zero extension and
  pointer/integer conversions. Internal i1 values become normalized Keel i8
  booleans, including truncation, pointer low-bit extraction and sign extension.
- Direct C calls with matching declaration/call types and ABI attributes.
  Ordinary and volatile scalar loads/stores retain access widths and volatility.
  `unreachable` becomes a trap. No memory, call or arithmetic effect is moved.

LLVM's undefined division inputs need not retain a particular outcome; defined
division inputs have the same result. Keel masks shift counts; LLVM's defined
shift counts produce the same result, while oversized LLVM shifts produce
poison. This importer does not claim identical behavior for LLVM-undefined or
poison-producing executions. Explicit poison/undef operands and `freeze` are
rejected. The initial implementation also conservatively rejects `nsw`, `nuw`,
`exact`, `disjoint`, `nneg`, flagged truncations and `samesign` comparisons.

Optimization and analysis contracts such as `noundef`, ranges, captures,
read-only effects and target CPU tuning are not imported as Keel facts. Debug
metadata and locations are not reproduced. This is not an equal-debug/unwind,
security-hardening, or source-level tooling replacement for LLVM.

## Explicit rejections

Multiple blocks, PHIs, GEP, stack allocation, global data/aliases/ifuncs, floating
point, vectors, aggregate values/ABIs, i128, i1 ABI parameters/returns, varargs,
indirect/non-C calls, intrinsics, operand bundles, inline assembly, atomics,
exception handling, GC, special sections, explicit prefix/prologue data and
unsupported ABI attributes are outside this first implementation. Failure is
module-wide; it does not silently discard unsupported functions. Consequently,
ordinary unoptimized Rust/C++ output and full applications usually need more
support. The first useful extension is CFG/PHI translation, followed by
layout-aware GEP and stack allocation, before richer language runtime features.

SME/ZA/streaming ABI attributes (`aarch64_*`) are rejected on definitions,
declarations and call sites, as are call-site `returns_twice`, null-pointer-valid
contracts, instrumentation/patchable-entry attributes and module autolink
requests. Required enum-attribute names are checked against the loaded library
at startup so an unrecognized name cannot silently disable a rejection.

The semantic reference is the [LLVM language reference](https://llvm.org/docs/LangRef.html).
Bindings follow the official [Core C API](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h)
and [Target C API](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Target.h).

## Reproducible validation

Use a fresh artifact directory. The wrapper serializes compiler/native work
with the other local investigations.

```sh
python3 suite/fast_opt_locked.py target/llvm-import-locks.jsonl \
  python3 suite/llvm_import_validate.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel "$PWD/target/debug/keel" --out target/llvm-import-check-1
```

The harness compiles eight Rust and eight C++ C-ABI exports, imports the actual
LLVM modules, and links/runs None/Fast/Full Keel objects against a C oracle.
Each scalar executable checks 20,289 boundary/random input pairs and volatile
memory effects, and matches the original Rust/Clang object's checksum. Another
LLVM fixture checks i1 truncation/sign extension, pointer low bits and narrow
returns across 65,536 values at all three tiers. Rejection tests check errors,
empty stdout and preservation of an existing output. Commands and results are
retained in the artifact directory. These are correctness checks, not timing
results or evidence that full Rust/C++ applications are supported.

Compiler-generated `-O2` LLVM input has already passed through LLVM optimization.
Future comparisons must include or clearly exclude IR production and import
cost, and state which optimizer prepared the common input. Importing optimized
LLVM is a way to exercise Keel's backend, not a replacement for a native Rust
or C++ frontend or an end-to-end compiler speed comparison.
