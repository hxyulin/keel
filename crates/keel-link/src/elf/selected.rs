//! Immutable post-GC symbol selection. This never evaluates symbol addresses:
//! omitted sections and unresolved locals must still fail at their original use.
use super::{Symbol, Unit, dynamic};
use crate::Result;
use bbtree::unsync::BBTreeMap as BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct Selection<'a> {
    pub key: (usize, usize),
    pub definition: &'a Symbol,
    // Import eligibility belongs to the original symbol, not its selected
    // definition. In particular, same-named locals must never become imports.
    pub import: Option<&'a str>,
}

pub(super) struct Index<'a> {
    units: &'a [Unit],
    globals: &'a BTreeMap<String, (usize, usize)>,
    imports: &'a BTreeMap<String, dynamic::Import>,
    dense: Vec<Vec<Option<Selection<'a>>>>,
}

impl<'a> Index<'a> {
    pub fn new(
        units: &'a [Unit],
        globals: &'a BTreeMap<String, (usize, usize)>,
        imports: &'a BTreeMap<String, dynamic::Import>,
    ) -> Self {
        let mut index = Self {
            units,
            globals,
            imports,
            dense: Vec::with_capacity(units.len()),
        };
        for (ui, unit) in units.iter().enumerate() {
            // Storage is bounded by twice the actual retained symbol count,
            // never by an unchecked raw input index. GC/COMDAT can leave large
            // holes; those units keep the existing tree lookup with no cache.
            let len = unit.symbols.last_key_value().and_then(|(&last, _)| {
                last.checked_add(1)
                    .filter(|&len| len.saturating_sub(unit.symbols.len()) <= unit.symbols.len())
            });
            let mut dense = Vec::new();
            if !unit.relocs.is_empty()
                && let Some(len) = len
            {
                dense.resize(len, None);
                for (&si, symbol) in &unit.symbols {
                    dense[si] = Some(index.select(ui, si, symbol));
                }
            }
            index.dense.push(dense);
        }
        index
    }

    fn select(&self, ui: usize, si: usize, symbol: &'a Symbol) -> Selection<'a> {
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
            import: (symbol.global() && self.imports.contains_key(&symbol.name))
                .then_some(symbol.name.as_str()),
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
    use bbtree::unsync::BBTreeSet as BTreeSet;
    use object::SymbolSection;

    fn symbol(name: &str, info: u8, section: SymbolSection) -> Symbol {
        Symbol {
            name: name.into(),
            section,
            value: 0,
            size: 0,
            info,
            other: 0,
        }
    }

    fn unit(symbols: BTreeMap<usize, Symbol>) -> Unit {
        Unit {
            name: "fixture.o".into(),
            sections: BTreeMap::new(),
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
    fn dense_holes_and_sparse_extreme_indices_remain_lazy_and_bounded() {
        let units = [
            unit(BTreeMap::from([
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
            unit(BTreeMap::from([
                (1, symbol("small", 0, SymbolSection::Absolute)),
                (usize::MAX, symbol("huge", 0, SymbolSection::Absolute)),
            ])),
            unit(BTreeMap::from([
                (1, symbol("small", 0, SymbolSection::Absolute)),
                (1_000_000_000, symbol("sparse", 0, SymbolSection::Absolute)),
            ])),
        ];
        let globals = BTreeMap::new();
        let imports = BTreeMap::new();
        let index = Index::new(&units, &globals, &imports);
        assert_eq!(index.dense[0].len(), 4);
        assert!(index.dense[1].is_empty());
        assert!(index.dense[2].is_empty());
        for (ui, si) in [(0, 0), (0, 2), (0, usize::MAX), (1, 2), (2, 2)] {
            assert_eq!(
                index.get(ui, si).err().unwrap(),
                "relocation references an invalid symbol index"
            );
        }
        for (ui, si) in [(0, 1), (0, 3), (1, 1), (1, usize::MAX), (2, 1_000_000_000)] {
            let selection = index.get(ui, si).unwrap();
            assert_eq!(selection.key, (ui, si));
            assert!(std::ptr::eq(selection.definition, &units[ui].symbols[&si]));
        }
    }

    #[test]
    fn selected_definitions_and_imports_preserve_original_local_identity() {
        for sparse in [false, true] {
            let mut units = [
                unit(BTreeMap::from([
                    (0, symbol("same", 0x20, SymbolSection::Undefined)),
                    (1, symbol("same", 0, SymbolSection::Absolute)),
                    (2, symbol("external", 0x20, SymbolSection::Undefined)),
                    (3, symbol("external", 0, SymbolSection::Absolute)),
                ])),
                unit(BTreeMap::from([(
                    0,
                    symbol("same", 0x10, SymbolSection::Common),
                )])),
            ];
            let globals = BTreeMap::from([("same".into(), (1, 0))]);
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
            if sparse {
                for unit in &mut units {
                    unit.symbols
                        .insert(1_000_000_000, symbol("padding", 0, SymbolSection::Absolute));
                }
            }
            let index = Index::new(&units, &globals, &imports);
            assert_eq!(index.dense[0].is_empty(), sparse);
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
