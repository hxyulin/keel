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
| Optional PCLMULQDQ | Explicit legacy 128-bit carryless multiplication, dedicated typed intrinsic, assembler and small C `wmmintrin.h` subset; disabled in every level/default. See [the bounded contract](x86-pclmulqdq.md). PCLMULQDQ is not in psABI v3. |
| AVX / AVX2 | Implemented 128/256-bit typed vector support, feature gating, arithmetic, transport, masks, gathers and permutations. Finite value, alias, footprint and exception tests passed. |
| FMA / F16C / BMI / LZCNT / MOVBE | Implemented v3 feature families with finite value/encoding/exception validation, as applicable. |
| Optional legacy AES-NI | Six explicit 128-bit AES round/key-assist operations, typed assembly/IR/codegen and a bounded C wrapper surface. Independently gated by `+aes`, disabled in default/v1/v2/v3; see [AES-NI](x86-aes-ni.md). |
| Strict v1 userspace | Explicit v1 is supported for the implemented scalar/SSE/SSE2 and ordinary SysV ABI paths. The userspace join below checks legality, fallbacks, transport and helper requirements. This is bounded implementation validation, not an exhaustive ISA certification. |
| Strict v2 userspace | Explicit v2 joins cumulative v1 behavior with SSE3, SSSE3, SSE4.1, SSE4.2, POPCNT, CX16 and long-mode LAHF/SAHF. The bounded audit and gates below validate the implemented userspace contract. |
| Strict v3 userspace | The bounded implementation audit and join below validate cumulative v1/v2 behavior, mixed 128/256-bit SysV transport and the existing v3 families. Independent review/integration remains a separate step; old broad full-machine flags are not its verdict. |

