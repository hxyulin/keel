# SIMD values in Keel IR

The IR supports six fixed 128-bit shapes: `i8x16`, `i16x8`, `i32x4`, `i64x2`,
`f32x4`, and `f64x2`. In Rust these are `Type::Vector(VectorType::...)`.
`is_int()` and `is_float()` continue to mean scalar types; use `is_vector()`
and `vector()` for vector classification and shape queries. Signedness belongs
to operations. Every vector occupies 16 bytes with natural alignment 16 in an
aggregate layout. Lane zero occupies the lowest-addressed bytes in memory.

These are target-independent IR operations. Parsing, verification, typed builders,
printing, operand rewriting and generic optimization support them. **Native
x86-64 and AArch64 lowering and vector calling conventions are not implemented
in this change.** Both native output APIs reject remaining vector IR with an
explicit diagnostic before scalar allocation or ABI classification. The current
x86 AVX assembler subset implements scalar XMM arithmetic, not general packed
SIMD. A packed encoder, vector allocation/spills and ABI lowering are subsequent
backend work. There is no automatic vectorization or SIMD execution claim here.

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
trapping/volatile behavior is unchanged. Vectors can also appear as block arguments,
function arguments/results and in aggregate layouts; this defines IR typing, not a
machine ABI. Atomic accesses, `va_arg`, scalar conditions and scalar arithmetic
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

The builder API exposes `vsplat`, `vector_unary`, `vector_binary`, `vextract`,
`vinsert`, `vshuffle`, `vicmp`, `vfcmp`, and `vselect`. `VectorBinaryOp` is separate
from scalar `BinaryOp` so scalar folds cannot accidentally apply whole-value
integer semantics to independent lanes. The verifier checks both typed API and
text inputs, including mask/result shapes and every lane index.

Validation: `cargo test -p keel-ir -p keel-opt -p keel-codegen --test vector --test vector_ir`.
