//! Constant expressions: integer constants for array sizes, `case` labels
//! and enumerators, and the arithmetic and address constants that
//! initialize objects with static storage.

use crate::ast::{BinOp, Builtin, Expr, ExprKind, UnOp};
use crate::types::{CDataLayout, IntKind, Ty};

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
    Complex(f64, f64),
    /// An address plus a byte offset.
    Addr(Base, i64),
}

// Native compatibility boundaries for lowering and proof callers. Recursive
// model-aware evaluation must never call these wrappers.
pub fn eval_int(e: &Expr<'_>) -> Option<i64> {
    eval_int_with_layout(e, CDataLayout::NATIVE_LP64)
}
pub fn normalize(v: i64, ty: &Ty) -> i64 {
    normalize_with_layout(v, ty, CDataLayout::NATIVE_LP64)
}
pub fn eval_constant(e: &Expr<'_>, allow_comma: bool) -> Option<Const> {
    eval_constant_with_layout(e, allow_comma, CDataLayout::NATIVE_LP64)
}
pub fn eval(e: &Expr<'_>) -> Option<Const> {
    eval_with_layout(e, CDataLayout::NATIVE_LP64)
}
pub fn eval_with(e: &Expr<'_>, nonnull: &dyn Fn(usize) -> bool) -> Option<Const> {
    eval_with_binding_and_layout(e, nonnull, CDataLayout::NATIVE_LP64)
}

/// The value of an integer constant expression.
pub fn eval_int_with_layout(e: &Expr<'_>, layout: CDataLayout) -> Option<i64> {
    if !e.ty.is_integer() {
        return None;
    }
    match eval_with_layout(e, layout)? {
        Const::Int(v) => Some(v),
        _ => None,
    }
}

/// Whether `v`, of integer type `from`, converted to integer type `to`
/// is still an `i64` constant, whose bits a 128-bit type gets sign
/// extended. Not an unsigned 64-bit value with the top bit set: as an
/// `__int128` it is above `i64::MAX`.
pub fn fits_i64_with_layout(v: i64, from: &Ty, to: &Ty, layout: CDataLayout) -> bool {
    let wide = to
        .int_kind()
        .is_some_and(|k| k.size_with_layout(layout) == 16);
    let unsigned64 = from
        .int_kind()
        .is_some_and(|k| k.size_with_layout(layout) == 8 && !k.signed());
    !(wide && unsigned64 && v < 0)
}

/// Truncate/sign-extend integer values, or keep numeric pointer address bits
/// as an unsigned value of the model's pointer width.
pub fn normalize_with_layout(v: i64, ty: &Ty, layout: CDataLayout) -> i64 {
    match ty {
        Ty::Int(IntKind::Bool) => i64::from(v != 0),
        Ty::Int(k) if k.size_with_layout(layout) < 8 => {
            let bits = k.size_with_layout(layout) * 8;
            let shift = 64 - bits;
            if k.signed() {
                (v << shift) >> shift
            } else {
                ((v as u64) << shift >> shift) as i64
            }
        }
        Ty::Ptr(_) if layout.pointer_bytes() == 4 => v as u32 as i64,
        _ => v,
    }
}

fn truthy(c: Const, nonnull: &dyn Fn(usize) -> bool) -> Option<bool> {
    match c {
        Const::Int(v) => Some(v != 0),
        Const::Float(f) => Some(f != 0.0),
        Const::Complex(r, i) => Some(r != 0.0 || i != 0.0),
        // Before final binding is known, a later weak redeclaration may
        // make a global address null.
        Const::Addr(Base::Global(g), _) => nonnull(g).then_some(true),
        Const::Addr(Base::Str(_), _) => Some(true),
    }
}

/// Evaluate an expression only when its evaluated operands are constant.
/// `eval_with_layout` alone may ignore the left side of a comma; hint arguments must
/// not turn an expression with runtime side effects into a constant.
/// Integer alignment expressions exclude commas, while GNU constant
/// floating expressions for probabilities may include constant commas.
pub fn eval_constant_with_layout(
    e: &Expr<'_>,
    allow_comma: bool,
    layout: CDataLayout,
) -> Option<Const> {
    eval_constant_with(e, allow_comma, &|_| false, layout)
}

