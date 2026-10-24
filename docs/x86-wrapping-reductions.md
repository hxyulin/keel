# x86 wrapping add reductions

The x86 backend keeps `vreduce_add` on **i64x2 and i32x4** through shared
vector legalization and selects it with SSE2 only: `pshufd`,
`paddq`/`paddd`, and `movq`/`movd`. No frontend, optimizer, preferred-width or
ABI policy is involved. C vector expressions and intrinsics keep their existing
frontend contracts; this selection applies whenever typed IR contains the
operation.

## Lowering

- **i64x2:** swap the two qwords with `pshufd 0x4e`, then `paddq`.
- **i32x4:** add opposite 64-bit halves using the same shuffle, then add
  adjacent dwords using `pshufd 0xb1`.
- Extract lane zero at the original lane width.

Packed addition wraps modulo 2^64 or 2^32, so this tree produces exactly the
same bits as a sequential lane sum, and signedness does not affect the result.
An i32 result is written through a 32-bit GPR view; subsequent `sext` and
`uext` keep their distinct meanings. No widening sum is implied.

## Register and memory contract

- The input is staged in reserved xmm14 before the scalar result is defined;
  reserved xmm15 holds permutations. Allocated input registers are never
  modified, including inputs live across calls or after the reduction.
- Allocator validation rejects allocations that overlap reserved scratch
  (physical SIMD registers 13, 14 and 15).
- Spilled inputs reload exactly 16 bytes; scalar spill/result protocols are
  unchanged. Unspilled reductions use no lane buffer or lane memory traffic.

## Scope

Other reduction operations, i8/i16 lanes, 64-bit vectors, 256-bit reductions
(AVX2 Y reductions keep their existing path) and all FP reductions keep their
existing behavior. FP evaluation order, min/max semantics, the PMULLD helper,
the AVX conversion result-width guard and Apple ARM64 lowering are unaffected.
The feature guard still checks debug and release emission.

## Tests

`crates/keel-codegen/tests/vector_reduce_add_x86.rs` covers strict v1/v2/v3,
effective v3 with AVX/AVX2/FMA/F16C disabled, three selection tiers, fast/full
allocation, normal/tiny/empty pools, forced register views and input/result/
shared spill homes, reserved-scratch rejection, text/typed/chunked whole-object
parity (including metadata), and excluded-operation controls. A legalizer unit
test checks all eight integer reduction operations across D/Q and
representative Y shapes at v1/v2/v3.

```sh
cargo test -p keel-codegen --test vector_reduce_add_x86
```

The native C oracle checks every combination of eight boundary values across
the lanes (64 i64x2 / 4,096 i32x4 cases), then 4,096 lane-probe/random inputs
per shape. It covers lane isolation, wrapping arithmetic, vector-parameter/
scalar-result ABI, signed/unsigned extension, live values across calls,
unaligned memory, output canaries, input preservation and both protected-page
edges. Its reference uses unsigned scalar arithmetic, independent of the
shuffle tree.

To run natively, set `KEEL_REDUCE_EVIDENCE=<root>/current` when running the
test, then on a Linux x86-64 host run
`crates/keel-codegen/tests/vector_reduce_add/run_native.py <root>`. It links
each oracle with Clang, executes it, and compares Clang-assembled `.text` bytes
with Keel's objects. `bench.py <root>` in the same directory alternates
CPU-pinned samples of a `current` and a `baseline` export.

The ignored `compile_cost` test measures typed code generation only:

```sh
cargo test -p keel-codegen --release --test vector_reduce_add_x86 compile_cost \
  -- --ignored --nocapture --test-threads=1
```

## Results

Native execution on x86-64 Linux passes all exported oracles in debug and
release. This was a newer x86 CPU, not a v1-only machine; v1 legality of the
emitted code rests on feature checks and encoding validation.

In the v1 Full-selection/Full-allocation unspilled `sum` functions, explicit
data-memory operands fell from three to zero (i64x2) and five to zero (i32x4),
and the reserved stack frame fell from 32 to 16 bytes. Instruction counts are
unchanged (12 and 14). Short external-call kernel timings showed overlapping
ranges before and after, so no runtime speedup is claimed.
