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
python3 suite/fast_opt_locked.py /tmp/keel-llvm-import-locks.jsonl \
  cargo build --offline --locked -j2 -p keel --target-dir /tmp/keel-llvm-import-target
python3 suite/fast_opt_locked.py /tmp/keel-llvm-import-locks.jsonl \
  rustc --edition 2024 --crate-type lib -C opt-level=2 -C panic=abort \
  -C overflow-checks=off --emit=llvm-ir=scalar.ll suite/llvm_import_fixtures/scalar.rs
python3 suite/llvm_import.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel "/tmp/keel-llvm-import-target/debug/keel" scalar.ll -o scalar.keel
python3 suite/fast_opt_locked.py /tmp/keel-llvm-import-locks.jsonl \
  /tmp/keel-llvm-import-target/debug/keel compile --opt-tier fast --emit-object scalar.keel -o scalar.o
```

For the C++ fixture, generate LLVM IR with
`clang++ -O2 -fno-exceptions -fno-rtti -S -emit-llvm suite/llvm_import_fixtures/scalar.cpp -o scalar.ll`.
These are actual compiler outputs, with no directives or attributes stripped.
`-` reads stdin or writes stdout. Translation and both verifiers must succeed
before output is published. File output uses a sibling temporary and atomic
rename; unsupported input leaves an existing destination intact.

## Supported scalar/CFG and memory subset

- Functions with the ordinary C calling convention, scalar
  i8/i16/i32/i64 parameters and returns, or address-space-zero pointers.
  `signext`/`zeroext` ABI attributes are preserved. Internal/private linkage
  remains local; other unsupported linkage and visibility are rejected.
  Definitions returning i8/i16 require `signext` or `zeroext`; no extension is
  guessed. Function COMDAT membership is rejected.
- Wrapping integer arithmetic, division/remainder, bit operations and shifts;
  integer comparisons and selects; truncation, sign/zero extension and
  pointer/integer conversions. Internal i1 values become normalized Keel i8
  booleans, including truncation, pointer low-bit extraction and sign extension.
- Arbitrary verified scalar CFGs using unconditional/conditional branches and
  integer switches. PHIs become typed Keel block parameters; every edge supplies
  incoming values in PHI order. Parallel PHIs, loop backedges, irreducible loops,
  forward references, repeated destinations (including default/case overlap),
  pointer/i1 PHIs and unreachable blocks are retained. LLVM's verifier requires
  repeated edges from one predecessor to have equal PHI incoming values; an
  inconsistent module fails before translation. Block order need not be dominance
  order. Constants for edge arguments are materialized before the terminator.
- Layout-aware instruction GEP over supported scalar types, arrays, and sized
  packed/unpacked structs. LLVM target data supplies allocation strides and
  struct offsets. i8/i16/i32 indices sign-extend to the supported 64-bit pointer
  index width; i64 indices retain their bits. Struct indices are verified LLVM
  constants. Negative offsets and wrapping unflagged address arithmetic work.
  Aggregate *layout* is supported, not aggregate SSA values or aggregate ABIs.
- Fixed entry-block `alloca` with a constant element count, positive total size
  at most `2^32-1` bytes, and alignment at most 16. Each allocation gets a distinct
  Keel stack slot using LLVM allocation size and at least LLVM ABI/explicit
  alignment. Dynamic, zero-sized, non-entry, over-aligned, special-ABI and
  unsupported-layout allocations are rejected. Stack contents remain uninitialized;
  validation only reads initialized bytes. Final native frame-size limits still apply at
  compilation. Lifetime/stack intrinsics remain rejected.
- Direct C calls with matching declaration/call types and ABI attributes.
  Ordinary and volatile scalar loads/stores retain access widths and volatility.
  `unreachable` becomes a trap. No memory, call or arithmetic effect is moved.

LLVM's undefined division inputs need not retain a particular outcome; defined
division inputs have the same result. Keel masks shift counts; LLVM's defined
shift counts produce the same result, while oversized LLVM shifts produce
poison. This importer does not claim identical behavior for LLVM-undefined or
poison-producing executions. Explicit poison/undef operands and `freeze` are
rejected. GEP `inbounds`, `nuw` and `nusw` constraints do not become Keel facts:
address arithmetic agrees on defined LLVM executions, with no promise for
executions violating those constraints. GEP `inrange` is rejected. Existing scalar
arithmetic guards are unchanged; the importer conservatively rejects `nsw`, `nuw`,
`exact`, `disjoint`, `nneg`, flagged truncations and `samesign` comparisons.

Optimization and analysis contracts such as `noundef`, ranges, captures,
read-only effects and target CPU tuning are not imported as Keel facts. Debug
metadata and locations are not reproduced. This is not an equal-debug/unwind,
security-hardening, or source-level tooling replacement for LLVM.

## Explicit rejections

Global data/aliases/ifuncs, floating point (including FP memory layouts), vectors,
aggregate values/ABIs, i128, i1 ABI parameters/returns, varargs,
indirect/non-C calls, intrinsics, operand bundles, inline assembly, atomics,
exception handling, GC, special sections, explicit prefix/prologue data and
unsupported ABI attributes are outside this first implementation. Failure is
module-wide; it does not silently discard unsupported functions. Consequently,
ordinary unoptimized Rust/C++ output and full applications usually need more
support. Followups need explicit global storage/initializer/relocation and symbol
contracts; FP types, constants, comparisons, conversions and fast-math handling;
indirect-call signatures and ABI validation; and aggregate SSA lowering plus
byval/sret ABI tests. Those features are not silently approximated here.

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
python3 suite/fast_opt_locked.py /tmp/keel-llvm-import-locks.jsonl \
  python3 suite/llvm_import_validate.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel "/tmp/keel-llvm-import-target/debug/keel" --out /tmp/keel-llvm-import-check-1
```

The harness compiles eight Rust and eight C++ C-ABI exports, imports the actual
LLVM modules, and links/runs None/Fast/Full Keel objects against a C oracle.
Each scalar executable checks 20,289 boundary/random input pairs and volatile
memory effects, and matches the original Rust/Clang object's checksum. Another
LLVM fixture checks i1 truncation/sign extension, pointer low bits and narrow
returns across 65,536 values at all three tiers. A CFG/memory fixture covers
4,096 loop/random inputs, parallel PHI rotation, irreducible cycles, forward
references, repeated edges, pointer/i1 PHIs, signed switch boundaries, fixed
stack counts, negative indices, struct padding and packed layout. Independent
C assertions check results and memory, then compare native reference/Keel
checksums. Actual untouched Clang `-O1` and rustc `opt-level=1` CFG/GEP modules
also run 8,192 inputs each against a C oracle with volatile memory checks.
There are 18 native Keel builds across None/Fast/Full. The 52 rejection cases
check errors, empty stdout and preservation of an existing output, including
target/layout restrictions and module-wide late failures. A separate failing
Keel-verifier check also verifies output preservation. Commands and results are
retained in the artifact directory. These are correctness checks, not timing
results or evidence that full Rust/C++ applications are supported.

Compiler-generated `-O2` LLVM input has already passed through LLVM optimization.
Future comparisons must include or clearly exclude IR production and import
cost, and state which optimizer prepared the common input. Importing optimized
LLVM is a way to exercise Keel's backend, not a replacement for a native Rust
or C++ frontend or an end-to-end compiler speed comparison.
