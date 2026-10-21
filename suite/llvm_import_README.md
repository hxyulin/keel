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
  remains local. ODR definitions retain exported/coalesced identity through
  Keel `weak_odr` and Mach-O weak definitions. Unsupported non-ODR linkage and
  visibility remain rejected.
  Definitions returning i8/i16 require `signext` or `zeroext`; no extension is
  guessed. Function COMDAT membership is rejected.
  Boolean i1 parameters and returns require explicit `zeroext` on declarations,
  definitions and call sites, and map to canonical Keel `i8 uext`. Unspecified
  or signed i1 extension is rejected (sign-extending true is not i8's value 1).
  `llvm_import_bool_validate.py` checks untouched Clang O0/O1/O2 IR against a
  native C caller and callbacks through None/Fast/Full, including stack arguments.
- Wrapping integer arithmetic, division/remainder, bit operations and shifts;
  integer comparisons and selects; truncation, sign/zero extension and
  pointer/integer conversions. Internal i1 values become normalized Keel i8
  booleans, including truncation, pointer low-bit extraction and sign extension.
- Scalar f32/f64 parameters, returns, memory, exact raw-bit constants, arithmetic
  and negation, all ordered/unordered comparisons, bitcasts, integer-to-FP and
  FP width conversions. FP-to-integer supports i32/i64 destinations and agrees
  on LLVM-defined inputs; Keel saturation on LLVM poison inputs is not a shared
  source-level saturation guarantee. Default IEEE environment/rounding only:
  fast-math flags, strict/constrained operations, denormal/rounding attributes
  and `nofpclass` promises are rejected. Constants, bitcasts and negation retain
  NaN payload bits; unspecified arithmetic NaNs are compared by classification.
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
  Aggregate *layout* is supported; aggregate SSA and ABI transport are limited
  to the exact contracts below.
- Fixed entry-block `alloca` with a constant element count, positive total size
  at most `2^32-1` bytes, and alignment at most 16. Each allocation gets a distinct
  Keel stack slot using LLVM allocation size and at least LLVM ABI/explicit
  alignment. Dynamic, zero-sized, non-entry, over-aligned, special-ABI and
  unsupported-layout allocations are rejected. Stack contents remain uninitialized;
  validation only reads initialized bytes. Final native frame-size limits still apply at
  compilation. Canonical lifetime start/end markers on direct fixed slots are
  omitted, keeping slots alive for the function without importing lifetime
  assumptions. LLVM's parser upgrades legacy marker forms. Other surviving
  lifetime-marker targets and stack intrinsics remain rejected.
- Global definitions and declarations with ordinary external/internal/private
  linkage, const/writable storage, integer/f32/f64/pointer fields and nested
  arrays/structs. TargetData determines layout and explicit padding; floating
  initializers serialize raw bits, including negative zero and NaN payloads.
  Function/data addresses and constant GEP relocations retain checked u32
  nonnegative addends. Pointer relocations require an eight-byte-aligned field
  and object alignment of at least eight. Defined-object addends may reach one
  past the object; external bounds are unknown. See the
  [global-object contract](llvm_import_globals_fixtures/README.md).
- Direct C calls with matching declaration/call types and ABI attributes, plus
  scalar indirect C calls using the call-site function type and explicit ABI
  extensions. Function addresses, pointer PHIs and stack roundtrips are supported.
  Narrow i8/i16 return calls require an explicit sign/zero extension. Unknown
  parameter/return ABI attributes and call function contracts are rejected.
  A bounded struct-return contract supports only a first pointer parameter with
  `sret` of unpacked `{i64,i64,i64}`, void result, size 24, alignment 8 and offsets
  0/8/16. This preserves Apple AArch64's x8 result transport; broader layouts,
  small/HFA returns and `byval` remain rejected rather than reclassified by guess.
  A separate parameter contract accepts only exact `[2 x i64]` (size 16,
  alignment 8) on this target, using one Keel `ptr byval {i64, i64}` parameter.
  Keel's existing aggregate ABI planner retains two-register placement,
  register exhaustion, aligned whole-aggregate stack transport and caller copies;
  the importer never expands it into two independent scalar ABI parameters.
  Incoming lanes are snapshotted at entry. Direct and indirect calls materialize
  separate fixed slots for each aggregate argument. Exact one-level extractvalue
  and insertvalue chains resolve each observed lane independently, including
  fully overwritten poison/undef seeds; an observed undefined lane is rejected.
  Aggregate returns, loads/stores, PHIs/selects, constant aggregates and all other
  aggregate shapes remain unsupported. No source-language struct type is inferred.
  Ordinary and volatile scalar loads/stores retain access widths and volatility.
  `unreachable` becomes a trap. No memory, call or arithmetic effect is moved.
- Bounded Itanium EH: direct invokes, typed/catch-all landingpads,
  cleanup, typeid, dedicated `{ptr,i32}` pairs and acyclic pair PHIs, and
  resume/rethrow through runtime calls. Invoke edge copies preserve normal and
  unwind PHIs. Only the symbolic external `__gxx_personality_v0` personality
  is accepted; its variadic runtime declaration does not enable variadic calls.
  Defined RTTI is transported through validated data and relocation contracts.
  Indirect invokes, filters, Windows funclets, reconstructed pairs
  and cyclic EH pair PHIs remain rejected. See the
  [EH contract](llvm_import_eh_fixtures/README.md).

LLVM's undefined division inputs need not retain a particular outcome; defined
division inputs have the same result. Keel masks shift counts; LLVM's defined
shift counts produce the same result, while oversized LLVM shifts produce
poison. This importer does not claim identical behavior for LLVM-undefined or
poison-producing executions. Explicit poison/undef operands and `freeze` are
rejected. GEP `inbounds`, `nuw` and `nusw` constraints do not become Keel facts:
address arithmetic agrees on defined LLVM executions, with no promise for
executions violating those constraints. `inrange` constraints are likewise
omitted without changing computed addresses or weakening relocation bounds. Supported
integer `nsw`/`nuw`, `exact`, `or disjoint`, flagged truncation, `zext nneg` and
integer `icmp samesign` refine to ordinary operations, without supplying Keel
optimizer facts. Signed division guards `INT_MIN/-1` so refining a poison
dividend cannot introduce an overflow trap on a select-masked path. Zero-divisor
traps remain. Explicit poison/undef/freeze, unsupported widths, i1 arithmetic
and ordered i1 comparisons remain rejected. See the
[integer flag contract](llvm_import_flags_fixtures/README.md) for defined-input
and poison-masking test domains. FP fast-math restrictions are unchanged.

Optimization and analysis contracts such as `noundef`, ranges, captures,
read-only effects and target CPU tuning are not imported as Keel facts. Debug
metadata and locations are not reproduced. This is not an equal-debug/unwind,
security-hardening, or source-level tooling replacement for LLVM.

## Explicit rejections

Global aliases/ifuncs, TLS/common/weak-any/linkonce-any/COMDAT/sections and unsupported
global attributes, half/bfloat/x87/FP128, vectors, `frem`,
aggregate SSA/ABIs beyond dedicated EH pairs, exact two-word parameters and the bounded sret contract,
i128, i1 ABI parameters/returns without explicit zeroext, ordinary varargs, non-C calls, intrinsics other
than EH typeid and bounded lifetime markers, operand bundles, inline assembly, atomics, unsupported EH forms,
GC, special sections, explicit prefix/prologue data and
unsupported ABI attributes are outside this first implementation. Failure is
module-wide; it does not silently discard unsupported functions. Consequently,
ordinary unoptimized Rust/C++ output and full applications usually need more
support. Followups need broader symbol/linkage contracts, aggregate SSA lowering
and byval/sret ABI proofs. Those features are
not silently approximated here.

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
There are 18 native Keel builds across None/Fast/Full. The 46 rejection cases
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

The aggregate runner also invokes `llvm_import_calls_validate.py`, which tests
indirect scalar calls, narrow extensions, stack-passed arguments and the bounded
sret contract. It replaces native objects individually and together at
None/Fast/Full; independent C assertions cover both call directions. Its 38
checks include 22 negative cases with source/destination preservation. The
validation JSON records a dependency plan for deferred aggregate forms.

`llvm_import_fp_validate.py` adds 53 rejection/preservation cases and nine native
builds for handwritten LLVM and untouched Clang O0/O1 output at None/Fast/Full.
Independent assertions cover NaNs, infinities, signed zeros, subnormals, ties,
conversion boundaries, PHIs/selects, stack-passed FP arguments and indirect FP
calls. The aggregate runner invokes it as `llvm-fp` under the same host lease.

`llvm_import_globals_validate.py` covers 45 rejection cases and 12 native builds
at None/Fast/Full, including untouched Clang C/C++ output. Independent assertions
check nested layouts, mutations, exact FP initializers, external resolution and
relocations. A combined case loads an FP callback from global storage, calls it
indirectly and reads an external FP object. Source/destination and downstream
verifier preservation checks are required. The aggregate stage is `llvm-globals`.

`--localize-odr-functions` enables a separate bounded experiment. The caller
asserts no outside translation unit needs the imported ODR helper symbols;
eligible direct-only `linkonce_odr`/`available_externally` definitions are kept
under their original names as local copies. Address escapes, named-storage
dependencies, calls outside the defined local helper closure, weak linkage and
COMDAT remain rejected. This is not general symbol coalescing and stays disabled
by default. See the [ODR contract](llvm_import_linkage_fixtures/README.md).
The `llvm-linkage` aggregate stage retains its local-copy restrictions and
mixed-TU controls. Default mode now accepts true ODR definitions with exported
identity; available-externally definitions still need the explicit local-copy
contract. This is independent of the true ODR path.

The aggregate stages `llvm-flags` and `llvm-eh` invoke the dedicated integer-flag
and exception validators. They cover independently asserted integer matrices,
poison-masked downstream division, actual compiler output, typed/catch-all
exceptions, destructors, resume/rethrow, edge PHIs and preservation failures at
None/Fast/Full. The C++ runner requires signed-contract and integer-EH profiles;
`cpp-odr` separately requires the constexpr helper fixture with an explicit
localization assertion. Default C++ coverage uses exported ODR definitions.

The `odr-core`, `llvm-odr-policy` and `llvm-odr` stages verify IR/object behavior,
import policy and mixed-TU execution. Hidden Itanium class/si RTTI retains
private-extern visibility, and `rtti_name @symbol` preserves Apple's tagged
type-name relocation. Generic hidden symbols and arbitrary integer relocations
remain rejected. The mixed-TU validator has 32 native runs covering identity,
shared storage, virtual dispatch, cleanup and cross-module user-defined catches.
See the [ODR/RTTI transport contract](llvm_import_odr_contract.md). All 14 default
C++ profiles are required; no probe rejection counts as a successful profile.

`llvm_import_aggregate_validate.py` validates the two-word parameter contract
with untouched Clang O1/O2 C and C++17 `std::string_view` input. Native C/C++
drivers call Keel objects, which call native direct helpers and callbacks.
The integer matrix covers 0–10 leading scalars, two aggregates, byte-sized
stack arguments, register boundaries, whole-aggregate spilling, lane mutations
and 1,408 independently asserted patterns per tier/input. The string-view case
checks pointer returns and 22 views, including empty views, per tier/input.
None/Fast/Full produce 12 native candidate builds; 18 rejection/preservation
cases retain source hashes, existing destinations and empty stdout. The runner
writes JSON command/evidence records and an HTML report. Run it under
`fast_opt_locked.py` with `--libllvm`, `--keel` and a fresh `--out` directory.
It is standalone; this change does not alter the default validation pipeline.
