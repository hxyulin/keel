//! Integer arithmetic, bit operations, comparisons, overflow and conversions.

use super::{binary_fn, eval, run, unary_fn};
use keel_ir::Type;
use keel_ir_interp::{Outcome, Trap, Val};

fn bin(op: &str, ty: &str, a: Val, b: Val) -> Val {
    eval(&binary_fn(op, ty, ty, ty), "f", &[a, b])
}

fn bin_outcome(op: &str, ty: &str, a: Val, b: Val) -> Outcome {
    run(&binary_fn(op, ty, ty, ty), "f", &[a, b])
}

fn i8v(x: i8) -> Val {
    Val::I8(x as u8)
}

fn i32v(x: i32) -> Val {
    Val::I32(x as u32)
}

#[test]
fn constants_sign_extend_to_their_type() {
    let src = "func @f() -> (i8, i128) {\nblock0:\n    v0 = iconst.i8 255\n    v1 = iconst.i128 -1\n    return v0, v1\n}\n";
    assert_eq!(
        run(src, "f", &[]),
        Outcome::Return(vec![Val::I8(255), Val::I128(u128::MAX)])
    );
}

#[test]
fn arithmetic_wraps_at_the_type_width() {
    assert_eq!(bin("iadd", "i8", Val::I8(200), Val::I8(100)), Val::I8(44));
    assert_eq!(
        bin("isub", "i16", Val::I16(1), Val::I16(2)),
        Val::I16(0xffff)
    );
    assert_eq!(bin("imul", "i32", i32v(-3), i32v(7)), i32v(-21));
    assert_eq!(
        bin("iadd", "i128", Val::I128(u128::MAX), Val::I128(2)),
        Val::I128(1)
    );
    assert_eq!(
        eval(&unary_fn("ineg", "i8", "i8"), "f", &[i8v(-128)]),
        i8v(-128)
    );
}

#[test]
fn division_signedness_and_remainder_sign() {
    assert_eq!(bin("udiv", "i8", i8v(-7), Val::I8(2)), Val::I8(124));
    assert_eq!(bin("sdiv", "i8", i8v(-7), Val::I8(2)), i8v(-3));
    assert_eq!(bin("urem", "i8", i8v(-7), Val::I8(2)), Val::I8(1));
    assert_eq!(bin("srem", "i8", i8v(-7), Val::I8(2)), i8v(-1));
    assert_eq!(bin("srem", "i32", i32v(i32::MIN), i32v(-1)), Val::I32(0));
    assert_eq!(
        bin("sdiv", "i128", Val::int(Type::I128, -10), Val::I128(3)),
        Val::int(Type::I128, -3)
    );
}

#[test]
fn division_traps() {
    for op in ["udiv", "sdiv", "urem", "srem"] {
        assert_eq!(
            bin_outcome(op, "i32", Val::I32(1), Val::I32(0)),
            Outcome::Trap(Trap::DivisionByZero),
            "{op}"
        );
    }
    assert_eq!(
        bin_outcome("sdiv", "i8", i8v(-128), i8v(-1)),
        Outcome::Trap(Trap::IntegerOverflow)
    );
    assert_eq!(
        bin_outcome("sdiv", "i128", Val::I128(1 << 127), Val::I128(u128::MAX)),
        Outcome::Trap(Trap::IntegerOverflow)
    );
}

#[test]
fn shift_amounts_wrap_and_may_have_another_type() {
    let src = binary_fn("shl", "i32", "i8", "i32");
    assert_eq!(eval(&src, "f", &[Val::I32(1), Val::I8(33)]), Val::I32(2));
    assert_eq!(bin("ushr", "i8", Val::I8(0x80), Val::I8(7)), Val::I8(1));
    assert_eq!(bin("sshr", "i8", Val::I8(0x80), Val::I8(7)), Val::I8(0xff));
    assert_eq!(
        bin("sshr", "i16", Val::I16(0x8000), Val::I16(16)),
        Val::I16(0x8000)
    );
    assert_eq!(
        bin("shl", "i128", Val::I128(1), Val::I128(127)),
        Val::I128(1 << 127)
    );
}

#[test]
fn rotates() {
    assert_eq!(bin("rotl", "i8", Val::I8(0x81), Val::I8(1)), Val::I8(0x03));
    assert_eq!(bin("rotr", "i8", Val::I8(0x81), Val::I8(1)), Val::I8(0xc0));
    assert_eq!(bin("rotl", "i32", Val::I32(5), Val::I32(32)), Val::I32(5));
    assert_eq!(
        bin("rotr", "i16", Val::I16(1), Val::I16(17)),
        Val::I16(0x8000)
    );
}

#[test]
fn bitwise() {
    assert_eq!(
        bin("and", "i8", Val::I8(0b1100), Val::I8(0b1010)),
        Val::I8(0b1000)
    );
    assert_eq!(
        bin("or", "i8", Val::I8(0b1100), Val::I8(0b1010)),
        Val::I8(0b1110)
    );
    assert_eq!(
        bin("xor", "i8", Val::I8(0b1100), Val::I8(0b1010)),
        Val::I8(0b0110)
    );
    assert_eq!(
        eval(&unary_fn("bnot", "i16", "i16"), "f", &[Val::I16(1)]),
        Val::I16(0xfffe)
    );
}

#[test]
fn high_multiplication() {
    assert_eq!(
        bin("umulhi", "i64", Val::I64(u64::MAX), Val::I64(2)),
        Val::I64(1)
    );
    assert_eq!(
        bin("smulhi", "i64", Val::I64(u64::MAX), Val::I64(2)),
        Val::I64(u64::MAX)
    );
    assert_eq!(bin("umulhi", "i8", Val::I8(16), Val::I8(16)), Val::I8(1));
}

