# Userspace ISA support

Keel targets userspace x86-64 at psABI levels v1, v2 and v3, plus baseline
NEON on AArch64. Baseline privileged-instruction support is an active
[separate goal](x86-system-support.md) and does not block userspace work.

The [x86-64 psABI feature levels](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
are cumulative CPU/OS capability groups. Userspace code relies on the OS to
enable the required state. Ordinary userspace lowering does not configure
CR4/MSRs; [explicit privileged MSR APIs](x86-msr.md) are a separate contract.
AVX additionally
requires the normal OSXSAVE/XCR0 availability contract.

## At a glance

| Area | Status |
|---|---|
| Baseline NEON | Complete for the supported typed IR/codegen baseline: 64/128-bit vectors, arithmetic, lanes, conversions, permutations, structured memory and ordinary ABI transport. Checked natively on Apple ARM64 and under emulated Linux AAPCS64. |
| x86 baseline / SSE / SSE2 | Implemented: feature legality, scalar and vector codegen, typed operations, F80 and ABI support. |
| SSE3 / SSSE3 / SSE4.1 / SSE4.2 | Implemented as feature-gated operations: horizontal/packed arithmetic, shuffle/blend, conversions, string and CRC families. |
| AVX / AVX2 | Implemented 128/256-bit typed vectors, feature gating, arithmetic, transport, masks, gathers and permutations. |
| FMA / F16C / BMI / LZCNT / MOVBE | Implemented v3 feature families. |
| PCLMULQDQ (optional) | Carryless multiply intrinsic, assembler support and a small C `wmmintrin.h` subset. Disabled at every level; enable with `--target-features +pclmulqdq`. Not part of psABI v3. See [PCLMULQDQ](x86-pclmulqdq.md). |
| AES-NI (optional) | Six 128-bit AES round/key-assist operations with assembler, IR, codegen and C wrapper support. Disabled at every level; enable with `--target-features +aes`. See [AES-NI](x86-aes-ni.md). |
| Strict v1 / v2 / v3 | Supported for the implemented operations and ordinary SysV ABI paths. See the per-level sections below. |
| AVX-512, post-baseline ARM extensions | Out of scope. |

Validation is by focused regression suites, not exhaustive ISA certification.

## Selecting a level

Pass the level explicitly to `compile`, `cc` and `asm`:

```sh
keel cc --target x86_64-unknown-linux-gnu --cpu-level v1 ...   # or v2, v3
```

The library equivalent is `Features::for_level(CpuLevel::V1)` (or `V2`, `V3`)
through `FeatureSelector`.

- The default is strict v1. POPCNT and SSE4.1 require explicit features or a
  suitable named profile. No legacy profile remains.
- Positive modifiers enable prerequisites; repeated feature lists accumulate.
  `none` clears optional features. See the
  [generated profile contract](../tools/x86-isa/cpu-profiles.md).
- `--target-features` modifiers change the contract on purpose. Tuning and
  optimization/allocation tiers do not.
- CLFSH is optional at every level and never implied.
- No newer feature is silently enabled to make an operation compile. Codegen
  returns a `CodegenError` (in debug and release builds) if an ordinary emitted
  instruction needs a feature outside the selected profile. This covers text,
  typed-object and chunked-object output; the assembler checks the selected
  feature set too. The guard does not inspect external code or raw bytes.

## C frontend coverage

IR/codegen support does not mean the C frontend implements every vendor
intrinsic header, or that every loop auto-vectorizes. The C frontend supports
16-byte vectors on all targets and 32-byte vectors on x86, plus a
[documented SSE/AVX header and builtin subset](c-x86-intrinsics.md) that uses
the same typed IR and feature gates. Full `immintrin.h` compatibility is out of
scope. Notable SSE2 subsets:

- [Packed shifts](c-x86-intrinsics.md#bounded-sse2-packed-shifts): sixteen
  128-bit wrappers with oversized-count zero/sign fill, checked immediates and
  low-64-bit runtime counts. No AVX2 shift wrappers.
- [Saturation](c-x86-intrinsics.md#bounded-sse2-saturation): eight signed and
  unsigned byte/word saturating add/subtract wrappers. No AVX2 saturation,
  narrowing packs or conversions.
- [Numeric conversions](c-x86-intrinsics.md#bounded-sse2-numeric-conversions):
  `_mm_cvtepi32_ps`, `_mm_cvttps_epi32` and `_mm_cvtps_epi32`.

## Runtime requirements

These apply at every level and are part of the deployment contract:

- **FMA:** explicit F32/F64 FMA calls `fmaf`/`fma` at v1/v2 (including
  scalarized vector lanes); F80 calls `fmal` at every level. Link a compatible
  libm (normally `-lm`). Multiply-plus-add is never substituted for a fused
  operation, and ordinary multiply/add is never contracted.
- **i128 division/remainder:** needs `__udivti3`, `__divti3`, `__umodti3` and
  `__modti3`, normally from libgcc or compiler-rt. Other i128 arithmetic needs
  no helper.
- **Memory intrinsics** may call `memcpy`, `memmove`, `memset` and `memcmp`.
  External functions and TLS need normal libc/loader support.

The target flag does not rebuild libraries, select a sysroot or audit linked
objects. Keel does not ship a v1 libc, libm or compiler runtime; the libraries
you link must themselves support the deployment CPU.

## Known limits

- Tests run on hardware that supports newer instructions, not on a v1/v2/v3-only
  CPU or under CPUID masking. Feature checks establish that the generated
  instructions are legal; native tests establish value and ABI behavior.
- Not covered: arbitrary library implementations, every FP exception
  configuration, i128/FP conversions, i128 high/overflow multiplication, i128
  switches, x86 inline assembly and exception handling. These must produce a
  diagnostic rather than silently upgrade the target.
- Ambiguous untagged C `f64x1` boundaries are rejected at every level; use
  explicit ABI metadata or pointer/scalar boundaries.

## Strict-v1 userspace contract and audit

V1 assumes baseline integer, x87, MMX, SSE and SSE2. It does **not** imply
POPCNT, SSE3, SSSE3, SSE4.x, CX16, long-mode LAHF/SAHF, AVX, BMI, LZCNT, MOVBE
or CLFSH.

Lowering:

- Integer operations through i128 (subject to the limits above), F32/F64
  arithmetic, comparisons, saturating conversions, rounding, and ordinary F80
  arithmetic and transport.
- Population count uses SWAR integer code. F32/F64 nearest-even, floor, ceil and
  truncation are done in software: they manipulate the significand and read
  MXCSR for DAZ without changing the caller's rounding mode. Neither needs a
  runtime helper.
- SSE/SSE2 handle native 128-bit primitives, including wrapping `vreduce_add`
  for i64x2/i32x4 (see [wrapping reductions](x86-wrapping-reductions.md)). Other
  vector operations use exact-width scalar lanes.
- SysV transport covers integer/FP register and stack arguments, results,
  direct/indirect calls, supported tail calls, spills, varargs and aggregate
  wrappers. There is no 256-bit SSA vector or register ABI at v1; MEMORY-class
  aggregates may still copy wide payloads as bytes.

Tests:

```sh
cargo test -p keel-codegen --test x86_userspace_v1
cargo test -p keel-codegen --lib x86_64::feature_guard_tests   # also with --release
cargo test -p keel --test x86_cpu_level
```

These cover all three selection tiers, both register allocators, normal and
forced-spill pools, every popcount width, rounding values under each ambient
rounding mode, exact memory footprints, helper symbol sets and fused-helper
results. Text and typed objects must match. CLI tests compile C with and
without optimization and check that representative optional instructions are
rejected at v1 and accepted at v2/v3. Related native suites: `vector_x86`,
`f80`, `x86_vector_varargs`, `x86_vector_wrappers` and `x86_legacy_raw_fp`.

## Strict-v2 userspace contract and audit

V2 adds SSE3, SSSE3, SSE4.1, SSE4.2, POPCNT, CX16 and long-mode LAHF/SAHF to v1.
It does not enable AVX/AVX2, BMI1/2, FMA, F16C, LZCNT or MOVBE. Ordinary 128-bit
vector SysV transport is supported; 256-bit SSA vectors and their register ABI
need an AVX-enabled profile. V2 uses hardware POPCNT and rounding.

Baseline selection may scalarize a generic vector operation even when v2 has a
native instruction for it. That is legal, and is tested separately from explicit
architectural intrinsics.

Tests:

- `cargo test -p keel-codegen --test x86_userspace_v2`: 128-bit vector
  operations, integer/FP masks, lanes and shuffles; SysV register/stack
  arguments, results, direct/indirect/tail calls, varargs, block-argument cycles
  and values live across calls. Representative SSE3, SSSE3 and SSE4.1
  operations are checked for aliasing, canaries and guard-page edges. Text,
  typed and chunked objects must match, and every typed instruction's
  operand-dependent feature requirements must fit v2. A separate LAHF/SAHF
  sequence checks all 256 low-status inputs in long mode.
- `cargo test -p keel-codegen --test x86_userspace_v1`: shared scalar/ABI
  emission at v1/v2/v3, plus native scalar and fused-helper results.
- `cargo test -p keel-codegen --lib x86_64::feature_guard_tests` and
  `cargo test -p keel --test x86_cpu_level` (also with `--release`): reject
  representative AVX/AVX2, BMI1/2, FMA, F16C, LZCNT, MOVBE and CLFSH emission
  under v2, and check explicit feature overrides including LAHF and SAHF.

Supporting family suites: `x86_crc32`, `x86_legacy_strings`, `x86_legacy_v2_fp`,
`x86_packed_integer`, `x86_packed_pair` and `x86_cx16`.

Set `KEEL_V2_EXPORT_DIR` to keep the join's objects and assembly for external
disassembly. An `llvm-objdump` scan of the exported objects found no AVX-family,
YMM/ZMM, BMI, FMA, F16C, LZCNT, MOVBE, CLFSH or XSAVE-family instructions. This
is a mnemonic/operand scan, not a decoder proof.

## Strict-v3 userspace contract and audit

V3 adds AVX/AVX2, BMI1/2, FMA, F16C, LZCNT and MOVBE to v2, with the normal
XSAVE/OSXSAVE and XCR0 contract for AVX state. The flag declares the deployment
contract; it does not probe the machine or enable OS state. AVX-512 and other
post-v3 extensions are not implied.

Support layers:

- Ordinary vector IR and the documented C subset use legal native selection or
  scalar expansion.
- Explicit F32/F64 FMA uses hardware and keeps fused rounding; F80 still needs
  `fmal`.
- BMI extract/deposit and F16C have typed architectural operations.
- LZCNT and MOVBE have assembler encoding and feature support only. Generic CLZ
  uses BSR-based lowering, and generic byte swaps need not select MOVBE.
- The C headers still exclude FMA, F16C, masks/gathers, general conversions and
  full vendor-header support, even where typed backend operations exist.

Tests:

- `cargo test -p keel-codegen --test x86_userspace_v3`: mixes 128/256-bit
  integer/FP vectors and scalar FP in one SysV signature, exhausting the
  eight-register SIMD argument bank and spilling both widths to the stack.
  Covers direct/indirect calls and tail calls in both C-to-Keel directions,
  values live across a call that clobbers every SIMD register, and LZCNT, TZCNT
  and MOVBE values at 16/32/64 bits. Runs with Clang and GCC harnesses across all
  selection tiers, allocators and pool configurations.
- `cargo test -p keel-codegen --test x86_avx256_generic`: wide scalar-reference
  corpus, register/stack ABI, callbacks, varargs, tail calls, block arguments and
  pressure. Shares the text/typed/chunked parity and feature gate from
  `common/userspace.rs`.
- `cargo test -p keel --test x86_cpu_level`: compiles wide IR and the C AVX
  header subset at v3 through text and direct-object paths. Reassembled text
  must match direct objects. Checks that v1/v2 reject the wide ABI, CLFSH is
  rejected without an override, and representative AVX-512 forms are rejected.
- Family suites: `x86_fma`, `x86_f16c`, `x86_bmi_semantics`,
  `x86_masked_memory`, `x86_gather`, `cargo test -p keel-c --test x86_intrinsics`
  and the `cargo test -p keel-asm --test x86_v3` encoding oracle (checked against
  Clang and GNU as).

The native C suite has been run with Clang 23 and GCC 16 on x86-64 Linux. An
`llvm-objdump` scan of the exported objects found no EVEX prefixes, ZMM or
opmask registers, CLFSH, or optional/post-v3 mnemonics (AES, PCLMUL, SHA, GFNI,
ADX, RDRAND/RDSEED, FSGSBASE, PKU, extended XSAVE, VNNI, IFMA, AMX). As with v2,
this is a finite byte/mnemonic scan, not a decoder proof.

## Separate low-level work

Privileged I/O, descriptor/status queries and permission tests exist. Raw FLAGS,
physical stack/frame, segment/event-return and saved-context contracts are
separate experiments. String-I/O guest validation and physical-stack lowering
are unfinished, and an advanced raw x87 pointer-image check still fails. None of
these block ordinary SSE/AVX code generation.
