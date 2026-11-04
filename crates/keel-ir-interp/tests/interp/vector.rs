//! Vector lane operations, masks, reductions and conversions.

use super::{binary_fn, eval, unary_fn};
use keel_ir::VectorType::{self, F32x4, F64x2, I8x16, I16x8, I32x4, I64x2};
use keel_ir_interp::Val;

fn v(ty: VectorType, lanes: &[u64]) -> Val {
    Val::vector(ty, lanes)
}

fn neg(x: i64) -> u64 {
    x as u64
}

fn f32s(lanes: [f32; 4]) -> Val {
    v(F32x4, &lanes.map(|x| u64::from(x.to_bits())))
}

fn bin(op: &str, a: Val, b: Val) -> Val {
    let ty = a.ty().to_string();
    eval(&binary_fn(op, &ty, &ty, &ty), "f", &[a, b])
}

fn i32x4(lanes: [i64; 4]) -> Val {
    v(I32x4, &lanes.map(neg))
}

#[test]
fn lane_arithmetic_wraps_per_lane() {
    let a = i32x4([1, -1, i32::MAX as i64, 7]);
    let b = i32x4([2, 1, 1, -7]);
    assert_eq!(
        bin("viadd", a.clone(), b.clone()),
        i32x4([3, 0, i32::MIN as i64, 0])
    );
    assert_eq!(
        bin("visub", a.clone(), b.clone()),
        i32x4([-1, -2, i32::MAX as i64 - 1, 14])
    );
    assert_eq!(
        bin("vimul", a.clone(), b.clone()),
        i32x4([2, -1, i32::MAX as i64, -49])
    );
    assert_eq!(bin("vand", a.clone(), b.clone()), i32x4([0, 1, 1, 1]));
    assert_eq!(
        bin("vor", a.clone(), b.clone()),
        i32x4([3, -1, i32::MAX as i64, -1])
    );
    assert_eq!(bin("vxor", a, b), i32x4([3, -2, i32::MAX as i64 - 1, -2]));
}

#[test]
fn lane_shifts_reduce_each_count() {
    let a = i32x4([1, -8, -8, 1]);
    let counts = i32x4([33, 1, 1, 31]);
    assert_eq!(
        bin("vshl", a.clone(), counts.clone()),
        i32x4([2, -16, -16, i32::MIN as i64])
    );
    assert_eq!(
        bin("vushr", a.clone(), counts.clone()),
        i32x4([0, 0x7fff_fffc, 0x7fff_fffc, 0])
    );
    assert_eq!(bin("vsshr", a, counts), i32x4([0, -4, -4, 0]));
}

