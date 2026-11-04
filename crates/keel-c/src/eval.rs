//! Constant expressions: integer constants for array sizes, `case` labels
//! and enumerators, and the arithmetic and address constants that
//! initialize objects with static storage.

use crate::ast::{BinOp, Builtin, Expr, ExprKind, UnOp};
use crate::int_const::{self, IntConst, IntTy, Policy};
use crate::types::{CDataLayout, IntKind, Ty};

/// What an address constant points into.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Base {
    Global(usize),
    Str(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Const {
    /// An integer, or a pointer made from one, as a value of its type.
    Int(IntConst),
    Float(f64),
    Complex(f64, f64),
    /// An address plus a byte offset.
    Addr(Base, i64),
    /// Integer-derived symbol/addend, with unknown numeric bits and truth value.
    Reloc(Base, i64),
}

impl Const {
    /// The low 64 bits of an integer constant: the AST's encoding for
    /// every type of 64 bits or less.
    pub(crate) fn int(self) -> Option<i64> {
        match self {
            Const::Int(c) => Some(c.bits64()),
            _ => None,
        }
    }
}

/// An integer type for `ty`. Any other type holds the `i64` bits of an
/// integer constant node, as `intmax_t` would.
fn int_ty(ty: &Ty, layout: CDataLayout) -> IntTy {
    IntTy::of(ty, layout).unwrap_or(IntTy::INTMAX)
}

/// An `i64` result, converted to the node's type by the caller.
fn int64(v: i64) -> Const {
    Const::Int(IntConst::from_literal(IntTy::INTMAX, v))
}

fn boolean(b: bool) -> Const {
    Const::Int(IntConst::wrap(IntTy::INT, u128::from(b)))
}

/// Convert an AST integer encoding of a value of `ty` to the same encoding
/// after its type's truncation and extension (pointers keep pointer-width
/// unsigned bits).
pub(crate) fn wrap_literal(v: i64, ty: &Ty, layout: CDataLayout) -> i64 {
    IntConst::from_literal(IntTy::INTMAX, v)
        .convert(int_ty(ty, layout))
        .bits64()
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
    eval_with_layout(e, layout)?.int()
}

fn truthy(c: Const, nonnull: &dyn Fn(usize) -> bool) -> Option<bool> {
    match c {
        Const::Int(v) => Some(!v.is_zero()),
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
    Eval::new(layout, nonnull, Policy::CONSTANT)
        .eval(e, comma)
        .map(|f| f.value)
}

/// How an evaluation treats the comma operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Comma {
    /// Use the right operand and ignore the left one.
    Ignore,
    /// A comma makes the expression non-constant.
    Reject,
    /// Both operands must be constant.
    Constant,
}

/// What kind of constant an expression is, by the most general node it
/// evaluated.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Class {
    /// An integer constant expression (C11 6.6p6): integer literals,
    /// `sizeof`, enumerators and floating literals converted directly to an
    /// integer type.
    Integer,
    /// An arithmetic constant expression: other floating operands.
    Arithmetic,
    /// An address, a pointer conversion or an evaluated comma.
    Other,
}

/// A constant with the class of the expression that produced it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Folded {
    pub value: Const,
    pub class: Class,
}

/// One evaluation: the data layout, which globals final binding makes
/// nonnull, and which undefined results the caller accepts.
pub(crate) struct Eval<'f> {
    layout: CDataLayout,
    nonnull: &'f dyn Fn(usize) -> bool,
    policy: Policy,
    class: std::cell::Cell<Class>,
}

impl<'f> Eval<'f> {
    pub(crate) fn new(
        layout: CDataLayout,
        nonnull: &'f dyn Fn(usize) -> bool,
        policy: Policy,
    ) -> Self {
        Eval {
            layout,
            nonnull,
            policy,
            class: std::cell::Cell::new(Class::Integer),
        }
    }

    pub(crate) fn eval(&self, e: &Expr<'_>, comma: Comma) -> Option<Folded> {
        self.class.set(Class::Integer);
        let value = fold(self, e, comma)?;
        Some(Folded {
            value,
            class: self.class.get(),
        })
    }

    fn mark(&self, class: Class) {
        self.class.set(self.class.get().max(class));
    }
}

#[cfg(test)]
thread_local! {
    static VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    Eval::new(layout, nonnull, Policy::CONSTANT)
        .eval(e, Comma::Ignore)
        .map(|f| f.value)
}

