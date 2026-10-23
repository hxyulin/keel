# Userspace ISA support

The primary goal is userspace x86-64-v1, then v2, then v3, plus baseline NEON
IR/codegen. Kernel and privileged-instruction support is an optional, separate
track. It does not block userspace completion. This scope supersedes the broader
physical-machine interpretation used by the archived proof project.

The [x86-64 psABI feature levels](https://gitlab.com/x86-psABIs/x86-64-ABI/-/raw/master/x86-64-ABI/low-level-sys-info.tex)
are cumulative CPU/OS capability groups. Normal userspace code relies on the OS
enabling the required state; implementing CR4/MSR writes or a kernel is not a
prerequisite. AVX requires the normal OSXSAVE/XCR0 availability contract.

| Area | Current status |
|---|---|
| Baseline NEON | Accepted complete for the supported typed IR/codegen baseline: 64/128-bit vectors, arithmetic, lanes, conversions, permutations, structured memory and ordinary ABI transport. Historical acceptance covered 12 requirements, 519 operation records, 1,227 typed signatures and native Apple ARM64 plus emulated Linux AAPCS64 checks. |
| x86 baseline / SSE / SSE2 | Implemented with feature legality, ordinary scalar/vector codegen, typed operations and regression tests. Core integer, packed values, F80 and ABI families have finite native validation history. |
| SSE3 / SSSE3 / SSE4.1 / SSE4.2 | Implemented feature-gated operations, including horizontal/packed arithmetic, shuffle/blend, conversions, strings and CRC families. Historical finite semantic and exception-delivery gates passed. |
| AVX / AVX2 | Implemented 128/256-bit typed vector support, feature gating, arithmetic, transport, masks, gathers and permutations. Finite value, alias, footprint and exception tests passed. |
| FMA / F16C / BMI / LZCNT / MOVBE | Implemented v3 feature families with finite value/encoding/exception validation, as applicable. |
| Strict v1 userspace | Explicit v1 is supported for the implemented scalar/SSE/SSE2 and ordinary SysV ABI paths. The userspace join below checks legality, fallbacks, transport and helper requirements. This is bounded implementation validation, not an exhaustive ISA certification. |
| Formal v2/v3 completion | Existing feature families and selection are preserved; a separate final userspace join for these levels has not been signed off. Old broad full-machine flags must not be used as their verdict. |

Select the intended level explicitly with `--cpu-level v1`, `v2` or `v3` on the
x86 target. The historical `legacy` default permits POPCNT/SSE4.1 and is not a
strict-v1 target. IR/codegen coverage is not a promise that the C frontend
implements every vendor intrinsic header or that every loop auto-vectorizes.
The C frontend exposes bounded 16-byte vectors and x86-only 32-byte vectors,
plus a [documented SSE/AVX header and builtin subset](c-x86-intrinsics.md).
This source surface uses the same typed IR and feature gates; complete
`immintrin.h` compatibility remains outside its contract.
AVX-512 and optional post-baseline ARM extensions are outside this goal.

## Strict-v1 userspace contract and audit

Use `--target x86_64-unknown-linux-gnu --cpu-level v1` for `compile`, `cc`
and `asm`. The library equivalent is `Features::for_level(CpuLevel::V1)`
through `FeatureSelector`. Adding `--target-features` intentionally changes
that contract; tuning and optimization/allocation tiers do not. V1 implies
baseline integer, x87, MMX, SSE and SSE2 support from the CPU/OS. It does not
imply POPCNT, SSE3, SSSE3, SSE4.x, CX16, long-mode LAHF/SAHF, AVX, BMI,
LZCNT, MOVBE or CLFSH. CLFSH is optional at every level.

The October 2026 worktree audit found two verification gaps: ordinary codegen's
emission guard was debug-only and checked opcode requirements without
operand-dependent AVX/AVX2 requirements; broad scalar fallback experiments
were not retained as a focused regression suite. The guard now returns a
`CodegenError` in both debug and release builds for illegal ordinary emitted
instructions on text, typed-object and chunked-object paths. The assembler
also checks the selected feature set. This guard supplements intrinsic
preflight checks; it does not infer features for external code or raw bytes.
The SIMD documentation's obsolete `--cpu v1` spelling is corrected.

Supported lowering includes integer operations through i128 (subject to the
explicit exclusions below), F32/F64 arithmetic, comparisons, saturating
conversions and rounding, plus ordinary F80 value arithmetic and transport.
V1 population counts use SWAR integer operations; F32/F64 nearest-even, floor,
ceil and truncation manipulate the significand and read MXCSR for DAZ without
changing the caller's rounding mode. Neither fallback needs a runtime helper.
SSE/SSE2 handle native 128-bit primitives; remaining supported vector operations
use exact-width scalar lanes. No newer feature is silently enabled to make an
operation compile. V2/V3 retain POPCNT and hardware rounding; V3 retains native
FMA where available in its contract.

Ordinary SysV transport covers integer/FP register and stack arguments,
results, direct/indirect calls, supported tail calls, spills, varargs and
aggregate wrappers. V1 has no 256-bit SSA vector/register ABI support; genuine
MEMORY aggregates can still copy wide payloads as bytes. Ambiguous untagged
C `f64x1` boundaries remain rejected; explicit producer ABI metadata or
pointer/scalar boundaries are required. This is independent of ISA level.

Runtime requirements are part of the deployment contract:

- Explicit FMA uses `fmaf`/`fma` in v1/v2, including scalarized vector lanes;
  F80 uses `fmal` at every level. Supply a compatible libm (normally `-lm`).
  Multiplication plus addition is not substituted for a fused operation.
- i128 division/remainder require `__udivti3`, `__divti3`, `__umodti3` and
  `__modti3` as applicable, normally supplied by the compiler driver through
  libgcc or compiler-rt. Other ordinary i128 arithmetic needs no such helper.
- Memory intrinsics may call `memcpy`, `memmove`, `memset` and `memcmp`;
  external functions and TLS may require their normal libc/loader support.
  The target flag does not rebuild libraries, select a sysroot or audit
  linked objects. Libraries, startup code and any runtime dispatch must
  themselves support the deployment CPU/OS. Keel does not bundle a standalone
  v1 libc/libm/compiler runtime; linking an ELF executable does not establish
  its libraries' ISA contract.

The focused gates are `keel-codegen --test x86_userspace_v1`, the
`x86_64::feature_guard_tests` library test (also run with `--release`), and
`keel --test x86_cpu_level`. They cover all three selection tiers, both
allocators, normal and forced-spill pools, all scalar popcount widths,
rounding values and ambient rounding modes, exact memory footprints,
helper symbol sets and helper ABI/fused results. Text and typed objects must
match under the explicit feature profile. CLI tests compile C with and without
optimization and reject representative optional instructions under v1 while
accepting them at v2/v3.

Validation uses `cargo` with `--offline --locked -j2` under
`suite/fast_opt_locked.py` and the existing shared lock. Native Linux execution
on LinuxHost (Ryzen 9 9950X3D) passed the new scalar/helper gate and existing
`vector_x86`, `f80`, `x86_vector_varargs`, `x86_vector_wrappers` and
`x86_legacy_raw_fp` suites: 28 tests. These include baseline vector ABI,
unaligned values, pressure, and MXCSR conversion behavior. Generated objects,
command logs and captures stay outside Git, under `/tmp` or a dedicated
external temporary directory. A supplemental native codegen batch passed
120 tests, including the emission guard, CX16, FMA, F16C and 256-bit v3
regressions; the local assembler/codegen batch passed 175 tests. CLI and
release-guard gates passed separately. The broad assembler v3 oracle currently
hard-codes a macOS cross-assembler path, so that oracle ran locally rather than
on LinuxHost. Local GNU-oracle runs set `KEEL_GNU_AS` and
`KEEL_LLVM_OBJDUMP` to the installed x86 ELF tools. Clippy completed with
repository warnings; no warning originated in the new regression files.

Remaining limits: the execution host supports newer instructions, so this is
not execution on an original v1-only CPU or a CPUID-masked emulator. Legality
and assembly checks establish feature exclusion for the tested generated
instructions; native tests establish finite value/ABI behavior. They do not
certify arbitrary library implementations, all FP exception configurations or
every program. Existing explicit backend gaps include i128/FP conversions,
i128 high/overflow multiplication, i128 switches, x86 inline assembly and
exception handling. Vendor C intrinsic-header completeness and automatic
vectorization are separate frontend/optimization concerns. These restrictions
must diagnose rather than silently upgrade the target.

## Separate low-level work

Privileged I/O, descriptor/status queries and finite permission tests already
exist. More ambitious raw FLAGS, physical stack/frame, segment/event-return and
arbitrary saved-context contracts remain separate experiments.

Complete string-I/O guest validation and physical-stack lowering remain
unfinished. An advanced raw x87 pointer-image gate remains failed and unexplained.
These optional contracts do not block ordinary SSE/AVX code generation.
