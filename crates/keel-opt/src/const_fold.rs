//! Constant folding: an instruction whose operands are all constants is
//! replaced, in place, by an `iconst` of its result.
//!
//! Folding must compute exactly what the machine would: wrapping
//! arithmetic, shift amounts taken modulo the width, `clz 0` = width. An
//! operation that would trap (division by zero, `sdiv MIN, -1`) is left
//! alone, so it still traps at run time.
//!
//! The direct pass visits definitions before uses in reverse postorder.
//! `sccp` also uses this numerical kernel with a data-flow constant lookup.
//!
//! Floats fold with the host's IEEE 754 arithmetic, which rounds like the
//! target's. Which NaN an operation produces is unspecified in the IR, so
//! a folded NaN may differ in payload from the one the machine would make.

use fp::Float;
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, OverflowOp, Type, UnaryOp,
    Value, ValueDef,
};

mod fp;

pub fn run(func: &mut Function) -> bool {
    let analysis = crate::analysis::get(func);
    let domtree = &analysis.dom;
    let mut changed = false;
    for &block in domtree.rpo() {
        for inst in func.block_insts(block).to_vec() {
            if let Some(constants) = fold_overflow(func.inst_data(inst), |v| constant_data(func, v))
            {
                replace_with_constants(func, inst, &constants);
                changed = true;
                continue;
            }
            if let InstData::Select { args } = *func.inst_data(inst) {
                if let Some(c) = constant(func, args[0]) {
                    let chosen = if c.is_zero() { args[2] } else { args[1] };
                    let result = func.first_result(inst);
                    func.replace_all_uses(result, chosen);
                    func.remove_inst(inst);
                    changed = true;
                }
                continue;
            }
            if matches!(
                func.inst_data(inst),
                InstData::Iconst { .. } | InstData::Fconst { .. }
            ) {
                continue;
            }
            if let Some(folded) = fold(func.inst_data(inst), |v| constant_data(func, v)) {
                *func.inst_data_mut(inst) = folded;
                changed = true;
            }
        }
    }
    changed
}

/// Fold both results of checked arithmetic. i128 operands stay unfolded:
/// their full result cannot generally be represented by an i64 immediate.
pub(crate) fn fold_overflow(
    data: &InstData,
    lookup: impl Fn(Value) -> Option<InstData>,
) -> Option<[InstData; 2]> {
    let InstData::Overflow { op, args } = *data else {
        return None;
    };
    let get = |v| match lookup(v)? {
        InstData::Iconst { ty, imm } if ty.is_int() && ty != Type::I128 => {
            Some(Const::new(ty, imm as u64))
        }
        _ => None,
    };
    let (a, b) = (get(args[0])?, get(args[1])?);
    let width = a.width();
    let unsigned_max = (1u128 << width) - 1;
    let (wrapped, overflow) = match op {
        OverflowOp::Uadd | OverflowOp::Umul => {
            let exact = if op == OverflowOp::Uadd {
                u128::from(a.bits) + u128::from(b.bits)
            } else {
                u128::from(a.bits) * u128::from(b.bits)
            };
            (exact as u64, exact > unsigned_max)
        }
        OverflowOp::Usub => (a.bits.wrapping_sub(b.bits), a.bits < b.bits),
        OverflowOp::Sadd | OverflowOp::Ssub | OverflowOp::Smul => {
            let (a, b) = (i128::from(a.signed()), i128::from(b.signed()));
            let exact = match op {
                OverflowOp::Sadd => a + b,
                OverflowOp::Ssub => a - b,
                _ => a * b,
            };
            let low = -(1i128 << (width - 1));
            let high = (1i128 << (width - 1)) - 1;
            (exact as u64, exact < low || exact > high)
        }
    };
    Some([
        InstData::Iconst {
            ty: a.ty,
            imm: Const::new(a.ty, wrapped).signed(),
        },
        InstData::Iconst {
            ty: Type::I8,
            imm: i64::from(overflow),
        },
    ])
}

pub(crate) fn constant_data(func: &Function, value: Value) -> Option<InstData> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(value)) else {
        return None;
    };
    match func.inst_data(inst) {
        data @ (InstData::Iconst { .. } | InstData::Fconst { .. }) => Some(data.clone()),
        _ => None,
    }
}

/// Materialize a multi-result fold without changing any result's type or
/// dropping an independently used overflow flag.
pub(crate) fn replace_with_constants(func: &mut Function, inst: Inst, constants: &[InstData]) {
    let results = func.inst_results(inst).to_vec();
    assert_eq!(results.len(), constants.len());
    for (result, constant) in results.into_iter().zip(constants) {
        let copy = func.insert_inst(InsertPoint::Before(inst), constant.clone());
        func.set_inst_loc(copy, func.inst_loc(inst));
        func.replace_all_uses(result, func.first_result(copy));
    }
    func.remove_inst(inst);
}

