# SIMD values in Keel IR

The IR supports six fixed 128-bit shapes: `i8x16`, `i16x8`, `i32x4`, `i64x2`,
`f32x4`, and `f64x2`. In Rust these are `Type::Vector(VectorType::...)`.
`is_int()` and `is_float()` continue to mean scalar types; use `is_vector()`
and `vector()` for vector classification and shape queries. Signedness belongs
to operations. Every vector occupies 16 bytes with natural alignment 16 in an
aggregate layout. Lane zero occupies the lowest-addressed bytes in memory.

These are target-independent IR operations. Parsing, verification, typed builders,
printing, operand rewriting and generic optimization support them. The x86-64 Linux
and AArch64 Apple backends compile every operation below, with packed SSE/SSE2 or
NEON instructions for common cases and shared scalar lane expansion for the rest. IR clients can construct these values explicitly. The C frontend emits scalar
loops that Full optimization can now vectorize, as described below. Explicit C
vector types and intrinsics are not part of this change.

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

Vector loads and stores use the existing memory instructions and flags, including
unaligned accesses. Loads read all 16 bytes, stores write all 16 bytes, and their
trapping/volatile behavior is unchanged. Vectors can also appear as block arguments and function arguments/results.
The backends implement the SysV x86 scalar-vector ABI and Apple arm64 vector ABI.
AArch64 also supports vector fields in byval/sret aggregates; x86 aggregate vector
fields remain rejected until SSEUP classification is implemented. Atomic accesses, `va_arg`, scalar conditions and scalar arithmetic
continue to require their existing scalar types.

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
- `bitcast.<type>` preserves all 128 bits between vector shapes or between a vector
  and `i128`. It does not convert individual lanes. Other vector conversions,
  integer division, saturation, reductions, gather/scatter, scalable vectors,
  256/512-bit types and fused vector FMA are outside this initial set.

Vector computation and lane operations are pure, non-trapping and have no memory
effects. Loads/stores retain their existing effect classification. Generic GVN,
DCE, scheduling, CFG/phi simplification and exact-type memory forwarding work;
scalar constant folding and integer combining do not reinterpret vector lanes.
There is no vector-specific constant folding or arithmetic combining yet.

## x86 lowering and calling convention

Packed lowering uses PADDB/W/D/Q and PSUBB/W/D/Q for wrapping integer addition
and subtraction, PMULLW for i16 multiplication, ANDPS/ORPS/XORPS for bitwise
operations, ADD/SUB/MUL/DIVPS/PD for floating arithmetic and SQRTPS/PD for sqrt.
These 19 additional instruction forms and the existing bitwise forms use the
[Intel SSE/SSE2 encodings](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).
No new AVX feature flag is required for these packed instructions. Scalar lane
fallbacks inherit the existing scalar backend's CPU requirements (for example,
POPCNT and SSE4.1 rounding), rather than adding new feature detection.

Other operations expand into scalar IR before instruction selection and allocation.
Private 16-byte stack buffers snapshot the inputs and assemble the result. Only
needed buffers are allocated, then reused across expansions. Scalar min/max,
comparisons, shifts, unary operations and selection preserve their existing exact
semantics, including noncanonical masks, per-lane shift count masking, NaNs and
signed zero. Lane shuffles, insertion, extraction, splats and all 128-bit bitcasts
are supported this way too. No optimizer runs after legalization, so snapshots
stay ordered. These fallbacks favor coverage and correctness; they can be much
slower than future packed lowering for the same operation.

Vector SSA values occupy one XMM register. Spilled vectors occupy two 8-byte units;
scalar and vector spill pools remain separate so slot widths never change on reuse.
Copies and spills preserve all 128 bits. Copy cycles use a reserved 16-byte stack
temporary in functions with live vectors, including mixed scalar/vector argument
copies. Unaligned memory accesses use MOVUPS. Values live across ordinary calls
spill because SysV provides no callee-saved XMM registers.

Named vector arguments share XMM0–XMM7 with scalar floating arguments. Each vector
uses one XMM register; overflow arguments use a 16-byte stack slot aligned to 16.
Vector results use XMM0. Direct and indirect calls, register-only tail calls, block
argument cycles and passing vectors to external variadic functions are supported.
Variadic vectors are not default-promoted; the caller includes their XMM registers
in AL's register count. Reading a vector with IR `va_arg` remains unsupported by
the verifier. Vector fields inside byval/sret aggregates return a specific SSEUP
ABI diagnostic. Existing stack-argument tail-call restrictions remain unchanged.

Native validation on the i7-1360P Linux NUC includes all six shapes and every
operation under three selection tiers, both allocation tiers and normal/tiny
register pools, with default and AVX/BMI feature settings. It checks unaligned
buffers, integer boundaries, NaNs/signed zero, noncanonical masks and i128 bitcasts.
Clang harnesses exercise direct/indirect calls, register and stack arguments,
returns, variadic calls, tail calls, copy cycles, and vector pressure across calls.
The new encoding matrix compares 6,992 cases plus relocations against Clang and
GNU as. Typed and parsed native object output matches with zero text IR emission.

The builder API exposes `vsplat`, `vector_unary`, `vector_binary`, `vextract`,
`vinsert`, `vshuffle`, `vicmp`, `vfcmp`, and `vselect`. `VectorBinaryOp` is separate
from scalar `BinaryOp` so scalar folds cannot accidentally apply whole-value
integer semantics to independent lanes. The verifier checks both typed API and
text inputs, including mask/result shapes and every lane index.

Validation: `cargo test -p keel-codegen --test vector_x86 --test vector_ir` and
`cargo test -p keel-asm --test x86_packed`. Linux x86-64 executes the runtime
harnesses; other hosts check emission and cross-compile the ABI harness.

