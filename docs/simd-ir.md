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
[GNU C modes and vectors](gnu-vector-mode.md). General frontend intrinsic
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

The regression `nested_shuffle_selector_boundaries_preserve_native_patterns`
checks two i8x16 ZIP1 shuffles and three SSE2 SHUFPS shuffles (`[0,1,4,5]`,
`[2,3,6,7]`, then `[0,2,4,6]`). General composition previously produced masks
that ARM or x86 scalarized. These chains now retain identical IR and text size,
and packed ZIP1/SHUFPS instructions, before and after cleanup across all three
selection tiers and both allocators. Preserving every nontrivial chain covers
both byte and dword counterexamples without target-specific tables or selector
changes. This is a conservative boundary, not a general profitability model.

Shuffle inspection uses two stack source slots and at most 32 stack lane
indices, allocating an output map only when it rewrites a shuffle. Source
searches inspect at most two entries. Single-use queries reuse the existing
lazy function analysis, rather than rescanning the function for each lane.

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

### Correctness evidence (2026-10-23)

`cargo test -p keel-opt --offline --locked -j2` passes. The dedicated
`vector_canonicalize` integration test checks positive and negative rules,
all 18 shapes, noncanonical nonzero masks, signedness boundaries, effect order,
out-of-layout consumers, Fast cleanup and both instruction-combiner and cleanup
pipeline idempotence. An independent lane oracle checks all 4,096 four-lane
outer shuffle maps over two inner maps.

`cargo test -p keel-codegen --lib --test vector_ir --test vector_widen_aarch64
--test vector_widen_x86 --test vector_mask_proofs --test vector_canonicalize
--test vector_shuffle_window --test vector_x86 --test vector_aarch64
--offline --locked -j2` passes. The new codegen test checks three selection tiers and
two allocation tiers, exact text/direct object equality and independent Clang
assembly on x86 Linux and Apple AArch64. On the native Apple AArch64 host, both
original and optimized objects pass 256 bit-pattern cases per configuration,
including infinities, signed zeros, signaling/quiet NaN payloads and all byte
values for unsigned-then-signed widening. x86 execution was not performed in
this track. This is finite evidence, not exhaustive FP or memory validation.

Round 2 on main through `9265dcf9` passes 575 optimizer tests (two opt-in
tests ignored) and 144 targeted codegen tests. A rebuilt release CLI passes 48
sample/target/selector/allocator configurations: both samples, x86 v1/v2/v3 and
baseline Apple ARM, all three selectors and both allocators, with text/direct
object equality. Twelve ARM configurations execute the native bit-pattern
oracle; x86 output is cross-generated and is not executed. The maintained
`suite/simd-compare/run.py` also completes 224 rows across all four profiles,
with eight passing capability probes and native Apple ARM correctness checks.
This harness run requests no timing measurements. Earlier general-composition
measurements do not describe the final supported scope.

### Revised applicability and compilation-cost evidence

The four-function codegen sample covers lane/bitcast copies, a preserved qword
shuffle chain, a nonzero splat mask and a widening chain. Its 100-copy synthetic
corpus (400 functions) falls from 2,500 to 1,700 IR instructions under
`instcombine,gvn,instcombine,dce`. With the current combined SSE2 selectors, the
sample's text shrinks from 387 to 201 bytes on x86 Baseline, 371 to 201 on x86
Trees/Full, and 248 to 204 on ARM; both allocators give the same sizes. The
counterexample sample stays at 808 bytes on x86 Baseline, 440 on Trees/Full and
148 on ARM. These are compact synthetic output-size observations, not runtime
speedup or general application claims.

The retained external release-mode cost driver parses once, then measures
module cloning, the four-pass cleanup pipeline and destruction. Each process
collects nine samples of 50 complete corpus cleanups, discards the first sample
and reports the median. Parsing, backend compilation and Rust builds are
excluded. Runs are serialized with `suite/fast_opt_locked.py` in
baseline/revised/revised/baseline order. The comparator is the retained
pre-feature `9ffab6bcdb610b74f9ac0fd92bfd6b1a4dd848a0` binary, whose SHA-256
matches the original provenance. This comparison spans other main changes; it
does not isolate the cost of this round's profitability gate.