/// Evaluate `e` in one walk. Every child is visited once, in the caller's
/// comma mode, and only the operands C evaluates are visited.
fn fold(cx: &Eval<'_>, e: &Expr<'_>, comma: Comma) -> Option<Const> {
    #[cfg(test)]
    VISITS.with(|v| v.set(v.get() + 1));
    let (nonnull, layout, policy) = (cx.nonnull, cx.layout, cx.policy);
    let child = |x: &Expr<'_>| fold(cx, x, comma);
    let constant = |x: &Expr<'_>| fold(cx, x, Comma::Constant);
    let value = match &e.kind {
        ExprKind::Int(v) => Const::Int(IntConst::from_literal(int_ty(&e.ty, layout), *v)),
        ExprKind::Float(f) => {
            cx.mark(Class::Arithmetic);
            Const::Float(*f)
        }
        ExprKind::Builtin(Builtin::Bswap | Builtin::Ffs, args) => {
            // Existing builtin signatures are native LP64; do not infer a
            // 64-bit swap from unsigned long on the private ILP32 profile.
            if layout != CDataLayout::NATIVE_LP64 {
                return None;
            }
            let [arg] = *args else { return None };
            // The argument's commas must be constant even where the
            // caller ignores them.
            let mode = if comma == Comma::Reject {
                Comma::Reject
            } else {
                Comma::Constant
            };
            let Const::Int(value) = fold(cx, arg, mode)? else {
                return None;
            };
            let bits = value.bits();
            let width = |ty: &Ty| Some(IntTy::of(ty, layout)?.width());
            let result = if matches!(e.kind, ExprKind::Builtin(Builtin::Ffs, _)) {
                let low = match width(&arg.ty)? {
                    32 => u64::from(bits as u32),
                    64 => bits as u64,
                    _ => return None,
                };
                if low == 0 {
                    0
                } else {
                    low.trailing_zeros() + 1
                }
                .into()
            } else {
                match width(&e.ty)? {
                    16 => u128::from((bits as u16).swap_bytes()),
                    32 => u128::from((bits as u32).swap_bytes()),
                    64 => u128::from((bits as u64).swap_bytes()),
                    _ => return None,
                }
            };
            Const::Int(IntConst::wrap(int_ty(&e.ty, layout), result))
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
        ExprKind::LabelAddr(n) => {
            cx.mark(Class::Other);
            Const::Int(IntConst::from_literal(int_ty(&e.ty, layout), *n as i64))
        }
        ExprKind::Addr(inner) => {
            cx.mark(Class::Other);
            eval_addr(cx, inner)?
        }
        // A floating literal converted directly to an integer type is an
        // integer constant expression.
        ExprKind::Cast(
            inner @ Expr {
                kind: ExprKind::Float(f),
                ..
            },
        ) if e.ty.is_integer() => {
            cast(Const::Float(*f), &inner.ty, &e.ty, nonnull, layout, policy)?
        }
        ExprKind::Cast(inner) => {
            if e.ty.is_ptr() {
                cx.mark(Class::Other);
            } else if e.ty.is_float() || e.ty.is_complex() {
                cx.mark(Class::Arithmetic);
            }
            cast(child(inner)?, &inner.ty, &e.ty, nonnull, layout, policy)?
        }
        ExprKind::Unary(op, a) => {
            let a = child(a)?;
            match (op, a) {
                (UnOp::Not, a) => boolean(!truthy(a, nonnull)?),
                (UnOp::Neg | UnOp::BitNot, Const::Int(v)) => {
                    Const::Int(int_const::unary(*op, v, policy).ok()?)
                }
                (UnOp::Neg, Const::Float(f)) => Const::Float(-f),
                (UnOp::Neg, Const::Complex(r, i)) => Const::Complex(-r, -i),
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
                boolean(if *op == BinOp::Eq { eq } else { !eq })
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
        ExprKind::Binary(op, a, b) => {
            binary(*op, &a.ty, &e.ty, child(a)?, child(b)?, layout, policy)?
        }
        ExprKind::PtrAdd(p, n, size) => {
            cx.mark(Class::Other);
            let Const::Int(n) = child(n)? else {
                return None;
            };
            let n = n.bits64();
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
                Const::Int(v) => int64(v.bits64().wrapping_add(delta)),
                Const::Float(_) | Const::Complex(..) => return None,
            }
        }
        ExprKind::PtrDiff(a, b, size) => {
            cx.mark(Class::Other);
            ptr_diff(child(a)?, child(b)?, *size, layout)?
        }
        ExprKind::LogAnd(a, b) => {
            boolean(truthy(child(a)?, nonnull)? && truthy(child(b)?, nonnull)?)
        }
        ExprKind::LogOr(a, b) => {
            boolean(truthy(child(a)?, nonnull)? || truthy(child(b)?, nonnull)?)
        }
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
                cx.mark(Class::Other);
                child(a)?;
                child(b)?
            }
        },
        _ => return None,
    };
    Some(match value {
        Const::Int(v) => Const::Int(v.convert(int_ty(&e.ty, layout))),
        other => other,
    })
}

/// `a - b` for pointers to elements of `size` bytes.
fn ptr_diff(a: Const, b: Const, size: u64, layout: CDataLayout) -> Option<Const> {
    Some(match (a, b) {
        (Const::Addr(x, i), Const::Addr(y, j)) if x == y => int64(if layout.pointer_bytes() == 4 {
            i.checked_sub(j)?.checked_div(i64::try_from(size).ok()?)?
        } else {
            (i - j) / size as i64
        }),
        (Const::Int(i), Const::Int(j)) => int64({
            let (i, j) = (i.bits64(), j.bits64());
            if layout.pointer_bytes() == 4 {
                i.wrapping_sub(j).checked_div(i64::try_from(size).ok()?)?
            } else {
                i.wrapping_sub(j) / size as i64
            }
        }),
        _ => return None,
    })
}

