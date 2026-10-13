//! The root module: one source file's external declarations, data and
//! functions.

use crate::{DataObject, FuncDecl, Function, Signature};
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct Module {
    /// Functions defined elsewhere, e.g. in libc.
    pub decls: Vec<FuncDecl>,
    /// Data objects, defined here or declared (`decl data`).
    pub data: Vec<DataObject>,
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

    pub fn data_object(&self, name: &str) -> Option<&DataObject> {
        self.data.iter().find(|d| d.name == name)
    }

    /// Whether `name` is a function declared with `decl`.
    pub fn is_external(&self, name: &str) -> bool {
        self.decls.iter().any(|d| d.name == name)
    }

    /// Whether `name` is a function or data object defined in this module,
    /// so its address is fixed relative to this module's code.
    pub fn defines(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f.name == name)
            || self.data_object(name).is_some_and(|d| d.init.is_some())
    }

    /// Whether `name` is any function or data object, here or elsewhere.
    pub fn has_symbol(&self, name: &str) -> bool {
        self.signature(name).is_some() || self.data_object(name).is_some()
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for decl in &self.decls {
            writeln!(f, "decl @{}{}", decl.name, decl.sig)?;
        }
        for data in &self.data {
            writeln!(f, "{data}")?;
        }
        let header = !self.decls.is_empty() || !self.data.is_empty();
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 || header {
                f.write_str("\n")?;
            }
            write!(f, "{func}")?;
        }
        Ok(())
    }
}