Median time per cleanup is 0.02018 versus 0.02104 ms for four scalar fixtures
(+4.25%), 11.617 versus 11.698 ms for 34 retained Lua IR files (+0.70%), and
0.3076 versus 0.4095 ms for the synthetic vector corpus (+33.1%). The two
process medians for the vector corpus span 0.3063–0.3090 ms before and
0.3970–0.4221 ms revised. Scalar output counts remain 193 of 198 input
instructions and 51,442 of 81,220 respectively. The Lua captures are diagnostic
application IR, not a versioned application corpus. Host load and the historical
comparator limit causal interpretation; the vector-heavy synthetic corpus
still pays measurable cleanup overhead. No end-to-end compiler-throughput or
runtime benefit is claimed.

Artifacts remain outside Git: `/tmp/keel-canonical-r2-evidence/` contains IR,
assembly, objects, oracles and the 48-configuration CLI report;
`/tmp/keel-canonical-r2-simd/report.json` contains the maintained harness results;
`/tmp/keel-canonical-r2-cost/` contains the driver, corpus hashes, raw timings
and binary provenance. Test/build/measurement commands and lock metadata are
in `/tmp/keel-canonical-r2-unique.jsonl`. Reproduce the retained cleanup cost
probe with `python3 suite/fast_opt_locked.py /tmp/vector-r2-cost.jsonl python3
/tmp/keel-canonical-r2-cost/measure.py`. Set `KEEL_VECTOR_CANONICAL_EVIDENCE`
when rerunning the dedicated codegen test to retain its generated captures.

## x86 lowering and calling convention

Q values use packed SSE/SSE2 operations or exact scalar lane expansion. Explicit
`--cpu-level v1` selects baseline fallbacks without requiring POPCNT/SSE4.1; v2 and v3
are cumulative profiles. The historical default remains the legacy feature
configuration. CPU features, instruction-selection tiers, allocation tiers and
optimization profiles are independent controls.

AVX-enabled 256-bit values use packed primitives, conversions, integer reductions
and lane recipes, with scalar legalization where needed. XMM/YMM share physical
register indices. Spill homes and copies retain exact widths. Functions requiring
32-byte local alignment realign the frame and use a reserved stable local base;
incoming arguments and unwind state retain the original frame reference.
Unaligned Q/Y memory operations use width-appropriate unaligned transfers.
SysV has no callee-saved SIMD registers, so live values across ordinary calls
need preservation. Scalar ABI boundaries receive conservative `vzeroupper`.

Named Q/Y arguments share SIMD0–7 with scalar floating arguments. Q overflow
slots are 16-byte aligned; Y overflow slots are 32-byte aligned. Results use
SIMD0. Unnamed Q variadic values use the SIMD save area when available and then
16-byte aligned overflow storage. Unnamed Y variadic values always use 32-byte
aligned overflow storage. IR vector `va_arg` implements those widths; D values
remain rejected. Direct/indirect calls and supported register-only tails retain
width-aware staging, parallel copies and result transport.

True single-vector Q/Y aggregate wrappers use the SSE/SSEUP register class,
including supported nested single-member layouts. Genuine MEMORY aggregates use
byval/sret transport and layout-derived stack alignment. GNU empty-member Y
layouts that Clang and GCC classify differently reject explicitly. Stack-byval
aggregate tail calls and unnamed compound aggregate varargs retain separate
planner/IR limitations; vector support does not establish arbitrary aggregate ABI
coverage. See the exact supported and rejected layouts in
[x86 coverage](x86-v3-codegen-coverage.html).

Portable FP operations retain their numerical contract, not exact incidental
MXCSR exception effects. Explicit x86 architectural intrinsics separately expose
raw conversions, all 32 AVX compare controls, hardware approximations, raw
source-biased min/max, rounding controls and F16C. These status-observing
operations are pinned and retain effects when their value is unused. Portable
explicit FMA separately guarantees a fused numerical result; its software fallback
and incidental exception flags do not become an architectural status contract. Typed
BMI, runtime permutations, movemasks and flag-result tests have their own
contracts. Gather and sign-masked memory operations preserve inactive-lane fault
suppression; the byte-mask non-temporal store instead requires a fully writable
16-byte destination and emits `SFENCE`. These operations require their declared
features and reject foreign backends. Legacy architectural SSE equivalents are
being staged separately; AVX support does not prove strict-v1/v2 exposure. The
[unmasked FP audit and proof](x86-unmasked-fp.md) records the existing raw
barrier/trap contract, export-only native packet and separate masked-CW x87 gap;
its local checks do not establish native acceptance or fullprofile completion.