fn eval_constant_with(
    e: &Expr<'_>,
    allow_comma: bool,
    nonnull: &dyn Fn(usize) -> bool,
    layout: CDataLayout,
) -> Option<Const> {
    match &e.kind {
        ExprKind::Cast(a) | ExprKind::Unary(_, a) => {
            eval_constant_with(a, allow_comma, nonnull, layout)?;
        }
        ExprKind::Binary(_, a, b) | ExprKind::PtrAdd(a, b, _) | ExprKind::PtrDiff(a, b, _) => {
            eval_constant_with(a, allow_comma, nonnull, layout)?;
            eval_constant_with(b, allow_comma, nonnull, layout)?;
        }
        ExprKind::Comma(a, b) => {
            if !allow_comma {
                return None;
            }
            eval_constant_with(a, allow_comma, nonnull, layout)?;
            eval_constant_with(b, allow_comma, nonnull, layout)?;
        }
        ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
            let first = truthy(
                eval_constant_with(a, allow_comma, nonnull, layout)?,
                nonnull,
            )?;
            if first == matches!(e.kind, ExprKind::LogAnd(..)) {
                eval_constant_with(b, allow_comma, nonnull, layout)?;
            }
        }
        ExprKind::Cond(c, a, b) => {
            let selected = if truthy(
                eval_constant_with(c, allow_comma, nonnull, layout)?,
                nonnull,
            )? {
                a
            } else {
                b
            };
            eval_constant_with(selected, allow_comma, nonnull, layout)?;
        }
        _ => {}
    }
    eval_with_binding_and_layout(e, nonnull, layout)
}

pub fn eval_with_layout(e: &Expr<'_>, layout: CDataLayout) -> Option<Const> {
    eval_with_binding_and_layout(e, &|_| false, layout)
}

