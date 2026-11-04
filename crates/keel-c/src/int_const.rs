//! Exact C integer constants and the one operator kernel every constant
//! folder uses: the general evaluator, builtin immediates, lowering folds
//! and `#if`.
//!
//! A value keeps the low `width` bits of its type, extended to 128 bits as
//! the type's signedness says. So `bits as i128` is the value of a signed
//! type and `bits` the value of an unsigned one, at every width up to 128.

use crate::ast::{BinOp, UnOp};
use crate::types::{CDataLayout, Ty};

/// A C integer type as arithmetic sees it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct IntTy {
    width: u8,
    signed: bool,
    boolean: bool,
}

impl IntTy {
    pub(crate) const BOOL: IntTy = IntTy {
        width: 1,
        signed: false,
        boolean: true,
    };
    pub(crate) const INT: IntTy = IntTy::new(32, true);
    pub(crate) const INTMAX: IntTy = IntTy::new(64, true);
    pub(crate) const UINTMAX: IntTy = IntTy::new(64, false);

    pub(crate) const fn new(width: u32, signed: bool) -> IntTy {
        assert!(width >= 1 && width <= 128);
        IntTy {
            width: width as u8,
            signed,
            boolean: false,
        }
    }

    /// An integer type by its size in `layout`; a pointer is an unsigned
    /// integer of pointer width.
    pub(crate) fn of(ty: &Ty, layout: CDataLayout) -> Option<IntTy> {
        match ty {
            Ty::Int(crate::types::IntKind::Bool) => Some(IntTy::BOOL),
            Ty::Int(k) => Some(IntTy::new(
                k.size_with_layout(layout) as u32 * 8,
                k.signed(),
            )),
            Ty::Ptr(_) => Some(IntTy::new(layout.pointer_bytes() as u32 * 8, false)),
            _ => None,
        }
    }

    /// A bit-field of `width` bits of this type. `_Bool` stays boolean.
    pub(crate) fn bit_field(self, width: u32) -> IntTy {
        if self.boolean {
            self
        } else {
            IntTy::new(width, self.signed)
        }
    }

    pub(crate) fn width(self) -> u32 {
        u32::from(self.width)
    }

    pub(crate) fn signed(self) -> bool {
        self.signed
    }

    fn mask(self) -> u128 {
        u128::MAX >> (128 - self.width())
    }

    pub(crate) fn min(self) -> IntConst {
        if self.signed {
            IntConst::wrap(self, 1 << (self.width() - 1))
        } else {
            IntConst::wrap(self, 0)
        }
    }

    pub(crate) fn max(self) -> IntConst {
        IntConst::wrap(self, self.mask() >> u32::from(self.signed))
    }
}

/// An exact integer value of an `IntTy`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct IntConst {
    bits: u128,
    ty: IntTy,
}

impl IntConst {
    /// `bits` modulo 2^width, as a value of `ty`.
    pub(crate) fn wrap(ty: IntTy, bits: u128) -> IntConst {
        let low = bits & ty.mask();
        let negative = ty.signed && (low >> (ty.width() - 1)) & 1 == 1;
        IntConst {
            bits: if negative { low | !ty.mask() } else { low },
            ty,
        }
    }

    /// Decode the AST's `i64` encoding of a value of `ty`: the bits for
    /// types of 64 bits or less, sign-extended to 128 bits otherwise.
    pub(crate) fn from_literal(ty: IntTy, v: i64) -> IntConst {
        IntConst::wrap(ty, v as i128 as u128)
    }

    /// The AST's `i64` encoding, or `None` for a 128-bit value outside the
    /// `i64` range.
    pub(crate) fn literal(self) -> Option<i64> {
        if self.ty.width() <= 64 {
            Some(self.bits64())
        } else {
            self.to_i64()
        }
    }

    /// The low 64 bits.
    pub(crate) fn bits64(self) -> i64 {
        self.bits as u64 as i64
    }

    pub(crate) fn bits(self) -> u128 {
        self.bits
    }

    pub(crate) fn ty(self) -> IntTy {
        self.ty
    }

