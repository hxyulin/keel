//! Module symbol queries answered from one index instead of a scan of every
//! function and data object for each symbol reference.
use keel_ir::fxhash::FxHashMap;
use keel_ir::{DataObject, Module};

#[derive(Clone, Copy, Default)]
struct Entry {
    function: bool,
    /// A function definition that is `weak_odr` and not hidden.
    exported_weak_odr: bool,
    external: bool,
    /// The first data object with this name, as `Module::data_object` finds.
    data: Option<u32>,
}

pub(crate) struct Symbols<'m> {
    pub module: &'m Module,
    index: FxHashMap<&'m str, Entry>,
    thread_locals: bool,
}

impl<'m> Symbols<'m> {
    pub fn new(module: &'m Module) -> Self {
        let mut index = FxHashMap::<&str, Entry>::default();
        for f in &module.functions {
            let entry = index.entry(&f.name).or_default();
            entry.function = !f.attrs.weak;
            entry.exported_weak_odr = f.weak_odr && !f.hidden;
        }
        for d in &module.decls {
            index.entry(&d.name).or_default().external = true;
        }
        for (i, d) in module.data.iter().enumerate() {
            let entry = index.entry(&d.name).or_default();
            entry.data.get_or_insert(i as u32);
        }
        let thread_locals = module.data.iter().any(|d| d.thread_local);
        Self {
            module,
            index,
            thread_locals,
        }
    }

    /// Whether any data object is thread-local.
    pub fn has_thread_locals(&self) -> bool {
        self.thread_locals
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Same as [`Module::data_object`].
    pub fn data_object(&self, name: &str) -> Option<&'m DataObject> {
        let i = self.index.get(name)?.data?;
        Some(&self.module.data[i as usize])
    }

    /// Same as [`Module::is_external`].
    pub fn is_external(&self, name: &str) -> bool {
        self.index.get(name).is_some_and(|e| e.external)
    }

    /// Same as [`Module::defines`].
    pub fn defines(&self, name: &str) -> bool {
        self.index.get(name).is_some_and(|e| e.function)
            || self
                .data_object(name)
                .is_some_and(|d| d.init.is_some() && !d.weak)
    }

    /// Whether `name` is a `weak_odr` definition that is not hidden. A
    /// Mach-O image exports it as a weak definition, which dyld may coalesce
    /// with another image's, so code there must take its address from the
    /// GOT. Hidden ones coalesce when the image is linked.
    pub fn exports_weak_odr(&self, name: &str) -> bool {
        self.index.get(name).is_some_and(|e| e.exported_weak_odr)
            || self
                .data_object(name)
                .is_some_and(|d| d.weak_odr && !d.hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_module_queries() {
        let module = keel_ir::parse(
            "decl @ext(i32) -> i32\n\
             data @a = align 4 { i32 1 }\n\
             decl data @b\n\
             func @f() {\nblock0:\n return\n}\n\
             data @odr = weak_odr align 8 { i64 1 }\n\
             data @hidden_odr = weak_odr hidden align 8 { i64 1 }\n\
             func @g() weak_odr {\nblock0:\n return\n}\n\
             func @hidden_g() weak_odr hidden {\nblock0:\n return\n}\n",
        )
        .unwrap();
        let symbols = Symbols::new(&module);
        for (name, exported) in [
            ("odr", true),
            ("g", true),
            ("hidden_odr", false),
            ("hidden_g", false),
            ("a", false),
            ("f", false),
            ("ext", false),
        ] {
            assert_eq!(symbols.exports_weak_odr(name), exported, "{name}");
        }
        for name in ["ext", "a", "b", "f", "missing"] {
            assert_eq!(
                symbols.is_external(name),
                module.is_external(name),
                "{name}"
            );
            assert_eq!(symbols.defines(name), module.defines(name), "{name}");
            assert_eq!(
                symbols.data_object(name).map(|d| &d.name),
                module.data_object(name).map(|d| &d.name),
                "{name}"
            );
        }
    }
}
