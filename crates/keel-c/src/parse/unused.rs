//! Source uses of declarations and labels for the unused-warning group.

use super::{Attrs, Error, Expr, ExprKind, Global, HashSet, Loc, Local, LocalId, Parser, Stmt, Ty};
use crate::UnusedWarnings;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Declaration {
    loc: Loc,
    suppressed: bool,
}

struct Uses {
    decl: Declaration,
    references: usize,
    writes: usize,
}

/// BTreeMap keeps diagnostics in declaration order. No tracking structures
/// are allocated for a translation unit with these warnings disabled.
#[derive(Default)]
pub(super) struct Objects(BTreeMap<usize, Uses>);

impl Objects {
    pub(super) fn declaration(&mut self, id: usize, loc: Loc, suppressed: bool) {
        let uses = self.0.entry(id).or_insert(Uses {
            decl: Declaration { loc, suppressed },
            references: 0,
            writes: 0,
        });
        uses.decl.suppressed |= suppressed;
    }

    pub(super) fn reference(&mut self, id: usize) {
        if let Some(uses) = self.0.get_mut(&id) {
            uses.references += 1;
        }
    }

    fn write(&mut self, id: usize) {
        if let Some(uses) = self.0.get_mut(&id) {
            uses.writes += 1;
        }
    }

    pub(super) fn local_warnings(
        self,
        checks: UnusedWarnings,
        params: &[LocalId],
        locals: &[Local<'_>],
        warnings: &mut Vec<Error>,
    ) {
        for (id, uses) in self.0 {
            if uses.decl.suppressed || uses.references > uses.writes || locals[id].cleanup.is_some()
            {
                continue;
            }
            let name = locals[id].name;
            let warning = match (params.contains(&id), uses.writes != 0) {
                (true, true) if checks.set_parameters => {
                    format!("parameter `{name}` set but not used [-Wunused-but-set-parameter]")
                }
                (true, false) if checks.parameters => {
                    format!("unused parameter `{name}` [-Wunused-parameter]")
                }
                (false, true) if checks.set_variables => {
                    format!("variable `{name}` set but not used [-Wunused-but-set-variable]")
                }
                (false, false) if checks.variables => {
                    format!("unused variable `{name}` [-Wunused-variable]")
                }
                _ => continue,
            };
            warnings.push(Error::new(uses.decl.loc, warning));
        }
    }

    pub(super) fn global_warnings(
        self,
        checks: UnusedWarnings,
        globals: &[Global<'_>],
        warnings: &mut Vec<Error>,
    ) {
        for (id, uses) in self.0 {
            let global = &globals[id];
            if uses.decl.suppressed
                || uses.references != 0
                || !global.internal
                || global.used
                || global.retain.is_some()
                || global.lifecycle.is_some()
            {
                continue;
            }
            let name = global.name;
            let warning = if global.ty.func().is_some() && checks.functions {
                if global.func.is_none() {
                    format!(
                        "function `{name}` declared static but never defined [-Wunused-function]"
                    )
                } else {
                    format!("unused function `{name}` [-Wunused-function]")
                }
            } else if global.defined && global.ty.func().is_none() {
                if global.is_const {
                    // The group enables level 1, which excludes headers.
                    if !checks.const_variables || uses.decl.loc.file != 0 {
                        continue;
                    }
                    format!("unused variable `{name}` [-Wunused-const-variable]")
                } else if checks.variables {
                    format!("unused variable `{name}` [-Wunused-variable]")
                } else {
                    continue;
                }
            } else {
                continue;
            };
            warnings.push(Error::new(uses.decl.loc, warning));
        }
    }
}

#[derive(Default)]
pub(super) struct Labels<'a> {
    declarations: Vec<(&'a str, &'a str, Loc)>,
    references: HashSet<&'a str>,
}

impl<'a> Labels<'a> {
    pub(super) fn reference(&mut self, resolved: &'a str) {
        self.references.insert(resolved);
    }

    pub(super) fn declaration(&mut self, source: &'a str, resolved: &'a str, loc: Loc) {
        self.declarations.push((source, resolved, loc));
    }