#[test]
fn bit_counting_is_defined_at_zero() {
    let count = |op: &str, ty: &str, x: Val| eval(&unary_fn(op, ty, ty), "f", &[x]);
    assert_eq!(count("clz", "i32", Val::I32(0)), Val::I32(32));
    assert_eq!(count("ctz", "i8", Val::I8(0)), Val::I8(8));
    assert_eq!(count("clz", "i16", Val::I16(1)), Val::I16(15));
    assert_eq!(count("ctz", "i64", Val::I64(8)), Val::I64(3));
    assert_eq!(
        count("popcnt", "i128", Val::I128(u128::MAX)),
        Val::I128(128)
    );
    assert_eq!(count("bswap", "i16", Val::I16(0x1234)), Val::I16(0x3412));
    assert_eq!(
        count("bswap", "i32", Val::I32(0x1234_5678)),
        Val::I32(0x7856_3412)
    );
}

#[test]
fn comparisons_take_signedness_from_the_condition() {
    let cmp = |cond: &str, a: i8, b: i8| {
        eval(
            &binary_fn(&format!("icmp.{cond}"), "i8", "i8", "i8"),
            "f",
            &[i8v(a), i8v(b)],
        )
    };
    let expect = [
        ("eq", 0),
        ("ne", 1),
        ("slt", 1),
        ("sle", 1),
        ("sgt", 0),
        ("sge", 0),
        ("ult", 0),
        ("ule", 0),
        ("ugt", 1),
        ("uge", 1),
    ];
    for (cond, result) in expect {
        assert_eq!(cmp(cond, -1, 1), Val::I8(result), "{cond}");
    }
    assert_eq!(cmp("sle", 5, 5), Val::I8(1));
}

#[test]
fn select_picks_on_nonzero() {
    let src = "func @f(i8, i64, i64) -> i64 {\nblock0(v0: i8, v1: i64, v2: i64):\n    v3 = select v0, v1, v2\n    return v3\n}\n";
    assert_eq!(
        eval(src, "f", &[Val::I8(2), Val::I64(1), Val::I64(9)]),
        Val::I64(1)
    );
    assert_eq!(
        eval(src, "f", &[Val::I8(0), Val::I64(1), Val::I64(9)]),
        Val::I64(9)
    );
}

#[test]
fn overflow_flags() {
    let ov = |op: &str, ty: &str, a: Val, b: Val| {
        let src = format!(
            "func @f({ty}, {ty}) -> ({ty}, i8) {{\nblock0(v0: {ty}, v1: {ty}):\n    v2, v3 = {op} v0, v1\n    return v2, v3\n}}\n"
        );
        match run(&src, "f", &[a, b]) {
            Outcome::Return(values) => values,
            other => panic!("{op}: {other:?}"),
        }
    };
    assert_eq!(
        ov("uadd_overflow", "i8", Val::I8(200), Val::I8(100)),
        [Val::I8(44), Val::I8(1)]
    );
    assert_eq!(
        ov("uadd_overflow", "i8", Val::I8(200), Val::I8(55)),
        [Val::I8(255), Val::I8(0)]
    );
    assert_eq!(
        ov("sadd_overflow", "i8", Val::I8(100), Val::I8(28)),
        [Val::I8(128), Val::I8(1)]
    );
    assert_eq!(
        ov("sadd_overflow", "i8", i8v(-100), i8v(-28)),
        [i8v(-128), Val::I8(0)]
    );
    assert_eq!(
        ov("usub_overflow", "i32", Val::I32(0), Val::I32(1)),
        [Val::I32(u32::MAX), Val::I8(1)]
    );
    assert_eq!(
        ov("ssub_overflow", "i16", Val::I16(0x8000), Val::I16(1)),
        [Val::I16(0x7fff), Val::I8(1)]
    );
    assert_eq!(
        ov("umul_overflow", "i64", Val::I64(1 << 32), Val::I64(1 << 32)),
        [Val::I64(0), Val::I8(1)]
    );
    assert_eq!(
        ov("smul_overflow", "i32", i32v(-65536), i32v(32768)),
        [i32v(i32::MIN), Val::I8(0)]
    );
    assert_eq!(
        ov("smul_overflow", "i64", Val::I64(1 << 62), Val::I64(2)),
        [Val::I64(1 << 63), Val::I8(1)]
    );
    assert_eq!(
        ov("uadd_overflow", "i128", Val::I128(u128::MAX), Val::I128(1)),
        [Val::I128(0), Val::I8(1)]
    );
    assert_eq!(
        ov("ssub_overflow", "i128", Val::I128(1 << 127), Val::I128(1)),
        [Val::I128(u128::MAX >> 1), Val::I8(1)]
    );
}

#[test]
fn width_conversions() {
    let conv = |op: &str, from: &str, to: &str, x: Val| {
        eval(&unary_fn(&format!("{op}.{to}"), from, to), "f", &[x])
    };
    assert_eq!(
        conv("sext", "i8", "i32", Val::I8(0x80)),
        Val::I32(0xffff_ff80)
    );
    assert_eq!(conv("uext", "i8", "i32", Val::I8(0x80)), Val::I32(0x80));
    assert_eq!(
        conv("sext", "i64", "i128", Val::I64(u64::MAX)),
        Val::I128(u128::MAX)
    );
    assert_eq!(conv("trunc", "i32", "i8", Val::I32(0x1ff)), Val::I8(0xff));
}
