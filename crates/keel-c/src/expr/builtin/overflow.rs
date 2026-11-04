//! Checked-arithmetic overflow builtins.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_{add,sub,mul}_overflow`, with mixed operand types.
    pub(super) fn generic_overflow(&mut self, op: OverflowOp, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(3, loc)?;
        if !args[0].ty.is_integer() || !args[1].ty.is_integer() {
            return Err(Error::new(
                loc,
                "generic overflow operands must be integers",
            ));
        }
        let result_sugar = args[2].sugar.pointee(&self.typedefs);
        fn is_const(sugar: &Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
            match sugar {
                Sugar::Const(_) => true,
                Sugar::Volatile(inner) | Sugar::Restrict(inner) => is_const(inner, defs),
                Sugar::Typedef(i) => is_const(&defs[*i].sugar, defs),
                _ => false,
            }
        }
        if !args[2].ty.is_ptr()
            || !matches!(args[2].ty.pointee(), Some(Ty::Int(k)) if *k != IntKind::Bool)
            || matches!(result_sugar.unqualified(&self.typedefs), Sugar::Enum(_))
            || is_const(&result_sugar, &self.typedefs)
        {
            return Err(Error::new(
                loc,
                "generic overflow result must point to a non-const, non-boolean, non-enum integer",
            ));
        }
        Ok(mk(
            ExprKind::Builtin(Builtin::GenericOverflow(op), self.slice(&args)),
            Ty::Int(IntKind::Bool),
            loc,
        ))
    }

    /// The fixed-type overflow builtins, such as `__builtin_sadd_overflow`.
    pub(super) fn typed_overflow(
        &mut self,
        op: OverflowOp,
        ty: Ty<'static>,
        loc: Loc,
    ) -> R<Expr<'a>> {
        let args = self.builtin_args(3, loc)?;
        if !args[0].ty.is_arith() || !args[1].ty.is_arith() {
            return Err(Error::new(
                loc,
                "overflow operands must be arithmetic values",
            ));
        }
        if !args[2].ty.is_ptr() && !is_null(&args[2]) {
            return Err(Error::new(loc, "overflow result must be a pointer"));
        }
        if let Some(result_ty) = args[2].ty.pointee()
            && *result_ty != ty
            && *result_ty != Ty::Void
        {
            return Err(Error::new(
                loc,
                "overflow result pointer has an incompatible type",
            ));
        }
        let params = [ty, ty, self.ptr_type(ty)];
        let args = args
            .into_iter()
            .zip(&params)
            .map(|(arg, ty)| self.convert_for_assign(arg, ty))
            .collect::<R<Vec<_>>>()?;
        Ok(mk(
            ExprKind::Builtin(Builtin::Overflow(op), self.slice(&args)),
            Ty::Int(IntKind::Bool),
            loc,
        ))
    }
}
