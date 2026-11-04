//! Scalar and lane arithmetic on bit patterns.
//!
//! Integer helpers take the operand width in bits and bits already truncated
//! to it, and return truncated bits. Float helpers take `f32` or `f64` and raw
//! bits. Vector instructions apply the same helpers per lane.

use crate::value::{mask, signed};
use crate::{Stop, Trap};
use keel_ir::{BinaryOp, FloatCC, IntCC, OverflowOp, Type, UnaryOp, VectorBinaryOp};

pub(crate) fn int_binary(op: BinaryOp, width: u32, x: u128, y: u128) -> Result<u128, Stop> {
    let m = mask(width);
    let sx = signed(x, width);
    let sy = signed(y, width);
    let amount = |y: u128| (y % u128::from(width)) as u32;
    let result = match op {
        BinaryOp::Iadd => x.wrapping_add(y),
        BinaryOp::Isub => x.wrapping_sub(y),
        BinaryOp::Imul => x.wrapping_mul(y),
        BinaryOp::Udiv | BinaryOp::Urem if y == 0 => return Err(Trap::DivisionByZero.into()),
        BinaryOp::Udiv => x / y,
        BinaryOp::Urem => x % y,
        BinaryOp::Sdiv | BinaryOp::Srem if y == 0 => return Err(Trap::DivisionByZero.into()),
        BinaryOp::Sdiv if sy == -1 && sx == signed(1 << (width - 1), width) => {
            return Err(Trap::IntegerOverflow.into());
        }
        BinaryOp::Sdiv => sx.wrapping_div(sy) as u128,
        BinaryOp::Srem => sx.wrapping_rem(sy) as u128,
        BinaryOp::And => x & y,
        BinaryOp::Or => x | y,
        BinaryOp::Xor => x ^ y,
        BinaryOp::Shl => x << amount(y),
        BinaryOp::Ushr => x >> amount(y),
        BinaryOp::Sshr => (sx >> amount(y)) as u128,
        BinaryOp::Rotl | BinaryOp::Rotr => {
            let mut s = amount(y);
            if op == BinaryOp::Rotr {
                s = (width - s) % width;
            }
            if s == 0 {
                x
            } else {
                (x << s) | (x >> (width - s))
            }
        }
        BinaryOp::Umulhi | BinaryOp::Smulhi if width > 64 => {
            return Err(Stop::Invalid(format!("{op} on i{width}")));
        }
        BinaryOp::Umulhi => (x * y) >> width,
        BinaryOp::Smulhi => ((sx * sy) >> width) as u128,
        BinaryOp::Ptradd
        | BinaryOp::Fadd
        | BinaryOp::Fsub
        | BinaryOp::Fmul
        | BinaryOp::Fdiv
        | BinaryOp::Fmin
        | BinaryOp::Fmax
        | BinaryOp::Fcopysign => {
            return Err(Stop::Invalid(format!("{op} on integers")));
        }
    };
    Ok(result & m)
}

pub(crate) fn int_unary(op: UnaryOp, width: u32, x: u128) -> Result<u128, Stop> {
    let m = mask(width);
    let result = match op {
        UnaryOp::Ineg => x.wrapping_neg(),
        UnaryOp::Bnot => !x,
        UnaryOp::Clz => u128::from(x.leading_zeros() - (128 - width)),
        UnaryOp::Ctz => u128::from(x.trailing_zeros().min(width)),
        UnaryOp::Popcnt => u128::from(x.count_ones()),
        UnaryOp::Bswap if width < 16 => return Err(Stop::Invalid("bswap on i8".into())),
        UnaryOp::Bswap => x.swap_bytes() >> (128 - width),
        UnaryOp::Fneg
        | UnaryOp::Fabs
        | UnaryOp::Fsqrt
        | UnaryOp::Fceil
        | UnaryOp::Ffloor
        | UnaryOp::Ftrunc
        | UnaryOp::Fnearest => return Err(Stop::Invalid(format!("{op} on integers"))),
    };
    Ok(result & m)
}

