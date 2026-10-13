//! Constant folding: an instruction whose operands are all constants is
//! replaced, in place, by an `iconst` of its result.
//!
//! Folding must compute exactly what the machine would: wrapping
//! arithmetic, shift amounts taken modulo the width, `clz 0` = width. An
//! operation that would trap (division by zero, `sdiv MIN, -1`) is left
//! alone, so it still traps at run time.
//!
//! Blocks are visited in reverse postorder and definitions dominate their
//! uses, so a chain `v1 = iadd v0, v0; v2 = imul v1, v1` folds in one pass.
//! A `select` with a constant condition is replaced by the chosen arm,
//! constant or not.
//!
//! Floats fold with the host's IEEE 754 arithmetic, which rounds like the
//! target's. Which NaN an operation produces is unspecified in the IR, so
//! a folded NaN may differ in payload from the one the machine would make.

use fp::Float;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{BinaryOp, ConvertOp, Function, InstData, IntCC, Type, UnaryOp, Value, ValueDef};

mod fp;

pub fn run(func: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(func);
    let domtree = DominatorTree::compute(func, &cfg);
    let mut changed = false;
    for &block in domtree.rpo() {
        for inst in func.block_insts(block).to_vec() {
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
            if let Some(folded) = fold(func, func.inst_data(inst)) {
                *func.inst_data_mut(inst) = folded;
                changed = true;
            }
        }
    }
    changed
}

/// The constant instruction `data` computes, if its operands are constants.
fn fold(func: &Function, data: &InstData) -> Option<InstData> {
    let int = |v: Value| constant(func, v);
    let float = |v: Value| fp::constant(func, v);
    let folded = match *data {
        InstData::Binary { op, args } if op.is_float() => {
            Folded::Float(fp::binary(op, float(args[0])?, float(args[1])?))
        }
        InstData::Binary { op, args } => Folded::Int(binary(op, int(args[0])?, int(args[1])?)?),
        InstData::Unary { op, arg } if op.is_float() => Folded::Float(fp::unary(op, float(arg)?)),
        InstData::Unary { op, arg } => Folded::Int(unary(op, int(arg)?)?),
        InstData::Convert { op, ty, arg } => match op {
            ConvertOp::Fpext | ConvertOp::Fptrunc => Folded::Float(fp::convert(ty, float(arg)?)),
            ConvertOp::Fptosi | ConvertOp::Fptoui => {
                let n = fp::to_int(float(arg)?, op == ConvertOp::Fptosi, ty);
                Folded::Int(Const::new(ty, n))
            }
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
}
