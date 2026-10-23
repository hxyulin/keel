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
| Formal userspace v1/v2/v3 completion | The feature implementation is broad. A final userspace-only requirement join has not been separately signed off; old broad full-machine flags must not be used as its verdict. |

Select the intended level explicitly with `--cpu-level v1`, `v2` or `v3` on the
x86 target. The historical `legacy` default permits POPCNT/SSE4.1 and is not a
strict-v1 target. IR/codegen coverage is not a promise that the C frontend
implements every vendor intrinsic header or that every loop auto-vectorizes.
AVX-512 and optional post-baseline ARM extensions are outside this goal.

## Separate low-level work

Privileged I/O, descriptor/status queries and finite permission tests already
exist. More ambitious raw FLAGS, physical stack/frame, segment/event-return and
arbitrary saved-context contracts remain separate experiments.

Complete string-I/O guest validation and physical-stack lowering remain
unfinished. An advanced raw x87 pointer-image gate remains failed and unexplained.
These optional contracts do not block ordinary SSE/AVX code generation.