Select the intended level explicitly with `--cpu-level v1`, `v2` or `v3` on the
x86 target. The historical `legacy` default permits POPCNT/SSE4.1 and is not a
strict-v1 target. IR/codegen coverage is not a promise that the C frontend
implements every vendor intrinsic header or that every loop auto-vectorizes.
The C frontend exposes bounded 16-byte vectors and x86-only 32-byte vectors,
plus a [documented SSE/AVX header and builtin subset](c-x86-intrinsics.md).
This source surface uses the same typed IR and feature gates; complete
`immintrin.h` compatibility remains outside its contract. The
[bounded SSE2 packed shift subset](c-x86-intrinsics.md#bounded-sse2-packed-shifts)
adds sixteen 128-bit C wrappers using existing architectural shifts with
oversized-count zero/sign-fill, checked immediate controls and low-64-bit
runtime vector counts. Native LinuxHost GCC/Clang, strict-v1 objects and ARM
frontend diagnostics are validated in debug/release; this does not add AVX2
shift wrappers or general C scalar broadcast shifts.
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
SSE/SSE2 handle native 128-bit primitives, including bounded wrapping
`vreduce_add` selection for i64x2/i32x4; see the
[reduction contract and validation](x86-wrapping-reductions.md). Remaining
supported vector operations use exact-width scalar lanes. No newer feature is
silently enabled to make an operation compile. V2/V3 retain POPCNT and hardware rounding; V3 retains native
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

## Strict-v2 userspace contract and audit

Use `--target x86_64-unknown-linux-gnu --cpu-level v2` explicitly for `compile`,
`cc` and `asm`; the library equivalent is `Features::for_level(CpuLevel::V2)`
through `FeatureSelector`. V2 adds SSE3, SSSE3, SSE4.1, SSE4.2, POPCNT, CX16
and long-mode LAHF/SAHF to the v1 contract. It does not enable AVX/AVX2, BMI1/2,
FMA, F16C, LZCNT or MOVBE. CLFSH remains independently gated at every level.
Explicit `--target-features` modifiers intentionally change this contract.
The v1 runtime-helper and unsupported-operation restrictions above also apply
to v2: in particular, fused F32/F64 operations require libm `fmaf`/`fma`, F80
requires `fmal`, and i128 division/remainder require the compiler runtime.
Ordinary 128-bit vector SysV transport is supported; 256-bit SSA vectors and
their register ABI require an explicitly AVX-enabled profile.

The October 23 audit started at main `74ecf5db`, including the completed v1 work.
It found concrete verification gaps before adding code: the scalar/helper join
executed only v1 despite generating v2 objects; CLI rejection checks established
only the v1 boundary; vector ABI execution used legacy/AVX profiles; and the
userspace join did not compare chunked objects with text and typed objects.
No missing v2 lowering was found in this audit, so the changes extend regression
coverage rather than alter feature selection or add instruction implementations.
Baseline selection may scalarize a generic vector operation even when a higher
level permits a native instruction. That is legal and is tested separately from
explicit architectural intrinsics.

The focused gates now include:

- `keel-codegen --test x86_userspace_v2`: cumulative 128-bit vector operations,
  integer/FP masks, lanes and shuffles; ordinary SysV register/stack arguments,
  results, direct/indirect/tail calls, varargs, block-argument cycles and values
  live across calls. All three selection tiers, both allocators, normal pools
  and empty pools are exercised. Representative SSE3 horizontal addition,
  SSSE3 byte shuffle, SSE4.1 widening multiply and signed i64 comparison check
  input/output aliases, live inputs, canaries and both guarded-page edges.
  Text, typed and chunked objects must match, and every typed instruction's
  operand-dependent feature requirements must fit strict v2. Representative
  operations require no undefined helper symbols. A separate self-contained
  LAHF/SAHF sequence checks all 256 low-status inputs in long mode; it claims
  no arbitrary physical FLAGS preservation across calls or IR boundaries.
- `keel-codegen --test x86_userspace_v1`: cumulative scalar/ordinary ABI emission
  at v1/v2/v3, plus native v1/v2 scalar and fused-helper results. Helper transport
  now covers all selection tiers, both allocators and normal/empty pools.
  V1 keeps SWAR population counts and software rounding; v2 keeps POPCNT and
  hardware rounding; v3 keeps native FMA and its smaller helper symbol set.
- `keel-codegen --lib x86_64::feature_guard_tests` and
  `keel --test x86_cpu_level`, also run with `--release`: reject representative
  AVX/AVX2, BMI1/2, FMA, F16C, LZCNT, MOVBE and CLFSH emission under v2. The CLI
  checks v1/v2/v3 acceptance and explicit feature overrides, including both
  LAHF and SAHF. It compiles C with and without optimization through text and
  direct objects using explicit CPU levels; this adds no C intrinsic headers.

Existing focused native gates supplement the join: `x86_crc32` uses a software
Castagnoli reference for every supported operand width; `x86_legacy_strings`
checks controls, signed lengths, masks, indices and aliases; `x86_legacy_v2_fp`
checks rounding, horizontal/dot operations and MXCSR effects;
`x86_packed_integer` and `x86_packed_pair` cover packed arithmetic, saturation,
register reuse and guarded memory. `x86_cx16` checks success/failure, i128 SysV
transport, fixed-register pressure, exact 16-byte accesses and atomic counters.
Its existing fault/unwind checks are supplemental finite tests, not a kernel or
arbitrary saved-context contract. Existing vector wrapper/varargs and v3
FMA/F16C/256-bit transport gates remain regression controls.

Validation uses `cargo` with `--offline --locked -j2`, serialized by
`suite/fast_opt_locked.py`; native Linux tests run on LinuxHost in an isolated
snapshot. `KEEL_V2_EXPORT_DIR` optionally retains the join's objects and assembly
for independent disassembly. Bulk objects and logs stay outside Git under
`/tmp`. The final focused native run passed five userspace-join tests in both
debug and release builds; seven additional FMA/F16C/256-bit regression tests
passed in debug. The supplemental native batch passed 28 existing CRC, strings,
FP, packed-integer/pair, CX16, vector-varargs and wrapper tests. Local debug and
release CLI and emission-guard gates passed. Clippy completed with repository
warnings; no new regression-file warning remained.

An independent `llvm-objdump` scan examined 36 exported strict-v2 objects per
profile (473,190 instruction lines each) and found no AVX-family mnemonic, YMM/ZMM
operand, BMI1/2, FMA, F16C, LZCNT, MOVBE, CLFSH or XSAVE-family match. This is a
mnemonic/operand exclusion scan, not a complete instruction decoder proof.
The checked objects were produced through all 12 selection/allocation/pool
combinations and the text/typed/chunked byte-parity checks above. Native logs
are `/tmp/keel-v2-native-{debug,final-debug,final-release}.log`; exported objects,
disassembly and `disassembly-audit.json` are under
`/tmp/keel-v2-objects-{debug,release}` on LinuxHost. The isolated source snapshot is
`/tmp/keel-strict-v2-20261023.QXrgEl`.

Limits remain explicit: the execution host supports newer ISA levels. This is
not a v2-only CPU or CPUID-masked execution test. Feature checks and finite
instruction-byte inspection establish exclusion for the tested generated code;
value, memory and ABI tests establish bounded behavior. They do not establish
exhaustive ISA certification, linked-library compatibility, every FP exception
configuration or correctness of every program. Kernel work, arbitrary physical
state and C vendor intrinsic-header completeness remain separate.

## Strict-v3 userspace contract and audit

Use `--target x86_64-unknown-linux-gnu --cpu-level v3` explicitly for `compile`,
`cc` and `asm`; the library equivalent is `Features::for_level(CpuLevel::V3)`
through `FeatureSelector`. V3 retains v1/v2 and adds AVX/AVX2, BMI1/2, FMA,
F16C, LZCNT and MOVBE, with the normal XSAVE/OSXSAVE and XCR0 availability
contract for AVX state. The target flag declares that deployment contract; it
does not probe the deployment machine or enable OS state. CLFSH remains an
independent optional feature. AVX-512 and other optional/post-v3 extensions
are not implied. Explicit feature modifiers intentionally change the contract.

The October 23 implementation audit started at `9265dcf9` and was rebased onto
main `3f6a30da` before final validation, including the reviewed final vector
canonicalization changes. Concrete remaining verification gaps were mixed-width
SIMD argument banking, missing chunked-object
parity in the existing wide generic/ABI suite, scalar/helper native execution
that skipped v3, and public CLI coverage limited to scalar C. No lowering defect
was found in this bounded audit. No optimizer, feature-policy or backend
implementation changes were needed.

The maintained join reuses the v1/v2 gates and the existing finite family suites:

- `keel-codegen --test x86_userspace_v3` mixes 128/256-bit integer/FP vectors
  and scalar FP in one SysV signature, exhausting the shared eight-register
  SIMD argument bank and placing both widths on the stack. Direct/indirect
  calls and supported direct/indirect tail calls transport distinct arguments
  and a 256-bit result in both C/Keel directions. A call that clobbers every
  SIMD register forces live arguments across calls; byte sentinels check
  stores. Clang and GCC harnesses execute all three selection tiers, both
  allocators, and normal/empty pools. A separate small assembled sequence
  checks 16/32/64-bit LZCNT, TZCNT and MOVBE values, in-place aliases, unaligned
  accesses and both protected-page edges.
- The shared `common/userspace.rs` helper retains the v2 text/typed/chunked
  object byte-parity and operand-dependent feature gate, and now applies it
  to that mixed join and `x86_avx256_generic`. The latter retains its existing
  six-shape wide scalar-reference corpus, register/stack ABI, callbacks,
  varargs, stack-to-stack tails, block arguments and pressure tests.
  The cumulative `x86_userspace_v1` gate now executes v3 scalar and helper
  results on capable native hosts, retaining its smaller native-FMA helper
  set. Native branches remain conditional on the instructions they execute;
  passing cross-object checks alone does not claim native execution.
- `keel --test x86_cpu_level` now compiles ordinary wide IR and the bounded C
  AVX header surface, with/without optimization, through public text and direct
  object paths at explicit v3. Reassembly must match direct objects. Ordinary
  multiply/add remains unfused; v1/v2 reject the wide ABI. Existing assembly
  gates accept v3 families, reject CLFSH without an override, and the new gate
  rejects representative AVX-512 forms. The emission guard remains checked in
  debug and release.

Support layers remain distinct. Ordinary vector IR and the documented C subset
use the backend's legal selection or scalar expansion. Explicit FMA preserves
fused rounding; ordinary multiply/add does not implicitly contract. V3 F32/F64
FMA can use hardware, while F80 still requires `fmal`. The v1 runtime/library
and unsupported-operation restrictions still apply. BMI extraction/deposit and
F16C have typed architectural operations. LZCNT and MOVBE have assembler
encoding/feature support; generic CLZ currently uses BSR-based lowering and
generic byte swaps need not select MOVBE. Their assembly checks do not claim
new C builtins or typed intrinsics. The bounded C headers still exclude FMA,
F16C, masks/gathers, general conversions and complete vendor-header support,
even where typed backend operations exist.

Native LinuxHost validation (Ryzen 9 9950X3D, existing rustc 1.99.0) passed the new
join, cumulative v1/v2 joins and wide generic/ABI suite in debug and release.
The existing debug `x86_fma`, `x86_f16c`, `x86_bmi_semantics`, `x86_masked_memory`
and `x86_gather` suites also passed: explicit fused versus separate results,
half-pattern conversion and rounding/MXCSR effects, integer extraction/deposit,
mask sign bits, exact active-lane accesses, fault suppression and source aliases.
The native `keel-c --test x86_intrinsics` suite passed with Clang 23.1.1 and
GCC 16.2.0 across its three configurations, including guarded 16/32-byte memory,
wide arguments, indirect calls and callbacks. Local Apple ARM64 C controls
(`gnu_types`, `gnu_vector_broadcast`, `gnu_vector_abi`, `x86_intrinsics`) passed
19 tests, including the maintained native 16-byte ABI/broadcast harness. Public
CLI gates passed locally in debug/release; the guard passed local debug and
native release. Clippy completed with existing repository warnings and no
warning in the new join/helper or extended CLI test.

Independent `llvm-objdump` decoding examined 149 exported objects (871,303
instruction lines): 96 debug family objects, 24 release wide objects, 13 mixed/
assembly objects per profile, and three Keel C objects. No EVEX prefix,
ZMM/extended SIMD/opmask register, CLFSH, or scanned optional/post-v3 mnemonic
(AES/PCLMUL/SHA/GFNI, ADX, random-number, FSGSBASE/PKU, extended XSAVE, VNNI,
IFMA, newer conversions or AMX) appeared. The complete decoded mnemonic
inventory was also retained. This is a finite byte/mnemonic exclusion scan,
not a complete independent feature-decoder proof. Typed operand-feature checks
and text/typed/chunked parity supplement it; the native C objects use the
frontend's existing typed object path.

The existing local `keel-asm --test x86_v3` oracle also passed 11 tests against
Clang and GNU as, independently checking the maintained finite encoding forms.

Builds used `cargo` with `--offline --locked -j2`, shared default caching and
per-checkout managed targets, `CARGO_CACHE_LINKS=0`, serialized through
`suite/fast_opt_locked.py` and `/tmp/keel-perf-20261019.lock`. No toolchains or
host configuration changed. LinuxHost source is
`/tmp/keel-strict-v3-final-20261023.nXp6FV`; logs are `/tmp/keel-v3-final-native-*.log`.
Exports are `/tmp/keel-v3-final-{objects,families}-{debug,release}` and
`/tmp/keel-v3-final-c-native`; decoded listings sit beside the objects, with the
scan script/report at `/tmp/keel-v3-final-scan.py` and
`/tmp/keel-v3-final-disassembly-audit.json`. Bulk evidence stays outside Git.

The host supports newer features: execution does not establish behavior on a
v3-only CPU or under CPUID masking. These checks establish finite userspace
values, ABI, footprints and generated-instruction legality, not arbitrary
library compatibility, every FP exception configuration or exhaustive ISA
certification. Kernel/raw x87 experiments, arbitrary physical stack/FLAGS,
full vendor headers and arbitrary auto-vectorization remain outside this join.
Independent review and parent integration are still required.

## Separate low-level work

Privileged I/O, descriptor/status queries and finite permission tests already
exist. More ambitious raw FLAGS, physical stack/frame, segment/event-return and
arbitrary saved-context contracts remain separate experiments.

Complete string-I/O guest validation and physical-stack lowering remain
unfinished. An advanced raw x87 pointer-image gate remains failed and unexplained.
These optional contracts do not block ordinary SSE/AVX code generation.
