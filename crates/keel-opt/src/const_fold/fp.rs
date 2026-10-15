//! Float folding. Each operation is written once for `f64` and once for
//! `f32` through the `at_width!` macro: f32 arithmetic must round to f32 at
//! every step, so computing in f64 and narrowing at the end would be wrong.

use keel_ir::{BinaryOp, FloatCC, Type, UnaryOp};

/// A float constant: its type and its bits (`f32` in the low 32).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Float {
    pub ty: Type,
    pub bits: u64,
}

/// Runs `$body` with `$x...` bound to the operands as native floats of
/// `$ty`'s width, and packs the float it returns. A NaN result becomes the
/// canonical quiet NaN: the IR leaves the payload unspecified, and the
/// host's choice (x86 makes 0/0 negative, aarch64 positive) would make
/// folded output differ between machines.
macro_rules! at_width {
    ($ty:expr, |$($x:ident),*| $body:expr) => {
        match $ty {
            Type::F32 => {
                $(let $x = f32::from_bits($x.bits as u32);)*
                let r: f32 = $body;
                let r = if r.is_nan() { f32::NAN } else { r };
                Float { ty: Type::F32, bits: u64::from(r.to_bits()) }
            }
            _ => {
                $(let $x = f64::from_bits($x.bits);)*
                let r: f64 = $body;
                let r = if r.is_nan() { f64::NAN } else { r };
                Float { ty: Type::F64, bits: r.to_bits() }
            }
        }
    };
}

pub(crate) fn binary(op: BinaryOp, a: Float, b: Float) -> Float {
    if op == BinaryOp::Fcopysign {
        // Only the sign bit moves, NaN or not, as for `fneg`.
        let sign = sign_bit(a.ty);
        return Float {
            bits: (a.bits & !sign) | (b.bits & sign),
            ..a
        };
    }
    at_width!(a.ty, |a, b| match op {
        BinaryOp::Fadd => a + b,
        BinaryOp::Fsub => a - b,
        BinaryOp::Fmul => a * b,
        BinaryOp::Fdiv => a / b,
        // IEEE minimum/maximum: NaN wins, and -0 < +0, which `f64::min`
        // does not promise.
        BinaryOp::Fmin | BinaryOp::Fmax if a.is_nan() || b.is_nan() => a + b,
        BinaryOp::Fmin if a == b =>
            if a.is_sign_negative() {
                a
            } else {
                b
            },
        BinaryOp::Fmax if a == b =>
            if a.is_sign_positive() {
                a
            } else {
                b
            },
        BinaryOp::Fmin => a.min(b),
        BinaryOp::Fmax => a.max(b),
        _ => unreachable!("not a float operation: {op}"),
    })
}

/// `a * b + c`, rounded once.
pub(crate) fn fma(a: Float, b: Float, c: Float) -> Float {
    at_width!(a.ty, |a, b, c| a.mul_add(b, c))
}

fn sign_bit(ty: Type) -> u64 {
    match ty {
        Type::F32 => 1 << 31,
        _ => 1 << 63,
    }
}

pub(crate) fn unary(op: UnaryOp, a: Float) -> Float {
    // `fneg` and `fabs` only touch the sign bit, NaN or not, on the
    // hardware and in IEEE 754, so they are exact bit operations here too.
    let sign = sign_bit(a.ty);
    match op {
        UnaryOp::Fneg => {
            return Float {
                bits: a.bits ^ sign,
                ..a
            };
        }
        UnaryOp::Fabs => {
            return Float {
                bits: a.bits & !sign,
                ..a
            };
        }
        _ => {}
    }
    at_width!(a.ty, |a| match op {
        UnaryOp::Fsqrt => a.sqrt(),
        UnaryOp::Fceil => a.ceil(),
        UnaryOp::Ffloor => a.floor(),
        UnaryOp::Ftrunc => a.trunc(),
        UnaryOp::Fnearest => a.round_ties_even(),
        _ => unreachable!("not a float operation: {op}"),
    })
}

/// `fpext` or `fptrunc` to `to`.
pub(crate) fn convert(to: Type, a: Float) -> Float {
    let x = match a.ty {
        Type::F32 => f64::from(f32::from_bits(a.bits as u32)),
        _ => f64::from_bits(a.bits),
    };
    let x = if x.is_nan() { f64::NAN } else { x };
    match to {
        Type::F32 => Float {
            ty: Type::F32,
            bits: u64::from((x as f32).to_bits()),
        },
        _ => Float {
            ty: Type::F64,
            bits: x.to_bits(),
        },
    }
}