/// The address of an lvalue with static storage.
fn eval_addr(cx: &Eval<'_>, e: &Expr<'_>) -> Option<Const> {
    let layout = cx.layout;
    match &e.kind {
        ExprKind::Global(g) => Some(Const::Addr(Base::Global(*g), 0)),
        ExprKind::Str(s) => Some(Const::Addr(Base::Str(*s), 0)),
        ExprKind::Member { base, offset, .. } => match eval_addr(cx, base)? {
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
            Const::Int(v) => Some(int64(v.bits64().wrapping_add(*offset as i64))),
            Const::Float(_) | Const::Complex(..) => None,
        },
        ExprKind::Deref(p) => match fold(cx, p, Comma::Constant)? {
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
    policy: Policy,
) -> Option<Const> {
    // Complex and 128-bit conversions are rejected at runtime, until the
    // runtime integer-floating conversions they go through exist.
    let wide = |ty: &Ty| {
        ty.int_kind()
            .is_some_and(|k| k.size_with_layout(layout) == 16)
    };
    if (from.is_complex() && wide(to)) || (to.is_complex() && wide(from)) {
        return None;
    }
    if *to == Ty::Int(IntKind::Bool) {
        return Some(Const::Int(IntConst::wrap(
            IntTy::BOOL,
            u128::from(truthy(c, nonnull)?),
        )));
    }
    if let Ty::Complex(elem) = to {
        let (r, i) = if let Const::Complex(r, i) = c {
            let Ty::Complex(src) = from else { return None };
            (
                cast(Const::Float(r), src, elem, nonnull, layout, policy)?,
                cast(Const::Float(i), src, elem, nonnull, layout, policy)?,
            )
        } else {
            (
                cast(c, from, elem, nonnull, layout, policy)?,
                Const::Float(0.0),
            )
        };
        let (Const::Float(r), Const::Float(i)) = (r, i) else {
            return None;
        };
        return Some(Const::Complex(r, i));
    }
    if let Const::Complex(r, _) = c {
        let Ty::Complex(elem) = from else { return None };
        return cast(Const::Float(r), elem, to, nonnull, layout, policy);
    }
    Some(match (c, to) {
        (Const::Int(v), Ty::Int(_)) => Const::Int(v.convert(int_ty(to, layout))),
        (Const::Float(f), Ty::Int(_)) => {
            // Out of range, C leaves the value undefined; GCC and Clang
            // saturate to the target type, with NaN giving 0.
            let ty = int_ty(to, layout);
            Const::Int(int_const::from_float(f, ty, policy).ok()?)
        }
        (Const::Addr(..) | Const::Reloc(..), Ty::Int(k))
            if layout == CDataLayout::NATIVE_LP64 && k.size_with_layout(layout) == 8 =>
        {
            c
        }
        (Const::Int(v), Ty::Float) => Const::Float(f64::from(int_const::to_f32(v))),
        (Const::Int(v), Ty::Double) => Const::Float(int_const::to_f64(v)),
        (Const::Float(f), Ty::Float) => Const::Float(f64::from(f as f32)),
        (Const::Float(f), Ty::Double) => Const::Float(f),
        (Const::Int(v), Ty::Ptr(_)) => Const::Int(v.convert(int_ty(to, layout))),
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
    policy: Policy,
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
            let delta = delta.bits64();
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
            Some(Const::Reloc(base, offset.checked_add(delta.bits64())?))
        }
        (Const::Int(x), Const::Int(y)) => {
            // Both operands, and a shift count, have the operand type.
            let ty = int_ty(operand, layout);
            let v = int_const::binary(op, x.convert(ty), y.convert(ty), policy).ok()?;
            Some(Const::Int(v.convert(int_ty(result, layout))))
        }
        (Const::Float(x), Const::Float(y)) => {
            let single = *operand == Ty::Float;
            let round = |f: f64| if single { f64::from(f as f32) } else { f };
            Some(match op {
                BinOp::Add => Const::Float(round(x + y)),
                BinOp::Sub => Const::Float(round(x - y)),
                BinOp::Mul => Const::Float(round(x * y)),
                BinOp::Div => Const::Float(round(x / y)),
                BinOp::Eq => boolean(x == y),
                BinOp::Ne => boolean(x != y),
                BinOp::Lt => boolean(x < y),
                BinOp::Le => boolean(x <= y),
                BinOp::Gt => boolean(x > y),
                BinOp::Ge => boolean(x >= y),
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
            let count = visits(|| {
                value = eval_constant_with_layout(&e, allow_comma, layout).and_then(Const::int)
            });
            assert_eq!(value, Some(terms as i64));
            assert_eq!(count, n);
        }
        let count = visits(|| assert!(eval_with_layout(&e, layout).is_some()));
        assert_eq!(count, n);
    }
}
