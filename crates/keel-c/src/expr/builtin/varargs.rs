//! Variable-argument builtins.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_va_start`, `va_arg`, `va_end` and `va_copy`.
    pub(super) fn va_builtin(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let ap = self.assign()?;
        let e = match base {
            "va_start" => {
                self.expect(",")?;
                self.assign()?;
                let ap = self.va_list_addr(ap)?;
                mk(ExprKind::VaStart(self.node(ap)), Ty::Void, loc)
            }
            "va_arg" => {
                self.expect(",")?;
                let ty = self.type_name()?;
                if ty.is_complex() {
                    return Err(Error::new(loc, "complex va_arg is not supported"));
                }
                if !ty.is_scalar() {
                    return Err(Error::new(loc, "`va_arg` of a non-scalar"));
                }
                let ap = self.va_list_addr(ap)?;
                mk(ExprKind::VaArg(self.node(ap)), ty, loc)
            }
            // SysV's va_list is an array, so copying is copying the
            // struct it holds.
            "va_copy" if self.target == Target::X86_64LinuxGnu => {
                self.expect(",")?;
                let src = self.assign()?;
                let [dst, src] = [ap, src].map(|e| -> R<_> {
                    let p = self.va_list_addr(e)?;
                    let ty = p.ty.pointee().cloned().expect("a pointer");
                    Ok(mk(ExprKind::Deref(self.node(p)), ty, loc))
                });
                self.assignment("=", dst?, src?, loc)?
            }
            "va_copy" => {
                self.expect(",")?;
                let src = self.assign()?;
                self.assignment("=", ap, src, loc)?
            }
            _ => self.cast(ap, Ty::Void),
        };
        self.expect(")")?;
        Ok(e)
    }
}
