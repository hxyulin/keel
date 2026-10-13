//! The root module: one source file's external declarations and functions.

use crate::{FuncDecl, Function, Signature};
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct Module {
    /// Functions defined elsewhere, e.g. in libc.
    pub decls: Vec<FuncDecl>,
    pub functions: Vec<Function>,
}

impl Module {
    /// The signature of a declared or defined function.
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        let decl = self.decls.iter().find(|d| d.name == name).map(|d| &d.sig);
        decl.or_else(|| {
            self.functions
                .iter()
                .find(|f| f.name == name)
                .map(|f| &f.sig)
        })
    }

    pub fn is_external(&self, name: &str) -> bool {
        self.decls.iter().any(|d| d.name == name)
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for decl in &self.decls {
            writeln!(f, "decl @{}{}", decl.name, decl.sig)?;
        }
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 || !self.decls.is_empty() {
                f.write_str("\n")?;
            }
            write!(f, "{func}")?;
        }
        Ok(())
    }
}
