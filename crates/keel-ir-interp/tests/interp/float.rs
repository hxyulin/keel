//! Float arithmetic, comparisons and conversions.

use super::{binary_fn, eval, unary_fn};
use keel_ir_interp::Val;

const NAN64: u64 = 0x7ff8_0000_0000_0000;
const NAN32: u32 = 0x7fc0_0000;

fn bin64(op: &str, a: f64, b: f64) -> Val {
    eval(
        &binary_fn(op, "f64", "f64", "f64"),
        "f",
        &[Val::f64(a), Val::f64(b)],
    )
}

fn un64(op: &str, a: f64) -> Val {
    eval(&unary_fn(op, "f64", "f64"), "f", &[Val::f64(a)])
}

#[test]
fn constants() {
    let src = "func @f() -> (f64, f32, f64) {\nblock0:\n    v0 = fconst.f64 1.5\n    v1 = fconst.f32 -0.25\n    v2 = fconst.f64 0x7ff0000000000001\n    return v0, v1, v2\n}\n";
    assert_eq!(
        super::run(src, "f", &[]),
        keel_ir_interp::Outcome::Return(vec![
            Val::f64(1.5),
            Val::f32(-0.25),
            Val::F64(0x7ff0_0000_0000_0001)
        ])
    );
}

#[test]
fn arithmetic_rounds_in_the_operand_type() {
    assert_eq!(bin64("fadd", 0.1, 0.2), Val::f64(0.1 + 0.2));
    assert_eq!(bin64("fsub", 1.0, 0.75), Val::f64(0.25));
    assert_eq!(bin64("fmul", 3.0, -0.5), Val::f64(-1.5));
    assert_eq!(bin64("fdiv", 1.0, 0.0), Val::f64(f64::INFINITY));
    let f32_add = binary_fn("fadd", "f32", "f32", "f32");
    assert_eq!(
        eval(&f32_add, "f", &[Val::f32(0.1), Val::f32(0.2)]),
        Val::f32(0.1f32 + 0.2f32)
    );
}

#[test]
fn nan_results_are_canonical() {
    assert_eq!(bin64("fdiv", 0.0, 0.0), Val::F64(NAN64));
    let payload = f64::from_bits(0x7ff8_0000_0000_1234);
    assert_eq!(bin64("fadd", payload, 1.0), Val::F64(NAN64));
    assert_eq!(un64("fsqrt", -1.0), Val::F64(NAN64));
    let f32_mul = binary_fn("fmul", "f32", "f32", "f32");
    assert_eq!(
        eval(&f32_mul, "f", &[Val::f32(f32::INFINITY), Val::f32(0.0)]),
        Val::F32(NAN32)
    );
}

#[test]
fn sign_operations_keep_nan_payloads() {
    let payload = 0x7ff8_0000_0000_1234;
    let nan = f64::from_bits(payload);
    assert_eq!(un64("fneg", nan), Val::F64(payload | 1 << 63));
    assert_eq!(
        un64("fabs", f64::from_bits(payload | 1 << 63)),
        Val::F64(payload)
    );
    assert_eq!(bin64("fcopysign", nan, -1.0), Val::F64(payload | 1 << 63));
    assert_eq!(bin64("fcopysign", 2.0, -0.0), Val::f64(-2.0));
}

#[test]
fn min_max_propagate_nan_and_order_zeros() {
    assert_eq!(bin64("fmin", f64::NAN, 1.0), Val::F64(NAN64));
    assert_eq!(bin64("fmax", 1.0, f64::NAN), Val::F64(NAN64));
    assert_eq!(bin64("fmin", 0.0, -0.0), Val::f64(-0.0));
    assert_eq!(bin64("fmax", -0.0, 0.0), Val::f64(0.0));
    assert_eq!(bin64("fmin", 2.0, -3.0), Val::f64(-3.0));
    assert_eq!(bin64("fmax", 2.0, -3.0), Val::f64(2.0));
}

