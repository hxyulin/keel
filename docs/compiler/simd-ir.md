# SIMD values in Keel IR

The IR has six lane families at each fixed width: integer lanes of 8, 16, 32,
and 64 bits, and floating lanes of 32 and 64 bits. The 64-bit D shapes are
`i8x8/i16x4/i32x2/i64x1/f32x2/f64x1`; the 128-bit Q shapes are
`i8x16/i16x8/i32x4/i64x2/f32x4/f64x2`; the 256-bit shapes double Q's lane counts.
Each value has its width's exact size and natural alignment: 8, 16, or 32 bytes.
Lane zero occupies the lowest-addressed bytes. `Type::Vector(VectorType::...)`
represents these values; scalar `is_int()`/`is_float()` remain scalar predicates.
Use `VectorType::ALL_64`, `ALL_128`, or `ALL_256` for a particular width;
`ALL` includes all widths. Signedness belongs to operations.

Target support is explicit: Apple AArch64 supports D/Q and rejects 256-bit
values; x86-64 supports Q and AVX-enabled 256-bit values, and rejects D values.
A valid target-independent type does not guarantee target ABI support.
Parsing, verification, builders, printing and generic rewriting support the
shapes. Codegen combines packed instructions with scalar lane legalization.
The C frontend's bounded `vector_size(16)` surface is documented separately in
[GNU C modes and vectors](../frontends/c/extensions.md). General frontend intrinsic
coverage is separate from explicit IR construction.

```text
func @add_lane(ptr, i32) -> i32 {
block0(v0: ptr, v1: i32):
    v2 = load.i32x4 align 1 v0
    v3 = vsplat.i32x4 v1
    v4 = viadd v2, v3
    store align 1 v4, v0
    v5 = vextract v4, 2
    return v5
}
```

Vector loads and stores access exactly the value's 8, 16, or 32 bytes using the
existing memory flags. Unaligned accesses retain their existing trapping and
volatile behavior. Vectors may be block arguments and function arguments/results
within the target's supported ABI. Scalar conditions and scalar arithmetic keep
their scalar contracts; vector atomics are not introduced. Supported vector
`va_arg` and aggregate rules are described by target below.

## Operations

- `vsplat.i32x4 scalar` broadcasts a scalar whose type matches the lane type.
  Constants can be built using scalar constants, splats and lane insertion.
- `vextract vector, index` returns a scalar lane. `vinsert vector, scalar, index`
  returns a new vector with that lane replaced. Indices are literal integers
  checked against the shape; they never wrap.
- `vshuffle a, b, [indices]` concatenates equal-shaped vectors and selects one
  source lane for each output lane. An index below N selects from `a`, otherwise
  from `b` at index-N. The index list has exactly N entries, each below 2N.
  Repeated indices are allowed. There are no undef or poison lanes.
- `viadd`, `visub`, `vimul` operate independently on equal-shaped integer vectors,
  wrapping modulo each lane's width. `vand`, `vor`, `vxor` operate on integer lanes.
- `vshl`, `vushr`, `vsshr` take equal-shaped integer vectors. Each count lane is
  reduced modulo the lane bit width. Logical right shift zero-fills; arithmetic
  right shift sign-extends each lane.
- `vfadd`, `vfsub`, `vfmul`, `vfdiv`, `vfmin`, `vfmax`, `vfcopysign` take equal-shaped
  float vectors with the corresponding scalar operation's semantics. In particular,
  min/max propagate NaNs and order -0 below +0; they are not x86 MINPS/MAXPS semantics.
- Unary operations use scalar `UnaryOp` through `VectorUnary`: `vineg`, `vbnot`,
  `vclz`, `vctz`, `vpopcnt`, `vbswap` on integer vectors, and `vfneg`, `vfabs`,
  `vfsqrt`, `vfceil`, `vffloor`, `vftrunc`, `vfnearest` on float vectors. Semantics
  are independently those of each scalar lane, including width for zero CLZ/CTZ.
  `vbswap` needs lanes at least 16 bits wide.
