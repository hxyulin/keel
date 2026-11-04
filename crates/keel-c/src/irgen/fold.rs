//! Frontend folds of literal operations, identities and branch conditions.

use super::*;

/// How deep `fold_int` looks, bounding the work for any tree. Flag masks
/// spelled `A | B | C | ...` are long left-leaning chains.
pub(super) const FOLD_DEPTH: u32 = 32;

/// An integer of at most 64 bits: the type whose values `fold_int` gives.
pub(super) fn small_int(ty: &Ty, layout: CDataLayout) -> bool {
    ty.int_kind()
        .is_some_and(|k| k.size_with_layout(layout) <= 8)
}

/// The value of integer expression `e` if it is made of literals alone,
/// in the AST's integer encoding.
pub(super) fn fold_int(e: &Expr<'_>, depth: u32, layout: CDataLayout) -> Option<i64> {
    fold_literals(e, depth, layout).map(IntConst::bits64)
}

/// Fold what the generated code would compute: overflow wraps, while
/// division traps and out-of-range shifts are left to the target.
fn fold_literals(e: &Expr<'_>, depth: u32, layout: CDataLayout) -> Option<IntConst> {
    if !small_int(&e.ty, layout) {
        return None;
    }
    let ty = IntTy::of(&e.ty, layout)?;
    let depth = depth.checked_sub(1)?;
    let fold = |a: &Expr<'_>| fold_literals(a, depth, layout);
    let policy = Policy::LOWERING;
    let v = match &e.kind {
        ExprKind::Int(v) => IntConst::from_literal(ty, *v),
        ExprKind::Cast(a) if small_int(&a.ty, layout) => fold(a)?,
        ExprKind::Unary(op, a) if small_int(&a.ty, layout) => {
            int_const::unary(*op, fold(a)?, policy).ok()?
        }
        ExprKind::Binary(op, a, b) if small_int(&a.ty, layout) && small_int(&b.ty, layout) => {
            let operand = IntTy::of(&a.ty, layout)?;
            let (x, y) = (fold(a)?.convert(operand), fold(b)?.convert(operand));
            int_const::binary(*op, x, y, policy).ok()?
        }
        // `sizeof(size_t) < sizeof(long) ? A : B`: the other arm is not
        // evaluated, whatever it is.
        ExprKind::Cond(c, a, b) if small_int(&c.ty, layout) => {
            let arm = if fold(c)?.is_zero() { b } else { a };
            if arm.ty != e.ty {
                return None;
            }
            fold(arm)?
        }
        _ => return None,
    };
    Some(v.convert(ty))
}

/// Whether `x op k` is `x` for integer `x` of type `ty`.
pub(super) fn right_identity(op: BinOp, k: i64, ty: &Ty, layout: CDataLayout) -> bool {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => k == 0,
        BinOp::Mul | BinOp::Div => k == 1,
        BinOp::And => k == crate::eval::wrap_literal(-1, ty, layout),
        _ => false,
    }
}

/// Whether `k op x` is `x`.
pub(super) fn left_identity(op: BinOp, k: i64, ty: &Ty, layout: CDataLayout) -> bool {
    match op {
        BinOp::Add | BinOp::Or | BinOp::Xor => k == 0,
        BinOp::Mul => k == 1,
        BinOp::And => k == crate::eval::wrap_literal(-1, ty, layout),
        _ => false,
    }
}

/// A literal, or an operation on literals, whose evaluation does nothing
/// and cannot fail.
pub(super) fn inert(e: &Expr<'_>, layout: CDataLayout) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Cast(a) => convertible(&a.ty, &e.ty) && inert(a, layout),
        _ => fold_int(e, FOLD_DEPTH, layout).is_some(),
    }
}

/// Whether `FnGen::convert` (or a cast to `void`) accepts this conversion.
pub(super) fn convertible(from: &Ty, to: &Ty) -> bool {
    let wide = |t: &Ty| t.int_kind().is_some_and(|k| k.size() == 16);
    *to == Ty::Void
        || from.is_scalar()
            && to.is_scalar()
            && from.unusable().is_none()
            && to.unusable().is_none()
            && !((from.is_float() || from.is_complex()) && (to.is_ptr() || wide(to)))
            && !((to.is_float() || to.is_complex()) && (from.is_ptr() || wide(from)))
}