/// Evaluate using constants supplied by the data-flow solver. The numerical
/// kernel is shared with the direct constant queries used by CFG cleanup.
pub(crate) fn fold(
    data: &InstData,
    lookup: impl Fn(Value) -> Option<InstData>,
) -> Option<InstData> {
    let int = |v: Value| match lookup(v)? {
        InstData::Iconst { ty, imm } if ty.is_int() && ty != Type::I128 => {
            Some(Const::new(ty, imm as u64))
        }
        _ => None,
    };
    let float = |v: Value| match lookup(v)? {
        InstData::Fconst { ty, bits } => Some(Float { ty, bits }),
        _ => None,
    };
    let folded = match *data {
        InstData::Iconst { .. } | InstData::Fconst { .. } => return Some(data.clone()),
        InstData::Binary { op, args } if op.is_float() => {
            Folded::Float(fp::binary(op, float(args[0])?, float(args[1])?))
        }
        InstData::Binary { op, args } => Folded::Int(binary(op, int(args[0])?, int(args[1])?)?),
        InstData::Fma { args } => {
            Folded::Float(fp::fma(float(args[0])?, float(args[1])?, float(args[2])?))
        }
        InstData::Unary { op, arg } if op.is_float() => Folded::Float(fp::unary(op, float(arg)?)),
        InstData::Unary { op, arg } => Folded::Int(unary(op, int(arg)?)?),
        // An `i128` result does not fit `iconst`'s `i64` immediate.
        InstData::Convert { ty: Type::I128, .. } => return None,
        InstData::Convert { op, ty, arg } => match op {
            ConvertOp::Fpext | ConvertOp::Fptrunc => Folded::Float(fp::convert(ty, float(arg)?)),
            ConvertOp::Fptosi | ConvertOp::Fptoui => Folded::Int(Const::new(
                ty,
                fp::to_int(float(arg)?, op == ConvertOp::Fptosi, ty),
            )),
            ConvertOp::Sitofp => Folded::Float(fp::from_int(int(arg)?.signed().into(), ty)),
            ConvertOp::Uitofp => Folded::Float(fp::from_int(int(arg)?.bits.into(), ty)),
            ConvertOp::Bitcast if ty.is_float() => Folded::Float(Float {
                ty,
                bits: int(arg)?.bits,
            }),
            ConvertOp::Bitcast => Folded::Int(Const::new(ty, float(arg)?.bits)),
            _ => Folded::Int(convert(op, ty, int(arg)?)?),
        },
        InstData::Icmp { cond, args } => Folded::Int(Const {
            ty: Type::I8,
            bits: icmp(cond, int(args[0])?, int(args[1])?) as u64,
        }),
        InstData::Fcmp { cond, args } => Folded::Int(Const {
            ty: Type::I8,
            bits: fp::compare(cond, float(args[0])?, float(args[1])?) as u64,
        }),
        _ => return None,
    };
    Some(match folded {
        Folded::Int(c) => InstData::Iconst {
            ty: c.ty,
            imm: c.signed(),
        },
        Folded::Float(x) => InstData::Fconst {
            ty: x.ty,
            bits: x.bits,
        },
    })
}

enum Folded {
    Int(Const),
    Float(Float),
}

/// An integer constant: its type and its bits, zero-extended from the width.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Const {
    ty: Type,
    bits: u64,
}

impl Const {
    fn new(ty: Type, bits: u64) -> Self {
        Self {
            ty,
            bits: bits & mask(ty),
        }
    }

    pub(crate) fn is_zero(self) -> bool {
        self.bits == 0
    }

    /// Whether `value`, taken as bits of this constant's type, equals it.
    pub(crate) fn same_bits(self, value: i64) -> bool {
        self.bits == value as u64 & mask(self.ty)
    }

    fn width(self) -> u32 {
        width(self.ty)
    }

    /// The value as a signed number, which is how `iconst` stores it.
    fn signed(self) -> i64 {
        let shift = 64 - self.width();
        ((self.bits << shift) as i64) >> shift
    }
}

fn width(ty: Type) -> u32 {
    8 * ty.bytes().expect("integer type")
}

fn mask(ty: Type) -> u64 {
    u64::MAX >> (64 - width(ty))
}

/// The value of `v` if it is an `iconst` of i8 to i64 (i128 does not fit
/// the `i64` immediate's arithmetic here).
pub(crate) fn constant(func: &Function, v: Value) -> Option<Const> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(v)) else {
        return None;
    };
    match *func.inst_data(inst) {
        InstData::Iconst { ty, imm } if ty.is_int() && ty != Type::I128 => {
            Some(Const::new(ty, imm as u64))
        }
        _ => None,
    }
}