- `vicmp.<IntCC>` and `vfcmp.<FloatCC>` compare equal-shaped integer or float
  vectors. Conditions are the existing scalar conditions. Each true result lane
  contains all one bits, each false lane zero. Float masks have integer lanes of
  the same width: `f32x4` comparisons give `i32x4`, `f64x2` give `i64x2`.
- `vselect mask, a, b` selects an entire lane from `a` when its mask lane is
  nonzero, otherwise from `b`. The arms have equal vector shapes; the mask has
  integer lanes of the same width and count. Any nonzero mask works. This is lane
  selection, not arbitrary bit blending. Scalar `select` may select a whole vector.
- `bitcast.<type>` preserves bits between equal-size vector/scalar types; it
  does not convert individual lanes. Widening, narrowing and pairwise conversion
  operations are described below. Generic packed integer division, scalable and
  512-bit vectors remain outside this set. Typed AVX2 gathers have a separate
  target-specific memory contract; ordinary loads are not masked gathers.

Vector computation and lane operations are pure, non-trapping and have no memory
effects. Loads/stores retain their existing effect classification. Generic GVN,
DCE, scheduling, CFG/phi simplification and exact-type memory forwarding work;
scalar constant folding does not reinterpret vector lanes. Bounded vector
transport and integer bitwise identities use the existing instruction combiner,
as described below; general vector arithmetic constant folding is separate.

## Bounded vector canonicalization

`instcombine` removes extracts of splats and inserted lanes, redirects extracts
through shuffles and unaffected inserted lanes, removes insertion of the same
extracted lane or splatted scalar, and bypasses immediately overwritten inserts.
A narrow C-intent pilot recognizes complete four-lane `i32x4` wrapping sums:
exact `i32` extract/add trees become the existing `vreduce_add` only when every
lane appears once and every intermediate extract/add is unshared. Inspection
has a fixed depth of three below the root and rejects conversions, mixed
sources, other widths, partial/repeated lanes and FP arithmetic. Fast keeps
the scalar tree because it has no use proof.

Identity shuffles become their selected input. Shuffles selecting only one
splatted scalar become a splat. Unnested equal-input and other single-source
shuffles use `vshuffle x, x` with indices in the first input's lane range.

Nested shuffle inspection is bounded to one producer level per worklist visit,
unshared intermediate shuffles and at most two original source vectors. It only
replaces a chain when the composed output is an existing original source:
identity/inverse permutations disappear without creating a new permutation.
Written identity shuffles are removed before inspecting their producers. If the
composed output still requires any permutation, the original chain is preserved.
General composition is deferred until target cost feedback can justify it.
Uncomposed genuine two-source shuffles retain their written operand order.

An integer splat mask selects the appropriate entire arm using the lane-width
constant's zero/nonzero value; `1` is true, and `256` in an i8 lane is false.
Integer AND/OR idempotence and zero/all-ones bitwise identities are simplified.
Arbitrary masks are never reinterpreted as all-ones comparison masks. Vector
bitcast chains collapse without converting lane values, preserving signed zeros
and NaN payloads. Low-lane widening chains compose signed/signed,
unsigned/unsigned and unsigned-then-signed extensions. Signed-then-unsigned,
high-lane, narrowing, saturation and pairwise conversions retain their original
semantics and are not canceled by these rules.

Existing GVN shares dominating duplicate splats and vector computations; aliases,
bitcasts and scalar selects provide the existing vector copy surface. DCE removes
dead pure producers. These rules do not create, move, narrow or widen memory
operations, add fault-freedom promises, touch architectural intrinsics or
reassociate floating arithmetic. Existing potentially trapping, volatile and
atomic effects remain pinned, including loads whose result becomes unused.
The bounded Fast local pass shares local rules but does not compose shuffle
producers because it has no single-use information. Loop discovery and loop
vectorization are unchanged.

