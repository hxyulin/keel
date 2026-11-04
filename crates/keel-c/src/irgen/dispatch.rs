//! Computed-goto dispatch tables and the expression visitors that find them.

use super::*;

/// A bounded fusion of `goto *constant_table[index]` into the existing
/// shared switch. Numeric label values and ordinary table uses never change.
pub(super) struct DispatchTable {
    pub(super) global: usize,
    pub(super) labels: Vec<usize>,
}

/// Deliberately accept only the parser's ordinary pointer-array subscript.
/// In particular, do not strip casts, commute pointer arithmetic, or infer
/// equivalence after modular multiplication by the element size.
pub(super) fn dispatch_index<'e, 'a>(target: &'e Expr<'a>) -> Option<(usize, &'e Expr<'a>)> {
    let ExprKind::Deref(address) = &target.kind else {
        return None;
    };
    let ExprKind::PtrAdd(base, index, 8) = &address.kind else {
        return None;
    };
    let ExprKind::Addr(array) = &base.kind else {
        return None;
    };
    let ExprKind::Global(global) = array.kind else {
        return None;
    };
    if !matches!(target.ty, Ty::Ptr(_)) || !matches!(index.ty, Ty::Int(IntKind::Long)) {
        return None;
    }
    Some((global, index))
}

// Global::is_const describes emitted data, and currently loses the pointer
// qualifier for arrays. Prove source immutability from the retained type shape
// instead; never mistake const pointees for const pointer elements.
fn constant_elements(sugar: &Sugar, defs: &[TypedefDef<'_>]) -> bool {
    match sugar {
        Sugar::Const(_) => true,
        Sugar::Array(inner) | Sugar::Restrict(inner) | Sugar::Volatile(inner) => {
            constant_elements(inner, defs)
        }
        Sugar::Typedef(i) => constant_elements(&defs[*i].sugar, defs),
        _ => false,
    }
}

impl DispatchTable {
    pub(super) fn for_function(p: &crate::unit::Unit<'_>, def: &FuncDef<'_>) -> Option<Self> {
        if def.label_addrs.is_empty() {
            return None;
        }
        let mut selected = None;
        for stmt in def.body {
            visit_stmt_exprs(stmt, &mut |target, use_| {
                if use_ != ExprUse::IndirectGoto {
                    return Some(());
                }
                let (global, _) = dispatch_index(target)?;
                if target.sugar.is_volatile(p.typedefs) {
                    return None;
                }
                if selected.is_some_and(|previous| previous != global) {
                    return None;
                }
                selected = Some(global);
                Some(())
            })?;
        }
        let global = selected?;
        let object = &p.globals[global];
        if !object.internal
            || !object.defined
            || !constant_elements(&object.sugar, p.typedefs)
            || object.thread_local
            || object.sugar.is_volatile(p.typedefs)
        {
            return None;
        }
        let Ty::Array(&ArrayTy {
            elem: element,
            len: Some(len),
            ..
        }) = object.ty
        else {
            return None;
        };
        if !matches!(element, Ty::Ptr(_))
            || len == 0
            || len > 4096
            || len > 2 * def.label_addrs.len() as u64
        {
            return None;
        }
        let mut labels = vec![0; len as usize];
        for item in object.init.as_ref()?.items {
            let InitItem::Expr {
                offset,
                expr,
                bits: None,
            } = item
            else {
                return None;
            };
            if offset % 8 != 0 || offset / 8 >= len || !matches!(expr.ty, Ty::Ptr(_)) {
                return None;
            }
            let label = eval(expr)?.int()?;
            let label = usize::try_from(label).ok()?;
            if label > def.label_addrs.len() {
                return None;
            }
            // Match data(): zero-fill first, then apply flattened writes in
            // order, including repeated and designated initializers.
            labels[*offset as usize / 8] = label;
        }
        Some(Self { global, labels })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ExprUse {
    Value,
    Address,
    IndirectGoto,
}

/// Inspect evaluated expressions, including GNU statement expressions. Keep
/// address computations distinct from value reads and identify indirect gotos
/// for dispatch-table recognition. Returning None stops the traversal.
pub(super) fn visit_stmt_exprs(
    stmt: &Stmt<'_>,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    match stmt {
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => visit_exprs(e, f)?,
        Stmt::GotoIndirect(e) => {
            visit_exprs_as(e, ExprUse::IndirectGoto, f)?;
        }
        Stmt::Init(_, init, _) => visit_init_exprs(init, f)?,
        Stmt::Asm(asm) => {
            for (e, _) in asm.outputs {
                visit_exprs(e, f)?;
            }
            for e in asm.inputs {
                visit_exprs(e, f)?;
            }
        }
        Stmt::Block(stmts) | Stmt::Scoped(_, stmts) => {
            for stmt in stmts.iter() {
                visit_stmt_exprs(stmt, f)?;
            }
        }
        Stmt::If(e, yes, no) => {
            visit_exprs(e, f)?;
            visit_stmt_exprs(yes, f)?;
            if let Some(no) = no {
                visit_stmt_exprs(no, f)?;
            }
        }
        Stmt::While(e, body) | Stmt::DoWhile(body, e) => {
            visit_exprs(e, f)?;
            visit_stmt_exprs(body, f)?;
        }
        Stmt::Switch { cond, body, .. } => {
            visit_exprs(cond, f)?;
            visit_stmt_exprs(body, f)?;
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(init) = init {
                visit_stmt_exprs(init, f)?;
            }
            if let Some(cond) = cond {
                visit_exprs(cond, f)?;
            }
            if let Some(step) = step {
                visit_exprs(step, f)?;
            }
            visit_stmt_exprs(body, f)?;
        }
        Stmt::Declare(_)
        | Stmt::Case(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Return(None, _)
        | Stmt::Goto(_)
        | Stmt::Label(_) => {}
    }
    Some(())
}

pub(super) fn visit_init_exprs(
    init: &Init<'_>,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    for item in init.items {
        if let InitItem::Expr { expr, .. } = item {
            visit_exprs(expr, f)?;
        }
    }
    Some(())
}

fn visit_exprs(e: &Expr<'_>, f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>) -> Option<()> {
    visit_exprs_as(e, ExprUse::Value, f)
}

fn visit_exprs_as(
    e: &Expr<'_>,
    use_: ExprUse,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    f(e, use_)?;
    match &e.kind {
        ExprKind::Unary(_, a)
        | ExprKind::Cast(a)
        | ExprKind::UnionCast(a, _)
        | ExprKind::Deref(a)
        | ExprKind::VaStart(a)
        | ExprKind::VaArg(a)
        | ExprKind::IncDec { lhs: a, .. } => visit_exprs(a, f)?,
        ExprKind::Addr(a) | ExprKind::Member { base: a, .. } => {
            visit_exprs_as(a, ExprUse::Address, f)?;
        }
        ExprKind::VectorIndex(a, b)
        | ExprKind::Binary(_, a, b)
        | ExprKind::PtrAdd(a, b, _)
        | ExprKind::PtrDiff(a, b, _)
        | ExprKind::LogAnd(a, b)
        | ExprKind::LogOr(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::Comma(a, b)
        | ExprKind::OpAssign { lhs: a, rhs: b, .. }
        | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => {
            visit_exprs(a, f)?;
            visit_exprs(b, f)?;
        }
        ExprKind::Cond(a, b, c) => {
            visit_exprs(a, f)?;
            visit_exprs(b, f)?;
            visit_exprs(c, f)?;
        }
        ExprKind::Call(callee, args) => {
            visit_exprs(callee, f)?;
            for arg in args.iter() {
                visit_exprs(arg, f)?;
            }
        }
        ExprKind::Builtin(_, args) => {
            for arg in args.iter() {
                visit_exprs(arg, f)?;
            }
        }
        ExprKind::CompoundLit(_, init) => visit_init_exprs(init, f)?,
        ExprKind::StmtExpr { stmts, value, .. } => {
            for stmt in stmts.iter() {
                visit_stmt_exprs(stmt, f)?;
            }
            if let Some(value) = value {
                visit_exprs(value, f)?;
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::LabelAddr(_)
        | ExprKind::Local(_)
        | ExprKind::Global(_) => {}
    }
    Some(())
}