fn binary(op: BinaryOp, a: Const, b: Const) -> Option<Const> {
    let ty = a.ty;
    let w = a.width();
    let (x, y) = (a.bits, b.bits);
    let (sx, sy) = (a.signed(), b.signed());
    // Shift and rotate amounts may have any integer type.
    let amount = (b.bits % u64::from(w)) as u32;
    let bits = match op {
        BinaryOp::Iadd => x.wrapping_add(y),
        BinaryOp::Isub => x.wrapping_sub(y),
        BinaryOp::Imul => x.wrapping_mul(y),
        BinaryOp::And => x & y,
        BinaryOp::Or => x | y,
        BinaryOp::Xor => x ^ y,
        BinaryOp::Udiv | BinaryOp::Urem if y == 0 => return None,
        BinaryOp::Udiv => x / y,
        BinaryOp::Urem => x % y,
        BinaryOp::Sdiv | BinaryOp::Srem if y == 0 => return None,
        BinaryOp::Sdiv if sx == min(ty) && sy == -1 => return None,
        // In i64, a narrower type's MIN / -1 cannot overflow; `wrapping`
        // covers i64's own `srem MIN, -1`, which is 0.
        BinaryOp::Sdiv => sx.wrapping_div(sy) as u64,
        BinaryOp::Srem => sx.wrapping_rem(sy) as u64,
        BinaryOp::Shl => x << amount,
        BinaryOp::Ushr => x >> amount,
        BinaryOp::Sshr => (sx >> amount) as u64,
        BinaryOp::Rotl => rotate_left(x, amount, w),
        BinaryOp::Rotr => rotate_left(x, (w - amount) % w, w),
        BinaryOp::Umulhi => ((u128::from(x) * u128::from(y)) >> w) as u64,
        BinaryOp::Smulhi => ((i128::from(sx) * i128::from(sy)) >> w) as u64,
        BinaryOp::Ptradd => return None,
        _ => unreachable!("float operations fold in fp"),
    };
    Some(Const::new(ty, bits))
}

fn rotate_left(x: u64, n: u32, w: u32) -> u64 {
    if n == 0 { x } else { (x << n) | (x >> (w - n)) }
}

fn min(ty: Type) -> i64 {
    i64::MIN >> (64 - width(ty))
}

fn unary(op: UnaryOp, a: Const) -> Option<Const> {
    let w = a.width();
    let x = a.bits;
    let bits = match op {
        UnaryOp::Ineg => x.wrapping_neg(),
        UnaryOp::Bnot => !x,
        UnaryOp::Clz => u64::from(x.leading_zeros() - (64 - w)),
        UnaryOp::Ctz => u64::from(x.trailing_zeros().min(w)),
        UnaryOp::Popcnt => u64::from(x.count_ones()),
        UnaryOp::Bswap => x.swap_bytes() >> (64 - w),
        _ => unreachable!("float operations fold in fp"),
    };
    Some(Const::new(a.ty, bits))
}

fn convert(op: ConvertOp, to: Type, a: Const) -> Option<Const> {
    let bits = match op {
        ConvertOp::Sext => a.signed() as u64,
        ConvertOp::Uext | ConvertOp::Trunc => a.bits,
        _ => return None,
    };
    to.is_int().then(|| Const::new(to, bits))
}

