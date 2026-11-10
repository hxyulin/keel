//! Immutable post-GC symbol selection. This never evaluates symbol addresses:
//! omitted sections and unresolved locals must still fail at their original use.
use super::{Symbol, Unit, dynamic};
use crate::Result;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct Selection<'a> {
    pub key: (usize, usize),
    pub definition: &'a Symbol<'a>,
    // Import eligibility belongs to the original symbol, not its selected
    // definition. In particular, same-named locals must never become imports.
    pub import: Option<&'a str>,
}

pub(super) struct Index<'a> {
    units: &'a [Unit<'a>],
    globals: &'a super::Globals<'a>,
    imports: &'a BTreeMap<String, dynamic::Import>,
    dense: Vec<Vec<Option<Selection<'a>>>>,
}

impl<'a> Index<'a> {
    pub fn new(
        units: &'a [Unit<'a>],
        globals: &'a super::Globals<'a>,
        imports: &'a BTreeMap<String, dynamic::Import>,
    ) -> Self {
        let mut index = Self {
            units,
            globals,
            imports,
            dense: Vec::with_capacity(units.len()),
        };
        for (ui, unit) in units.iter().enumerate() {
            // Symbol indices are dense, so the cache is bounded by the input
            // symbol table. Units without relocations select on demand.
            let mut dense = Vec::new();
            if !unit.relocs.is_empty() {
                dense.resize(unit.symbols.bound(), None);
                for (si, symbol) in unit.symbols.iter() {
                    dense[si] = Some(index.select(ui, si, symbol));
                }
            }
            index.dense.push(dense);
        }
        index
    }

    fn select(&self, ui: usize, si: usize, symbol: &'a Symbol<'a>) -> Selection<'a> {
        let key = if symbol.global() {
            self.globals.get(&symbol.name).copied().unwrap_or((ui, si))
        } else {
            (ui, si)
        };
        Selection {
            key,
            definition: if key == (ui, si) {
                symbol
            } else {
                &self.units[key.0].symbols[&key.1]
            },
            import: (symbol.global() && self.imports.contains_key(symbol.name))
                .then_some(symbol.name),
        }
    }

    pub fn get(&self, ui: usize, si: usize) -> Result<Selection<'a>> {
        let dense = &self.dense[ui];
        if !dense.is_empty() {
            return dense
                .get(si)
                .copied()
                .flatten()
                .ok_or_else(|| "relocation references an invalid symbol index".into());
        }
        let symbol = self.units[ui]
            .symbols
            .get(&si)
            .ok_or("relocation references an invalid symbol index")?;
        Ok(self.select(ui, si, symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::slots::Slots;
    use object::SymbolSection;
    use std::collections::BTreeSet;

    fn symbol(name: &'static str, info: u8, section: SymbolSection) -> Symbol<'static> {
        Symbol {
            name,
            section,
            value: 0,
            size: 0,
            info,
            other: 0,
        }
    }

    fn unit(symbols: Slots<Symbol<'static>>) -> Unit<'static> {
        Unit {
            name: "fixture.o".into(),
            sections: Slots::default(),
            symbols,
            // Construction uses only the presence of relocations, never their
            // raw symbol indices or validity.
            relocs: vec![super::super::Reloc {
                section: 0,
                offset: 0,
                symbol: usize::MAX,
                kind: 1,
                addend: 0,
            }],
            consumed_properties: BTreeSet::new(),
            groups: BTreeMap::new(),
        }
    }

    #[test]
    fn holes_and_out_of_range_indices_are_invalid_and_stay_lazy() {
        let units = [
            unit(Slots::from_iter([
                (
                    1,
                    symbol(
                        "omitted",
                        0,
                        SymbolSection::Section(object::SectionIndex(99)),
                    ),
                ),
                (3, symbol("unresolved", 0, SymbolSection::Undefined)),
            ])),
            unit(Slots::from_iter([(
                1,
                symbol("small", 0, SymbolSection::Absolute),
            )])),
        ];
        let globals = crate::elf::Globals::default();
        let imports = BTreeMap::new();
        let index = Index::new(&units, &globals, &imports);
        assert_eq!(index.dense[0].len(), 4);
        for (ui, si) in [(0, 0), (0, 2), (0, 4), (0, usize::MAX), (1, 0), (1, 2)] {
            assert_eq!(
                index.get(ui, si).err().unwrap(),
                "relocation references an invalid symbol index"
            );
        }
        for (ui, si) in [(0, 1), (0, 3), (1, 1)] {
            let selection = index.get(ui, si).unwrap();
            assert_eq!(selection.key, (ui, si));
            assert!(std::ptr::eq(selection.definition, &units[ui].symbols[&si]));
        }
    }

    #[test]
    fn selected_definitions_and_imports_preserve_original_local_identity() {
        for relocations in [true, false] {
            let mut units = [
                unit(Slots::from_iter([
                    (0, symbol("same", 0x20, SymbolSection::Undefined)),
                    (1, symbol("same", 0, SymbolSection::Absolute)),
                    (2, symbol("external", 0x20, SymbolSection::Undefined)),
                    (3, symbol("external", 0, SymbolSection::Absolute)),
                ])),
                unit(Slots::from_iter([(
                    0,
                    symbol("same", 0x10, SymbolSection::Common),
                )])),
            ];
            let globals = crate::elf::Globals::from_iter([("same", (1, 0))]);
            let imports = BTreeMap::from([(
                "external".into(),
                dynamic::Import {
                    index: 1,
                    kind: 0,
                    weak: true,
                    library: None,
                    version: None,
                },
            )]);
            if !relocations {
                // Units without relocations select on demand.
                units[0].relocs.clear();
            }
            let index = Index::new(&units, &globals, &imports);
            assert_eq!(index.dense[0].is_empty(), !relocations);
            assert_eq!(index.get(0, 0).unwrap().key, (1, 0));
            assert!(std::ptr::eq(
                index.get(0, 0).unwrap().definition,
                &units[1].symbols[&0]
            ));
            assert_eq!(index.get(0, 1).unwrap().key, (0, 1));
            assert_eq!(index.get(0, 2).unwrap().import, Some("external"));
            assert_eq!(index.get(0, 3).unwrap().import, None);
        }
    }
}