### Tests

```sh
cargo test -p keel-opt --test vector_canonicalize
cargo test -p keel-codegen --lib --test vector_ir --test vector_widen_aarch64 \
  --test vector_widen_x86 --test vector_mask_proofs --test vector_canonicalize \
  --test vector_shuffle_window --test vector_x86 --test vector_aarch64
```

## Target lowering

The [x86-64 code generation](../targets/x86_64/codegen.md#vector-lowering-and-calling-convention)
and [AArch64](../targets/aarch64/README.md#neon-lowering-and-calling-convention)
pages describe how each backend lowers these values and passes them in calls.
Portable FP operations keep their numerical contract, not exact incidental
MXCSR or FPSR exception effects; explicit architectural intrinsics expose
status-observing operations separately.

## Automatic loop vectorization

Full optimization (`-O` or `--opt-tier full`) runs `loop-vectorize` after
scalar cleanup and code motion. The C frontend emits ordinary scalar IR; the
pass discovers vector operations, and codegen chooses their machine encodings.
There is no frontend dependency in the pass. Basic optimization and unoptimized
builds retain scalar loops. Explicit `-p` pipelines include the pass only when
listed, for example `-p simplify-cfg,canonicalize,instcombine,simplify-cfg,gvn,dce,gcm,loop-vectorize`.

The counted-map subset is increasing-by-one loops with a 32- or
64-bit induction variable, an invariant starting index, a separate header and
straight-line body, one store, and contiguous streams of one scalar element type.
Loads, copies, fills, integer
add/subtract/bitwise operations, and floating add/subtract/multiply/divide can be
widened to the matching 128-bit shape. Uniform scalar operands are broadcast once
outside the vector loop. NEON broadcasts use typed `DUP` instructions for all six
shapes. Signed positive bounds and scalar cleanup are supported, without floating
reassociation or a fast-math requirement.

The full `-O` pipeline includes `loop-reduce`, which lets read-only i32/i64 sum reductions use independent vector accumulators, provided
the only extra loop-carried value is the wrapping sum. The initial scalar
accumulator is added once after reducing the vector lanes; the original scalar
loop handles the tail. The same bounded lane-wise integer expression and
contiguous-stream analysis applies. Hidden reads, writes, calls, extra
recurrences and volatile accesses reject the candidate. Floating-point sums
are not reassociated. At most one read may potentially fault; additional reads
need explicit `notrap` promises so widening cannot change first-fault order.
Negative signed starts, short/empty ranges and unsafe
address arithmetic keep the scalar path. No new IR opcode or frontend memory
promise is needed. Cheap trip-count checks reject short ranges before pointer-range
checks. Simple expressions use four independent accumulators, larger expressions
or stream sets use two. Fixed-offset loads share a pointer advanced once per group;
a countdown controls the vector loop. The original scalar tail handles the remainder.
Fast includes `fast-loop-reduce`, with the same transformation but an eligibility cap
of 256 instructions, 16 blocks, one stream address and eight expression values.
These are compile-effort policies, independent of selection and allocation tiers.

Starts may be constant or runtime values, for example `for (i = lo; i < n; i++)`.
Either branch orientation is recognized, including `if (i >= n) break`, as are
comparisons with swapped operands and commuted element-size multiplication.
Dynamic signed starts below zero retain the scalar path; known negative starts
are left unchanged. Empty ranges and subtraction wraparound cannot enter the
vector path. The trip threshold uses `n - lo`, and the vector tail starts at
`lo + floor((n - lo) / lanes) * lanes`, without rounding the absolute index.

A runtime guard requires at least four vector groups, checks byte-range overflow,
and permits each input to be either identical to the output or disjoint from it.
Partial overlap runs the unchanged scalar loop, preserving dependencies such as
`out == input + 1`. Reads may overlap one another freely. The vector loop covers
only complete groups; its final index enters the original scalar loop for the
tail. Neither path widens access beyond the original iteration range. Accesses
remain unaligned-safe and volatile operations are excluded.
Alias and byte-overflow guards conservatively check the full base-to-bound
range even for nonzero starts; disjoint accessed subranges can therefore still
fall back to scalar execution.

The first cost gate uses packed operation availability, fixed register pressure
limits (24 expression values, four streams), and a trip-count threshold. It is
not a full target cost model. `VectorTarget::{Portable,Aarch64Neon,X86Sse2}`
configures `run_passes_for_target` or `run_pipeline_for_target`; existing optimizer
APIs default to the portable intersection. The driver supplies the selected target
for `cc`/`compile`, including explicit pipelines. `keel opt --target <triple>`
selects that target's gate; without a target, `opt` uses portable capabilities.
For example, i32 multiplication vectorizes for NEON but remains scalar for the
current SSE2 lowering; i64 multiplication remains scalar on both.

Loop discovery computes control-flow and dominance analysis lazily and reuses it
across rejected candidates. A successful transformation invalidates both analyses
before examining another loop, since it adds blocks and redirects edges.

For counted maps, offsets between streams, non-unit strides, reductions, induction values used as
lane data or live-outs, additional loop-carried values, calls, atomics, volatile
memory, mixed element widths, narrowing/widening conversions, conditional bodies,
and general SLP vectorization are not supported. Narrow C arithmetic often
includes integer promotions and therefore falls outside this subset. Loops that
do not match are left untouched. Constant loops below the trip threshold are
also left untouched.

Validation: `cargo test -p keel-opt --test loop_vectorize` and
`cargo test -p keel --test vectorize`. The C tests check target-specific IR,
compile both object formats, and execute on native AArch64 macOS or x86-64 Linux,
including overlapping and aliased streams, short inputs, negative bounds,
tails and protected page boundaries.

### Bounded ASCII prefix scans

`loop-vectorize` also recognizes the pointer loop `while (p < end && *p < 128)
p++`, as emitted by the C frontend for QuickJS's `JS_NewStringLen`. The bound
must be invariant, the byte load ordinary and nonvolatile, and the character
test and pointer increment must have no other effects. The induction pointer
may be used after the loop. Swapped bound operands, inverted branches, and
unsigned-byte or zero-extended-byte comparisons are recognized; arbitrary
character predicates and unbounded sentinel scans are not.

An initial scalar prefix advances to sixteen-byte alignment, stopping at either
the bound or the first non-ASCII byte. Each chunk then checks pointer ordering
and at least sixteen remaining bytes before issuing an `i64x2` load. Alignment
keeps each load within a protection granule on the supported targets, including
when the scalar scan would stop before a protected page but its bound lies
beyond that page. Extracting and ORing its two lanes,
followed by a repeated-byte high-bit mask, detects whether all sixteen bytes
are ASCII. Entirely ASCII chunks advance by sixteen. A failing chunk or a short
tail enters the original scalar header at the current pointer, so the scalar
loop determines the exact first non-ASCII position and preserves exit values.
No chunk reads before the current pointer or at/after the end bound. Empty and
reversed ranges take the original scalar path without dereferencing memory.

Integer `i64x2` extraction selects `UMOV` on NEON, and `MOVQ` plus `MOVHLPS`
for the upper lane on SSE2. Short inputs pay an additional range check.

## Packed integer predicates and lane selection

NEON emits native comparisons for every integer shape and predicate. Baseline
SSE2 emits packed equality/inequality and signed/unsigned ordering for all integer shapes.
i64 equality combines both dword halves; signed ordering compares signed high
dwords and breaks ties with unsigned low dwords. Unsigned ordering biases every
lane's sign bit; unsigned i64 ordering biases both dword halves for the same
lexicographic recipe. No newer CPU feature is assumed.

Both backends lower lane selection with packed operations. Each mask lane is
normalized by comparison with zero: a nonzero mask selects the entire true lane,
including masks with only one bit set. Float arms are selected bitwise, preserving
NaN payloads and signed zero. Sources are captured before publishing the result,
including spilled operands and results; allocated inputs may remain live. SSE2
skips normalization when the resolved mask is defined directly by integer vector
comparison, whose lanes are proven all-ones or zero. Loads, block parameters and
bitcasts keep the general nonzero-mask path.

Full's integer reduction discovery also accepts same-width integer comparisons
and selects in independent wrapping i32/i64 contributions. Predicates must use
packed operations on the selected target: NEON admits all integer predicates;
SSE2 admits signed/unsigned i32/i64 ordering and i32/i64 equality. Fast keeps its previous
eligibility rules. Floating comparisons/reassociation, coupled recurrences,
induction-as-data and multiple potentially faulting streams remain excluded.

A bounded trial (256 instructions, 16 blocks) handles C ternary diamonds. Existing
successful-load reuse removes a duplicate dominated read before the guarded
if-converter runs. Empty edge blocks adapt triangle CFGs to that converter;
only nontrapping arm operations may be speculated. The trial replaces the
original function only when a reduction actually vectorizes, so rejected
candidates retain their original CFG. Scalar tails, zero-trip behavior, access
widths, memory effects and first-fault restrictions remain unchanged.

For a comparison-controlled contribution with a zero false arm, reduction
emission uses `vand` directly with the canonical comparison mask. This is local
to predicates proven by the reduction matcher; general `vselect` still accepts
arbitrary nonzero lane masks and normalizes them unless SSA proves a canonical
integer comparison mask.

## Low-lane integer widening

`swiden_low.DEST arg` and `uwiden_low.DEST arg` preserve a 128-bit vector while
extending its lowest destination-lane-count integer lanes. The destination has
wider integer lanes; all higher source lanes are ignored. For example,
`swiden_low.i64x2` on i16x8 observes only its first two lanes and sign-extends
each to i64. These operations do not saturate or move arithmetic across an
extension. NEON emits `sshll`/`ushll` chains. Baseline SSE2 uses low unpack
chains with zero lanes for unsigned widening and signed comparison masks for
signed widening. Both backends keep vector-to-vector bitcasts in registers,
preserving every bit, including floating NaN payloads and signed zero. The
diagnostic Cranelift bridge extracts/extends/inserts the same low lanes.

Full ARM and SSE2 reduction discovery can extend a contiguous i8/i16/i32 load directly
into an i32/i64 accumulator expression. For i32-to-i64 reductions it also keeps
source-width wrapping arithmetic and selects before explicit extension, and
widens canonical i32 predicates for i64 selects. Bounded nested ternaries can
therefore vectorize without changing narrow arithmetic or predicate semantics.
Its grouped read is exactly the number
of output lanes times the source element width, never an unconditional 16-byte
read. It uses a packed scalar read, splat and bitcast before widening. All
streams share one element width; original first-fault, address overflow, scalar
tail and hidden-effect restrictions remain. Fast and Portable widening
eligibility stay unchanged. ARM additionally covers an adjacent, ordinary,
single-use scalar load and splat with an exact-width `ld1r`, and a one-step
low-lane widening followed by a single-use adjacent packed add with
`saddw`/`uaddw`. These covers preserve custom allocation constraints and
separate source lines; shared values, volatile reads, and unsupported address
selection keep their original instructions. Compound narrowing and packed
division are outside this subset.

## Baseline NEON semantic extensions

Portable integer binary operations include `vsmin/vsmax/vumin/vumax`,
`vsabd/vuabd`, signed/unsigned halving add/subtract (`vshadd/vuhadd/vshsub/vuhsub`),
rounding halving add (`vsrhadd/vurhadd`), and saturating add/subtract
(`vsadd_sat/vuadd_sat/vssub_sat/vusub_sat`). ABD returns the low W bits of the exact
mathematical magnitude. Halving computes floor((a +/- b)/2) with an extended
intermediate; rounding halving add computes floor((a+b+1)/2). Saturation clamps
the exact mathematical result to the named signed or unsigned W-bit range.
Portable clamps do **not** update FPSR.QC.

`vpadd/vpsmin/vpsmax/vpumin/vpumax` and `vfpadd/vfpmin/vfpmax` pair adjacent lanes
of the concatenation of both input vectors. Output lane i applies the named
binary operation to concatenated lanes 2i and 2i+1. Floating operand order,
NaN propagation and signed-zero behavior follow the corresponding scalar
operation; these operations do not authorize further reassociation.

`vreduce_add/and/or/xor/smin/smax/umin/umax` reduce all integer input lanes to a
scalar of the lane type. Addition wraps at that width. No floating reassociation
is implied. The loop vectorizer uses `vreduce_add` for integer sums.

`swiden_high/uwiden_high` extend the highest destination-lane-count source lanes,
including multi-step width changes. `narrow_low.DEST` truncates every input lane
into the lowest output lanes and zero-fills every remaining lane. The pure
`snarrow_sat_low/unarrow_sat_low/sunarrow_sat_low` forms first clamp to signed,
unsigned, or signed-source-to-unsigned destination range respectively; they do
not update QC. `spairwise_add_long/upairwise_add_long` require exactly doubled
lane width and add each adjacent input pair in that wider output lane. High
placement of a narrow result and high-half long arithmetic can be expressed
with checked `vshuffle` operations.

ARM architectural intrinsics, D-shape transport and structured memory
operations are described with the [AArch64 backend](../targets/aarch64/README.md#neon-intrinsics).

## Optimizer target context

`keel_ir::VectorCapabilities` separates the maximum accepted value width, the
complete signed-i32 sum recipe width and the preferred width.
`keel_opt::OptimizationContext` pairs it with a `VectorTarget`, and
`run_passes_with_context_and_options` and `run_pipeline_with_context_and_options`
accept it. Existing target APIs keep a 128-bit policy. A pass that overrides
`run_with_context` and requires canonical form must also override
`run_canonical_with_context`.

`keel_codegen::x86_features::vector_capabilities(target, selector)` projects
the selector's effective features, including explicit additions and removals,
without detecting the host. AVX accepts 256-bit values and AVX2 permits the
complete integer recipe, but every target prefers 128 bits. A library caller
can opt into wider preference with `capabilities.with_wide_preference(true)`;
this never grants recipe support, and preferences above the accepted width are
normalized to 128. The only wide recipe is a Full `loop-reduce` variant for a
single-stream `load.i32 -> sext.i64 -> iadd` sum using `i64x4` accumulators.
No CLI option enables it.

The CLI applies the same projection before optimization, including for IR
output. `opt --target x86_64-unknown-linux-gnu` accepts `--cpu-level` and
`--target-features` like `compile` and `cc`. Reports keep the schema-2 JSON;
`report.to_json_with_context(context)` writes a schema-3 envelope with the
context and an effective-feature fingerprint, for reports collected under that
one context.

## Conditional multi-reduction normalization

Full's `loop-reduce-multi` tries conditional integer recurrences in a private
copy, limited to 256 instructions and 16 blocks. The existing guarded diamond
converter speculates only its nontrapping scalar allowlist. A wrapping update
`select(p, a+x, a)` becomes `a + select(p, x, 0)` in the same integer width.
Independent sums retain the existing vector expression, stream and fault-order
proofs. Coupled sums, writes, volatile accesses and unsupported expressions fail.

An unsigned, strict-less-than loop with a unit induction step can fold an i32 or
i64 unit counter observed only after the loop. Its final value is the original
counter plus `max(bound-start, 0)`, reduced to the counter's width. Nonzero seeds,
zero trips, nonzero starts and narrow wrapping counts retain their semantics.
The trial is published only after vectorization succeeds. This does not enable
general if-conversion in the default pipeline.
