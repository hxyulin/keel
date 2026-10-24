# Generic x86 SIMD selection

This page covers several bounded 128-bit SIMD selection rules in the x86-64
backend: generic shuffles, unsigned byte comparisons, a signed dword widening
load, and dword multiply retention. All of them reuse existing allocation,
spill and typed-assembler paths, and none changes optimizer widths, feature
policy, ABI or FP semantics.

## Generic shuffles

The generic 128-bit shuffle selector uses existing SSE2 encodings for:

- Every one-source dword permutation, including resolved `[x,x]` inputs.
- Two-source dword shuffles with two low lanes from one source and two high
  lanes from the other, in either source order.
- All qword permutations; the existing two-source `shufpd` window is retained.
- Byte/word permutations that preserve aligned groups of four bytes.

Classification is shared by vector legalization and final emission. `pshufd`
reads its source before defining the destination; destructive two-source
shuffles stage both inputs in reserved scratch before publishing the result.
These recipes are legal at explicit v1, v2 and v3.

Only register shuffles change. Original loads/stores keep their sizes, ordering
and alignment. The recipes move bits, including NaN payloads and signed zeros,
and do not modify MXCSR. No FP arithmetic, FMA contraction or min/max rewrite
is introduced.

### Tests

`x86_generic_shuffle` and `vector_shuffle_window` verify typed/text object
equality for every selector/allocator/level/pool combination, and all 256
`pshufd`/`shufps` immediates against Clang's x86 assembler. A unit test models
all 4,096 dword masks and all 16 qword pairs, including same-input selection
and rejection of non-group-preserving masks. Window tests cover destination
aliases, independent spilled inputs/results and boundary edits.

```sh
cargo test -p keel-codegen --test x86_generic_shuffle --test vector_shuffle_window
```

Set `KEEL_SIMD_EVIDENCE` / `KEEL_SHUFFLE_EVIDENCE` to export ELF+C oracles.
On Linux x86-64, `crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>`
runs the generic exports and `crates/keel-codegen/tests/vector_shuffle/run_native.py`
runs the window exports. The generic oracle checks all 256 byte patterns plus
FP edge payloads, offsets 0..15, output aliasing either input, both inputs
remaining live, accessible-page boundaries and unchanged MXCSR. All exported
oracles pass on x86-64 Linux. This is finite semantic coverage, not exhaustive
equivalence.

### Static measurements

Run `measure_generic_shuffle_codegen` with `-- --ignored --nocapture`. The
128-function corpus in `x86_generic_shuffle::fixture` includes all qword index
pairs, representative dword/byte-group/word-group masks, repeated and reversed
lanes, same inputs, and both inputs remaining live. Counts use Full selection
without an IR optimization pass, include all function overhead, and are the
same at v1, v2 and v3:

| Configuration | Instructions | `.text` bytes | Frame-memory (vector) |
| --- | ---: | ---: | ---: |
| Full alloc, normal pools | 4,268 -> 2,528 | 16,063 -> 8,700 | 892 -> 256 (372 -> 0) |
| Fast alloc, normal pools | 4,396 -> 2,656 | 16,465 -> 8,702 | 892 -> 256 (372 -> 0) |
| Full alloc, empty vector pools | 5,528 -> 3,360 | 23,708 -> 14,323 | 2,164 -> 1,184 (1,396 -> 928) |
| Fast alloc, empty vector pools | 5,656 -> 3,488 | 24,031 -> 14,326 | 2,164 -> 1,184 (1,396 -> 928) |

Frame-memory counts include ABI staging and the old scalarization buffers; they
are not allocator spill-slot counts. No runtime speedup is claimed.

## Unsigned byte comparisons

The unsigned `i8x16` compare recipe builds its sign-bit bias with
`pcmpeqd; psllw 7; packsswb`. Each all-ones word becomes `0xff80` (-128), which
packs exactly into one `0x80` byte; packing the same register as both inputs
fills all sixteen lanes. This replaces six bias-building instructions with
three, using the same reserved scratch register, with no new memory access or
feature requirement. Ordering, operand swapping, canonical masks and non-strict
comparison inversion keep their existing code paths.

`x86_unsigned_byte_compare` checks strict v1/v2/v3 encoding through typed and
textual object paths and compares the bias bytes with Clang. Its exported
oracles (`KEEL_BYTE_COMPARE_EVIDENCE`) check all 65,536 operand pairs at every
lane for all four unsigned ordering predicates, with unaligned/aliased outputs
and live inputs; they pass on x86-64 Linux together with the existing
comparison/mask oracles.

```sh
cargo test -p keel-codegen --test x86_unsigned_byte_compare
```

The ignored `measure_unsigned_byte_compare_codegen` test measures the
four-predicate corpus (same at every CPU level):