    pub(super) fn emit(self, warnings: &mut Vec<Error>) {
        for (source, resolved, loc) in self.declarations {
            if !self.references.contains(resolved) {
                warnings.push(Error::new(
                    loc,
                    format!("unused label `{source}` [-Wunused-label]"),
                ));
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Typedefs {
    declarations: Vec<(usize, Loc)>,
    references: HashSet<usize>,
}

impl Typedefs {
    pub(super) fn declaration(&mut self, id: usize, loc: Loc) {
        self.declarations.push((id, loc));
    }

    pub(super) fn reference(&mut self, id: usize) {
        self.references.insert(id);
    }

    pub(super) fn emit(self, names: &[crate::types::TypedefDef<'_>], warnings: &mut Vec<Error>) {
        for (id, loc) in self.declarations {
            if !self.references.contains(&id) {
                warnings.push(Error::new(
                    loc,
                    format!(
                        "unused local typedef `{}` [-Wunused-local-typedefs]",
                        names[id].name
                    ),
                ));
            }
        }
    }
}

impl<'a> Parser<'a> {
    pub(super) fn unused_local_declaration(&mut self, id: LocalId, loc: Loc, attrs: &Attrs) {
        if let Some(objects) = self
            .fs
            .as_mut()
            .and_then(|state| state.unused_objects.as_mut())
        {
            objects.declaration(id, loc, attrs.unused);
        }
    }

    pub(crate) fn record_store_without_read(&mut self, dest: &Expr<'a>) {
        if let ExprKind::Local(id) = dest.kind
            && let Some(objects) = self
                .fs
                .as_mut()
                .and_then(|state| state.unused_objects.as_mut())
        {
            objects.write(id);
        }
    }

    pub(crate) fn reference_label(&mut self, label: &'a str) {
        if let Some(labels) = self
            .fs
            .as_mut()
            .and_then(|state| state.unused_labels.as_mut())
        {
            labels.reference(label);
        }
    }

    pub(crate) fn discarded_statement_items(&mut self, items: &[Stmt<'a>]) {
        if !(self.language.unused.values || self.language.unused.track_locals()) {
            return;
        }
        for mut item in items {
            while let Stmt::Block([Stmt::Label(_), next]) = item {
                item = next;
            }
            if let Stmt::Expr(expr) = item {
                self.discarded_value(expr);
            }
        }
    }

    pub(crate) fn discarded_value(&mut self, expr: &Expr<'a>) {
        if !(self.language.unused.values || self.language.unused.track_locals()) {
            return;
        }
        self.discard_value(expr, self.language.unused.values);
    }

    fn discard_value(&mut self, expr: &Expr<'a>, diagnose: bool) {
        match expr.kind {
            ExprKind::IncDec { lhs, .. }
            | ExprKind::OpAssign { lhs, .. }
            | ExprKind::PtrOpAssign { lhs, .. } => self.record_store_without_read(lhs),
            // The parser checks the left operand when it consumes the comma.
            ExprKind::Comma(_, right) => self.discard_value(right, diagnose),
            ExprKind::Cond(_, left, right) => {
                self.discard_value(left, false);
                self.discard_value(right, false);
                self.warn_discarded_control_expression(expr, diagnose);
            }
            ExprKind::LogAnd(..) | ExprKind::LogOr(..) => {
                self.warn_discarded_control_expression(expr, diagnose);
            }
            ExprKind::Call(..)
            | ExprKind::Builtin(..)
            | ExprKind::StmtExpr { .. }
            | ExprKind::Assign(..)
            | ExprKind::VaStart(_)
            | ExprKind::VaArg(_) => {}
            _ if diagnose
                && expr.ty != Ty::Void
                && !expr.sugar.is_volatile(&self.typedefs)
                && !matches!(expr.ty, Ty::Atomic(_)) =>
            {
                self.warnings.push(Error::new(
                    expr.loc,
                    "expression result unused [-Wunused-value]",
                ));
            }
            _ => {}
        }
    }

    fn warn_discarded_control_expression(&mut self, expr: &Expr<'a>, diagnose: bool) {
        if diagnose && expr.ty != Ty::Void && !self.may_have_side_effects(expr) {
            self.warnings.push(Error::new(
                expr.loc,
                "expression result unused [-Wunused-value]",
            ));
        }
    }

    // A conditional or short-circuit expression can intentionally guard a
    // side effect. Diagnose it as a whole, not each discarded arm.
    fn may_have_side_effects(&self, expr: &Expr<'a>) -> bool {
        if expr.sugar.is_volatile(&self.typedefs) || matches!(expr.ty, Ty::Atomic(_)) {
            return true;
        }
        match expr.kind {
            ExprKind::Assign(..)
            | ExprKind::IncDec { .. }
            | ExprKind::OpAssign { .. }
            | ExprKind::PtrOpAssign { .. }
            | ExprKind::StmtExpr { .. }
            | ExprKind::Call(..)
            | ExprKind::Builtin(..)
            | ExprKind::VaStart(_)
            | ExprKind::VaArg(_)
            | ExprKind::CompoundLit(..) => true,
            ExprKind::Unary(_, value)
            | ExprKind::Cast(value)
            | ExprKind::UnionCast(value, _)
            | ExprKind::Deref(value)
            | ExprKind::Addr(value)
            | ExprKind::Member { base: value, .. } => self.may_have_side_effects(value),
            ExprKind::Binary(_, left, right)
            | ExprKind::PtrAdd(left, right, _)
            | ExprKind::PtrDiff(left, right, _)
            | ExprKind::VectorIndex(left, right)
            | ExprKind::LogAnd(left, right)
            | ExprKind::LogOr(left, right)
            | ExprKind::Comma(left, right) => {
                self.may_have_side_effects(left) || self.may_have_side_effects(right)
            }
            ExprKind::Cond(cond, left, right) => {
                self.may_have_side_effects(cond)
                    || self.may_have_side_effects(left)
                    || self.may_have_side_effects(right)
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::LabelAddr(_)
            | ExprKind::Local(_)
            | ExprKind::Global(_) => false,
        }
    }
}
