//! Named register declarations and their checked asm-operand bindings.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn local_register_declaration(
        &self,
        spec: &DeclSpec<'a>,
        d: &Declarator<'a>,
        label: &str,
    ) -> R<keel_ir::PhysReg> {
        let loc = d.name.expect("register declarator has a name").1;
        if !spec.register_storage || self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(
                loc,
                "asm labels on automatic locals/register variables require register storage on x86-64 Linux",
            ));
        }
        let invalid = || {
            Error::new(
                loc,
                "asm labels on automatic locals/register variables require a nonreserved x86 GPR (not rsp, rbp or high-byte aliases)",
            )
        };
        let name = label.strip_prefix('%').unwrap_or(label);
        let register = keel_ir::PhysReg::parse(name).map_err(|_| invalid())?;
        if register.arch != keel_ir::AsmArch::X86
            || register.float
            || matches!(register.num, 4 | 5)
            || ["ah", "ch", "dh", "bh"]
                .iter()
                .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            return Err(invalid());
        }
        if !matches!(d.ty, Ty::Int(kind) if kind.size() <= 8) && !d.ty.is_ptr() {
            return Err(Error::new(
                loc,
                "named register variable requires an integer of at most 64 bits or a pointer",
            ));
        }
        Ok(register)
    }

    pub(super) fn asm_local_register(&self, expr: &Expr<'a>) -> Option<keel_ir::PhysReg> {
        if let ExprKind::Local(id) = expr.kind {
            self.fs.as_ref()?.locals[id].asm_register
        } else {
            None
        }
    }

    pub(super) fn stack_pointer_declaration(
        &mut self,
        spec: &DeclSpec<'a>,
        d: &Declarator<'a>,
        label: Option<&str>,
    ) -> R<()> {
        let (name, loc) = d
            .name
            .expect("register declarator name checked by declaration parser");
        if self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(
                loc,
                "global register variables require x86-64 Linux",
            ));
        }
        if label != Some("rsp") {
            return Err(Error::new(
                loc,
                "only the read-only global rsp register is supported",
            ));
        }
        if d.ty != ULONG
            || spec.infer
            || spec.thread_local
            || spec.auto_storage
            || spec.inline
            || spec.is_const
            || spec.is_volatile
            || d.is_const
            || d.is_volatile
            || d.sugar.is_const(&self.typedefs)
            || d.sugar.is_volatile(&self.typedefs)
            || d.sugar.is_restrict(&self.typedefs)
        {
            return Err(Error::new(
                loc,
                "global rsp register requires unqualified unsigned long without other storage classes",
            ));
        }
        if d.attrs != Attrs::default() {
            return Err(Error::new(
                loc,
                "global rsp register does not support declaration attributes",
            ));
        }
        if self.is("=") {
            return Err(Error::new(
                loc,
                "global register variables cannot have initializers",
            ));
        }
        if self
            .lookup(name)
            .is_some_and(|s| !matches!(s, Sym::StackPointer))
        {
            return Err(Error::new(
                loc,
                "global register variable conflicts with an existing declaration",
            ));
        }
        self.bind(name, Sym::StackPointer);
        Ok(())
    }
}