#[test]
fn min_max_difference_halving_and_saturation() {
    let a = v(
        I8x16,
        &[neg(-128), 127, 200, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let b = v(I8x16, &[1, 1, 100, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let lanes = |op: &str| bin(op, a.clone(), b.clone()).lanes().unwrap()[..4].to_vec();
    assert_eq!(lanes("vsmin"), [0x80, 1, 200, 1]);
    assert_eq!(lanes("vumin"), [1, 1, 100, 1]);
    assert_eq!(lanes("vsmax"), [1, 127, 100, 2]);
    assert_eq!(lanes("vumax"), [0x80, 127, 200, 2]);
    assert_eq!(lanes("vsabd"), [129, 126, 156, 1]);
    assert_eq!(lanes("vuabd"), [127, 126, 100, 1]);
    assert_eq!(lanes("vshadd"), [neg(-64) & 0xff, 64, 22, 1]);
    assert_eq!(lanes("vuhadd"), [64, 64, 150, 1]);
    assert_eq!(lanes("vsrhadd"), [neg(-63) & 0xff, 64, 22, 2]);
    assert_eq!(lanes("vurhadd"), [65, 64, 150, 2]);
    assert_eq!(
        lanes("vshsub"),
        [neg(-65) & 0xff, 63, neg(-78) & 0xff, neg(-1) & 0xff]
    );
    assert_eq!(lanes("vuhsub"), [63, 63, 50, 0xff]);
    assert_eq!(lanes("vsadd_sat"), [neg(-127) & 0xff, 127, 44, 3]);
    assert_eq!(lanes("vuadd_sat"), [129, 128, 255, 3]);
    assert_eq!(lanes("vssub_sat"), [0x80, 126, 0x80, 0xff]);
    assert_eq!(lanes("vusub_sat"), [127, 126, 100, 0]);
}

#[test]
fn pairwise_operations_pair_the_concatenation() {
    let a = i32x4([1, 2, 3, 4]);
    let b = i32x4([10, -20, 30, 40]);
    assert_eq!(bin("vpadd", a.clone(), b.clone()), i32x4([3, 7, -10, 70]));
    assert_eq!(bin("vpsmin", a.clone(), b.clone()), i32x4([1, 3, -20, 30]));
    assert_eq!(
        bin("vpumax", a, b),
        i32x4([2, 4, neg(-20) as i64 & 0xffff_ffff, 40])
    );
    let x = f32s([1.0, 2.0, f32::NAN, 0.0]);
    let y = f32s([-0.0, 0.0, 5.0, 6.0]);
    assert_eq!(
        bin("vfpadd", x.clone(), y.clone()),
        f32s([3.0, f32::NAN, 0.0, 11.0])
    );
    assert_eq!(bin("vfpmin", x, y), f32s([1.0, f32::NAN, -0.0, 5.0]));
}

#[test]
fn float_lanes_follow_scalar_rules() {
    let a = f32s([1.0, f32::NAN, -0.0, 9.0]);
    let b = f32s([2.0, 1.0, 0.0, -3.0]);
    assert_eq!(
        bin("vfadd", a.clone(), b.clone()),
        f32s([3.0, f32::NAN, 0.0, 6.0])
    );
    assert_eq!(
        bin("vfmin", a.clone(), b.clone()),
        f32s([1.0, f32::NAN, -0.0, -3.0])
    );
    assert_eq!(
        bin("vfmax", a.clone(), b.clone()),
        f32s([2.0, f32::NAN, 0.0, 9.0])
    );
    assert_eq!(
        bin("vfdiv", a.clone(), b.clone()),
        f32s([0.5, f32::NAN, f32::NAN, -3.0])
    );
    assert_eq!(bin("vfcopysign", b, a), f32s([2.0, 1.0, -0.0, 3.0]));
}

#[test]
fn unary_lanes() {
    let un = |op: &str, a: Val| {
        let ty = a.ty().to_string();
        eval(&unary_fn(op, &ty, &ty), "f", &[a])
    };
    let a = v(I16x8, &[0, 1, 0x8000, 0x1234, 0xffff, 3, 0, 0]);
    let lanes = |op: &str| un(op, a.clone()).lanes().unwrap()[..6].to_vec();
    assert_eq!(lanes("vclz"), [16, 15, 0, 3, 0, 14]);
    assert_eq!(lanes("vctz"), [16, 0, 15, 2, 0, 0]);
    assert_eq!(lanes("vpopcnt"), [0, 1, 1, 5, 16, 2]);
    assert_eq!(lanes("vbswap"), [0, 0x100, 0x80, 0x3412, 0xffff, 0x300]);
    assert_eq!(lanes("vineg"), [0, 0xffff, 0x8000, 0xedcc, 1, 0xfffd]);
    assert_eq!(lanes("vbnot"), [0xffff, 0xfffe, 0x7fff, 0xedcb, 0, 0xfffc]);
    let f = f32s([-1.5, 2.5, 4.0, -0.0]);
    assert_eq!(un("vfabs", f.clone()), f32s([1.5, 2.5, 4.0, 0.0]));
    assert_eq!(un("vfneg", f.clone()), f32s([1.5, -2.5, -4.0, 0.0]));
    assert_eq!(un("vfnearest", f.clone()), f32s([-2.0, 2.0, 4.0, -0.0]));
    assert_eq!(un("vffloor", f.clone()), f32s([-2.0, 2.0, 4.0, -0.0]));
    assert_eq!(un("vfceil", f.clone()), f32s([-1.0, 3.0, 4.0, -0.0]));
    assert_eq!(un("vftrunc", f), f32s([-1.0, 2.0, 4.0, -0.0]));
    assert_eq!(
        un("vfsqrt", f32s([4.0, 2.25, -1.0, 0.0])),
        f32s([2.0, 1.5, f32::NAN, 0.0])
    );
}

#[test]
fn splat_extract_insert_shuffle() {
    let src = "\
func @f(i32, i32x4) -> (i32x4, i32, i32x4, i32x4) {
block0(v0: i32, v1: i32x4):
    v2 = vsplat.i32x4 v0
    v3 = vextract v1, 2
    v4 = vinsert v1, v0, 0
    v5 = vshuffle v1, v2, [7, 0, 3, 3]
    return v2, v3, v4, v5
}
";
    assert_eq!(
        super::run(src, "f", &[Val::I32(9), i32x4([1, 2, 3, 4])]),
        keel_ir_interp::Outcome::Return(vec![
            i32x4([9, 9, 9, 9]),
            Val::I32(3),
            i32x4([9, 2, 3, 4]),
            i32x4([9, 1, 4, 4]),
        ])
    );
}

#[test]
fn comparisons_give_lane_masks_and_select_uses_them() {
    let a = i32x4([1, -1, 5, 0]);
    let b = i32x4([1, 1, 4, 0]);
    assert_eq!(bin("vicmp.slt", a.clone(), b.clone()), i32x4([0, -1, 0, 0]));
    assert_eq!(bin("vicmp.ult", a.clone(), b.clone()), i32x4([0, 0, 0, 0]));
    assert_eq!(bin("vicmp.eq", a, b), i32x4([-1, 0, 0, -1]));
    let x = v(F64x2, &[f64::NAN.to_bits(), 1f64.to_bits()]);
    let y = v(F64x2, &[0f64.to_bits(), 2f64.to_bits()]);
    let cmp = binary_fn("vfcmp.lt", "f64x2", "f64x2", "i64x2");
    assert_eq!(
        eval(&cmp, "f", &[x.clone(), y.clone()]),
        v(I64x2, &[0, u64::MAX])
    );
    let cmp = binary_fn("vfcmp.uno", "f64x2", "f64x2", "i64x2");
    assert_eq!(
        eval(&cmp, "f", &[x.clone(), y.clone()]),
        v(I64x2, &[u64::MAX, 0])
    );
    let select = "\
func @f(i64x2, f64x2, f64x2) -> f64x2 {
block0(v0: i64x2, v1: f64x2, v2: f64x2):
    v3 = vselect v0, v1, v2
    return v3
}
";
    // Any nonzero mask lane selects the first arm.
    assert_eq!(
        eval(select, "f", &[v(I64x2, &[0, 2]), x, y]),
        v(F64x2, &[0f64.to_bits(), 1f64.to_bits()])
    );
}

#[test]
fn reductions() {
    let red = |op: &str, a: Val| eval(&unary_fn(op, "i8x16", "i8"), "f", &[a]);
    let mut lanes = [1u64; 16];
    lanes[3] = 0xff;
    lanes[7] = 0x80;
    let a = v(I8x16, &lanes);
    assert_eq!(
        red("vreduce_add", a.clone()),
        Val::I8(((14 + 0xff + 0x80) % 256) as u8)
    );
    assert_eq!(red("vreduce_smin", a.clone()), Val::I8(0x80));
    assert_eq!(red("vreduce_smax", a.clone()), Val::I8(1));
    assert_eq!(red("vreduce_umin", a.clone()), Val::I8(1));
    assert_eq!(red("vreduce_umax", a.clone()), Val::I8(0xff));
    assert_eq!(red("vreduce_and", a.clone()), Val::I8(0));
    assert_eq!(red("vreduce_or", a.clone()), Val::I8(0xff));
    assert_eq!(red("vreduce_xor", a), Val::I8(0xff ^ 0x80));
}

#[test]
fn widening_narrowing_and_pairwise_conversions() {
    let conv = |op: &str, a: Val, to: VectorType| {
        let from = a.ty().to_string();
        let to = to.to_string();
        eval(&unary_fn(&format!("{op}.{to}"), &from, &to), "f", &[a])
    };
    let a = v(I16x8, &[0xffff, 2, 3, 4, 5, 6, 7, 0x8000]);
    assert_eq!(conv("swiden_low", a.clone(), I32x4), i32x4([-1, 2, 3, 4]));
    assert_eq!(
        conv("uwiden_low", a.clone(), I32x4),
        i32x4([0xffff, 2, 3, 4])
    );
    assert_eq!(
        conv("swiden_high", a.clone(), I32x4),
        i32x4([5, 6, 7, -32768])
    );
    assert_eq!(
        conv("uwiden_high", a.clone(), I64x2),
        v(I64x2, &[7, 0x8000])
    );
    assert_eq!(
        conv("swiden_low", a.clone(), I64x2),
        v(I64x2, &[u64::MAX, 2])
    );
    assert_eq!(
        conv("spairwise_add_long", a.clone(), I32x4),
        i32x4([1, 7, 11, 7 - 32768])
    );
    assert_eq!(
        conv("upairwise_add_long", a, I32x4),
        i32x4([0x10001, 7, 11, 0x8007])
    );
    let wide = i32x4([-1, 300, -300, 70000]);
    let mut expect = vec![0xffff, 300, neg(-300) & 0xffff, 70000 & 0xffff];
    expect.resize(8, 0);
    assert_eq!(conv("narrow_low", wide.clone(), I16x8), v(I16x8, &expect));
    let narrow = |op: &str| conv(op, wide.clone(), I8x16).lanes().unwrap();
    assert_eq!(narrow("snarrow_sat_low")[..5], [0xff, 0x7f, 0x80, 0x7f, 0]);
    assert_eq!(narrow("unarrow_sat_low")[..5], [0xff, 0xff, 0xff, 0xff, 0]);
    assert_eq!(narrow("sunarrow_sat_low")[..5], [0, 0xff, 0, 0xff, 0]);
}

#[test]
fn bitcasts_keep_bytes() {
    let a = i32x4([1, 2, 3, -1]);
    let to_i128 = unary_fn("bitcast.i128", "i32x4", "i128");
    let bits = 1u128 | 2 << 32 | 3 << 64 | (0xffff_ffffu128 << 96);
    assert_eq!(
        eval(&to_i128, "f", std::slice::from_ref(&a)),
        Val::I128(bits)
    );
    let to_f32 = unary_fn("bitcast.f32x4", "i32x4", "f32x4");
    assert_eq!(eval(&to_f32, "f", &[a]), v(F32x4, &[1, 2, 3, 0xffff_ffff]));
}

#[test]
fn vector_memory_is_lane_zero_first() {
    let src = "\
func @f(i16x8) -> (i16, i16x8) {
    ss0 = stack_slot 16, align 16
block0(v0: i16x8):
    v1 = stack_addr ss0
    store align 16 v0, v1
    v2 = iconst.i64 2
    v3 = ptradd v1, v2
    v4 = load.i16 v3
    v5 = load.i16x8 align 16 v1
    return v4, v5
}
";
    let a = v(I16x8, &[10, 11, 12, 13, 14, 15, 16, 17]);
    assert_eq!(
        super::run(src, "f", std::slice::from_ref(&a)),
        keel_ir_interp::Outcome::Return(vec![Val::I16(11), a])
    );
}

#[test]
fn fma_and_select_on_vectors() {
    let src = "\
func @f(i8, f64x2, f64x2) -> f64x2 {
block0(v0: i8, v1: f64x2, v2: f64x2):
    v3 = fma v1, v2, v1
    v4 = select v0, v3, v2
    return v4
}
";
    let a = v(F64x2, &[2f64.to_bits(), (-1f64).to_bits()]);
    let b = v(F64x2, &[3f64.to_bits(), 0.5f64.to_bits()]);
    assert_eq!(
        eval(src, "f", &[Val::I8(1), a.clone(), b.clone()]),
        v(F64x2, &[8f64.to_bits(), (-1.5f64).to_bits()])
    );
    assert_eq!(eval(src, "f", &[Val::I8(0), a, b.clone()]), b);
}