fn icmp(cond: IntCC, a: Const, b: Const) -> bool {
    let (x, y) = (a.bits, b.bits);
    let (sx, sy) = (a.signed(), b.signed());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(ty: Type, v: i64) -> Const {
        Const::new(ty, v as u64)
    }

    #[test]
    fn wraps_and_masks() {
        let r = binary(BinaryOp::Iadd, c(Type::I8, 127), c(Type::I8, 1)).unwrap();
        assert_eq!(r.signed(), -128);
        let r = binary(BinaryOp::Shl, c(Type::I32, 1), c(Type::I64, 33)).unwrap();
        assert_eq!(r.signed(), 2, "shift amount is taken mod 32");
        let r = binary(BinaryOp::Sshr, c(Type::I16, -16), c(Type::I8, 2)).unwrap();
        assert_eq!(r.signed(), -4);
        let r = binary(BinaryOp::Ushr, c(Type::I16, -16), c(Type::I8, 2)).unwrap();
        assert_eq!(r.signed(), 0x3ffc);
    }

    #[test]
    fn rotates() {
        let r = binary(BinaryOp::Rotl, c(Type::I8, 0x81), c(Type::I8, 1)).unwrap();
        assert_eq!(r.bits, 0x03);
        let r = binary(BinaryOp::Rotr, c(Type::I8, 0x81), c(Type::I8, 1)).unwrap();
        assert_eq!(r.bits, 0xc0);
        let r = binary(BinaryOp::Rotr, c(Type::I64, 5), c(Type::I64, 64)).unwrap();
        assert_eq!(r.bits, 5);
    }

    #[test]
    fn division_traps_are_kept() {
        assert_eq!(
            binary(BinaryOp::Udiv, c(Type::I32, 1), c(Type::I32, 0)),
            None
        );
        assert_eq!(
            binary(BinaryOp::Sdiv, c(Type::I8, -128), c(Type::I8, -1)),
            None
        );
        let r = binary(BinaryOp::Srem, c(Type::I64, i64::MIN), c(Type::I64, -1)).unwrap();
        assert_eq!(r.bits, 0);
        let r = binary(BinaryOp::Sdiv, c(Type::I32, -7), c(Type::I32, 2)).unwrap();
        assert_eq!(r.signed(), -3, "rounds toward zero");
        let r = binary(BinaryOp::Udiv, c(Type::I8, -1), c(Type::I8, 2)).unwrap();
        assert_eq!(r.bits, 127, "unsigned division sees 255");
    }

    #[test]
    fn bit_counting_and_high_multiply() {
        assert_eq!(unary(UnaryOp::Clz, c(Type::I16, 0)).unwrap().bits, 16);
        assert_eq!(unary(UnaryOp::Ctz, c(Type::I8, 0)).unwrap().bits, 8);
        assert_eq!(unary(UnaryOp::Clz, c(Type::I8, 1)).unwrap().bits, 7);
        assert_eq!(unary(UnaryOp::Popcnt, c(Type::I8, -1)).unwrap().bits, 8);
        assert_eq!(
            unary(UnaryOp::Bswap, c(Type::I16, 0x1234)).unwrap().bits,
            0x3412
        );
        let r = binary(BinaryOp::Smulhi, c(Type::I32, -1), c(Type::I32, 1)).unwrap();
        assert_eq!(r.signed(), -1);
        let r = binary(BinaryOp::Umulhi, c(Type::I32, -1), c(Type::I32, 2)).unwrap();
        assert_eq!(r.signed(), 1);
    }

    #[test]
    fn conversions_and_compares() {
        assert_eq!(
            convert(ConvertOp::Sext, Type::I32, c(Type::I8, -1))
                .unwrap()
                .signed(),
            -1
        );
        assert_eq!(
            convert(ConvertOp::Uext, Type::I32, c(Type::I8, -1))
                .unwrap()
                .signed(),
            255
        );
        assert_eq!(
            convert(ConvertOp::Trunc, Type::I8, c(Type::I32, 0x1ff))
                .unwrap()
                .signed(),
            -1
        );
        assert!(icmp(IntCC::Ult, c(Type::I8, 1), c(Type::I8, -1)));
        assert!(!icmp(IntCC::Slt, c(Type::I8, 1), c(Type::I8, -1)));
    }
    #[test]
    fn checked_arithmetic_matches_rust_for_every_pair_of_i8_values() {
        use keel_ir::EntityRef;
        let (a, b) = (Value::new(0), Value::new(1));
        for op in [
            OverflowOp::Uadd,
            OverflowOp::Usub,
            OverflowOp::Umul,
            OverflowOp::Sadd,
            OverflowOp::Ssub,
            OverflowOp::Smul,
        ] {
            for x in 0u16..256 {
                for y in 0u16..256 {
                    let (result, overflow) = match op {
                        OverflowOp::Uadd => (x as u8).overflowing_add(y as u8),
                        OverflowOp::Usub => (x as u8).overflowing_sub(y as u8),
                        OverflowOp::Umul => (x as u8).overflowing_mul(y as u8),
                        _ => {
                            let (r, o) = match op {
                                OverflowOp::Sadd => (x as i8).overflowing_add(y as i8),
                                OverflowOp::Ssub => (x as i8).overflowing_sub(y as i8),
                                _ => (x as i8).overflowing_mul(y as i8),
                            };
                            (r as u8, o)
                        }
                    };
                    let folded = fold_overflow(&InstData::Overflow { op, args: [a, b] }, |v| {
                        Some(InstData::Iconst {
                            ty: Type::I8,
                            imm: i64::from(if v == a { x } else { y }),
                        })
                    })
                    .unwrap();
                    assert_eq!(
                        folded,
                        [
                            InstData::Iconst {
                                ty: Type::I8,
                                imm: i64::from(result as i8)
                            },
                            InstData::Iconst {
                                ty: Type::I8,
                                imm: i64::from(overflow)
                            }
                        ],
                        "{op:?}, {x}, {y}"
                    );
                }
            }
        }
    }
}
