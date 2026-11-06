//! Whole-program merging: several modules become one, as a linker would
//! combine their objects, so module passes see every definition.

use crate::{DataItem, DataObject, InstData, Module};
use std::collections::{HashMap, HashSet};

/// Merges `modules` into one. A local symbol whose name another module also
/// uses is renamed `name.N`, for module N. Of several definitions of a global
/// name, a strong one replaces weak ones and the first weak one wins among
/// them; two strong ones are an error. Declarations of defined names go.
///
/// Modules with debug info are rejected, as are renames of a symbol that
/// assembly text may name.
pub fn merge_modules(modules: Vec<Module>) -> Result<Module, String> {
    if modules
        .iter()
        .any(|m| !m.files.is_empty() || !m.debug_types.is_empty() || !m.debug_globals.is_empty())
    {
        return Err("merging modules with debug info is not supported".into());
    }
    let mentions: Vec<HashSet<String>> = modules.iter().map(mentioned).collect();
    let mut taken: HashSet<String> = mentions.iter().flatten().cloned().collect();
    let mut merged = Module::default();
    let mut functions: HashMap<String, usize> = HashMap::new();
    let mut data: HashMap<String, usize> = HashMap::new();
    let mut decls: HashMap<String, usize> = HashMap::new();
    for (index, mut module) in modules.into_iter().enumerate() {
        let elsewhere = |name: &str| {
            mentions
                .iter()
                .enumerate()
                .any(|(j, names)| j != index && names.contains(name))
        };
        let locals = module
            .functions
            .iter()
            .filter(|f| f.local)
            .map(|f| f.name.as_str())
            .chain(
                module
                    .data
                    .iter()
                    .filter(|d| d.local)
                    .map(|d| d.name.as_str()),
            );
        let mut renames = HashMap::new();
        for name in locals.filter(|name| elsewhere(name)) {
            if assembly_texts(&module).any(|text| text.contains(name)) {
                return Err(format!(
                    "cannot rename local `{name}` of module {index}: assembly may name it"
                ));
            }
            let mut fresh = format!("{name}.{index}");
            while taken.contains(&fresh) {
                fresh.push('_');
            }
            taken.insert(fresh.clone());
            renames.insert(name.to_string(), fresh);
        }
        if !renames.is_empty() {
            rename(&mut module, &renames);
        }

        merged.assembly.append(&mut module.assembly);
        merged.debug_language = merged.debug_language.or(module.debug_language);
        for func in module.functions {
            if !func.local {
                if let Some(&at) = functions.get(&func.name) {
                    let old = &merged.functions[at];
                    match (
                        old.attrs.weak || old.weak_odr,
                        func.attrs.weak || func.weak_odr,
                    ) {
                        (_, true) => {}
                        (true, false) => merged.functions[at] = func,
                        (false, false) => {
                            return Err(format!("`{}` is defined more than once", func.name));
                        }
                    }
                    continue;
                }
                functions.insert(func.name.clone(), merged.functions.len());
            }
            merged.functions.push(func);
        }
        for object in module.data {
            if object.local {
                merged.data.push(object);
                continue;
            }
            let Some(&at) = data.get(&object.name) else {
                data.insert(object.name.clone(), merged.data.len());
                merged.data.push(object);
                continue;
            };
            let old = &mut merged.data[at];
            match (old.init.is_some(), object.init.is_some()) {
                (_, false) => old.weak &= object.weak || old.init.is_some(),
                (false, true) => *old = object,
                (true, true) => match (weak(old), weak(&object)) {
                    (_, true) => {}
                    (true, false) => *old = object,
                    (false, false) => {
                        return Err(format!("`{}` is defined more than once", object.name));
                    }
                },
            }
        }
        for decl in module.decls {
            match decls.get(&decl.name) {
                Some(&at) => merged.decls[at].attrs.weak &= decl.attrs.weak,
                None => {
                    decls.insert(decl.name.clone(), merged.decls.len());
                    merged.decls.push(decl);
                }
            }
        }
    }
    merged.decls.retain(|d| !functions.contains_key(&d.name));
    merged.sync_function_attrs();
    Ok(merged)
}

/// Makes every global definition local except those `exported` keeps,
/// those that assembly text may name, and constructors, destructors and
/// retained ones. Weak definitions become strong: no other definition can
/// replace them now.
pub fn internalize(module: &mut Module, exported: impl Fn(&str) -> bool) {
    let texts: Vec<String> = assembly_texts(module).map(str::to_string).collect();
    let keep = |name: &str| exported(name) || texts.iter().any(|t| t.contains(name));
    for func in &mut module.functions {
        let a = &func.attrs;
        if func.local || keep(&func.name) || a.constructor || a.destructor || a.retain || a.used {
            continue;
        }
        func.local = true;
        func.hidden = false;
        func.weak_odr = false;
        func.attrs.weak = false;
    }
    for object in &mut module.data {
        if object.local || object.init.is_none() || object.retain || keep(&object.name) {
            continue;
        }
        object.local = true;
        object.hidden = false;
        object.weak_odr = false;
        object.weak = false;
    }
    module.sync_function_attrs();
}

