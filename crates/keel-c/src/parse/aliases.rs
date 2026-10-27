//! Checked function aliases, resolved after all declarations and asm labels.

use super::*;

impl Parser<'_> {
    pub(super) fn function_alias(&mut self, g: usize, attrs: Attrs, file_scope: bool) -> R<()> {
        if let Some(loc) = attrs.alias_conflict {
            return Err(Error::new(loc, "duplicate alias attribute"));
        }
        let Some((target, loc)) = attrs.alias else {
            return Ok(());
        };
        if self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(loc, "function aliases require x86-64 Linux"));
        }
        if !file_scope {
            return Err(Error::new(
                loc,
                "alias requires a file-scope function declaration",
            ));
        }
        if self.aliases.insert(g, (target, loc)).is_some() {
            return Err(Error::new(loc, "duplicate alias definition"));
        }
        Ok(())
    }

    pub(super) fn resolve_aliases(&mut self) -> R<()> {
        // Source order makes diagnostics and generated assembly deterministic.
        let mut aliases: Vec<_> = self.aliases.keys().copied().collect();
        aliases.sort_unstable();
        let mut targets = HashMap::default();
        for &g in &aliases {
            let (name, loc) = self.aliases[&g];
            let global = &self.globals[g];
            if global.func.is_some() {
                return Err(Error::new(loc, "alias cannot also have a function body"));
            }
            if global.lifecycle.is_some() || global.retain.is_some() || global.section.is_some() {
                return Err(Error::new(
                    loc,
                    "alias cannot carry lifecycle, retain or section attributes",
                ));
            }
            if self
                .globals
                .iter()
                .enumerate()
                .any(|(i, other)| i != g && other.symbol == global.symbol)
            {
                return Err(Error::new(
                    loc,
                    "alias assembler name conflicts with another declaration",
                ));
            }
            let target_name = self.alias_names[name];
            let mut matches = self
                .globals
                .iter()
                .enumerate()
                .filter(|(_, other)| other.symbol == target_name);
            let Some((target, other)) = matches.next() else {
                return Err(Error::new(
                    loc,
                    format!("alias target `{target_name}` is not defined in this translation unit"),
                ));
            };
            if matches.next().is_some() {
                return Err(Error::new(loc, "alias target assembler name is ambiguous"));
            }
            if !matches!((global.ty, other.ty), (Ty::Func(_), Ty::Func(_)))
                || !global.ty.compatible(&other.ty)
                || global.incompatible_return
                || other.incompatible_return
            {
                return Err(Error::new(
                    loc,
                    "alias and target require compatible function types",
                ));
            }
            if global.returns_twice != other.returns_twice
                || (global.function_attrs.noreturn && !other.function_attrs.noreturn)
                || (global.function_attrs.nounwind && !other.function_attrs.nounwind)
            {
                return Err(Error::new(
                    loc,
                    "alias and target have incompatible function contracts",
                ));
            }
            for name in [global.symbol, target_name] {
                if !plain_symbol(name) {
                    return Err(Error::new(
                        loc,
                        "alias requires plain assembler symbol names",
                    ));
                }
            }
            targets.insert(g, target);
        }
        for &g in &aliases {
            let (_, loc) = self.aliases[&g];
            let mut current = g;
            let mut seen = HashSet::default();
            while let Some(&target) = targets.get(&current) {
                if !seen.insert(current) {
                    return Err(Error::new(loc, "cycle in function aliases"));
                }
                current = target;
            }
            if self.globals[current].func.is_none() {
                return Err(Error::new(
                    loc,
                    "alias target requires a function definition in this translation unit",
                ));
            }
            // Assembly references are invisible to the IR reachability pass.
            // Keep the ultimate definition even when it has internal linkage.
            self.globals[current].used = true;
            self.globals[current].function_attrs.used = true;
        }
        for g in aliases {
            let target = targets[&g];
            let global = &mut self.globals[g];
            global.defined = true;
            let name = global.symbol;
            let binding = if global.internal {
                ".local"
            } else if global.weak.is_some() {
                ".weak"
            } else {
                ".globl"
            };
            let mut text = format!("{binding} {name}\n.type {name}, @function\n");
            if global.hidden() {
                text.push_str(&format!(".hidden {name}\n"));
            }
            text.push_str(&format!(".set {name}, {}\n", self.globals[target].symbol));
            self.assembly.push(text);
        }
        Ok(())
    }
}

fn plain_symbol(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'_' | b'.' | b'$'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$'))
        && name != "."
}