pub(crate) fn icmp(cond: IntCC, width: u32, x: u128, y: u128) -> bool {
    let (sx, sy) = (signed(x, width), signed(y, width));
    match cond {
        IntCC::Eq => x == y,
        IntCC::Ne => x != y,
        IntCC::Slt => sx < sy,
        IntCC::Sle => sx <= sy,
        IntCC::Sgt => sx > sy,
        IntCC::Sge => sx >= sy,
        IntCC::Ult => x < y,
        IntCC::Ule => x <= y,
        IntCC::Ugt => x > y,
        IntCC::Uge => x >= y,
    }
}

/// The wrapped result and whether the exact result does not fit.
pub(crate) fn overflow(op: OverflowOp, width: u32, x: u128, y: u128) -> (u128, bool) {
    let m = mask(width);
    let (sx, sy) = (signed(x, width), signed(y, width));
    let fits_signed = |exact: i128| signed(exact as u128 & m, width) == exact;
    let (result, overflowed) = match op {
        OverflowOp::Uadd if width == 128 => x.overflowing_add(y),
        OverflowOp::Uadd => (x + y, x + y > m),
        OverflowOp::Usub => (x.wrapping_sub(y), x < y),
        OverflowOp::Umul if width == 128 => x.overflowing_mul(y),
        OverflowOp::Umul => (x * y, x * y > m),
        OverflowOp::Sadd if width == 128 => {
            let (r, o) = sx.overflowing_add(sy);
            (r as u128, o)
        }
        OverflowOp::Ssub if width == 128 => {
            let (r, o) = sx.overflowing_sub(sy);
            (r as u128, o)
        }
        OverflowOp::Smul if width == 128 => {
            let (r, o) = sx.overflowing_mul(sy);
            (r as u128, o)
        }
        OverflowOp::Sadd => (sx.wrapping_add(sy) as u128, !fits_signed(sx + sy)),
        OverflowOp::Ssub => (sx.wrapping_sub(sy) as u128, !fits_signed(sx - sy)),
        OverflowOp::Smul => (sx.wrapping_mul(sy) as u128, !fits_signed(sx * sy)),
    };
    (result & m, overflowed)
}

/// Lane operations of [`VectorBinaryOp`] beyond the scalar integer set.
pub(crate) fn int_lane(op: VectorBinaryOp, width: u32, x: u128, y: u128) -> Result<u128, Stop> {
    use VectorBinaryOp as V;
    let m = mask(width);
    let (sx, sy) = (signed(x, width), signed(y, width));
    let (smin, smax) = (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1);
    let result = match op {
        V::Iadd => return int_binary(BinaryOp::Iadd, width, x, y),
        V::Isub => return int_binary(BinaryOp::Isub, width, x, y),
        V::Imul => return int_binary(BinaryOp::Imul, width, x, y),
        V::And => return int_binary(BinaryOp::And, width, x, y),
        V::Or => return int_binary(BinaryOp::Or, width, x, y),
        V::Xor => return int_binary(BinaryOp::Xor, width, x, y),
        V::Shl => return int_binary(BinaryOp::Shl, width, x, y),
        V::Ushr => return int_binary(BinaryOp::Ushr, width, x, y),
        V::Sshr => return int_binary(BinaryOp::Sshr, width, x, y),
        V::Smin => sx.min(sy) as u128,
        V::Smax => sx.max(sy) as u128,
        V::Umin => x.min(y),
        V::Umax => x.max(y),
        V::Sabd => sx.abs_diff(sy),
        V::Uabd => x.abs_diff(y),
        V::Shadd => ((sx + sy) >> 1) as u128,
        V::Uhadd => (x + y) >> 1,
        V::Srhadd => ((sx + sy + 1) >> 1) as u128,
        V::Urhadd => (x + y + 1) >> 1,
        V::Shsub => ((sx - sy) >> 1) as u128,
        V::Uhsub => ((x as i128 - y as i128) >> 1) as u128,
        V::SaddSat => (sx + sy).clamp(smin, smax) as u128,
        V::SsubSat => (sx - sy).clamp(smin, smax) as u128,
        V::UaddSat => (x + y).min(m),
        V::UsubSat => x.saturating_sub(y),
        V::Padd
        | V::Psmin
        | V::Psmax
        | V::Pumin
        | V::Pumax
        | V::Fpadd
        | V::Fpmin
        | V::Fpmax
        | V::Fadd
        | V::Fsub
        | V::Fmul
        | V::Fdiv
        | V::Fmin
        | V::Fmax
        | V::Fcopysign => return Err(Stop::Invalid(format!("{op} as a lane operation"))),
    };
    Ok(result & m)
}