/// Evaluate after declaration merging, using final binding for address truth.
/// Parser-time evaluation must keep using `eval_with_layout`, since binding can change.
pub fn eval_with_binding_and_layout(
    e: &Expr<'_>,
    nonnull: &dyn Fn(usize) -> bool,
    layout: CDataLayout,
) -> Option<Const> {
    let value = match &e.kind {
        ExprKind::Int(v) => Const::Int(*v),
        ExprKind::Float(f) => Const::Float(*f),
        ExprKind::Builtin(Builtin::Complex, args) => {
            let (Const::Float(r), Const::Float(i)) = (
                eval_with_binding_and_layout(&args[0], nonnull, layout)?,
                eval_with_binding_and_layout(&args[1], nonnull, layout)?,
            ) else {
                return None;
            };
            Const::Complex(r, i)
        }
        ExprKind::Member { base, offset, .. } if base.ty.is_complex() => {
            let Const::Complex(r, i) = eval_with_binding_and_layout(base, nonnull, layout)? else {
                return None;
            };
            Const::Float(if *offset == 0 { r } else { i })
        }
        ExprKind::Builtin(Builtin::Hint | Builtin::AssumeAligned(_), args) => {
            let value = eval_constant_with(&args[0], true, nonnull, layout)?;
            for arg in &args[1..] {
                eval_constant_with(arg, true, nonnull, layout)?;
            }
            value
        }
        ExprKind::LabelAddr(n) => Const::Int(*n as i64),
        ExprKind::Addr(inner) => eval_addr(inner, nonnull, layout)?,
        ExprKind::Cast(inner) => cast(
            eval_with_binding_and_layout(inner, nonnull, layout)?,
            &inner.ty,
            &e.ty,
            nonnull,
            layout,
        )?,
        ExprKind::Unary(op, a) => {
            let a = eval_with_binding_and_layout(a, nonnull, layout)?;
            match (op, a) {
                (UnOp::Not, a) => Const::Int(i64::from(!truthy(a, nonnull)?)),
                (UnOp::Neg, Const::Int(v)) => {
                    Const::Int(normalize_with_layout(v.wrapping_neg(), &e.ty, layout))
                }
                (UnOp::Neg, Const::Float(f)) => Const::Float(-f),
                (UnOp::Neg, Const::Complex(r, i)) => Const::Complex(-r, -i),
                (UnOp::BitNot, Const::Int(v)) => {
                    Const::Int(normalize_with_layout(!v, &e.ty, layout))
                }
                _ => return None,
            }
        }
        ExprKind::Binary(op, a, b) if a.ty.is_complex() || b.ty.is_complex() => {
            let (x, y) = (
                eval_with_binding_and_layout(a, nonnull, layout)?,
                eval_with_binding_and_layout(b, nonnull, layout)?,
            );
            if matches!(op, BinOp::Eq | BinOp::Ne) {
                let (Const::Complex(xr, xi), Const::Complex(yr, yi)) = (x, y) else {
                    return None;
                };
                let eq = xr == yr && xi == yi;
                Const::Int(i64::from(if *op == BinOp::Eq { eq } else { !eq }))
            } else {
                let (xr, xi) = match x {
                    Const::Complex(r, i) => (r, Some(i)),
                    Const::Float(r) => (r, None),
                    _ => return None,
                };
                let (yr, yi) = match y {
                    Const::Complex(r, i) => (r, Some(i)),
                    Const::Float(r) => (r, None),
                    _ => return None,
                };
                if matches!(op, BinOp::Mul | BinOp::Div) {
                    let Ty::Complex(elem) = e.ty else { return None };
                    let (r, i) =
                        crate::complex_eval::arithmetic(*op, *elem == Ty::Float, xr, xi, yr, yi)?;
                    return Some(Const::Complex(r, i));
                }
                let r = match op {
                    BinOp::Add => xr + yr,
                    BinOp::Sub => xr - yr,
                    _ => return None,
                };
                let i = match (xi, yi) {
                    (Some(x), Some(y)) => {
                        if *op == BinOp::Add {
                            x + y
                        } else {
                            x - y
                        }
                    }
                    (Some(x), None) => x,
                    (None, Some(y)) => {
                        if *op == BinOp::Add {
                            y
                        } else {
                            -y
                        }
                    }
                    _ => return None,
                };
                let Ty::Complex(elem) = e.ty else { return None };
                let round = |v: f64| {
                    if *elem == Ty::Float {
                        f64::from(v as f32)
                    } else {
                        v
                    }
                };
                Const::Complex(round(r), round(i))
            }
        }
        ExprKind::Binary(op, a, b) => binary(
            *op,
            &a.ty,
            &e.ty,
            eval_with_binding_and_layout(a, nonnull, layout)?,
            eval_with_binding_and_layout(b, nonnull, layout)?,
            layout,
        )?,
        ExprKind::PtrAdd(p, n, size) => {
            let Const::Int(n) = eval_with_binding_and_layout(n, nonnull, layout)? else {
                return None;
            };
            let delta = n.wrapping_mul(*size as i64);
            match eval_with_binding_and_layout(p, nonnull, layout)? {
                Const::Addr(base, off) => Const::Addr(base, off + delta),
                Const::Int(v) => Const::Int(v.wrapping_add(delta)),
                Const::Float(_) | Const::Complex(..) => return None,
            }
        }
        ExprKind::PtrDiff(a, b, size) => match (
            eval_with_binding_and_layout(a, nonnull, layout)?,
            eval_with_binding_and_layout(b, nonnull, layout)?,
        ) {
            (Const::Addr(x, i), Const::Addr(y, j)) if x == y => Const::Int((i - j) / *size as i64),
            (Const::Int(i), Const::Int(j)) => Const::Int(i.wrapping_sub(j) / *size as i64),
            _ => return None,
        },
        ExprKind::LogAnd(a, b) => Const::Int(i64::from(
            truthy(eval_with_binding_and_layout(a, nonnull, layout)?, nonnull)?
                && truthy(eval_with_binding_and_layout(b, nonnull, layout)?, nonnull)?,
        )),
        ExprKind::LogOr(a, b) => Const::Int(i64::from(
            truthy(eval_with_binding_and_layout(a, nonnull, layout)?, nonnull)?
                || truthy(eval_with_binding_and_layout(b, nonnull, layout)?, nonnull)?,
        )),
        ExprKind::Cond(c, a, b) => {
            if truthy(eval_with_binding_and_layout(c, nonnull, layout)?, nonnull)? {
                eval_with_binding_and_layout(a, nonnull, layout)?
            } else {
                eval_with_binding_and_layout(b, nonnull, layout)?
            }
        }
        ExprKind::Comma(_, b) => eval_with_binding_and_layout(b, nonnull, layout)?,
        _ => return None,
    };
    Some(match value {
        Const::Int(v) => Const::Int(normalize_with_layout(v, &e.ty, layout)),
        other => other,
    })
}