| Configuration | Instructions | `.text` bytes | Frame-memory |
| --- | ---: | ---: | ---: |
| Full alloc, normal pools | 120 -> 108 | 469 -> 404 | 8 (unchanged) |
| Fast alloc, normal pools | 124 -> 112 | 471 -> 406 | 8 (unchanged) |
| Full alloc, empty vector pools | 144 -> 132 | 601 -> 536 | 40 (unchanged) |
| Fast alloc, empty vector pools | 148 -> 136 | 636 -> 571 | 40 (unchanged) |

## Reproducing revision comparisons

The measurement tests are ignored by default:

```sh
cargo test -p keel-codegen --test x86_generic_shuffle \
  --test x86_unsigned_byte_compare -- --ignored --nocapture
```

To compare against an older revision that lacks these tests, export that
revision into a scratch directory (for example with `git archive`), copy only
the test files into it, and run only the ignored measurements there: the
ordinary encoding tests assert that the new recipes are selected. Compare the
printed instruction, byte and frame-access counts directly.

Cross-compiling from a non-x86 host checks object generation, typed/text
equality, Clang encodings and C syntax but does not execute the exported
oracles; run those on Linux x86-64 with the native runners above.

## Remaining limitations

Arbitrary cross-source dword masks and byte/word masks that split dword groups
still use the lane legalizer. D (64-bit) vectors and 256-bit shuffle selection
keep their existing paths (see [AVX2 shuffle selection](x86-avx2-shuffle-selection.md)
for the half-local 256-bit case). Single-source identity shuffles can still emit
`pshufd` when an earlier IR optimization has not removed them. Two-source
staging is conservative and can add copies. No generalized blend, FP
comparison, narrowing or addressing policy is added.

General widening-load fusion still needs separate profitability and semantic
checks; the signed-dword m64 cover below handles one closed chain. Noncanonical
select masks and other faulting-load shapes remain outside it.

## Exact signed dword widening load pilot

The exact-low-eight-byte load cover recognizes this adjacent chain:

```text
v2 = load.i64 align 1 v0
v3 = vsplat.i64x2 v2
v4 = bitcast.i32x4 v3
v5 = swiden_low.i64x2 v4
```

When the effective feature set permits SSE4.1, it emits `pmovsxdq xmm, m64` at
the widen, covering the first three instructions.

Conditions:

- Each intermediate has exactly one use.
- All four instructions keep their own SSA result, share the same complete
  source location, and have no existing selection, condition or allocation
  constraint.
- Volatile loads, nonadjacent operations, shared intermediates, other shapes
  and unsigned/high widening keep existing lowering.
- Explicit v1 and `-sse41` keep the SSE2 fallback; v1 plus `+sse41` permits
  the cover.

The memory operand is exactly the original eight-byte access, at the same
position relative to effects and other accesses; only adjacent pure vector
operations precede the replacement. Address leaves, register allocation,
reserved address scratch, destination allocation and spill publication reuse
the existing low-load path. The instruction changes neither integer flags nor
MXCSR. Optimizer widths, feature policy, general conversion lowering, the
positive32 DAG and the AVX2 recipe are unchanged. The 256-bit AVX2 analogue is
described in [AVX2 signed m128 widening load](avx2-signed-widen-load-pilot.md).

### Tests

`x86_signed_widen_load` checks text/typed/chunked object parity and each typed
instruction's required features for Baseline/Trees/Full selection, Fast/Full
allocation, normal/tiny/empty pools and six effective profiles (v1/v2/v3,
v1+sse41, v2-sse41, v3-sse41). It covers negative forms, shared operands,
constraints at every chain node, source line/column boundaries, optimized and
unused results, and Clang m64 encodings with extended registers and
indexed/displaced addresses. Selector unit tests cover conditions, covered
nodes, resolved aliases and load operand transfer.

```sh
cargo test -p keel-codegen --test x86_signed_widen_load --test x86_packed_low_load \
  --test vector_widen_x86 --test x86_packed_positive
```

Set `KEEL_SIGNED_LOAD_EVIDENCE=<dir>` to export native bundles and run the
exported `run_native.py --dir <dir>` on Linux x86-64. All 108 bundles pass on
x86-64 Linux: direct, offset and indexed addressing, live vector/address
sources, destination memory aliasing the source, signed extrema and
deterministic xorshift samples, all 16 alignment offsets, both protected-page
edges, faulting accesses crossing either edge before any output write, canaries
and MXCSR preservation. These are finite tests on an SSE4.1 host, not a
v1-only execution proof.

### Maintained widen32 measurement

The maintained `suite/runtime-kernels` kernels were compiled with
`cc -O --opt-tier full --cpu-level v1|v2|v3 --emit-object` before and after
this cover, linked with the same Clang `-O2` driver, and checked for
correctness before timing (cases `widen32`, `positive32`, `adjacent`).

