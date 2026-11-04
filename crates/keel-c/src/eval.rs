//! Constant expressions: integer constants for array sizes, `case` labels
//! and enumerators, and the arithmetic and address constants that
//! initialize objects with static storage.

use crate::ast::{BinOp, Builtin, Expr, ExprKind, UnOp};
use crate::int_const::{self, IntTy, Policy};
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
    /// Integer-derived symbol/addend, with unknown numeric bits and truth value.
    Reloc(Base, i64),
}

// Native compatibility boundaries for lowering and proof callers. Recursive
// model-aware evaluation must never call these wrappers.
pub fn eval_int(e: &Expr<'_>) -> Option<i64> {
    eval_int_with_layout(e, CDataLayout::NATIVE_LP64)
}
pub fn eval_constant(e: &Expr<'_>, allow_comma: bool) -> Option<Const> {
    eval_constant_with_layout(e, allow_comma, CDataLayout::NATIVE_LP64)
}
pub fn eval(e: &Expr<'_>) -> Option<Const> {
    eval_with_layout(e, CDataLayout::NATIVE_LP64)
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
        Const::Reloc(..) => None,
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
    eval_constant_with_binding_and_layout(e, allow_comma, &|_| false, layout)
}

/// Prove evaluated operands constant while using final declaration binding.
pub(crate) fn eval_constant_with_binding_and_layout(
    e: &Expr<'_>,
    allow_comma: bool,
    nonnull: &dyn Fn(usize) -> bool,
    layout: CDataLayout,
) -> Option<Const> {
    let comma = if allow_comma {
        Comma::Constant
    } else {
        Comma::Reject
    };
    fold(e, comma, nonnull, layout)
}

/// How an evaluation treats the comma operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Comma {
    /// Use the right operand and ignore the left one.
    Ignore,
    /// A comma makes the expression non-constant.
    Reject,
    /// Both operands must be constant.
    Constant,
}

#[cfg(test)]
thread_local! {
    static VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Integer builtin proofs exclude effects and intermediates wider than Const::Int.
/// Keep comma policy even inside builtin arguments and bound the extra walk.
fn scalar_constant_tree(e: &Expr<'_>, allow_comma: bool, layout: CDataLayout, depth: u8) -> bool {
    if depth == 0
        || !e
            .ty
            .int_kind()
            .is_some_and(|k| k.size_with_layout(layout) <= 8)
    {
        return false;
    }
    let child = |e| scalar_constant_tree(e, allow_comma, layout, depth - 1);
    match &e.kind {
        ExprKind::Int(_) => true,
        ExprKind::Cast(a) | ExprKind::Unary(_, a) => child(a),
        ExprKind::Binary(_, a, b) | ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
            child(a) && child(b)
        }
        ExprKind::Comma(a, b) => allow_comma && child(a) && child(b),
        ExprKind::Cond(c, a, b) => child(c) && child(a) && child(b),
        ExprKind::Builtin(Builtin::Bswap | Builtin::Ffs, args) => args.iter().all(child),
        _ => false,
    }
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
    fold(e, Comma::Ignore, nonnull, layout)
}