The native evidence includes three selection tiers, two allocation tiers,
normal/forced spill pools, direct/text object parity, Clang/GCC ABI interop,
guard pages and numerical/status oracles. Finite corpus results and encoder
inventories do not establish exhaustive architectural correctness or performance.
Current commands, source hashes, failures and limits are retained in
[the stage ledger](isa-stages-20261022.json) and per-family reports.

## AArch64 NEON lowering and Apple calling convention

D/Q values use exact-width 8/16-byte memory, copies and spill homes. Packed
operations cover arithmetic, comparisons, lane selection, widening/narrowing,
reductions and common permutations; shared legalization supplies other portable
recipes. Exact static ZIP/UZP/TRN and window permutations select native forms.
Architectural intrinsics additionally preserve the named ARM instruction's
FPCR/FPSR behavior. Their scalar forms use valid B/H/S/D encodings rather than
reserved vector arrangements. Details and exceptions are listed below.

Named vectors share v0–v7 with scalar floating arguments; overflow arguments
retain each value's size/alignment. Results use v0. Apple variadic D/Q values
use exact 8/16-byte stack slots, and vector `va_arg` aligns and advances them
accordingly. Homogeneous vector aggregates of up to four members use one SIMD
register per member when the bank has capacity; otherwise the aggregate uses
the stack. Other aggregates use the applicable indirect-copy/x8-result ABI.
D/Q callers, mixed scalar/vector varargs and aggregate boundaries are tested
against Clang. Upper SIMD halves are not assumed preserved across ordinary calls.

Native ARM witnesses cover protected memory, exact access footprints, source and
result aliases, status flags, forced spills, all selector/allocation combinations
and both object routes. The baseline 519-record reconciliation distinguishes
principal-operation signatures from helper instructions and semantic recipes.
It does not equate semantic coverage with universal one-instruction lowering.
Optional FP16 arithmetic, crypto, SVE and SME are separate from baseline NEON.
See [the reconciliation](neon-519-reconciliation-20261022.html) for per-record
witnesses and limitations.

## Automatic loop vectorization

Full optimization (`-O` or `--opt-tier full`) now runs `loop-vectorize` after
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
Runtime measurements currently cover a masked u64 sum on AArch64; they do not
establish broad application or native x86 speedups.

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

Integer `i64x2` extraction now selects native `UMOV` on NEON, and `MOVQ` plus
`MOVHLPS` for the upper lane on SSE2, preserving live vector inputs. Other
extraction shapes retain shared lane legalization. No new vector IR operations
or CPU feature requirement is introduced. Structural and native tests cover
every exit position, all byte values and alignments, volatile rejection,
protected-page boundaries, and multiple selector/allocation configurations.

The profiling evidence identifies this scan as an opportunity, not a dominant
application cost. Short inputs pay an additional range check; benchmark long
ASCII runs and early failures separately before attributing application gains
to this transformation.

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
selection retain their original instructions. This is a foundation for mixed-width reductions;
compound narrowing, packed division and the full `k_narrow` expression remain
outside this subset.

## Baseline NEON semantic extensions (2026-10-22)

The implementation and outstanding families are tracked separately from assembler
encoding coverage in `neon-ir-coverage-20261022.json` and its HTML rendering. A
519-record assembler inventory does not establish IR or semantic completeness.

Portable integer binary operations now include `vsmin/vsmax/vumin/vumax`,
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

### Architectural state and explicit intrinsics

