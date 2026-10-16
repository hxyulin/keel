//! Unsigned reciprocal division, using the round-up algorithm also described in
//! https://ridiculousfish.com/blog/posts/labor-of-division-episode-i.html and
//! libdivide's unsigned generator. No target operations beyond existing IR.

use keel_ir::{BinaryOp, Function, InsertPoint, Inst, InstData, Type, Value};

#[derive(Debug)]
struct Reciprocal {
    multiplier: u64,
    shift: u32,
    add: bool,
}

impl Reciprocal {
    // Width is parameterized for exhaustive small-width testing; only 32/64
    // are emitted. All generator arithmetic is exact in u128 (including 2*r).
    fn new(width: u32, divisor: u64) -> Self {
        assert!((2..=64).contains(&width));
        assert!(divisor > 1 && !divisor.is_power_of_two());
        assert!(u128::from(divisor) < (1u128 << width));
        let d = u128::from(divisor);
        let shift = divisor.ilog2();
        let numerator = 1u128 << (width + shift);
        let proposed = numerator / d;
        let remainder = numerator % d;
        // If the rounding error fits, the w-bit rounded-up reciprocal works.
        // Otherwise use a w+1-bit reciprocal and account for its leading bit
        // with the overflow-safe average below, rather than adding n to hi.
        let add = d - remainder >= (1u128 << shift);
        let multiplier = if add {
            2 * proposed + u128::from(2 * remainder >= d) + 1
        } else {
            proposed + 1
        };
        Self {
            multiplier: (multiplier & ((1u128 << width) - 1)) as u64,
            shift,
            add,
        }
    }
}

pub(super) fn reduce(
    func: &mut Function,
    inst: Inst,
    op: BinaryOp,
    dividend: Value,
    divisor_value: Value,
    ty: Type,
    divisor: u128,
) -> Option<InstData> {
    if !matches!(ty, Type::I32 | Type::I64) || divisor <= 1 || divisor.is_power_of_two() {
        return None;
    }
    let reciprocal = Reciprocal::new(ty.bytes().unwrap() * 8, divisor as u64);
    let mut emit = |data| {
        let inserted = func.insert_inst(InsertPoint::Before(inst), data);
        func.set_inst_loc(inserted, func.inst_loc(inst));
        func.first_result(inserted)
    };
    // Iconst's signed immediate carries the raw bit pattern, including bit 63.
    let multiplier = emit(InstData::Iconst {
        ty,
        imm: reciprocal.multiplier as i64,
    });
    let mut quotient = emit(InstData::Binary {
        op: BinaryOp::Umulhi,
        args: [dividend, multiplier],
    });
    if reciprocal.add {
        let difference = emit(InstData::Binary {
            op: BinaryOp::Isub,
            args: [dividend, quotient],
        });
        let one = emit(InstData::Iconst {
            ty: Type::I64,
            imm: 1,
        });
        let half = emit(InstData::Binary {
            op: BinaryOp::Ushr,
            args: [difference, one],
        });
        // hi <= dividend, and hi + (dividend-hi)/2 <= dividend: no overflow.
        quotient = emit(InstData::Binary {
            op: BinaryOp::Iadd,
            args: [quotient, half],
        });
    }
    let shift = emit(InstData::Iconst {
        ty: Type::I64,
        imm: i64::from(reciprocal.shift),
    });
    let quotient_data = InstData::Binary {
        op: BinaryOp::Ushr,
        args: [quotient, shift],
    };
    if op == BinaryOp::Udiv {
        return Some(quotient_data);
    }
    let quotient = emit(quotient_data);
    let product = emit(InstData::Binary {
        op: BinaryOp::Imul,
        args: [quotient, divisor_value],
    });
    Some(InstData::Binary {
        op: BinaryOp::Isub,
        args: [dividend, product],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quotient(width: u32, n: u64, r: &Reciprocal) -> u64 {
        let hi = ((u128::from(n) * u128::from(r.multiplier)) >> width) as u64;
        let q = if r.add { hi + ((n - hi) >> 1) } else { hi };
        q >> r.shift
    }

    #[test]
    fn exhaustive_small_width_oracle() {
        for width in 2..=10 {
            for d in 3..(1u64 << width) {
                if d.is_power_of_two() {
                    continue;
                }
                let r = Reciprocal::new(width, d);
                for n in 0..(1u64 << width) {
                    let q = quotient(width, n, &r);
                    assert_eq!(q, n / d, "w={width}, n={n}, d={d}, {r:?}");
                    assert_eq!(n - q * d, n % d);
                }
            }
        }
    }

    #[test]
    fn wide_random_and_quotient_boundaries() {
        let mut random = 0x81a3_3456_789a_bcdeu64;
        for width in [32, 64] {
            let mask = u64::MAX >> (64 - width);
            for i in 0..20_000 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let d = match i {
                    0..=255 => i + 2,
                    256 => (mask >> 1) + 2,
                    257 => mask,
                    _ => random & mask,
                };
                if d <= 1 || d.is_power_of_two() {
                    continue;
                }
                let r = Reciprocal::new(width, d);
                let boundary = (mask / d) * d;
                for n in [
                    0,
                    1,
                    d - 1,
                    d,
                    d.wrapping_add(1) & mask,
                    boundary - 1,
                    boundary,
                    mask,
                    random & mask,
                ] {
                    let q = quotient(width, n, &r);
                    assert_eq!(
                        u128::from(q),
                        u128::from(n) / u128::from(d),
                        "w={width}, n={n}, d={d}"
                    );
                    assert_eq!(
                        u128::from(n) - u128::from(q) * u128::from(d),
                        u128::from(n) % u128::from(d)
                    );
                }
            }
        }
    }
}