/// `f32` and `f64` behind one interface, over raw bits.
trait Float: Copy + PartialOrd {
    const SIGN: u64;
    const CANONICAL_NAN: u64;
    fn from_raw(bits: u64) -> Self;
    fn raw(self) -> u64;
    fn is_nan(self) -> bool;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
    fn div(self, other: Self) -> Self;
    fn sqrt(self) -> Self;
    fn ceil(self) -> Self;
    fn floor(self) -> Self;
    fn trunc(self) -> Self;
    fn round_ties_even(self) -> Self;
    fn mul_add(self, b: Self, c: Self) -> Self;
}

macro_rules! float {
    ($t:ty, $bits:ty, $sign:expr, $nan:expr) => {
        impl Float for $t {
            const SIGN: u64 = $sign;
            const CANONICAL_NAN: u64 = $nan;
            fn from_raw(bits: u64) -> Self {
                <$t>::from_bits(bits as $bits)
            }
            fn raw(self) -> u64 {
                self.to_bits().into()
            }
            fn is_nan(self) -> bool {
                <$t>::is_nan(self)
            }
            fn add(self, other: Self) -> Self {
                self + other
            }
            fn sub(self, other: Self) -> Self {
                self - other
            }
            fn mul(self, other: Self) -> Self {
                self * other
            }
            fn div(self, other: Self) -> Self {
                self / other
            }
            fn sqrt(self) -> Self {
                <$t>::sqrt(self)
            }
            fn ceil(self) -> Self {
                <$t>::ceil(self)
            }
            fn floor(self) -> Self {
                <$t>::floor(self)
            }
            fn trunc(self) -> Self {
                <$t>::trunc(self)
            }
            fn round_ties_even(self) -> Self {
                <$t>::round_ties_even(self)
            }
            fn mul_add(self, b: Self, c: Self) -> Self {
                <$t>::mul_add(self, b, c)
            }
        }
    };
}
float!(f32, u32, 1 << 31, 0x7fc0_0000);
float!(f64, u64, 1 << 63, 0x7ff8_0000_0000_0000);

/// Arithmetic results that are NaN become the canonical quiet NaN: which NaN
/// an operation gives is unspecified, and the optimizer's folds give this one.
fn canonical<F: Float>(x: F) -> u64 {
    if x.is_nan() {
        F::CANONICAL_NAN
    } else {
        x.raw()
    }
}

fn minmax<F: Float>(a: F, b: F, max: bool) -> u64 {
    if a.is_nan() || b.is_nan() {
        return F::CANONICAL_NAN;
    }
    if a == b {
        // Equal, or zeros of either sign: -0 orders below +0.
        let negative = |x: F| x.raw() & F::SIGN != 0;
        let pick_a = negative(a) != max;
        return if pick_a { a.raw() } else { b.raw() };
    }
    let a_wins = (a > b) == max;
    if a_wins { a.raw() } else { b.raw() }
}