`neon_<operation>.RESULT(args...)` is a typed ARM intrinsic. Every operation has
an explicit accepted result type, operand types and arity in
`NeonIntrinsic::operand_types`; x86 rejects these target-specific operations.
Examples include `neon_sqadd.i8x16(a,b)`,
`neon_smlal.i64x2(accumulator,low_i32_vector_a,low_i32_vector_b)`, and
`neon_fcvtzs.i32x4(float_vector)`. Long operands use their low half; narrow
results zero all unused upper lanes. Architecture-defined rounding, doubling,
intermediate saturation, NaN handling and accumulation are those of the named
baseline ARM instruction. FMLA/FMLS take accumulator first, then multiplicands
in written instruction order. They round once; generic multiply/add remains
unfused. Immediate shift and fixed-point forms take a final literal `i32` SSA
constant whose range is verified. Dynamic counts use the separate variable
shift intrinsics, which interpret the signed low byte of each count lane;
portable shifts continue to mask counts modulo lane width.

Explicit intrinsics execute with the current FPCR and update FPSR, including
sticky QC and exception flags, according to the named ARM instruction. They are
pinned barriers, conservatively may trap, and retain status effects when their
value result is unused. `neon_read_fpsr.i64()` and `neon_read_fpcr.i64()` read
state. `neon_swap_fpsr.i64(value)` and `neon_swap_fpcr.i64(value)` return the
previous register value and then write the supplied value, following ARM rules
for reserved register bits.

**Portable floating operations retain their pre-existing numerical contract;
their interaction with FPSR is unspecified.** In particular, mixing `vfadd`,
scalar floating arithmetic, or other portable FP operations with an explicit
FPSR read/swap does not make their intermediate exception flags observable IR
semantics. Their scalar or packed implementation and optimization can change
those incidental flags. Clients requiring architectural FPCR/FPSR behavior must
use explicit architectural intrinsics for the operations they observe. Portable
integer saturation remains independently specified to preserve QC.

Baseline half-storage conversions use `i16x8` to carry IEEE binary16 bit patterns;
this does not add optional FP16 arithmetic types or instructions. Numeric/crypto
extensions, SVE and SME remain outside the baseline inventory.

### D transport and structured memory

The six D shapes have 8-byte size and alignment. ARM ordinary memory,
spills, stack arguments, and homogeneous vector aggregate members transfer
exactly eight bytes. Named arguments and results use the low SIMD register
bits. Packed architectural D forms use explicit Q=0 encodings; single-lane
64-bit architectural forms use the corresponding scalar encoding.

Structured memory operations use the existing `neon_` instruction syntax,
with a vector shape suffix. `neon_ld2`, `neon_ld3`, and `neon_ld4` return
2–4 vectors in structure order; `neon_ld1` returns one. `neon_ld1x2`,
`neon_ld1x3`, and `neon_ld1x4` read contiguous vector lists. Stores take the
address followed by the vectors and have no results. `neon_ldNr` reads one
lane per structure and replicates it. `neon_ldN_lane` takes an address,
N original vectors, and a literal i32 lane index, returning N updated vectors;
`neon_stN_lane` takes the same inputs and returns nothing. N ranges from 1
through 4. All six D and six Q shapes are supported; lane indices are checked
against the shape. Address writeback is an explicit `ptradd` in SSA.

For example:

```text
v2, v3 = neon_ld2.i16x4(v0)
neon_st2.i16x4(v1, v2, v3)
```

These instructions remain pinned memory barriers. They make no alignment or
fault-freedom promise and are not atomic. Full transfers access exactly N
vector widths; lane and replicate transfers access exactly N element widths.
The backend selects native list, interleaved, lane and replicate structure
instructions where their arrangements are legal, retaining exact scalar
legalization for arrangements without a native structure encoding. Native
guard-page tests cover every shape, count and lane, overlaps, normal/forced
spill pools, all pipeline tiers and FPSR preservation. Post-index recipes
perform the pinned memory access before computing the next SSA pointer with
`ptradd`; successful pointer values and early/late memory faults are checked
against raw post-index instructions. Physical exception-context register
contents are outside the exposed SSA semantics.

### Scalar architectural forms

Explicit NEON saturation accepts scalar i8/i16/i32/i64 where the scalar
instruction defines that size. Plain variable/immediate shifts accept i64;
saturating scalar shifts accept all four widths. Doubling multiply-high
accepts i16/i32, doubling long multiply/accumulate i32/i64 results, and
saturating narrowing i8/i16/i32 results with twice-width inputs. Raw scalar
FP operations and conversions use f32/f64 and i32/i64 as appropriate. Scalar
pairwise FP results consume f32x2 or f64x2. One-lane D64 forms use these scalar
encodings rather than reserved Q=0,size=64 vector encodings.