/// An expression that is non-zero exactly when `e` is, and evaluates the
/// same way: conversions to `_Bool`, between pointers and to an integer at
/// least as wide, and `__builtin_expect` with inert expectations.
pub(super) fn same_truth<'e, 'a>(e: &'e Expr<'a>, layout: CDataLayout) -> Option<&'e Expr<'a>> {
    match &e.kind {
        ExprKind::Cast(a) => {
            let to_bool = e.ty == Ty::Int(IntKind::Bool)
                && (a.ty.is_integer() || a.ty.is_ptr() || matches!(a.ty, Ty::Float | Ty::Double));
            let keeps = to_bool
                || e.ty.is_ptr() && a.ty.is_ptr()
                || matches!((e.ty.int_kind(), a.ty.int_kind()),
                    (Some(to), Some(from)) if to.size_with_layout(layout) >= from.size_with_layout(layout));
            keeps.then_some(&**a)
        }
        ExprKind::Builtin(Builtin::Hint, args) if args[1..].iter().all(|e| inert(e, layout)) => {
            Some(&args[0])
        }
        _ => None,
    }
}

/// `e` with `same_truth` applied as far as it goes.
fn strip_truth<'e, 'a>(mut e: &'e Expr<'a>, layout: CDataLayout) -> &'e Expr<'a> {
    while let Some(inner) = same_truth(e, layout) {
        e = inner;
    }
    e
}

/// An integer comparison `a op b` under truth-preserving wrappers, so that
/// `(a op b) != 0` is `a op b`. Float comparisons are excluded: with NaN,
/// negating one is not the opposite comparison.
pub(super) fn int_comparison<'e, 'a>(
    e: &'e Expr<'a>,
    layout: CDataLayout,
) -> Option<(BinOp, &'e Expr<'a>, &'e Expr<'a>)> {
    match &strip_truth(e, layout).kind {
        ExprKind::Binary(op, a, b) if op.is_comparison() && !a.ty.is_float() => Some((*op, a, b)),
        _ => None,
    }
}

/// Whether `e` contains no statement (so no label) and no compound literal,
/// looking at most `budget` nodes deep.
pub(super) fn no_statements(e: &Expr<'_>, budget: u32) -> bool {
    let Some(budget) = budget.checked_sub(1) else {
        return false;
    };
    let all = |es: &[&Expr<'_>]| es.iter().all(|e| no_statements(e, budget));
    match &e.kind {
        ExprKind::Unary(_, a)
        | ExprKind::Cast(a)
        | ExprKind::UnionCast(a, _)
        | ExprKind::Deref(a)
        | ExprKind::Addr(a)
        | ExprKind::VaStart(a)
        | ExprKind::VaArg(a)
        | ExprKind::IncDec { lhs: a, .. }
        | ExprKind::Member { base: a, .. } => all(&[a]),
        ExprKind::VectorIndex(a, b)
        | ExprKind::Binary(_, a, b)
        | ExprKind::PtrAdd(a, b, _)
        | ExprKind::PtrDiff(a, b, _)
        | ExprKind::LogAnd(a, b)
        | ExprKind::LogOr(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::Comma(a, b)
        | ExprKind::OpAssign { lhs: a, rhs: b, .. }
        | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => all(&[a, b]),
        ExprKind::Cond(a, b, c) => all(&[a, b, c]),
        ExprKind::Call(callee, args) => {
            no_statements(callee, budget) && args.iter().all(|a| no_statements(a, budget))
        }
        ExprKind::Builtin(_, args) => args.iter().all(|a| no_statements(a, budget)),
        ExprKind::CompoundLit(..) | ExprKind::StmtExpr { .. } => false,
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::LabelAddr(_)
        | ExprKind::Local(_)
        | ExprKind::Global(_) => true,
    }
}

/// The comparison true exactly when `op` is false.
pub(super) fn negated(op: BinOp) -> BinOp {
    match op {
        BinOp::Eq => BinOp::Ne,
        BinOp::Ne => BinOp::Eq,
        BinOp::Lt => BinOp::Ge,
        BinOp::Ge => BinOp::Lt,
        BinOp::Le => BinOp::Gt,
        BinOp::Gt => BinOp::Le,
        _ => unreachable!("not a comparison"),
    }
}

/// `x != 0` or `x == 0` with integer `x` and a literal zero: whether it is
/// `!=`, and `x`.
pub(super) fn zero_test<'e, 'a>(e: &'e Expr<'a>) -> Option<(bool, &'e Expr<'a>)> {
    match &e.kind {
        ExprKind::Binary(op, x, z) => Some((zero_test_op(*op, x, z)?, x)),
        _ => None,
    }
}

/// `zero_test` of `x op z`.
pub(super) fn zero_test_op(op: BinOp, x: &Expr<'_>, z: &Expr<'_>) -> Option<bool> {
    (matches!(op, BinOp::Ne | BinOp::Eq) && x.ty.is_integer() && matches!(z.kind, ExprKind::Int(0)))
        .then_some(op == BinOp::Ne)
}
