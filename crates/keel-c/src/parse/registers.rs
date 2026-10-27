//! The bounded read-only x86 stack-register declaration.

use super::*;

impl<'a> Parser<'a> {
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
