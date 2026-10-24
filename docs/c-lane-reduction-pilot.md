# C lane reduction recognition pilot

This pilot adds one target-independent instcombine rule: an exact four-lane
32-bit wrapping sum becomes a `vreduce_add`. It changes no frontend, backend,
public API, pipeline schedule, feature, ABI, allocator, width policy or FP
semantics.

## Trace and exact rule

The `suite/simd-compare/shuffle.c` compound literal becomes private stack
stores and a vector reload, rather than an extract/insert SSA chain, so an
insertion-chain rule would miss it. Widening is deferred too. The `reduce.c`
case, however, already exposes this Full-optimized SSA:

```text
v4 = load.i32x4 v1
v7 = vextract v4, 0
v10 = vextract v4, 1
v11 = iadd v7, v10
v14 = vextract v4, 2
v15 = iadd v11, v14
v18 = vextract v4, 3
v19 = iadd v15, v18
```

The rule replaces only the pure scalar tree with `v19 = vreduce_add v4`.
The input load, output store, private initialization stores and result reload
remain. Existing DCE removes dead pure extracts/adds. Existing lowering uses
packed register shuffles/adds; the result construction still uses the stack.

The rule accepts only a complete `i32x4` source and `i32` scalar `iadd` tree,
with every source lane exactly once and single-use intermediate extracts/adds.
It inspects at most three producer levels below the root (at most fourteen
node visits) with fixed storage and no allocation. It rejects conversions,
mixed vector sources, partial/repeated lanes, other widths, shared
intermediates, scalar operations other than addition and FP. The root can have
multiple consumers. The Fast tier has no single-use proof and retains the tree.
Associativity is valid modulo 2^32 under existing IR semantics; frontend C
signedness, promotions and evaluation have already been resolved. The rule
never inspects memory or calls, invents lanes, duplicates evaluation, or
changes effect order. It adds no new target instruction or legality bypass.

The shared lazy use-count table can require one function-wide operand scan;
this is existing instcombine infrastructure. Existing pass invocation and
worklist caps are unchanged.

## Tests

```sh
cargo test -p keel-opt --test vector_canonicalize
cargo test -p keel --test c_lane_reduce
```

- `vector_canonicalize` runs all 24 lane permutations across all five binary
  tree shapes against a wrapping oracle with seeded inputs. Negative tests cover
  missing/repeated lanes, mixed sources, shared sums and extracts, integer
  promotions/extensions and floating sums. A dedicated test uses aliased vector
  operands, multiple root consumers, signed/unsigned result extension and
  interleaved volatile loads/stores and a call, and checks that effect
  instructions, flags, layout order and source locations are preserved.
- `c_lane_reduce` compiles actual C with unaligned byte-copy inputs, an
  intervening external call, a vector live across that call and a scalar
  result. Before/after native objects check 1,024 input vectors (625 exhaustive
  four-lane combinations of five edge values plus random inputs), exact
  input/output aliases, one-byte misalignment, both guard-page edges, canaries
  and single call evaluation. Normal and empty register pools cover forced
  spills across Baseline/Trees/Full selection. It also checks unsigned modulo
  before zero extension, mixed signed/unsigned operands and signed widening
  before addition; negative C cases (short promotions, widening-before-add,
  dynamic indices, repeated lanes, distinct vector arguments, FP sums) must keep
  their scalar operations.
- The C gate checks text/direct whole-object parity at x86-64 strict v1/v2/v3,
  v3 with AVX/AVX2/F16C/FMA disabled, and baseline Apple arm64.

These are finite value, footprint, feature and ABI checks; they are not a
strict-v1-only CPU execution, exhaustive ISA decoding, or a proof for
arbitrary C programs.

## Measured effect

On x86-64 Linux, with Full optimization and matching Clang
`-O2 -ffp-contract=off` as reference, the `reduce.c` C case improved by about
3% in a warm call/checksum harness (roughly 5.63 to 5.44 ns/call at each strict
level, versus about 0.83 ns/call for Clang). C text size dropped from 120 to
108 bytes at every strict level. This is a harness-level effect, not an
instruction-throughput or application gain, and no compile-time improvement is
claimed.

The remaining roughly 6.5x gap to Clang is substantial. Private result
construction, constant permutation recognition and widening remain open; this
pilot does not address them.