    pub(crate) fn is_zero(self) -> bool {
        self.bits == 0
    }

    pub(crate) fn is_negative(self) -> bool {
        self.ty.signed && (self.bits as i128) < 0
    }

    pub(crate) fn to_i128(self) -> Option<i128> {
        if self.ty.signed {
            Some(self.bits as i128)
        } else {
            i128::try_from(self.bits).ok()
        }
    }

    pub(crate) fn to_u128(self) -> Option<u128> {
        (!self.is_negative()).then_some(self.bits)
    }

    pub(crate) fn to_i64(self) -> Option<i64> {
        i64::try_from(self.to_i128()?).ok()
    }

    pub(crate) fn to_u64(self) -> Option<u64> {
        u64::try_from(self.to_u128()?).ok()
    }

    /// Convert to another integer type (C11 6.3.1.2 and 6.3.1.3; signed
    /// targets wrap, as every supported compiler defines).
    pub(crate) fn convert(self, to: IntTy) -> IntConst {
        if to.boolean {
            IntConst::wrap(to, u128::from(!self.is_zero()))
        } else {
            IntConst::wrap(to, self.bits)
        }
    }

    fn signed_value(self) -> i128 {
        self.bits as i128
    }
}

/// Undefined behavior the kernel found.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Ub {
    /// Signed `+ - *`, unary `-`, `MIN / -1` or `MIN % -1` left the range.
    Overflow,
    DivideByZero,
    /// A shift count outside `[0, width)`.
    ShiftCount,
    /// `<<` of a negative value or past the sign bit.
    SignedShift,
    /// A floating value outside the target integer range, or NaN.
    FloatRange,
}

/// Which undefined results a caller accepts, and as what. Division by
/// zero and out-of-range shift counts are never accepted.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Policy {
    /// Signed overflow wraps.
    pub wrap_overflow: bool,
    /// `<<` of a negative value or past the sign bit wraps.
    pub wrap_signed_shift: bool,
    /// A float out of range saturates, and NaN gives 0.
    pub saturate_float: bool,
}

impl Policy {
    /// Static initializers and integer constant expressions: accept the
    /// results GCC and Clang agree on.
    pub(crate) const CONSTANT: Policy = Policy {
        wrap_overflow: true,
        wrap_signed_shift: true,
        saturate_float: true,
    };
    /// Builtin immediates: any undefined behavior is an error.
    pub(crate) const STRICT: Policy = Policy {
        wrap_overflow: false,
        wrap_signed_shift: false,
        saturate_float: false,
    };
    /// Folds that must give what the generated code computes.
    pub(crate) const LOWERING: Policy = Policy {
        wrap_overflow: true,
        wrap_signed_shift: true,
        saturate_float: false,
    };
}

fn accept(ok: bool, wrap: bool, ub: Ub, value: IntConst) -> Result<IntConst, Ub> {
    if ok || wrap { Ok(value) } else { Err(ub) }
}

/// `-a` or `~a` in `a`'s type, or `!a` as an `int`.
pub(crate) fn unary(op: UnOp, a: IntConst, p: Policy) -> Result<IntConst, Ub> {
    let ty = a.ty;
    match op {
        UnOp::Neg => {
            let value = IntConst::wrap(ty, a.bits.wrapping_neg());
            let ok = !ty.signed || a != ty.min();
            accept(ok, p.wrap_overflow, Ub::Overflow, value)
        }
        UnOp::BitNot => Ok(IntConst::wrap(ty, !a.bits)),
        UnOp::Not => Ok(IntConst::wrap(IntTy::INT, u128::from(a.is_zero()))),
    }
}

/// A comparison of two values of one type.
pub(crate) fn compare(op: BinOp, a: IntConst, b: IntConst) -> bool {
    debug_assert_eq!(a.ty, b.ty);
    let ord = if a.ty.signed {
        a.signed_value().cmp(&b.signed_value())
    } else {
        a.bits.cmp(&b.bits)
    };
    match op {
        BinOp::Eq => ord.is_eq(),
        BinOp::Ne => ord.is_ne(),
        BinOp::Lt => ord.is_lt(),
        BinOp::Le => ord.is_le(),
        BinOp::Gt => ord.is_gt(),
        BinOp::Ge => ord.is_ge(),
        _ => unreachable!("{op:?} is not a comparison"),
    }
}