Integer SSA values are transferred into and out of low SIMD lanes without
arithmetic conversion. All explicit operations retain pinned FPCR/FPSR
semantics, including when the numeric result is dead. Scalar `neon_fmla` and
`neon_fmls` use architectural fused instructions with accumulator first and
written multiplicand order. Indexed recipes extract a lane then invoke these
pinned operations. Portable FMA does not supply this architectural-status
contract. `neon_frecpx` accepts f32/f64 and one-lane f64 vectors.

`neon_cls` and `neon_rbit` expose the baseline sign-count and per-byte bit
reversal operations. `neon_mla`, `neon_mls`, `neon_saba`, and `neon_uaba` take
the accumulator first; `neon_sadalp` and `neon_uadalp` consume a same-size
vector of half-width source lanes after the accumulator. Accepted D/Q lane
widths are checked per operation. These architectural intrinsics preserve
FPSR for the integer operations whose instruction definitions leave it alone.

The 519-record reconciliation links each pinned baseline record to a native
execution witness or an executed recipe. That semantic route closure does
not imply direct one-operation lowering of every record. Indexed broadcasts,
constant materialization, explicit high-half placement, table sequences and
post-index `ptradd` remain valid multi-instruction routes. Optional FP16
arithmetic and crypto/SVE/SME extensions remain outside this baseline.

## Effective optimizer target context and signed widening pilot

`keel_ir::VectorCapabilities` is a small immutable value separating maximum
accepted value width, complete signed-i32 sum recipe width, and preferred width.
`keel_opt::OptimizationContext` pairs it with the existing `VectorTarget`.
The additive `run_passes_with_context_and_options` and
`run_pipeline_with_context_and_options` entrypoints have observed counterparts.
Existing target APIs retain their 128-bit policy, and default `Pass` context
methods delegate to the existing target/canonical overrides. A new pass overriding
`run_with_context` and requesting canonical form must explicitly override
`run_canonical_with_context` as well (usually forwarding to `run_with_context`);
the default canonical hook preserves existing `run_canonical` custom overrides.
`OptimizeOptions` and all existing public report structs are unchanged. Analyses
and deterministic-pass version skips remain scoped to one invocation; no target-dependent persistent cache is introduced.

`keel_codegen::x86_features::vector_capabilities(target, selector)` first
validates the target and projects the selector's **effective** features, including
explicit additions/removals. It does not detect the host or infer width from a
CPU level name. AVX allows 256-bit values; AVX2 permits this complete integer
recipe. Every projected target, including v3/AVX2, prefers 128 bits. V3 with
`-avx2` accepts wide values but keeps this recipe at 128 bits. Legacy, strict
v1/v2, Portable, ARM and Fast retain their previous optimization behavior.
AES and PCLMULQDQ do not affect width. ISA support does not establish profitability.
A library caller can explicitly opt into the experiment with
`capabilities.with_wide_preference(true)`. This never grants recipe support;
preferences above accepted value width are conservatively normalized to 128.
The constructor applies the same normalization, and the optimizer still checks
both the complete-recipe capability and the x86 target. Feature fingerprint bits
are metadata and never grant legality. No CLI widening experiment flag exists.

The CLI uses the same projection before optimization, including when emitting
only IR. `opt --target x86_64-unknown-linux-gnu` accepts `--cpu-level` and
`--target-features` as `compile` and `cc` do. Existing flag semantics remain:
repeated feature lists replace earlier modifiers relative to the selected level;
a later level selection resets the feature contract. Invalid dependencies fail
before optimization. Existing report APIs, CLI reports and `to_json()` retain
the exact schema-2 shape with no context metadata. Library callers may explicitly
use `report.to_json_with_context(context)` for a schema-3 envelope containing
`context` and the unchanged schema-2 `report`. This method requires that all
invocations in the report were collected using the supplied context; callers
must use separate reports for different contexts. It records `context.target`,
`context.value_bits`, `context.signed_i32_sum_bits`, `context.preferred_bits` and
a normalized `context.effective_x86_features` fingerprint. The fingerprint
includes every effective enabled/disabled modifier, not raw flag spelling or
order. Equivalent effective contracts produce identical metadata.
Fingerprint bits 0–20 are, in order: aes, avx, bmi1, bmi2,
clflush, pclmulqdq, cx16, lahf_sahf, popcnt, sse3, ssse3, sse41, sse42, avx2,
f16c, fma, lzcnt, movbe, xsave, osxsave, explicit. Non-x86 projections and old
optimizer API contexts use zero. The context and bit order make effective modifiers reproducible.

