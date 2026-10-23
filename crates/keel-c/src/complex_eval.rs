// Constant evaluation adapted from LLVM compiler-rt's mul[sd]c3.c/div[sd]c3.c.
// Part of the LLVM Project, under the Apache License v2.0 with LLVM Exceptions.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
// License: third-party/licenses/LLVM.txt
// Modifications: Rust implementation, shared precision macro, local binary
// exponent/scaling operations, and conservative rejection of nonfinite inputs
// and intermediate range loss. The runtime is supplied by the target, not this
// host implementation. Do not fold exceptional/range-sensitive expressions:
// runtime versions can differ, and Clang's static evaluator also differs from
// its runtime on some such inputs. Returning None diagnoses static initializers.
use crate::ast::BinOp;

macro_rules! evaluator {
    ($name:ident, $real:ty, $bits:ty, $mantissa:expr, $bias:expr) => {
        fn $name(
            op: BinOp,
            a: $real,
            b: Option<$real>,
            c: $real,
            d: Option<$real>,
        ) -> Option<(f64, f64)> {
            // A rounded subnormal is fine for a single scalar operation, but
            // in a sequence it may lose range that a different target runtime
            // version preserves. Fold only normal-or-zero intermediates.
            let checked = |x: $real| (x.is_finite() && (x == 0.0 || x.is_normal())).then_some(x);
            let product = |x: $real, y: $real| {
                let r = x * y;
                if r == 0.0 && x != 0.0 && y != 0.0 {
                    None
                } else {
                    checked(r)
                }
            };
            let quotient = |x: $real, y: $real| {
                let r = x / y;
                if r == 0.0 && x != 0.0 {
                    None
                } else {
                    checked(r)
                }
            };
            if !a.is_finite()
                || !c.is_finite()
                || b.is_some_and(|x| !x.is_finite())
                || d.is_some_and(|x| !x.is_finite())
            {
                return None;
            }
            if let (Some(b), None) = (b, d) {
                let (r, i) = if op == BinOp::Mul {
                    (product(a, c)?, product(b, c)?)
                } else {
                    (quotient(a, c)?, quotient(b, c)?)
                };
                return Some((r as f64, i as f64));
            }
            if op == BinOp::Mul
                && let (None, Some(d)) = (b, d)
            {
                return Some((product(a, c)? as f64, product(a, d)? as f64));
            }
            let b = b.unwrap_or(0.0);
            let d = d.unwrap_or(0.0);
            if op == BinOp::Mul {
                let ac = product(a, c)?;
                let bd = product(b, d)?;
                let ad = product(a, d)?;
                let bc = product(b, c)?;
                return Some((checked(ac - bd)? as f64, checked(ad + bc)? as f64));
            }
            // compiler-rt scales the divisor by its maximum binary exponent.
            // Restrict folding to normal divisors and normal exact scales, so
            // constant evaluation is independent of host scalbn/libm/fenv.
            let max = c.abs().max(d.abs());
            if !max.is_normal() {
                return None;
            }
            let exponent = ((max.to_bits() >> $mantissa) as i32 & (2 * $bias + 1)) - $bias;
            let power = |n: i32| -> Option<$real> {
                if n <= -$bias || n > $bias {
                    return None;
                }
                Some(<$real>::from_bits(((n + $bias) as $bits) << $mantissa))
            };
            let scale = power(-exponent)?;
            let c = product(c, scale)?;
            let d = product(d, scale)?;
            let cc = product(c, c)?;
            let dd = product(d, d)?;
            let denom = checked(cc + dd)?;
            let ac = product(a, c)?;
            let bd = product(b, d)?;
            let bc = product(b, c)?;
            let ad = product(a, d)?;
            let r = product(quotient(checked(ac + bd)?, denom)?, scale)?;
            let i = product(quotient(checked(bc - ad)?, denom)?, scale)?;
            Some((r as f64, i as f64))
        }
    };
}
evaluator!(single, f32, u32, 23, 127);
evaluator!(double, f64, u64, 52, 1023);

pub fn arithmetic(
    op: BinOp,
    float: bool,
    a: f64,
    b: Option<f64>,
    c: f64,
    d: Option<f64>,
) -> Option<(f64, f64)> {
    if float {
        single(
            op,
            a as f32,
            b.map(|x| x as f32),
            c as f32,
            d.map(|x| x as f32),
        )
    } else {
        double(op, a, b, c, d)
    }
}
