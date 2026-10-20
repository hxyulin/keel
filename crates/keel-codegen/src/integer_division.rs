//! Width-normalized constant division plans shared by the native backends.
use crate::fold::iconst;
use keel_ir::{BinaryOp, Function, InstData, Type};

/// The divisor of an integer division by a constant that is lowered without
/// a divide instruction, extended from the IR width to the register width:
/// not 0 or 1, nor -1 if signed, so no check or trap is needed.
pub(crate) fn reducible_divisor(func: &Function, data: &InstData) -> Option<i64> {
    let InstData::Binary { op, args } = *data else {
        return None;
    };
    let signed = match op {
        BinaryOp::Sdiv | BinaryOp::Srem => true,
        BinaryOp::Udiv | BinaryOp::Urem => false,
        _ => return None,
    };
    let ty = func.value_type(func.resolve(args[0]));
    if !matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64) {
        return None;
    }
    // Iconst's immediate wraps to the IR width, even if it spells a trivial
    // divisor with a noncanonical bit pattern.
    let unused = 64 - ty.bytes()? * 8;
    let bits = (iconst(func, args[1])? as u64) << unused;
    let divisor = if signed {
        bits as i64 >> unused
    } else {
        (bits >> unused) as i64
    };
    let trivial = if signed {
        (-1..=1).contains(&divisor)
    } else {
        divisor as u64 <= 1
    };
    (!trivial).then_some(divisor)
}

/// The `width`-bit multiplier and shift dividing by `divisor`, |d| >= 3 and
/// not a power of two: shift `mulhs(n, m)`, plus n if d > 0 > m or minus n
/// if d < 0 < m, right by `shift`, then add one if negative (Hacker's
/// Delight, figure 10-1). Width is a parameter for exhaustive small-width
/// testing; the arithmetic is exact in u128.
pub(crate) fn signed_reciprocal(width: u32, divisor: i64) -> (u64, u32) {
    let ad = u128::from(divisor.unsigned_abs());
    debug_assert!(ad > 2 && !ad.is_power_of_two() && ad < 1 << (width - 1));
    let two = 1u128 << (width - 1);
    let t = two + u128::from(divisor < 0);
    // The largest dividend magnitude for which the remainder is |d| - 1.
    let anc = t - 1 - t % ad;
    let mut p = width - 1;
    let (mut q1, mut r1) = (two / anc, two % anc);
    let (mut q2, mut r2) = (two / ad, two % ad);
    loop {
        p += 1;
        q1 *= 2;
        r1 *= 2;
        if r1 >= anc {
            q1 += 1;
            r1 -= anc;
        }
        q2 *= 2;
        r2 *= 2;
        if r2 >= ad {
            q2 += 1;
            r2 -= ad;
        }
        let delta = ad - r2;
        if q1 > delta || (q1 == delta && r1 != 0) {
            break;
        }
    }
    let mask = (1u128 << width) - 1;
    let m = q2 + 1;
    let m = if divisor < 0 { m.wrapping_neg() } else { m } & mask;
    (m as u64, p - width)
}

#[derive(Debug)]
pub(crate) struct UnsignedReciprocal {
    pub(crate) multiplier: u64,
    pub(crate) shift: u32,
    pub(crate) add: bool,
}

/// The round-up unsigned reciprocal of `divisor`, > 1 and not a power of two,
/// as in keel-opt's constant division: q = (mulhu(n, m) [averaged with n if
/// `add`]) >> shift.
pub(crate) fn unsigned_reciprocal(width: u32, divisor: u64) -> UnsignedReciprocal {
    let d = u128::from(divisor);
    debug_assert!(d > 1 && !d.is_power_of_two() && d < 1 << width);
    let shift = divisor.ilog2();
    let numerator = 1u128 << (width + shift);
    let (proposed, remainder) = (numerator / d, numerator % d);
    // If the rounding error fits, the w-bit rounded-up reciprocal works;
    // otherwise a w+1-bit one, with its leading bit handled by averaging.
    let add = d - remainder >= (1u128 << shift);
    let multiplier = if add {
        2 * proposed + u128::from(2 * remainder >= d) + 1
    } else {
        proposed + 1
    };
    UnsignedReciprocal {
        multiplier: (multiplier & ((1u128 << width) - 1)) as u64,
        shift,
        add,
    }
}