/// `a op b` with both operands, and the result, of one type (a shift
/// count has the left operand's type). A comparison gives an `int`.
pub(crate) fn binary(op: BinOp, a: IntConst, b: IntConst, p: Policy) -> Result<IntConst, Ub> {
    debug_assert_eq!(a.ty, b.ty);
    let ty = a.ty;
    let wrap = |bits: u128| IntConst::wrap(ty, bits);
    // The exact signed result, if it is in range.
    let in_range = |v: Option<i128>| {
        v.is_some_and(|v| ty.min().signed_value() <= v && v <= ty.max().signed_value())
    };
    let (x, y) = (a.signed_value(), b.signed_value());
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => {
            let (exact, bits) = match op {
                BinOp::Add => (x.checked_add(y), a.bits.wrapping_add(b.bits)),
                BinOp::Sub => (x.checked_sub(y), a.bits.wrapping_sub(b.bits)),
                _ => (x.checked_mul(y), a.bits.wrapping_mul(b.bits)),
            };
            let ok = !ty.signed || in_range(exact);
            accept(ok, p.wrap_overflow, Ub::Overflow, wrap(bits))
        }
        BinOp::Div | BinOp::Rem => {
            if b.is_zero() {
                return Err(Ub::DivideByZero);
            }
            let div = op == BinOp::Div;
            if ty.signed {
                if a == ty.min() && y == -1 {
                    let value = if div { a } else { wrap(0) };
                    return accept(false, p.wrap_overflow, Ub::Overflow, value);
                }
                Ok(wrap(if div { x / y } else { x % y } as u128))
            } else {
                Ok(wrap(if div {
                    a.bits / b.bits
                } else {
                    a.bits % b.bits
                }))
            }
        }
        BinOp::Shl | BinOp::Shr => {
            let n = match b.to_u128() {
                Some(n) if n < u128::from(ty.width()) => n as u32,
                _ => return Err(Ub::ShiftCount),
            };
            if op == BinOp::Shr {
                return Ok(wrap(if ty.signed {
                    (x >> n) as u128
                } else {
                    a.bits >> n
                }));
            }
            let value = wrap(a.bits << n);
            let ok = !ty.signed || (!a.is_negative() && a.bits >> (ty.width() - 1 - n) == 0);
            accept(ok, p.wrap_signed_shift, Ub::SignedShift, value)
        }
        BinOp::And => Ok(wrap(a.bits & b.bits)),
        BinOp::Or => Ok(wrap(a.bits | b.bits)),
        BinOp::Xor => Ok(wrap(a.bits ^ b.bits)),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            Ok(IntConst::wrap(IntTy::INT, u128::from(compare(op, a, b))))
        }
    }
}

/// Convert a floating value to an integer type, truncating toward zero.
/// A `float` operand is exact in the `f64`.
pub(crate) fn from_float(f: f64, to: IntTy, p: Policy) -> Result<IntConst, Ub> {
    if to.boolean {
        return Ok(IntConst::wrap(to, u128::from(f != 0.0)));
    }
    let t = f.trunc();
    // The bounds are powers of two, which `f64` holds exactly.
    let top = 2f64.powi(to.width() as i32 - i32::from(to.signed));
    let low = if to.signed { -top } else { 0.0 };
    if t >= low && t < top {
        let bits = if to.signed {
            t as i128 as u128
        } else {
            t as u128
        };
        return Ok(IntConst::wrap(to, bits));
    }
    let saturated = if f.is_nan() {
        IntConst::wrap(to, 0)
    } else if t < low {
        to.min()
    } else {
        to.max()
    };
    accept(false, p.saturate_float, Ub::FloatRange, saturated)
}

/// The nearest `double`, rounding once, ties to even.
pub(crate) fn to_f64(a: IntConst) -> f64 {
    if a.ty.signed {
        a.signed_value() as f64
    } else {
        a.bits as f64
    }
}

