# AVX2 signed m128 widening load

The x86-64 backend emits a single fused instruction

```text
vpmovsxdq ymmResult, xmmword ptr [address]
```

for a native `load.i32x4` immediately followed by `swiden_low.i64x4`. This is a
backend-only instruction-selection cover: the optimizer, capability context,
CLI, ABI and default 128-bit reduction policy are unchanged. The existing
m64/XMM `PMOVSXDQ` cover is also unchanged.

## When the cover applies

- Both effective AVX and AVX2 are required, independently of the nominal CPU
  level.
- The load is nonvolatile and has exactly one resolved use.
- Both instructions have canonical results, identical complete source
  locations, and no existing cover, match, constraint or condition.
- The pair occupies adjacent positions in the same block.

Other element types, unsigned or high widening, shared inputs and intervening
instructions keep the existing lowering. Generic legacy SIMD loads are not
rewritten.

## Selection and emission

Selection transfers the load's selected operands to the widening root, so the
selected address leaves (including resolved aliases and spilled leaves) stay
live until emission. Emission uses the existing address materialization path
before writing the allocated full YMM destination, and reads exactly the
original 16 bytes.

- GPR address registers cannot alias the SIMD destination.
- A spilled 256-bit destination is completed through the reserved scratch
  register.
- Unrelated live YMM upper halves remain allocated and untouched; the removed
  XMM intermediate has no other observer.
- No FP arithmetic or FP control instruction is introduced.
- Adjacency preserves fault order.

Without AVX2, AVX-only targets use the generic fallback described in
[AVX-only vector widening fallback](avx-widen-fallback.md).

## Validation

`x86_signed_wide_load` covers all three selection tiers, both allocators,
normal, one-SIMD-register and empty pools; typed/text/chunked object parity;
effective feature overrides; constraint and source-location boundaries; the
shared, volatile, unsigned, high, other-load-shape and side-effect exclusions;
optimized and dead results; and an independent Clang byte oracle for low/high
registers, indexed memory and negative displacement. Private unit tests in
`x86_64::packed_load::wide_tests` exercise selected leaves, result aliases,
conditions, matches, existing covers and both feature gates.

```sh
cargo test -p keel-codegen --test x86_signed_wide_load --test x86_avx_widen_fallback
cargo test -p keel-codegen --lib x86_64::packed_load::wide_tests
cargo test -p keel-opt -p keel --test vector_context
```

Native execution: set `KEEL_WIDE_LOAD_EVIDENCE=<dir>` to export ELF objects,
assembly and C harnesses, then run the exported `run_native.py` on a Linux
x86-64 host. The 72 native configurations (18 each for v3 AVX2, v1+AVX,
v2+AVX and v3-minus-AVX2) run five shapes: direct, offset, indexed, unrelated
live YMM, and wrapping accumulation from `UINT64_MAX`. The oracle checks signed
extrema, deterministic random inputs, all 16 byte alignments, guard pages on
both ends with exact 16-byte boundaries, overlapping input/output, live pointer
preservation and MXCSR preservation, in both normal and forced-spill cases. The
C entry point checks AVX2 support before calling generated code.

All 72 configurations pass on x86-64 Linux, together with the 121 fallback
bundles from `x86_avx_widen_fallback`. Validation is on an AVX2-capable CPU
with AVX-only code generation; it does not include an AVX-only physical CPU.

## Performance

One finite runtime probe on x86-64 Linux compared the previous explicit-wide
lowering with this cover on a wrapping-sum kernel (n = 1048576, both compiled
with `keel compile --target x86_64-unknown-linux-gnu --cpu-level v3`). Median
time went from 208.6 to 49.2 microseconds. Static loop size dropped from 45 to
33 instructions per 16 inputs: four legacy loads and eight widening-stage
copies disappear, with no accumulator spills.

This single probe predates the conservative wide-context policy and does not
establish general profitability across targets and sizes, or a controlled
comparison against the default 128-bit output. Explicit wide optimization still
requires the complete capability recipe and preference; default CLI and
library contexts keep 128-bit output.
