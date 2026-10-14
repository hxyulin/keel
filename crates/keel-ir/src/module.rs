//! The root module: one source file's external declarations, data and
//! functions.

use crate::debug::{DebugGlobal, DebugTypeData, DebugTypeLine};
use crate::{DataObject, EntityRef, FuncDecl, Function, Signature, SourceFile};
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct Module {
    /// Functions defined elsewhere, e.g. in libc.
    pub decls: Vec<FuncDecl>,
    /// Data objects, defined here or declared (`decl data`).
    pub data: Vec<DataObject>,
    pub functions: Vec<Function>,
    /// The source files instruction locations refer to, indexed by
    /// [`SourceFile`]. Code generation emits debug info if there are any.
    pub files: Vec<String>,
    /// Source-level types for debuggers, indexed by [`crate::DebugType`].
    pub debug_types: Vec<DebugTypeData>,
    /// Source-level variables with static storage.
    pub debug_globals: Vec<DebugGlobal>,
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

    /// Adds a source file for instruction locations to refer to.
    pub fn add_file(&mut self, path: impl Into<String>) -> SourceFile {
        self.files.push(path.into());
        SourceFile::new(self.files.len() - 1)
    }

    /// Whether `name` is any function or data object, here or elsewhere.
    pub fn has_symbol(&self, name: &str) -> bool {
        self.signature(name).is_some() || self.data_object(name).is_some()
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, path) in self.files.iter().enumerate() {
            write!(f, "file {} = ", SourceFile::new(i))?;
            crate::data::write_quoted(f, path.as_bytes())?;
            f.write_str("\n")?;
        }
        for (i, ty) in self.debug_types.iter().enumerate() {
            writeln!(f, "{}", DebugTypeLine(i, ty))?;
        }
        for global in &self.debug_globals {
            writeln!(f, "{global}")?;
        }
        for decl in &self.decls {
            writeln!(f, "decl @{}{}", decl.name, decl.sig)?;
        }
        for data in &self.data {
            writeln!(f, "{data}")?;
        }
        let header = !self.files.is_empty()
            || !self.decls.is_empty()
            || !self.data.is_empty()
            || !self.debug_types.is_empty();
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 || header {
                f.write_str("\n")?;
            }
            write!(f, "{func}")?;
        }
        Ok(())
    }
}