With explicit library wide preference, the Full `loop-reduce` pilot upgrades
only an already eligible, single-stream
`load.i32 -> sext.i64 -> iadd` wrapping sum. It retains four independent
accumulators, now `i64x4`, with four exact 16-byte `load.i32x4` operations followed
by `swiden_low.i64x4`. One group consumes 16 inputs at byte offsets 0/16/32/48,
advances 64 bytes, and requires at least 16 elements. The existing cross-half
finish, scalar tail, initial accumulator, negative/reversed-range checks and
pointer-overflow fallback remain in place. External arguments/results stay
scalar. Eligibility still rejects volatile/extra faulting reads and side effects;
no source-width arithmetic is reassociated or widened. Unsigned, conditional,
floating, multistream and Fast recipes retain their previous widths. `ALL_128`
is unchanged. This is not general loop widening, a cost model, ABI widening,
implicit FMA, or a vendor-header coverage claim.

### Historical wide-pilot validation and measurement (2026-10-23)

Automatic widening is parked. The measurements below describe the original
candidate (`0804a50b` recipe, originally default-wide at v3), not the revised
128-bit defaults. No runtime gain is claimed and these measurements must not be
reused to describe the new default behavior. There is no new benchmark campaign.

Focused tests are `keel-opt --test vector_context --test loop_vectorize`,
`keel --test vector_context`, and `keel-codegen --test x86_widen_sum`.
Context plumbing also has `keel-codegen --test x86_features` and CLI unit tests.
The native sum gate executes on Linux x86-64 with the required ISA, otherwise
retaining cross-object validation only. It checks text/typed/chunked byte parity
and operand-dependent feature legality at v1/v2/v3/v3-minus-AVX2, all three
selection tiers, both allocators, and normal/empty register pools (48 profiles).
Native guarded-page cases cover every length 0–81, both accessible-page edges,
unaligned pointers, starts 0–5, signed extrema and a nonzero wrapping seed.
Reversed ranges, negative starts/bounds and source-address scaling overflow
exercise scalar fallback. This is finite correctness evidence, not exhaustive
fault certification or execution on a strict-v1-only processor.

Native release validation passed on LinuxHost (Ryzen 9 9950X3D), including the
integrated PMULLD controls, release emission-legality guard, and independent
optional AES/PCLMUL gates. Local clippy completed with repository warnings and
no warning in the new regression files. Measurements used
frozen current-main `6ff8911b` (including reviewed PMOV and PMULLD improvements) and the
candidate, CPU 2 affinity, the shared host lock, one warmup and seven shuffled
paired rounds. The maintained `runtime-kernels` source, driver and independent
Python oracle were reused; all twelve cases passed at thirteen lengths for each
compiler/profile. Runtime samples below are median ns/call, including call and
checksum overhead, for the maintained `widen32` case:

| Length | Main v3 | Wide pilot v3 | Main / pilot |
| --- | ---: | ---: | ---: |
| 8 | 2.06 | 3.07 | 0.670x |
| 17 | 2.37 | 3.30 | 0.718x |
| 257 | 14.26 | 51.95 | 0.274x |
| 4096 | 223.59 | 835.68 | 0.268x |
| 65536 | 3602.33 | 13228.99 | 0.272x |
| 1048576 | 56745.42 | 209195.89 | 0.271x |

