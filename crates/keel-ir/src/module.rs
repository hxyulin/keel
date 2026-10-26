//! The root module: one source file's external declarations, data and
//! functions.

use crate::debug::{DebugGlobal, DebugTypeData, DebugTypeLine};
use crate::{DataObject, EntityRef, FuncDecl, Function, Signature, SourceFile};
use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct Module {
    /// Ordered GNU assembly fragments, emitted before generated definitions.
    /// Currently supported by the native x86-64 Linux backend only.
    pub assembly: Vec<String>,
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
    /// The source language, for debuggers.
    pub debug_language: Option<crate::Language>,
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

    pub fn function_attrs(&self, name: &str) -> Option<crate::FunctionAttrs> {
        self.decls
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.attrs)
            .or_else(|| {
                self.functions
                    .iter()
                    .find(|f| f.name == name)
                    .map(|f| f.attrs)
            })
    }

    /// Refresh direct callee contracts after constructing a complete module.
    /// A name defined more than once takes its last entry: functions after
    /// declarations, later ones after earlier ones.
    pub fn sync_function_attrs(&mut self) {
        let mut attrs =
            std::collections::HashMap::with_capacity(self.decls.len() + self.functions.len());
        for (name, a) in self
            .decls
            .iter()
            .map(|d| (d.name.as_str(), d.attrs))
            .chain(self.functions.iter().map(|f| (f.name.as_str(), f.attrs)))
        {
            attrs.insert(name, a);
        }
        // Only callee tables change, so every lookup sees the original attrs.
        let attrs = &attrs;
        let updates: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .flat_map(|(i, f)| {
                f.callees()
                    .filter_map(move |(r, d)| attrs.get(d.name.as_str()).map(|&a| (i, r, a)))
            })
            .collect();
        for (i, r, a) in updates {
            self.functions[i].callee_mut(r).attrs = a;
        }
    }

    pub fn data_object(&self, name: &str) -> Option<&DataObject> {
        self.data.iter().find(|d| d.name == name)
    }

    /// Whether `name` is a function declared with `decl`.
    pub fn is_external(&self, name: &str) -> bool {
        self.decls.iter().any(|d| d.name == name)
    }

    /// Whether `name` has a definition codegen may address directly.
    /// Ordinary weak definitions must use symbol binding instead.
    pub fn defines(&self, name: &str) -> bool {
        self.functions
            .iter()
            .any(|f| f.name == name && !f.attrs.weak)
            || self
                .data_object(name)
                .is_some_and(|d| d.init.is_some() && !d.weak)
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
        if let Some(language) = self.debug_language {
            writeln!(f, "dlanguage {language}")?;
        }
        for assembly in &self.assembly {
            f.write_str("module_asm ")?;
            crate::data::write_quoted(f, assembly.as_bytes())?;
            f.write_str("\n")?;
        }
        for decl in &self.decls {
            writeln!(f, "decl @{}{}{}", decl.name, decl.sig, decl.attrs)?;
        }
        for data in &self.data {
            writeln!(f, "{data}")?;
        }
        let header = !self.assembly.is_empty()
            || !self.files.is_empty()
            || !self.decls.is_empty()
            || !self.data.is_empty()
            || !self.debug_types.is_empty()
            || self.debug_language.is_some();
        for (i, func) in self.functions.iter().enumerate() {
            if i > 0 || header {
                f.write_str("\n")?;
            }
            write!(f, "{func}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FuncDecl, FunctionAttrs, Type};

    fn attrs(noreturn: bool, noinline: bool) -> FunctionAttrs {
        FunctionAttrs {
            noreturn,
            noinline,
            ..FunctionAttrs::default()
        }
    }

    #[test]
    fn synced_callee_attrs_take_the_last_entry_for_a_name() {
        let sig = Signature::new(&[Type::I64], &[]);
        let decl = |name: &str, a| FuncDecl {
            attrs: a,
            name: name.into(),
            sig: sig.clone(),
        };
        let mut caller = Function::new("caller", sig.clone());
        caller.attrs = attrs(false, true);
        let stale = attrs(true, true);
        for name in ["both", "twice", "only_decl", "elsewhere", "caller"] {
            let r = caller.import_function(name, sig.clone());
            caller.callee_mut(r).attrs = stale;
        }
        let mut both = Function::new("both", sig.clone());
        both.attrs = attrs(false, true);
        let mut module = Module {
            decls: vec![
                decl("both", attrs(true, false)),
                decl("twice", attrs(true, false)),
                decl("twice", attrs(false, true)),
                decl("only_decl", FunctionAttrs::default()),
            ],
            functions: vec![caller, both],
            ..Module::default()
        };
        module.sync_function_attrs();
        let got: Vec<_> = module.functions[0]
            .callees()
            .map(|(_, d)| (d.name.as_str(), d.attrs))
            .collect();
        assert_eq!(
            got,
            [
                ("both", attrs(false, true)),
                ("twice", attrs(false, true)),
                ("only_decl", FunctionAttrs::default()),
                ("elsewhere", stale),
                ("caller", attrs(false, true)),
            ]
        );
    }
}