fn float_binary_typed<F: Float>(op: BinaryOp, a: u64, b: u64) -> Result<u64, Stop> {
    let (x, y) = (F::from_raw(a), F::from_raw(b));
    Ok(match op {
        BinaryOp::Fadd => canonical(x.add(y)),
        BinaryOp::Fsub => canonical(x.sub(y)),
        BinaryOp::Fmul => canonical(x.mul(y)),
        BinaryOp::Fdiv => canonical(x.div(y)),
        BinaryOp::Fmin => minmax(x, y, false),
        BinaryOp::Fmax => minmax(x, y, true),
        BinaryOp::Fcopysign => (a & !F::SIGN) | (b & F::SIGN),
        _ => return Err(Stop::Invalid(format!("{op} on floats"))),
    })
}

pub(crate) fn float_binary(op: BinaryOp, ty: Type, a: u64, b: u64) -> Result<u64, Stop> {
    match ty {
        Type::F32 => float_binary_typed::<f32>(op, a, b),
        Type::F64 => float_binary_typed::<f64>(op, a, b),
        _ => Err(Stop::Invalid(format!("{op} on {ty}"))),
    }
}

fn float_unary_typed<F: Float>(op: UnaryOp, a: u64) -> Result<u64, Stop> {
    let x = F::from_raw(a);
    Ok(match op {
        UnaryOp::Fneg => a ^ F::SIGN,
        UnaryOp::Fabs => a & !F::SIGN,
        UnaryOp::Fsqrt => canonical(x.sqrt()),
        UnaryOp::Fceil => canonical(x.ceil()),
        UnaryOp::Ffloor => canonical(x.floor()),
        UnaryOp::Ftrunc => canonical(x.trunc()),
        UnaryOp::Fnearest => canonical(x.round_ties_even()),
        _ => return Err(Stop::Invalid(format!("{op} on floats"))),
    })
}

pub(crate) fn float_unary(op: UnaryOp, ty: Type, a: u64) -> Result<u64, Stop> {
    match ty {
        Type::F32 => float_unary_typed::<f32>(op, a),
        Type::F64 => float_unary_typed::<f64>(op, a),
        _ => Err(Stop::Invalid(format!("{op} on {ty}"))),
    }
}

pub(crate) fn fma(ty: Type, a: u64, b: u64, c: u64) -> Result<u64, Stop> {
    fn typed<F: Float>(a: u64, b: u64, c: u64) -> u64 {
        canonical(F::from_raw(a).mul_add(F::from_raw(b), F::from_raw(c)))
    }
    match ty {
        Type::F32 => Ok(typed::<f32>(a, b, c)),
        Type::F64 => Ok(typed::<f64>(a, b, c)),
        _ => Err(Stop::Invalid(format!("fma on {ty}"))),
    }
}

pub(crate) fn fcmp(cond: FloatCC, ty: Type, a: u64, b: u64) -> Result<bool, Stop> {
    fn typed<F: Float>(cond: FloatCC, a: u64, b: u64) -> bool {
        let (x, y) = (F::from_raw(a), F::from_raw(b));
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
    match ty {
        Type::F32 => Ok(typed::<f32>(cond, a, b)),
        Type::F64 => Ok(typed::<f64>(cond, a, b)),
        _ => Err(Stop::Invalid(format!("fcmp on {ty}"))),
    }
}

/// The scalar float operation a float [`VectorBinaryOp`] applies per lane.
pub(crate) fn vector_float_op(op: VectorBinaryOp) -> Option<BinaryOp> {
    Some(match op {
        VectorBinaryOp::Fadd => BinaryOp::Fadd,
        VectorBinaryOp::Fsub => BinaryOp::Fsub,
        VectorBinaryOp::Fmul => BinaryOp::Fmul,
        VectorBinaryOp::Fdiv => BinaryOp::Fdiv,
        VectorBinaryOp::Fmin => BinaryOp::Fmin,
        VectorBinaryOp::Fmax => BinaryOp::Fmax,
        VectorBinaryOp::Fcopysign => BinaryOp::Fcopysign,
        _ => return None,
    })
}

pub(crate) fn canonical_nan(ty: Type, bits: u64) -> u64 {
    match ty {
        Type::F32 => canonical(f32::from_raw(bits)),
        _ => canonical(f64::from_raw(bits)),
    }
}