/// `fptosi`/`fptoui` to `i32` or `i64`, as bits. Rust's `as` saturates and
/// maps NaN to 0, exactly like `fcvtzs`/`fcvtzu`. An f32 widens to f64
/// exactly, so converting from f64 loses nothing.
pub(crate) fn to_int(a: Float, signed: bool, to: Type) -> u64 {
    let x = match a.ty {
        Type::F32 => f64::from(f32::from_bits(a.bits as u32)),
        _ => f64::from_bits(a.bits),
    };
    match (signed, to) {
        (true, Type::I32) => x as i32 as u64,
        (true, _) => x as i64 as u64,
        (false, Type::I32) => u64::from(x as u32),
        (false, _) => x as u64,
    }
}

/// `sitofp`/`uitofp` of `n`, rounded once to `to`.
pub(crate) fn from_int(n: i128, to: Type) -> Float {
    match to {
        Type::F32 => Float {
            ty: Type::F32,
            bits: u64::from((n as f32).to_bits()),
        },
        _ => Float {
            ty: Type::F64,
            bits: (n as f64).to_bits(),
        },
    }
}

pub(crate) fn compare(cond: FloatCC, a: Float, b: Float) -> bool {
    // Comparing in f64 is exact for f32 operands.
    let wide = |f: Float| match f.ty {
        Type::F32 => f64::from(f32::from_bits(f.bits as u32)),
        _ => f64::from_bits(f.bits),
    };
    let (x, y) = (wide(a), wide(b));
    match cond {
        FloatCC::Eq => x == y,
        FloatCC::Ne => x != y,
        FloatCC::Lt => x < y,
        FloatCC::Le => x <= y,
        FloatCC::Gt => x > y,
        FloatCC::Ge => x >= y,
        FloatCC::Ord => !x.is_nan() && !y.is_nan(),
        FloatCC::Uno => x.is_nan() || y.is_nan(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f64c(x: f64) -> Float {
        Float {
            ty: Type::F64,
            bits: x.to_bits(),
        }
    }

    fn f32c(x: f32) -> Float {
        Float {
            ty: Type::F32,
            bits: u64::from(x.to_bits()),
        }
    }

    #[test]
    fn f32_rounds_at_f32() {
        // 2^24 + 1 is not an f32; in f64 it would survive.
        let r = binary(BinaryOp::Fadd, f32c(16777216.0), f32c(1.0));
        assert_eq!(r, f32c(16777216.0));
        assert_eq!(from_int(16777217, Type::F32), f32c(16777216.0));
        // u64::MAX rounds up to 2^64.
        assert_eq!(
            from_int(u64::MAX.into(), Type::F64),
            f64c(18446744073709551616.0)
        );
    }

    #[test]
    fn min_max_signed_zero_and_nan() {
        let min = binary(BinaryOp::Fmin, f64c(0.0), f64c(-0.0));
        assert_eq!(min, f64c(-0.0));
        let max = binary(BinaryOp::Fmax, f64c(-0.0), f64c(0.0));
        assert_eq!(max, f64c(0.0));
        let nan = binary(BinaryOp::Fmin, f64c(1.0), f64c(f64::NAN));
        assert!(f64::from_bits(nan.bits).is_nan());
    }

    #[test]
    fn conversions_saturate() {
        assert_eq!(to_int(f64c(-3.7), true, Type::I32), (-3i32) as u64);
        assert_eq!(to_int(f64c(1e20), true, Type::I64), i64::MAX as u64);
        assert_eq!(to_int(f64c(f64::NAN), true, Type::I64), 0);
        assert_eq!(to_int(f64c(-1.0), false, Type::I32), 0);
        assert_eq!(to_int(f32c(5e9), false, Type::I32), u64::from(u32::MAX));
        assert_eq!(unary(UnaryOp::Fnearest, f64c(2.5)), f64c(2.0));
        assert_eq!(unary(UnaryOp::Fnearest, f64c(3.5)), f64c(4.0));
    }

    #[test]
    fn unordered_compares() {
        let nan = f64c(f64::NAN);
        for cond in [
            FloatCC::Eq,
            FloatCC::Lt,
            FloatCC::Le,
            FloatCC::Gt,
            FloatCC::Ge,
            FloatCC::Ord,
        ] {
            assert!(!compare(cond, nan, f64c(1.0)), "{cond}");
        }
        assert!(compare(FloatCC::Ne, nan, nan));
        assert!(compare(FloatCC::Uno, f64c(1.0), nan));
        assert!(compare(FloatCC::Eq, f64c(0.0), f64c(-0.0)));
    }
}