At v2/v3 the `rk_widen32` body fell from 106 instructions / 378 bytes to
78 instructions / 256 bytes. The hot loop per eight inputs fell from 43
instructions / 180 bytes to 15 instructions / 61 bytes: four exact m64
`pmovsxdq` loads replace the four chains, removing all sixteen `movaps` copies.
V1 output and all other maintained kernel functions are byte-identical.

Median nanoseconds per call (before -> after), seven samples per cell, on
x86-64 Linux:

| Length | v1 | v2 | v3 |
| --- | --- | --- | --- |
| 257 | 38.97 -> 39.11 | 39.15 -> 13.40 | 38.91 -> 13.40 |
| 4096 | 624.99 -> 622.63 | 622.24 -> 221.14 | 622.55 -> 221.04 |
| 65536 | 10343.84 -> 10326.95 | 10386.62 -> 3404.84 | 10348.92 -> 3448.92 |
| 1048576 | 158800.00 -> 158816.67 | 158756.67 -> 55738.00 | 159533.67 -> 57761.33 |

The v2/v3 median ratio is 2.76-3.05x for these lengths; v1 and the
byte-identical controls stay within 0.9%. This is a warm repeated-input result
on one host; short calls include call/checksum overhead, and code layout,
boost and load are uncontrolled. It is not an application-wide speedup claim.

## Effective-feature dword multiply retention pilot

The target-private `packed(op, shape, features)` helper recognizes generic
`vimul` on `i32x4` when `features.sse41` is enabled and returns the legacy
`PMULLD` encoding. Legalization retention and emission call the same helper
with the effective feature set:

- Explicit v2/v3 and v1+sse41 select `PMULLD`.
- v1 and v2/v3 with `-sse41` keep lane legalization (identical assembly to
  before).

The low 32 bits of each product implement the IR's modular integer semantics
for both signed and unsigned interpretation. Existing two-address source
protection, scratch registers and spill publication are reused. There are no
load folds or width changes: original 128-bit loads/stores keep their order
and footprints. The AVX FP helper, FP source order, ABI, release legality
enforcement and optimizer eligibility are unchanged, and no wider
multiplication or multiplication-loop recognition is added.

### Tests

`x86_pmulld_selection` checks all three selection tiers, Fast/Full allocation,
normal/empty SIMD pools and six effective feature profiles. It compares text,
typed and chunked objects and checks every emitted instruction's
operand-dependent feature requirements. Controlled allocations cover either
input aliasing the result and independent input/result spills; other cases keep
live sources, repeated inputs and distinct SSA inputs resolving to the same
value. A unit test forces shared owned legalization rounds and verifies the
retained multiply's instruction, result and source location. Clang checks all
256 register-pair `PMULLD` encodings, including extended registers.

```sh
cargo test -p keel-codegen --release --test x86_pmulld_selection \
  -- --include-ignored --nocapture
```

Set `KEEL_PMULLD_EVIDENCE=<dir>` to export ELF+C bundles and run them with
`crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>` on an
SSE4.1-capable Linux x86-64 host. All 108 bundles pass on x86-64 Linux,
covering modular extrema/random products, live sources, aliasing, unaligned
data, exact 128-bit guarded boundaries (including both crossing-page input
faults before any output write) and MXCSR.

### Measurements

The ignored `measure_multiply_codegen` gate reports static counts for a fixed
three-function corpus (ordinary, repeated and resolved-equal inputs, both
sources stored after the result), Full selection without an IR optimizer pass,
at v2/v3 (v1+sse41 matches):

| Configuration | Instructions | `.text` bytes | Frame-memory |
| --- | ---: | ---: | ---: |
| Full alloc, normal pools | 114 -> 60 | 382 -> 192 | 24 -> 6 |
| Fast alloc, normal pools | 117 -> 63 | 384 -> 226 | 24 -> 6 |
| Full alloc, empty SIMD pools | 137 -> 80 | 518 -> 323 | 47 -> 29 |
| Fast alloc, empty SIMD pools | 140 -> 83 | 521 -> 326 | 47 -> 29 |

Totals include prologues, ABI argument staging, original memory operations and
padding; frame-memory counts include legalization buffers and ABI staging.

The ignored `measure_multiply_backend_time` gate times typed-program backend
compilation of the pre-parsed corpus (excluding parsing and assembly). Median
time per corpus was v1 29.2 -> 29.1 us, v2 29.4 -> 10.2 us and v3 28.8 ->
10.1 us. These are backend micro-measurements, not whole-compiler throughput.
The maintained runtime-kernel suite has no multiply kernel, so no runtime
speedup is claimed.

## Related

The [exact 16-byte constant-address pilot](x86-constant-vector-address.md)
lets ordinary nonvolatile `i32x4` loads keep a selected base plus signed disp32
through the AVX2 m128 widening cover.
