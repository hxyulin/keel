# SSE4.1 signed low dword register widening

When SSE4.1 is enabled, `swiden_low.i64x2` from exactly `i32x4` selects a single
`pmovsxdq destination, source` instead of the multi-instruction SSE2 recipe.
The change is in `x86_64/vector.rs::vector_convert` and reuses
`V2Opcode::Pmovsxdq`.

## Contract

- Eligibility is the effective `sse41` feature bit, not a CPU-level label.
  V1 and explicit `-sse41` keep the SSE2 recipe.
- The destination is the allocated result register, or reserved FS0 when the
  result is spilled.
- The source is resolved and read before the destination is defined. Register
  aliasing is legal and live allocated sources are not overwritten. A spilled
  input is reloaded from its complete 16-byte vector home before the register
  operation; a spilled result is stored at its full 16-byte width.
- This selector never emits a memory-source PMOVSXDQ or folds a user load. The
  existing m64/m128 load covers ([signed widening loads](avx2-signed-widen-load-pilot.md))
  keep priority and their existing memory footprints.

Unchanged: unsigned/high widening, other lane widths, 64-bit vectors, FP
conversions and bitcasts, any 256-bit source or result, PMULLD selection, and
the [AVX-only wide-result fallback](avx-widen-fallback.md). There are no
dispatch, ARM, FP, ABI, tuning or default-policy changes.

The fold improves already formed IR, including vectorized ordinary scalar
loops. It does not change C frontend recognition or vectorizer formation, and
adds no numeric vector C builtin.

## Testing

```sh
cargo test -p keel-codegen \
  --test vector_widen_x86 --test x86_signed_widen_load \
  --test x86_signed_wide_load --test x86_avx_widen_fallback \
  --test x86_wide_conversions --test vector_widen_aarch64 \
  --test x86_pmulld_selection
cargo test -p keel-codegen --lib feature_guard_tests
```

Repeat with `--release`. `vector_widen_x86` covers v1/v2/v3, v1 plus SSE4.1 and
v2/v3 minus SSE4.1 through all three selectors, both allocators and
normal/constrained/empty vector pools. Controlled layouts test equal/distinct
registers and independent input/result spills. Text, typed and chunked objects
must agree, and every typed instruction must fit the effective feature set. A
Clang encoding test compares all 256 XMM register pairs.

Setting `KEEL_X86_WIDEN_EVIDENCE=<dir>` (and `KEEL_SIGNED_LOAD_EVIDENCE`,
`KEEL_WIDE_LOAD_EVIDENCE` for the load-cover suites) exports ELF objects and C
harnesses. Run them on x86-64 Linux with
`crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>`. The harnesses
use bitwise sign-extension oracles over signed extrema, every source bit and
random vectors, with aliased outputs, sentinels, live sources across calls that
clobber all sixteen XMM registers, exact 16-byte page edges, faults before
publication and unchanged MXCSR. These are finite value/ABI/footprint checks,
not exhaustive ISA certification.

## Measured effect

In `suite/runtime-kernels/kernels.c` at v2/v3 (and v3 minus AVX2):

| Function       | Register widens | Instructions | Bytes      | Vector-loop block |
|----------------|-----------------|--------------|------------|-------------------|
| `rk_balance32` | 4               | 163 -> 147   | 610 -> 542 | 85 -> 69          |
| `rk_clamped`   | 2               | 158 -> 150   | 602 -> 568 | 65 -> 57          |

Every other function is unchanged; v1, explicit `-sse41` and ARM objects are
byte-identical.

Median time per call on x86-64 Linux (Ryzen 9 9950X3D, pinned core, seven
paired rounds):

| Kernel             | 65,536 elements    | 1,048,576 elements   |
|--------------------|--------------------|----------------------|
| v2 `balance32`     | 21.235 -> 16.777 us | 333.662 -> 262.433 us |
| v3 `balance32`     | 21.190 -> 16.794 us | 333.742 -> 262.623 us |
| v2 `clamped`       | 33.125 -> 27.531 us | 523.650 -> 433.654 us |
| v3 `clamped`       | 33.221 -> 27.503 us | 524.016 -> 433.481 us |

Byte-identical controls varied by at most 1.33%. These are bounded single-host
kernel samples, not an application or general width-profitability claim.
Compile time for the kernel corpus was unchanged within noise.