## AArch64 NEON lowering and Apple calling convention

Packed integer operations include add/subtract at every lane width, multiply at
8/16/32 bits, bitwise operations, negation, CLZ through 32 bits, byte popcount,
and byte reversal within 16/32/64-bit lanes. Both float shapes use packed
add/subtract/multiply/divide, NaN-propagating min/max, negation, absolute value,
square root and all four rounding operations. These use typed instruction
encodings directly, with no text assembly fallback.

Comparisons, lane selection, shifts, shuffles, extract/insert, bitcasts,
copy-sign, 64-bit multiplication and the remaining integer unary operations use
the same scalar legalization as x86. This is complete operation coverage, not
complete packed-instruction coverage. SVE/SME and scalable vector IR are not part
of these six fixed-width types.

Values occupy one 128-bit Q register or a 16-byte spill slot. Copies, parallel
copy cycles, unaligned accesses and spills preserve all bits. Large unaligned
spill offsets materialize an address when immediate Q addressing cannot encode
them; dynamic stack allocation retains a stable frame base. Vectors live across
calls spill because [AAPCS64 preserves only the low 64 bits of v8–v15](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst).
Scalar floating values can still use those callee-saved registers.

Named vector arguments share v0–v7 with scalar floats; overflow arguments use
16-byte stack slots aligned to 16. Results use v0. Direct/indirect calls,
register-only tail calls, block argument cycles and external variadic calls are
supported. Apple variadic vectors go on the stack with size/alignment 16;
IR vector `va_arg` remains unsupported. Homogeneous vector aggregates of one to
four 128-bit members use one vector register per member, or the stack when the
FP bank has insufficient registers. Lane types do not affect homogeneity.
Larger aggregates use the existing indirect-copy and x8 result-address convention.
These rules follow [AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst)
with [Apple's arm64 ABI changes](https://developer.apple.com/documentation/xcode/writing-arm64-code-for-apple-platforms).

Native Mac tests execute every operation and shape under three selection tiers,
both allocation tiers, and normal/tiny register pools. Clang C harnesses check
unaligned memory, integer boundaries, NaNs/signed zero, noncanonical masks, i128
bitcasts, calls, stack arguments, variadics, return values, register pressure,
and clobbering the upper halves of v8–v15. Aggregate tests cover one through five
members, mixed lane types, FP-bank exhaustion, and dynamic stack allocation with
large unaligned spills. Packed instruction bytes match Clang, and typed versus
parsed Keel Mach-O objects match with zero text IR emission.

Validation: `cargo test -p keel-codegen --test vector_aarch64 --test vector_ir`.
AArch64 macOS executes the runtime harnesses; other hosts check emission and
instruction encodings.

## Automatic loop vectorization

Full optimization (`-O` or `--opt-tier full`) now runs `loop-vectorize` after
scalar cleanup and code motion. The C frontend emits ordinary scalar IR; the
pass discovers vector operations, and codegen chooses their machine encodings.
There is no frontend dependency in the pass. Basic optimization and unoptimized
builds retain scalar loops. Explicit `-p` pipelines include the pass only when
listed, for example `-p simplify-cfg,canonicalize,instcombine,simplify-cfg,gvn,dce,gcm,loop-vectorize`.

The supported subset is increasing-by-one counted loops with a 32- or
64-bit induction variable, an invariant starting index, a separate header and
straight-line body, one store, and contiguous streams of one scalar element type.
Loads, copies, fills, integer
add/subtract/bitwise operations, and floating add/subtract/multiply/divide can be
widened to the matching 128-bit shape. Uniform scalar operands are broadcast once
outside the vector loop. NEON broadcasts use typed `DUP` instructions for all six
shapes. Signed positive bounds and scalar cleanup are supported, without floating
reassociation or a fast-math requirement.

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

Offsets between streams, non-unit strides, reductions, induction values used as
lane data or live-outs, additional loop-carried values, calls, atomics, volatile
memory, mixed element widths, narrowing/widening conversions, conditional bodies,
and general SLP vectorization are not yet supported. Narrow C arithmetic often
includes integer promotions and therefore falls outside this subset. Loops that
do not match are left untouched. Constant loops below the trip threshold are
also left untouched.

Validation: `cargo test -p keel-opt --test loop_vectorize` and
`cargo test -p keel --test vectorize`. The C tests check target-specific IR,
compile both object formats, and execute on native AArch64 macOS or x86-64 Linux
under three codegen configurations. Runtime cases cover overlapping streams in
both directions, exact aliases, disjoint streams, zero/short inputs, signed
negative bounds, tails, and protected page boundaries. The reproducible
`suite/scripts/bench_vectorize.py` compares the same compiler pipeline with and
without the vectorizer, plus Clang, reporting compilation and runtime separately.
Additional cases cover sequential loops after rejected candidates, overlapping
byte/short copies, signed 32-bit indices, and bit-preserving floating copies.
The three-host follow-up results are recorded in
`suite/reports/loop-vectorize-fleet-20261015.json`.
The recognition expansion and its on/off and before/after measurements are in
`suite/reports/loop-vectorize-recognition-20261015.json`. Use
`--pattern range`, `--pattern inverted`, or `--pattern fixed-start` with
`bench_vectorize.py` to measure the newly supported forms; `--pattern zero`
retains the zero-based control. `--start` selects the runtime starting index.

Runtime profiles and diagnostic application probes are recorded in
`suite/reports/app-hot-loops-20261016.json`. They identify helper-call and
aggregate-materialization costs in QuickJS, scalar decimal formatting in SQLite,
and bounded byte scans/short copies as future vectorization investigations.
The probes change retained application source or IR copies; they do not add a
compiler inliner or constant-division pass. See the runtime profiling commands
in `suite/README.md`.