fn weak(object: &DataObject) -> bool {
    object.weak || object.weak_odr
}

/// Module and inline assembly text.
fn assembly_texts(module: &Module) -> impl Iterator<Item = &str> {
    let inline = module.functions.iter().flat_map(|f| {
        f.blocks()
            .flat_map(move |b| f.block_insts(b).iter())
            .filter_map(move |&inst| match f.inst_data(inst) {
                InstData::InlineAsm { asm, .. } => Some(asm.template.as_str()),
                _ => None,
            })
    });
    module.assembly.iter().map(String::as_str).chain(inline)
}

/// Every symbol name a module defines or refers to.
fn mentioned(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    names.extend(module.decls.iter().map(|d| d.name.clone()));
    for object in &module.data {
        names.insert(object.name.clone());
        for item in object.init.iter().flat_map(|i| &i.items) {
            if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item {
                names.insert(symbol.clone());
            }
        }
    }
    for func in &module.functions {
        names.insert(func.name.clone());
        names.extend(func.callees().map(|(_, d)| d.name.clone()));
        names.extend(func.data_symbols().map(|(_, n)| n.to_string()));
        names.extend(func.personality.clone());
    }
    names
}

fn rename(module: &mut Module, renames: &HashMap<String, String>) {
    let new = |name: &str| renames.get(name).cloned();
    for decl in &mut module.decls {
        if let Some(name) = new(&decl.name) {
            decl.name = name;
        }
    }
    for object in &mut module.data {
        if let Some(name) = new(&object.name) {
            object.name = name;
        }
        for item in object.init.iter_mut().flat_map(|i| &mut i.items) {
            if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item
                && let Some(name) = new(symbol)
            {
                *symbol = name;
            }
        }
    }
    for func in &mut module.functions {
        if let Some(name) = new(&func.name) {
            func.name = name;
        }
        func.rename_symbols(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn merge(texts: &[&str]) -> Result<Module, String> {
        merge_modules(texts.iter().map(|t| parse(t).unwrap()).collect())
    }

    #[test]
    fn colliding_locals_are_renamed_and_globals_resolve_across_modules() {
        let a = "decl @shared() -> i32\n\nfunc @helper() -> i32 local {\nblock0:\n    v0 = iconst.i32 1\n    return v0\n}\n\nfunc @main() -> i32 {\nblock0:\n    v0 = call @helper()\n    v1 = call @shared()\n    v2 = iadd v0, v1\n    return v2\n}\n";
        let b = "func @helper() -> i32 local {\nblock0:\n    v0 = iconst.i32 2\n    return v0\n}\n\nfunc @shared() -> i32 {\nblock0:\n    v0 = call @helper()\n    return v0\n}\n";
        let m = merge(&[a, b]).unwrap();
        let names: Vec<_> = m.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["helper.0", "main", "helper.1", "shared"]);
        assert!(m.decls.is_empty());
        let callees = |i: usize| -> Vec<String> {
            m.functions[i]
                .callees()
                .map(|(_, d)| d.name.clone())
                .collect()
        };
        assert_eq!(callees(1), ["helper.0", "shared"]);
        assert_eq!(callees(3), ["helper.1"]);
        crate::verify_module(&m).unwrap();
        assert_eq!(parse(&m.to_string()).unwrap().to_string(), m.to_string());
    }

    #[test]
    fn strong_definitions_replace_weak_ones_and_duplicates_are_errors() {
        let weak = "func @f() -> i32 weak {\nblock0:\n    v0 = iconst.i32 1\n    return v0\n}\n";
        let strong = "func @f() -> i32 {\nblock0:\n    v0 = iconst.i32 2\n    return v0\n}\n";
        let m = merge(&[weak, strong, weak]).unwrap();
        assert_eq!(m.functions.len(), 1);
        assert!(!m.functions[0].attrs.weak);
        let err = merge(&[strong, strong]).unwrap_err();
        assert!(err.contains("`f` is defined more than once"), "{err}");
        let data = "data @x = align 4 { i32 7 }\n";
        let decl = "decl data @x\n";
        let m = merge(&[decl, data]).unwrap();
        assert_eq!(m.data.len(), 1);
        assert!(m.data[0].init.is_some());
    }

    #[test]
    fn internalize_keeps_exports_and_assembly_names() {
        let text = "module_asm \".globl _asm_named\"\n\nfunc @main() -> i32 {\nblock0:\n    v0 = iconst.i32 0\n    return v0\n}\n\nfunc @asm_named() -> i32 weak {\nblock0:\n    v0 = iconst.i32 0\n    return v0\n}\n\nfunc @other() -> i32 weak {\nblock0:\n    v0 = iconst.i32 0\n    return v0\n}\n";
        let mut m = parse(text).unwrap();
        internalize(&mut m, |name| name == "main");
        let local: Vec<_> = m
            .functions
            .iter()
            .map(|f| (f.local, f.attrs.weak))
            .collect();
        assert_eq!(local, [(false, false), (false, true), (true, false)]);
    }
}
