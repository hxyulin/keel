//! Compile-time query builtins: offsets, object sizes, attributes and types.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_offsetof`.
    pub(super) fn offsetof(&mut self, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let ty = self.type_name()?;
        self.expect(",")?;
        let mut ty = ty;
        let layout = self.c_layout;
        let constant = |value: u64| {
            mk(
                ExprKind::Int(wrap_literal(value as i64, &ULONG, layout)),
                ULONG,
                loc,
            )
        };
        let mut offset = constant(0);
        let mut first = true;
        loop {
            if !first && self.eat("[") {
                let index = self.expr()?;
                self.expect("]")?;
                let Ty::Array(array) = ty else {
                    return Err(Error::new(loc, "`offsetof` subscript of a non-array"));
                };
                let index = self.rvalue(index);
                if !index.ty.is_integer() {
                    return Err(Error::new(
                        index.loc,
                        "`offsetof` array index must be an integer",
                    ));
                }
                // Compute a byte offset in size_t arithmetic. No object
                // exists here, so array subscripts must not create a
                // pointer or dereference memory. Negative indices wrap
                // in the target's unsigned size domain, as GNU does.
                self.elem_size(&ty, loc)?;
                let index = self.cast(index, ULONG);
                let stride = self.size_expr(&array.elem, loc);
                let scaled = self.binop("*", index, stride, loc)?;
                offset = self.binop("+", offset, scaled, loc)?;
                ty = array.elem;
                continue;
            }
            if !offset_member_follows(self, first) {
                if first {
                    return Err(Error::new(loc, "`offsetof` requires a member designator"));
                }
                break;
            }
            first = false;
            let name = self.ident()?;
            let Ty::Struct(id) = ty else {
                return Err(Error::new(loc, "`offsetof` member of a non-struct"));
            };
            let Some((off, m)) =
                self.structs[id].find(name, &self.structs, &self.typedefs, self.arena)
            else {
                return Err(Error::new(loc, format!("no member `{name}`")));
            };
            if m.bits.is_some() {
                return Err(Error::new(loc, "`offsetof` cannot name a bit-field"));
            }
            offset = self.binop("+", offset, constant(off), loc)?;
            ty = m.ty;
        }
        self.expect(")")?;
        // Keep static designators as integer constants for enum values,
        // initializers and existing ICE consumers. The checked-operand
        // evaluator preserves side effects in runtime index expressions.
        Ok(
            match eval_constant_with_layout(&offset, true, layout).and_then(Const::int) {
                Some(value) => mk(ExprKind::Int(value), ULONG, loc),
                _ => offset,
            },
        )
    }

    /// `__builtin_object_size` and `__builtin_dynamic_object_size`.
    pub(super) fn object_size(&mut self, loc: Loc) -> R<Expr<'a>> {
        // The size is never known here: "unknown" is -1 for types 0
        // and 1 (maximum), 0 for types 2 and 3 (minimum).
        let args = self.builtin_args(2, loc)?;
        let Some(kind) = eval_int_with_layout(&args[1], self.c_layout) else {
            return Err(Error::new(
                loc,
                "`__builtin_object_size` type must be constant",
            ));
        };
        let v = if kind & 2 == 0 { -1 } else { 0 };
        Ok(mk(ExprKind::Int(v), ULONG, loc))
    }

    /// `__builtin_has_attribute`.
    pub(super) fn has_attribute(&mut self, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let nonstring = if self.is_typename() {
            self.type_name()?;
            false
        } else {
            let operand = self.assign()?;
            self.has_nonstring(&operand)
        };
        self.expect(",")?;
        let attribute = self.ident()?;
        if attribute.trim_matches('_') != "nonstring" {
            return Err(Error::new(
                loc,
                "__builtin_has_attribute currently supports only nonstring queries",
            ));
        }
        self.expect(")")?;
        Ok(mk(
            ExprKind::Int(i64::from(nonstring)),
            Ty::Int(IntKind::Bool),
            loc,
        ))
    }

    /// `__builtin_types_compatible_p`.
    pub(super) fn types_compatible(&mut self, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let a = self.type_name()?;
        self.expect(",")?;
        let b = self.type_name()?;
        self.expect(")")?;
        let same = a.unatomic() == b.unatomic();
        Ok(mk(ExprKind::Int(i64::from(same)), INT, loc))
    }

    /// `__builtin_choose_expr`.
    pub(super) fn choose_expr(&mut self, loc: Loc) -> R<Expr<'a>> {
        // Like `?:` with a constant condition, but the chosen
        // operand keeps its own type.
        self.expect("(")?;
        let c = self.assign()?;
        let Some(c) = eval_int_with_layout(&c, self.c_layout) else {
            return Err(Error::new(loc, "`__builtin_choose_expr` needs a constant"));
        };
        self.expect(",")?;
        let a = self.assign()?;
        self.expect(",")?;
        let b = self.assign()?;
        self.expect(")")?;
        Ok(if c != 0 { a } else { b })
    }

    /// `__builtin_constant_p`.
    pub(super) fn constant_p(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(1, loc)?;
        let known = args[0].ty.is_integer()
            && matches!(
                eval_constant_with_layout(&args[0], true, self.c_layout),
                Some(Const::Int(_))
            );
        Ok(mk(ExprKind::Int(i64::from(known)), INT, loc))
    }
}
