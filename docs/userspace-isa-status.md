# ISA support

Assembly, decoding, compiler IR, C intrinsics and runtime qualification are
separate layers. An imported instruction record does not imply support in all
of them.

## Supported families

| Area | Supported scope |
| --- | --- |
| x86-64 baseline | Integer, SSE/SSE2, x87/F80 and ordinary SysV ABI code generation |
| x86-64 v2/v3 families | Feature-gated SSE3 through SSE4.2, AVX/AVX2, FMA, F16C, BMI, LZCNT and MOVBE operations |
| Optional x86 extensions | Selected AES, carryless multiply, query, state-management and system operations |
| EVEX | Imported AVX-512F/VL forms and foundation mask operations in assembler/decoder; not complete AVX-512 compiler or C intrinsic coverage |
| AArch64 | Scalar native code generation and baseline NEON vectors, memory operations and ABI transport |
| Privileged x86 | Generated instruction forms plus explicit typed system APIs and owned return adapters where documented |

The [generated coverage ledger](x86-coverage/README.md) records structural
assembly/decoder coverage. [Intrinsic selection](x86-intrinsics.md) describes
the compiler-facing subsets. Detailed privileged APIs retain their own
preconditions; ordinary userspace code does not configure privilege or CPU state.

## Feature selection

The x86 default is strict v1. Select v1, v2 or v3 with `--cpu-level` and use
`--target-features` for explicit additions. Named profiles and prerequisite
rules are documented in the [CPU profile reference](../tools/x86-isa/cpu-profiles.md).
Tuning and optimization settings do not grant instruction availability.

CPU feature permission does not establish OS state availability. AVX needs the
appropriate OSXSAVE/XCR0 state; privileged instructions need their documented
privilege, model and machine-state contracts. Full vendor intrinsic-header
compatibility and post-baseline ARM extensions are not implied.

## Runtime dependencies

Fused operations may require `fmaf`, `fma` or `fmal`; i128 division may require
compiler runtime helpers. Memory operations can call libc routines. The caller
must link libraries compatible with the deployment target. Selecting a CPU
profile does not rebuild or audit those libraries.