/// The address of an lvalue with static storage.
fn eval_addr(e: &Expr<'_>, nonnull: &dyn Fn(usize) -> bool, layout: CDataLayout) -> Option<Const> {
    match &e.kind {
        ExprKind::Global(g) => Some(Const::Addr(Base::Global(*g), 0)),
        ExprKind::Str(s) => Some(Const::Addr(Base::Str(*s), 0)),
        ExprKind::Member { base, offset, .. } => match eval_addr(base, nonnull, layout)? {
            Const::Addr(b, off) => Some(Const::Addr(b, off + *offset as i64)),
            Const::Int(v) => Some(Const::Int(v + *offset as i64)),
            Const::Float(_) | Const::Complex(..) => None,
        },
        ExprKind::Deref(p) => match eval_with_binding_and_layout(p, nonnull, layout)? {
            c @ (Const::Addr(..) | Const::Int(_)) => Some(c),
            Const::Float(_) | Const::Complex(..) => None,
        },
        _ => None,
    }
}

fn cast(
    c: Const,
    from: &Ty,
    to: &Ty,
    nonnull: &dyn Fn(usize) -> bool,
    layout: CDataLayout,
) -> Option<Const> {
    // Match runtime conversion limits before decomposing a complex value.
    // Const::Int holds only 64 bits: folding a complex-to-__int128 cast
    // would otherwise silently saturate/truncate through that representation.
    let wide = |ty: &Ty| {
        ty.int_kind()
            .is_some_and(|k| k.size_with_layout(layout) == 16)
    };
    if (from.is_complex() && wide(to)) || (to.is_complex() && wide(from)) {
        return None;
    }
    if *to == Ty::Int(IntKind::Bool) {
        return Some(Const::Int(i64::from(truthy(c, nonnull)?)));
    }
    if let Ty::Complex(elem) = to {
        let (r, i) = if let Const::Complex(r, i) = c {
            let Ty::Complex(src) = from else { return None };
            (
                cast(Const::Float(r), src, elem, nonnull, layout)?,
                cast(Const::Float(i), src, elem, nonnull, layout)?,
            )
        } else {
            (cast(c, from, elem, nonnull, layout)?, Const::Float(0.0))
        };
        let (Const::Float(r), Const::Float(i)) = (r, i) else {
            return None;
        };
        return Some(Const::Complex(r, i));
    }
    if let Const::Complex(r, _) = c {
        let Ty::Complex(elem) = from else { return None };
        return cast(Const::Float(r), elem, to, nonnull, layout);
    }
    Some(match (c, to) {
        (Const::Int(v), Ty::Int(_)) if fits_i64_with_layout(v, from, to, layout) => {
            Const::Int(normalize_with_layout(v, to, layout))
        }
        (Const::Int(_), Ty::Int(_)) => return None,
        (Const::Float(f), Ty::Int(k)) => {
            let v = if k.signed() {
                f as i64
            } else {
                f as u64 as i64
            };
            Const::Int(normalize_with_layout(v, to, layout))
        }
        (Const::Addr(..), Ty::Int(k))
            if layout == CDataLayout::NATIVE_LP64 && k.size_with_layout(layout) == 8 =>
        {
            c
        }
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
        (Const::Int(v), Ty::Ptr(_)) => Const::Int(normalize_with_layout(v, to, layout)),
        (Const::Addr(..), Ty::Ptr(_)) => c,
        _ => return None,
    })
}

fn binary(
    op: BinOp,
    operand: &Ty,
    result: &Ty,
    a: Const,
    b: Const,
    layout: CDataLayout,
) -> Option<Const> {
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
            Some(Const::Int(normalize_with_layout(v, result, layout)))
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