/// The nearest `float`, rounding once, ties to even; overflow gives inf.
pub(crate) fn to_f32(a: IntConst) -> f32 {
    if a.ty.signed {
        a.signed_value() as f32
    } else {
        a.bits as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [BinOp; 16] = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::Rem,
        BinOp::Shl,
        BinOp::Shr,
        BinOp::And,
        BinOp::Or,
        BinOp::Xor,
        BinOp::Eq,
        BinOp::Ne,
        BinOp::Lt,
        BinOp::Le,
        BinOp::Gt,
        BinOp::Ge,
    ];

    fn range(ty: IntTy) -> std::ops::RangeInclusive<i64> {
        let w = ty.width();
        if ty.signed {
            -(1 << (w - 1))..=(1 << (w - 1)) - 1
        } else {
            0..=(1 << w) - 1
        }
    }

    fn value(ty: IntTy, v: i64) -> IntConst {
        IntConst::wrap(ty, v as i128 as u128)
    }

    fn math(c: IntConst) -> i64 {
        c.to_i64().unwrap()
    }

    /// The modular value of `v` in `ty`, by `i64` arithmetic.
    fn reduce(ty: IntTy, v: i64) -> i64 {
        let w = ty.width();
        let m = v.rem_euclid(1 << w);
        if ty.signed && m >= 1 << (w - 1) {
            m - (1 << w)
        } else {
            m
        }
    }

    /// The reference result: the exact value and the UB it has, if any.
    fn reference(op: BinOp, ty: IntTy, x: i64, y: i64) -> Result<(i64, Option<Ub>), Ub> {
        let w = i64::from(ty.width());
        let max = *range(ty).end();
        let exact = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div | BinOp::Rem if y == 0 => return Err(Ub::DivideByZero),
            BinOp::Div => x / y,
            BinOp::Rem => {
                if x / y > max {
                    return Ok((0, Some(Ub::Overflow)));
                }
                x % y
            }
            BinOp::Shl | BinOp::Shr if y < 0 || y >= w => return Err(Ub::ShiftCount),
            BinOp::Shl => {
                let r = x << y;
                let ub = (ty.signed && (x < 0 || r > max)).then_some(Ub::SignedShift);
                return Ok((reduce(ty, r), ub));
            }
            BinOp::Shr => x >> y,
            BinOp::And => x & y,
            BinOp::Or => x | y,
            BinOp::Xor => x ^ y,
            _ => {
                let r = match op {
                    BinOp::Eq => x == y,
                    BinOp::Ne => x != y,
                    BinOp::Lt => x < y,
                    BinOp::Le => x <= y,
                    BinOp::Gt => x > y,
                    _ => x >= y,
                };
                return Ok((i64::from(r), None));
            }
        };
        let ub = (ty.signed && !range(ty).contains(&exact)).then_some(Ub::Overflow);
        Ok((reduce(ty, exact), ub))
    }

    fn narrow_types() -> [IntTy; 4] {
        [
            IntTy::new(8, true),
            IntTy::new(8, false),
            IntTy::new(5, true),
            IntTy::new(5, false),
        ]
    }

    #[test]
    fn binary_matches_i64_arithmetic_on_narrow_types() {
        for ty in narrow_types() {
            for x in range(ty) {
                for y in range(ty) {
                    let (a, b) = (value(ty, x), value(ty, y));
                    assert_eq!(math(a), x);
                    for op in ALL {
                        let want = reference(op, ty, x, y);
                        for p in [Policy::CONSTANT, Policy::STRICT, Policy::LOWERING] {
                            let got = binary(op, a, b, p).map(math);
                            let expect = match want {
                                Err(ub) => Err(ub),
                                Ok((v, None)) => Ok(v),
                                Ok((v, Some(Ub::Overflow))) if p.wrap_overflow => Ok(v),
                                Ok((v, Some(Ub::SignedShift))) if p.wrap_signed_shift => Ok(v),
                                Ok((_, Some(ub))) => Err(ub),
                            };
                            assert_eq!(got, expect, "{x} {op:?} {y} in {ty:?} {p:?}");
                        }
                    }
                }
                let a = value(ty, x);
                let neg = unary(UnOp::Neg, a, Policy::STRICT).map(math);
                if !ty.signed() {
                    assert_eq!(neg, Ok(reduce(ty, -x)));
                } else if range(ty).contains(&-x) {
                    assert_eq!(neg, Ok(-x));
                } else {
                    assert_eq!(neg, Err(Ub::Overflow));
                    assert_eq!(math(unary(UnOp::Neg, a, Policy::CONSTANT).unwrap()), x);
                }
                assert_eq!(
                    math(unary(UnOp::BitNot, a, Policy::STRICT).unwrap()),
                    reduce(ty, !x)
                );
                assert_eq!(
                    math(unary(UnOp::Not, a, Policy::STRICT).unwrap()),
                    i64::from(x == 0)
                );
            }
        }
    }

    #[test]
    fn conversions_match_i64_arithmetic_on_narrow_types() {
        for from in narrow_types() {
            for to in narrow_types().into_iter().chain([IntTy::BOOL]) {
                for x in range(from) {
                    let got = value(from, x).convert(to);
                    let want = if to == IntTy::BOOL {
                        i64::from(x != 0)
                    } else {
                        reduce(to, x)
                    };
                    assert_eq!(math(got), want, "{x} {from:?} -> {to:?}");
                    assert_eq!(got.ty(), to);
                }
            }
        }
        for ty in narrow_types() {
            for v in range(ty) {
                for f in [v as f64, v as f64 + 0.75, v as f64 - 0.75] {
                    let t = f.trunc() as i64;
                    let got = from_float(f, ty, Policy::STRICT).map(math);
                    if range(ty).contains(&t) {
                        assert_eq!(got, Ok(t), "{f} {ty:?}");
                    } else {
                        assert_eq!(got, Err(Ub::FloatRange), "{f} {ty:?}");
                    }
                }
                assert_eq!(to_f64(value(ty, v)), v as f64);
            }
        }
    }

    #[test]
    fn wide_boundaries_are_exact() {
        let i128t = IntTy::new(128, true);
        let u128t = IntTy::new(128, false);
        let u64t = IntTy::UINTMAX;
        let u = |v: u128| IntConst::wrap(u128t, v);
        let s = |v: i128| IntConst::wrap(i128t, v as u128);
        let c = Policy::CONSTANT;
        assert_eq!(binary(BinOp::Shl, u(1), u(70), c).unwrap().bits(), 1 << 70);
        assert_eq!(
            binary(BinOp::Div, u(1 << 64), u(3), c).unwrap().bits(),
            (1u128 << 64) / 3
        );
        assert_eq!(
            binary(BinOp::Shl, s(1), s(63), c).unwrap().to_i128(),
            Some(1 << 63)
        );
        assert_eq!(binary(BinOp::Shr, u(7 << 64), u(64), c).unwrap().bits(), 7);
        assert_eq!(binary(BinOp::Shr, u(1), u(128), c), Err(Ub::ShiftCount));
        assert_eq!(
            binary(BinOp::Shl, s(1), s(127), Policy::STRICT),
            Err(Ub::SignedShift)
        );
        assert_eq!(binary(BinOp::Shl, s(1), s(127), c).unwrap(), i128t.min());
        assert_eq!(
            binary(BinOp::Div, i128t.min(), s(-1), Policy::STRICT),
            Err(Ub::Overflow)
        );
        assert_eq!(
            binary(BinOp::Div, i128t.min(), s(-1), c).unwrap(),
            i128t.min()
        );
        assert_eq!(binary(BinOp::Rem, i128t.min(), s(-1), c).unwrap(), s(0));
        assert_eq!(
            binary(BinOp::Add, i128t.max(), s(1), Policy::STRICT),
            Err(Ub::Overflow)
        );
        assert_eq!(
            binary(BinOp::Add, i128t.max(), s(1), c).unwrap(),
            i128t.min()
        );
        assert_eq!(
            binary(BinOp::Sub, u(0), u(1), Policy::STRICT).unwrap(),
            u128t.max()
        );
        assert!(compare(BinOp::Lt, s(-1), s(0)));
        assert!(!compare(BinOp::Lt, u(u128::MAX), u(0)));
        assert!(compare(BinOp::Gt, u(1 << 127), u(1 << 64)));

        // 2^64 - 1 of unsigned long widens without a sign change.
        let m = IntConst::wrap(u64t, u128::from(u64::MAX));
        assert_eq!(m.bits64(), -1);
        assert_eq!(m.convert(i128t).to_i128(), Some(i128::from(u64::MAX)));
        assert_eq!(m.convert(i128t).literal(), None);
        assert_eq!(m.literal(), Some(-1));
        assert_eq!(IntConst::from_literal(u128t, -1), u128t.max());
        assert_eq!(IntConst::from_literal(u64t, -1), m);
        assert_eq!(s(-5).literal(), Some(-5));
        assert_eq!(u128t.max().to_u128(), Some(u128::MAX));
        assert_eq!(u128t.max().to_i128(), None);
        assert_eq!(s(-1).to_u64(), None);
        assert_eq!(u(1 << 64).to_u64(), None);
        for v in [1u128 << 63, (1 << 64) - 1, 1 << 64, 1 << 127] {
            assert_eq!(u(v).to_u128(), Some(v));
            assert_eq!(u(v).convert(i128t).convert(u128t), u(v));
        }
        assert_eq!(IntTy::new(5, true).bit_field(3), IntTy::new(3, true));
        assert_eq!(IntTy::BOOL.bit_field(1), IntTy::BOOL);
    }

    #[test]
    fn wide_float_conversions_round_once() {
        let i128t = IntTy::new(128, true);
        let u128t = IntTy::new(128, false);
        let c = Policy::CONSTANT;
        let s = Policy::STRICT;
        let two = |n: i32| 2f64.powi(n);
        assert_eq!(to_f64(u128t.max()), two(128));
        assert_eq!(to_f32(u128t.max()), f32::INFINITY);
        assert_eq!(to_f64(i128t.min()), -two(127));
        assert_eq!(
            from_float(1e30, u128t, s).unwrap().bits(),
            1_000_000_000_000_000_019_884_624_838_656
        );
        assert_eq!(from_float(two(63), IntTy::INTMAX, s), Err(Ub::FloatRange));
        assert_eq!(
            from_float(two(63), IntTy::INTMAX, c).unwrap(),
            IntTy::INTMAX.max()
        );
        assert_eq!(
            from_float(two(64), IntTy::UINTMAX, c).unwrap(),
            IntTy::UINTMAX.max()
        );
        assert_eq!(
            from_float(two(63), IntTy::UINTMAX, s).unwrap().bits(),
            1 << 63
        );
        assert_eq!(from_float(two(127), u128t, s).unwrap().bits(), 1 << 127);
        assert_eq!(from_float(two(127), i128t, s), Err(Ub::FloatRange));
        assert_eq!(from_float(-two(127), i128t, s).unwrap(), i128t.min());
        assert_eq!(from_float(f64::INFINITY, i128t, c).unwrap(), i128t.max());
        assert_eq!(
            from_float(f64::NEG_INFINITY, u128t, c).unwrap(),
            u128t.min()
        );
        assert_eq!(from_float(f64::NAN, IntTy::INT, c).unwrap().bits(), 0);
        assert_eq!(from_float(f64::NAN, IntTy::INT, s), Err(Ub::FloatRange));
        assert_eq!(from_float(f64::NAN, IntTy::BOOL, s).unwrap().bits(), 1);
        assert_eq!(from_float(1e10, IntTy::INT, c).unwrap(), IntTy::INT.max());
        assert_eq!(
            from_float(-1.0, IntTy::new(32, false), c).unwrap().bits(),
            0
        );
        assert_eq!(
            from_float(-0.5, IntTy::new(32, false), s).unwrap().bits(),
            0
        );
    }
}
