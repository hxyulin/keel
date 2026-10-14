//! Constant expressions: integer constants for array sizes, `case` labels
//! and enumerators, and the arithmetic and address constants that
//! initialize objects with static storage.

use crate::ast::{BinOp, Expr, ExprKind, UnOp};
use crate::types::{IntKind, Ty};

/// What an address constant points into.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Base {
    Global(usize),
    Str(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Const {
    /// The bits of an integer (or of a pointer made from one).
    Int(i64),
    Float(f64),
    /// An address plus a byte offset.
    Addr(Base, i64),
}

/// The value of an integer constant expression.
pub fn eval_int(e: &Expr) -> Option<i64> {
    if !e.ty.is_integer() {
        return None;
    }
    match eval(e)? {
        Const::Int(v) => Some(v),
        _ => None,
    }
}

/// `v` as a value of integer type `ty`: truncated and sign- or
/// zero-extended back to 64 bits.
pub fn normalize(v: i64, ty: &Ty) -> i64 {
    match ty {
        Ty::Int(IntKind::Bool) => i64::from(v != 0),
        Ty::Int(k) if k.size() < 8 => {
            let bits = k.size() * 8;
            let shift = 64 - bits;
            if k.signed() {
                (v << shift) >> shift
            } else {
                ((v as u64) << shift >> shift) as i64
            }
        }
        _ => v,
    }
}

fn truthy(c: Const) -> bool {
    match c {
        Const::Int(v) => v != 0,
        Const::Float(f) => f != 0.0,
        Const::Addr(..) => true,
    }
}

pub fn eval(e: &Expr) -> Option<Const> {
    Some(match &e.kind {
        ExprKind::Int(v) => Const::Int(*v),
        ExprKind::Float(f) => Const::Float(*f),
        ExprKind::Addr(inner) => eval_addr(inner)?,
        ExprKind::Cast(inner) => cast(eval(inner)?, &inner.ty, &e.ty)?,
        ExprKind::Unary(op, a) => {
            let a = eval(a)?;
            match (op, a) {
                (UnOp::Not, a) => Const::Int(i64::from(!truthy(a))),
                (UnOp::Neg, Const::Int(v)) => Const::Int(normalize(v.wrapping_neg(), &e.ty)),
                (UnOp::Neg, Const::Float(f)) => Const::Float(-f),
                (UnOp::BitNot, Const::Int(v)) => Const::Int(normalize(!v, &e.ty)),
                _ => return None,
            }
        }
        ExprKind::Binary(op, a, b) => binary(*op, &a.ty, &e.ty, eval(a)?, eval(b)?)?,
        ExprKind::PtrAdd(p, n, size) => {
            let Const::Int(n) = eval(n)? else {
                return None;
            };
            let delta = n.wrapping_mul(*size as i64);
            match eval(p)? {
                Const::Addr(base, off) => Const::Addr(base, off + delta),
                Const::Int(v) => Const::Int(v.wrapping_add(delta)),
                Const::Float(_) => return None,
            }
        }
        ExprKind::PtrDiff(a, b, size) => match (eval(a)?, eval(b)?) {
            (Const::Addr(x, i), Const::Addr(y, j)) if x == y => Const::Int((i - j) / *size as i64),
            (Const::Int(i), Const::Int(j)) => Const::Int(i.wrapping_sub(j) / *size as i64),
            _ => return None,
        },
        ExprKind::LogAnd(a, b) => Const::Int(i64::from(truthy(eval(a)?) && truthy(eval(b)?))),
        ExprKind::LogOr(a, b) => Const::Int(i64::from(truthy(eval(a)?) || truthy(eval(b)?))),
        ExprKind::Cond(c, a, b) => {
            if truthy(eval(c)?) {
                eval(a)?
            } else {
                eval(b)?
            }
        }
        ExprKind::Comma(_, b) => eval(b)?,
        _ => return None,
    })
}

/// The address of an lvalue with static storage.
fn eval_addr(e: &Expr) -> Option<Const> {
    match &e.kind {
        ExprKind::Global(g) => Some(Const::Addr(Base::Global(*g), 0)),
        ExprKind::Str(s) => Some(Const::Addr(Base::Str(*s), 0)),
        ExprKind::Member { base, offset, .. } => match eval_addr(base)? {
            Const::Addr(b, off) => Some(Const::Addr(b, off + *offset as i64)),
            Const::Int(v) => Some(Const::Int(v + *offset as i64)),
            Const::Float(_) => None,
        },
        ExprKind::Deref(p) => match eval(p)? {
            c @ (Const::Addr(..) | Const::Int(_)) => Some(c),
            Const::Float(_) => None,
        },
        _ => None,
    }
}

fn cast(c: Const, from: &Ty, to: &Ty) -> Option<Const> {
    Some(match (c, to) {
        (_, Ty::Int(IntKind::Bool)) => Const::Int(i64::from(truthy(c))),
        (Const::Int(v), Ty::Int(_)) => Const::Int(normalize(v, to)),
        (Const::Float(f), Ty::Int(k)) => {
            let v = if k.signed() {
                f as i64
            } else {
                f as u64 as i64
            };
            Const::Int(normalize(v, to))
        }
        (Const::Addr(..), Ty::Int(k)) if k.size() == 8 => c,
        (Const::Int(v), Ty::Float | Ty::Double) => {
            let f = if from.is_signed() || !from.is_integer() {
                v as f64
            } else {
                v as u64 as f64
            };
            let f = if *to == Ty::Float {
                // Straight to f32, not through f64, to round once.
                f64::from(if from.is_signed() || !from.is_integer() {
                    v as f32
                } else {
                    v as u64 as f32
                })
            } else {
                f
            };
            Const::Float(f)
        }
        (Const::Float(f), Ty::Float) => Const::Float(f64::from(f as f32)),
        (Const::Float(f), Ty::Double) => Const::Float(f),
        (Const::Int(_) | Const::Addr(..), Ty::Ptr(_)) => c,
        _ => return None,
    })
}

fn binary(op: BinOp, operand: &Ty, result: &Ty, a: Const, b: Const) -> Option<Const> {
    match (a, b) {
        (Const::Int(x), Const::Int(y)) => {
            let signed = operand.is_signed();
            let (ux, uy) = (x as u64, y as u64);
            let v = match op {
                BinOp::Add => x.wrapping_add(y),
                BinOp::Sub => x.wrapping_sub(y),
                BinOp::Mul => x.wrapping_mul(y),
                BinOp::Div | BinOp::Rem if y == 0 => return None,
                BinOp::Div if signed => x.wrapping_div(y),
                BinOp::Div => (ux / uy) as i64,
                BinOp::Rem if signed => x.wrapping_rem(y),
                BinOp::Rem => (ux % uy) as i64,
                BinOp::Shl => x.wrapping_shl(y as u32),
                BinOp::Shr if signed => x.wrapping_shr(y as u32),
                BinOp::Shr => ux.wrapping_shr(y as u32) as i64,
                BinOp::And => x & y,
                BinOp::Or => x | y,
                BinOp::Xor => x ^ y,
                BinOp::Eq => i64::from(x == y),
                BinOp::Ne => i64::from(x != y),
                BinOp::Lt if signed => i64::from(x < y),
                BinOp::Le if signed => i64::from(x <= y),
                BinOp::Gt if signed => i64::from(x > y),
                BinOp::Ge if signed => i64::from(x >= y),
                BinOp::Lt => i64::from(ux < uy),
                BinOp::Le => i64::from(ux <= uy),
                BinOp::Gt => i64::from(ux > uy),
                BinOp::Ge => i64::from(ux >= uy),
            };
            Some(Const::Int(normalize(v, result)))
        }
        (Const::Float(x), Const::Float(y)) => {
            let single = *operand == Ty::Float;
            let round = |f: f64| if single { f64::from(f as f32) } else { f };
            Some(match op {
                BinOp::Add => Const::Float(round(x + y)),
                BinOp::Sub => Const::Float(round(x - y)),
                BinOp::Mul => Const::Float(round(x * y)),
                BinOp::Div => Const::Float(round(x / y)),
                BinOp::Eq => Const::Int(i64::from(x == y)),
                BinOp::Ne => Const::Int(i64::from(x != y)),
                BinOp::Lt => Const::Int(i64::from(x < y)),
                BinOp::Le => Const::Int(i64::from(x <= y)),
                BinOp::Gt => Const::Int(i64::from(x > y)),
                BinOp::Ge => Const::Int(i64::from(x >= y)),
                _ => return None,
            })
        }
        _ => None,
    }
}