#[test]
fn rounding_to_integral() {
    assert_eq!(un64("fceil", 1.25), Val::f64(2.0));
    assert_eq!(un64("ffloor", -1.25), Val::f64(-2.0));
    assert_eq!(un64("ftrunc", -1.75), Val::f64(-1.0));
    assert_eq!(un64("fnearest", 2.5), Val::f64(2.0));
    assert_eq!(un64("fnearest", 3.5), Val::f64(4.0));
    assert_eq!(un64("fnearest", -0.5), Val::f64(-0.0));
    assert_eq!(un64("fsqrt", 2.25), Val::f64(1.5));
}

#[test]
fn fma_rounds_once() {
    let src = "func @f(f64, f64, f64) -> f64 {\nblock0(v0: f64, v1: f64, v2: f64):\n    v3 = fma v0, v1, v2\n    return v3\n}\n";
    let e = f64::EPSILON;
    // (1 + e)(1 - e) - 1 is -e^2 exactly; rounding the product first gives 0.
    assert_eq!(
        eval(
            src,
            "f",
            &[Val::f64(1.0 + e), Val::f64(1.0 - e), Val::f64(-1.0)]
        ),
        Val::f64(-e * e)
    );
}

#[test]
fn comparisons_with_nan() {
    let cmp = |cond: &str, a: f64, b: f64| {
        eval(
            &binary_fn(&format!("fcmp.{cond}"), "f64", "f64", "i8"),
            "f",
            &[Val::f64(a), Val::f64(b)],
        )
    };
    for (cond, with_nan, one_two) in [
        ("eq", 0, 0),
        ("ne", 1, 1),
        ("lt", 0, 1),
        ("le", 0, 1),
        ("gt", 0, 0),
        ("ge", 0, 0),
        ("ord", 0, 1),
        ("uno", 1, 0),
    ] {
        assert_eq!(cmp(cond, f64::NAN, 1.0), Val::I8(with_nan), "{cond} nan");
        assert_eq!(cmp(cond, 1.0, 2.0), Val::I8(one_two), "{cond} 1 2");
    }
    assert_eq!(cmp("eq", 0.0, -0.0), Val::I8(1));
}

#[test]
fn conversions() {
    let conv = |op: &str, from: &str, to: &str, x: Val| {
        eval(&unary_fn(&format!("{op}.{to}"), from, to), "f", &[x])
    };
    assert_eq!(
        conv("fpext", "f32", "f64", Val::f32(0.1)),
        Val::f64(f64::from(0.1f32))
    );
    assert_eq!(conv("fptrunc", "f64", "f32", Val::f64(0.1)), Val::f32(0.1));
    assert_eq!(
        conv("fptrunc", "f64", "f32", Val::f64(1e300)),
        Val::f32(f32::INFINITY)
    );
    assert_eq!(
        conv("fptosi", "f64", "i32", Val::f64(-1.9)),
        Val::I32(-1i32 as u32)
    );
    assert_eq!(
        conv("fptosi", "f64", "i32", Val::f64(1e20)),
        Val::I32(i32::MAX as u32)
    );
    assert_eq!(
        conv("fptosi", "f32", "i64", Val::f32(f32::NAN)),
        Val::I64(0)
    );
    assert_eq!(conv("fptoui", "f64", "i32", Val::f64(-5.0)), Val::I32(0));
    assert_eq!(
        conv("fptoui", "f64", "i64", Val::f64(1e30)),
        Val::I64(u64::MAX)
    );
    assert_eq!(conv("sitofp", "i8", "f64", Val::I8(0xff)), Val::f64(-1.0));
    assert_eq!(conv("uitofp", "i8", "f32", Val::I8(0xff)), Val::f32(255.0));
    assert_eq!(
        conv("uitofp", "i64", "f64", Val::I64(u64::MAX)),
        Val::f64(18446744073709551616.0)
    );
    // 2^24 + 1 is not representable in f32 and rounds to even.
    assert_eq!(
        conv("sitofp", "i32", "f32", Val::I32((1 << 24) + 1)),
        Val::f32(16777216.0)
    );
    assert_eq!(
        conv("bitcast", "f32", "i32", Val::f32(1.0)),
        Val::I32(0x3f80_0000)
    );
    assert_eq!(
        conv("bitcast", "i64", "f64", Val::I64(0x7ff8_0000_0000_0001)),
        Val::F64(0x7ff8_0000_0000_0001)
    );
}