/// Evaluate `e` in one walk. Every child is visited once, in the caller's
/// comma mode, and only the operands C evaluates are visited.
fn fold(
    e: &Expr<'_>,
    comma: Comma,
    nonnull: &dyn Fn(usize) -> bool,
    layout: CDataLayout,
) -> Option<Const> {
    #[cfg(test)]
    VISITS.with(|v| v.set(v.get() + 1));
    let child = |x: &Expr<'_>| fold(x, comma, nonnull, layout);
    let constant = |x: &Expr<'_>| fold(x, Comma::Constant, nonnull, layout);
    let value = match &e.kind {
        ExprKind::Int(v) => Const::Int(*v),
        ExprKind::Float(f) => Const::Float(*f),
        ExprKind::Builtin(Builtin::Bswap | Builtin::Ffs, args) => {
            // Existing builtin signatures are native LP64; do not infer a
            // 64-bit swap from unsigned long on the private ILP32 profile.
            if layout != CDataLayout::NATIVE_LP64 {
                return None;
            }
            let [arg] = *args else { return None };
            if !scalar_constant_tree(arg, comma != Comma::Reject, layout, 64) {
                return None;
            }
            let Const::Int(value) = fold(arg, Comma::Ignore, nonnull, layout)? else {
                return None;
            };
            if matches!(e.kind, ExprKind::Builtin(Builtin::Ffs, _)) {
                let value = match arg.ty.int_kind()?.size_with_layout(layout) {
                    4 => u64::from(value as u32),
                    8 => value as u64,
                    _ => return None,
                };
                return Some(Const::Int(if value == 0 {
                    0
                } else {
                    i64::from(value.trailing_zeros() + 1)
                }));
            }
            Const::Int(match e.ty.int_kind()?.size_with_layout(layout) {
                2 => i64::from((value as u16).swap_bytes()),
                4 => i64::from((value as u32).swap_bytes()),
                8 => (value as u64).swap_bytes() as i64,
                _ => return None,
            })
        }
        ExprKind::Builtin(Builtin::Complex, args) => {
            let (Const::Float(r), Const::Float(i)) = (constant(&args[0])?, constant(&args[1])?)
            else {
                return None;
            };
            Const::Complex(r, i)
        }
        ExprKind::Member { base, offset, .. } if base.ty.is_complex() => {
            let Const::Complex(r, i) = constant(base)? else {
                return None;
            };
            Const::Float(if *offset == 0 { r } else { i })
        }
        ExprKind::Builtin(Builtin::Hint | Builtin::AssumeAligned(_), args) => {
            let value = constant(&args[0])?;
            for arg in &args[1..] {
                constant(arg)?;
            }
            value
        }
        ExprKind::LabelAddr(n) => Const::Int(*n as i64),
        ExprKind::Addr(inner) => eval_addr(inner, nonnull, layout)?,
        ExprKind::Cast(inner) => cast(child(inner)?, &inner.ty, &e.ty, nonnull, layout)?,
        ExprKind::Unary(op, a) => {
            let a = child(a)?;
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
            let (x, y) = (child(a)?, child(b)?);
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
        ExprKind::Binary(op, a, b) => binary(*op, &a.ty, &e.ty, child(a)?, child(b)?, layout)?,
        ExprKind::PtrAdd(p, n, size) => {
            let Const::Int(n) = child(n)? else {
                return None;
            };
            let delta = n.wrapping_mul(*size as i64);
            match child(p)? {
                Const::Addr(base, off) => Const::Addr(
                    base,
                    if layout.pointer_bytes() == 4 {
                        off.checked_add(n.checked_mul(i64::try_from(*size).ok()?)?)?
                    } else {
                        off + delta
                    },
                ),
                Const::Reloc(base, off) => Const::Reloc(base, off.checked_add(delta)?),
                Const::Int(v) => Const::Int(v.wrapping_add(delta)),
                Const::Float(_) | Const::Complex(..) => return None,
            }
        }
        ExprKind::PtrDiff(a, b, size) => match (child(a)?, child(b)?) {
            (Const::Addr(x, i), Const::Addr(y, j)) if x == y => {
                Const::Int(if layout.pointer_bytes() == 4 {
                    i.checked_sub(j)?.checked_div(i64::try_from(*size).ok()?)?
                } else {
                    (i - j) / *size as i64
                })
            }
            (Const::Int(i), Const::Int(j)) => Const::Int(if layout.pointer_bytes() == 4 {
                i.wrapping_sub(j).checked_div(i64::try_from(*size).ok()?)?
            } else {
                i.wrapping_sub(j) / *size as i64
            }),
            _ => return None,
        },
        ExprKind::LogAnd(a, b) => Const::Int(i64::from(
            truthy(child(a)?, nonnull)? && truthy(child(b)?, nonnull)?,
        )),
        ExprKind::LogOr(a, b) => Const::Int(i64::from(
            truthy(child(a)?, nonnull)? || truthy(child(b)?, nonnull)?,
        )),
        ExprKind::Cond(c, a, b) => {
            if truthy(child(c)?, nonnull)? {
                child(a)?
            } else {
                child(b)?
            }
        }
        ExprKind::Comma(a, b) => match comma {
            Comma::Ignore => child(b)?,
            Comma::Reject => return None,
            Comma::Constant => {
                child(a)?;
                child(b)?
            }
        },
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
            Const::Addr(b, off) => Some(Const::Addr(
                b,
                if layout.pointer_bytes() == 4 {
                    off.checked_add(i64::try_from(*offset).ok()?)?
                } else {
                    off + *offset as i64
                },
            )),
            Const::Reloc(b, off) => Some(Const::Reloc(
                b,
                off.checked_add(i64::try_from(*offset).ok()?)?,
            )),
            Const::Int(v) => Some(Const::Int(if layout.pointer_bytes() == 4 {
                v.wrapping_add(*offset as i64)
            } else {
                v + *offset as i64
            })),
            Const::Float(_) | Const::Complex(..) => None,
        },
        ExprKind::Deref(p) => match fold(p, Comma::Constant, nonnull, layout)? {
            c @ (Const::Addr(..) | Const::Reloc(..) | Const::Int(_)) => Some(c),
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
        (Const::Float(f), Ty::Int(k)) if k.size_with_layout(layout) <= 8 => {
            // Out of range, C leaves the value undefined; GCC and Clang
            // saturate to the target type, with NaN giving 0.
            let ty = IntTy::of(to, layout)?;
            Const::Int(
                int_const::from_float(f, ty, Policy::CONSTANT)
                    .ok()?
                    .bits64(),
            )
        }
        (Const::Float(f), Ty::Int(k)) => {
            let v = if k.signed() {
                f as i64
            } else {
                f as u64 as i64
            };
            Const::Int(normalize_with_layout(v, to, layout))
        }
        (Const::Addr(..) | Const::Reloc(..), Ty::Int(k))
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
        (Const::Addr(..) | Const::Reloc(..), Ty::Ptr(_)) => c,
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
    let integer_address = layout == CDataLayout::NATIVE_LP64
        && [operand, result].into_iter().all(|ty| {
            ty.int_kind()
                .is_some_and(|k| k.size_with_layout(layout) == 8)
        });
    match (a, b) {
        (Const::Addr(base, offset) | Const::Reloc(base, offset), Const::Int(delta))
            if integer_address =>
        {
            let offset = match op {
                BinOp::Add => offset.checked_add(delta)?,
                BinOp::Sub => offset.checked_sub(delta)?,
                _ => return None,
            };
            Some(Const::Reloc(base, offset))
        }
        (Const::Int(delta), Const::Addr(base, offset) | Const::Reloc(base, offset))
            if integer_address && op == BinOp::Add =>
        {
            Some(Const::Reloc(base, offset.checked_add(delta)?))
        }
        (Const::Int(x), Const::Int(y)) => {
            let signed = operand.is_signed();
            let (ux, uy) = (x as u64, y as u64);
            // The count has the left operand's type; outside [0, width) the
            // shift is undefined, and compilers disagree on its value.
            let width = operand.int_kind()?.size_with_layout(layout) as i64 * 8;
            if matches!(op, BinOp::Shl | BinOp::Shr) && !(0..width).contains(&y) {
                return None;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Parser;
    use crate::strings::Strings;
    use bumpalo::Bump;
    use keel_ir::Target;

    fn nodes(e: &Expr<'_>) -> usize {
        1 + match &e.kind {
            ExprKind::Cast(a) | ExprKind::Unary(_, a) => nodes(a),
            ExprKind::Binary(_, a, b) | ExprKind::Comma(a, b) => nodes(a) + nodes(b),
            _ => 0,
        }
    }

    fn visits(f: impl FnOnce()) -> usize {
        VISITS.with(|v| v.set(0));
        f();
        VISITS.with(|v| v.get())
    }

    #[test]
    fn long_chains_visit_each_node_once() {
        // Parsing and evaluation recurse once per level; debug frames are large.
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(chain)
            .unwrap()
            .join()
            .unwrap();
    }

    fn chain() {
        let arena = Bump::new();
        let terms = 2_000;
        let src = format!("0{}", " + 1".repeat(terms));
        let mut p = Parser::new(&src, "chain.c", Target::default(), Strings::new(&arena));
        let e = p.expr().unwrap();
        let n = nodes(&e);
        assert!(n > terms);
        let layout = CDataLayout::NATIVE_LP64;
        for allow_comma in [false, true] {
            let mut value = None;
            let count = visits(|| value = eval_constant_with_layout(&e, allow_comma, layout));
            assert_eq!(value, Some(Const::Int(terms as i64)));
            assert_eq!(count, n);
        }
        let count = visits(|| assert!(eval_with_layout(&e, layout).is_some()));
        assert_eq!(count, n);
    }
}