**The pilot regresses on this host; wider registers are not evidence of a gain.**
At length 1048576 the seven paired main/pilot ratios range 0.269–0.278x.
V1/v2 `widen32` instruction bytes are identical between compilers; their long
runtime ratios are 1.000x/1.005x, with ordinary sample noise at shorter lengths.
At v3, function text grows 256→454 bytes. The steady loop grows from 15
instructions/8 inputs (61 bytes) to 45/16 inputs (184 bytes); the finish grows
8→18 instructions (31→83 bytes). Neither loop spills vector accumulators. The
baseline finish uses one vector stack store and two scalar reads; the wide
finish uses registers. Both functions have a scalar argument spill/reload outside
the loop. Wide lowering emits numerous scratch-register copies and separate
legacy SSE loads alongside AVX widening/adds. A subsequent independent
control/variant/control probe changed only the four
legacy MOVUPS loads to VMOVUPS: 208.596 → 72.302 → 208.597 microseconds, with
86 native oracle cases passing. This establishes a large encoding-dependent
penalty here, not its exact microarchitectural cause. The 72.302 microsecond
variant remains above the separately measured 56.745 microsecond baseline;
this was not a fresh matched baseline comparison. XMM/YMM views share physical
registers, so global mnemonic replacement
would require an upper-half liveness proof. No backend change or unroll tuning is
included in this context revision. Probe evidence is at
`/tmp/keel-wide-vexload-probe-20261023` in the independent investigation.

Compile time was measured separately as fresh process-to-object time for all
twelve maintained kernels, excluding link/runtime and assembly capture. Median
main→candidate times were 2.735→2.728 ms (v1), 2.727→2.715 ms (v2), and
2.728→3.076 ms (v3), a 12.7% v3 increase. No compile-time or runtime improvement
is claimed. This checkpoint supplies correctness and capability plumbing, but
automatic widening remains parked pending independent review and fresh matched
profitability evidence. The retained library experiment is capability-gated.
Artifacts on LinuxHost are `/tmp/keel-wide-measure-rebased-4188196c/` (compiler hashes, exact
flags, raw rounds, objects, assembly, disassembly and summary),
`/tmp/keel-wide-rebased-measure-lock.jsonl`, and
`/tmp/keel-wide-rebased-native.log`. Supplemental logs are
`/tmp/keel-wide-rebased-final-guard.log` and
`/tmp/keel-wide-rebased-final-optionals.log`. The compiler source snapshot is
`/tmp/keel-wide-candidate-4188196c`. These are historical wide-pilot artifacts;
they do not measure the revised default policy. All generated artifacts stay
outside Git.

### Revised 128-bit defaults: compatibility validation (2026-10-23)

The focused suite and CLI units passed 81 tests locally and 80 on LinuxHost in
release mode (one CLI test is macOS-only). Two benchmark-only PMULLD tests stayed
ignored. The existing 48-profile signed-sum gate now requests wide preference
explicitly; LinuxHost executes its guarded native oracle. Custom legacy target and
canonical overrides, explicit context hooks, invocation-scoped deterministic
skips, ordinary/observed parity, malformed capabilities, exhaustive public
struct literals, schema-2 preservation and explicit schema-3 metadata all have
focused coverage. Normalized feature fingerprints include added AES/PCLMULQDQ
and removed AVX2 independently of preference. PMULLD remains the integrated
SSE4.1 binary signature and its selection/encoding controls pass.

Against frozen `6ff8911b` (all 1,943 baseline source blobs verified), 120
IR/object comparisons and 120 schema-2 report comparisons (timings removed)
match exactly. The finite matrix uses the signed-sum IR, conditional-reduction
IR and maintained twelve-kernel C source; Fast/Full; legacy/v1/v2/v3,
v3-minus-AVX2, v1-plus-AVX/AVX2, independent crypto and repeated modifier
replacement; and ARM ELF/Mach-O. Main's `opt` predates feature flags, so its IR
fixture comparisons use the original target-only 128-bit API; `cc` and object
comparisons use the same effective feature flags on both revisions.

Evidence is `/tmp/keel-wide-followup-parity-20261023/summary.json`, with exact
comparison commands in `/tmp/keel-wide-followup-parity.py`. Serialized build/test
commands are recorded in `/tmp/keel-wide-followup-unique.jsonl` on each host.
LinuxHost logs are `/tmp/keel-wide-followup-native.log`,
`/tmp/keel-wide-followup-native-context.log` and
`/tmp/keel-wide-followup-native-cli.log` (also copied locally). These tests are
finite correctness and compatibility evidence; no new runtime or compile-time
performance measurement was made. Backend load/encoding changes are separate
work and are not part of this revision. Independent parent review is required
before integration.
