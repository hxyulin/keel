//! Memory builtins: copy, move, fill and compare.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_memcpy` and its relatives, as calls or as function designators.
    pub(super) fn library_memory_builtin(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        if !self.is("(") {
            // Library builtins also denote their ordinary library
            // function when used as an address or parenthesized callee.
            let base = self.text.intern(base);
            let function = self.lookup_value(base, loc)?;
            if !matches!(function.ty, Ty::Func(_)) {
                return Err(Error::new(loc, "memory builtin needs a library function"));
            }
            return Ok(function);
        }
        self.memory_builtin(base, false, loc)
    }

    /// The fortified `__builtin___memcpy_chk` family, which keeps its bound.
    pub(super) fn checked_memory_builtin(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let plain = base
            .strip_prefix("__")
            .unwrap()
            .strip_suffix("_chk")
            .unwrap();
        self.memory_builtin(plain, true, loc)
    }

    /// These explicit names have standard prototypes independent of whether
    /// the translation unit declares the corresponding ordinary libc symbol.
    fn memory_builtin(&mut self, name: &str, checked: bool, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(if checked { 4 } else { 3 }, loc)?;
        let fill = name == "memset";
        for (i, arg) in args.iter().enumerate() {
            if i >= 2 || (i == 1 && fill) {
                if !arg.ty.is_arith() {
                    return Err(Error::new(
                        arg.loc,
                        "memory builtin needs an arithmetic argument",
                    ));
                }
                continue;
            }
            let null = arg.ty.is_integer()
                && eval_constant_with_layout(arg, false, self.c_layout).and_then(Const::int)
                    == Some(0);
            if (!arg.ty.is_ptr() && !null) || matches!(arg.ty.pointee(), Some(Ty::Func(_))) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin needs an object pointer",
                ));
            }
            let pointee = arg.sugar.pointee(&self.typedefs);
            if pointee.is_volatile(&self.typedefs) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin cannot discard volatile qualification",
                ));
            }
            if i == 0 && name != "memcmp" && memory_builtin_const(&pointee, &self.typedefs) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin destination must not be const",
                ));
            }
        }
        let ptr = self.ptr_type(Ty::Void);
        let mut params = vec![ptr, if fill { INT } else { ptr }, ULONG];
        if checked {
            params.push(ULONG);
        }
        let args = args
            .into_iter()
            .zip(&params)
            .map(|(arg, ty)| self.convert_for_assign(arg, ty))
            .collect::<R<Vec<_>>>()?;
        let id = match name {
            "memcpy" => Intrinsic::Memcpy,
            "memmove" => Intrinsic::Memmove,
            "memcmp" => Intrinsic::Memcmp,
            _ => Intrinsic::Memset,
        };
        let result = if name == "memcmp" { INT } else { ptr };
        Ok(mk(
            ExprKind::Builtin(Builtin::Memory(id), self.slice(&args)),
            result,
            loc,
        ))
    }
}
