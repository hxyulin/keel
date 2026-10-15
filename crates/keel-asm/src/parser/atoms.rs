//! Linker atoms and temporary-label ownership. A label at a function's end
//! belongs to that function even when the next function starts at the same
//! offset; retain source order rather than inferring all ownership by address.
use super::data::Location;
use crate::{Assembly, Error, RelocationKind};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Atoms {
    boundaries: BTreeMap<(usize, u64), String>,
    owners: BTreeMap<String, String>,
    defaults: BTreeMap<usize, String>,
}
impl Atoms {
    pub(super) fn new(
        order: &[(String, Location)],
        globals: &BTreeMap<String, usize>,
        labels: &mut BTreeMap<String, Location>,
        prefix: &str,
        enabled: bool,
    ) -> Self {
        let mut result = Self {
            boundaries: BTreeMap::new(),
            owners: BTreeMap::new(),
            defaults: BTreeMap::new(),
        };
        if !enabled {
            return result;
        }
        let mut current = BTreeMap::new();
        for (name, location) in order {
            if !name.starts_with('L') || globals.contains_key(name) {
                // Aliases at one address share an atom.
                let owner = result
                    .boundaries
                    .entry((location.section, location.offset))
                    .or_insert_with(|| name.clone())
                    .clone();
                // A leading temporary label can precede its public alias.
                // Both denote the initial atom at offset zero.
                if location.offset == 0
                    && let Some(default) = result.defaults.remove(&location.section)
                {
                    for previous in result.owners.values_mut() {
                        if *previous == default {
                            *previous = owner.clone();
                        }
                    }
                }
                current.insert(location.section, owner);
            }
            let owner = current
                .get(&location.section)
                .cloned()
                .unwrap_or_else(|| result.default(location.section, prefix));
            result.owners.insert(name.clone(), owner);
        }
        for (section, name) in &result.defaults {
            labels.insert(
                name.clone(),
                Location {
                    section: *section,
                    offset: 0,
                },
            );
        }
        result
    }
    fn default(&mut self, section: usize, prefix: &str) -> String {
        self.defaults
            .entry(section)
            .or_insert_with(|| format!("ltmp_{prefix}section{section}"))
            .clone()
    }
    pub(super) fn owner(&self, name: &str, location: Location) -> Option<&str> {
        self.owners
            .get(name)
            .map(String::as_str)
            .or_else(|| self.at(location))
    }
    fn at(&self, location: Location) -> Option<&str> {
        self.boundaries
            .range((location.section, 0)..=(location.section, location.offset))
            .next_back()
            .map(|(_, s)| s.as_str())
            .or_else(|| self.defaults.get(&location.section).map(String::as_str))
    }
    pub(super) fn same(&self, a: (&str, Location), b: (&str, Location)) -> bool {
        a.1.section == b.1.section && self.owner(a.0, a.1) == self.owner(b.0, b.1)
    }
    pub(super) fn contains(&self, point: Location, target: (&str, Location)) -> bool {
        point.section == target.1.section && self.at(point) == self.owner(target.0, target.1)
    }
    /// Rebase temporary references onto real atom symbols. This preserves
    /// end-label ownership and keeps temporary labels out of ld's atom table.
    pub(super) fn rebase(
        &mut self,
        a: &mut Assembly,
        labels: &mut BTreeMap<String, Location>,
        globals: &BTreeMap<String, usize>,
        prefix: &str,
    ) -> Result<(), Error> {
        let names: BTreeSet<_> = a
            .relocations
            .iter()
            .flat_map(|r| std::iter::once(&r.symbol).chain(r.subtract_symbol.iter()))
            .cloned()
            .collect();
        let mut targets = BTreeMap::new();
        for name in names {
            if !name.starts_with('L') || globals.contains_key(&name) {
                continue;
            }
            let Some(location) = labels.get(&name).copied() else {
                continue;
            };
            let owner = self
                .owner(&name, location)
                .map(str::to_string)
                .unwrap_or_else(|| self.default(location.section, prefix));
            let base = labels.get(&owner).copied().unwrap_or(Location {
                section: location.section,
                offset: 0,
            });
            labels.entry(owner.clone()).or_insert(base);
            targets.insert(
                name,
                (owner, i128::from(location.offset) - i128::from(base.offset)),
            );
        }
        for r in &mut a.relocations {
            if !matches!(
                r.kind,
                RelocationKind::Unsigned
                    | RelocationKind::Branch26
                    | RelocationKind::Page21
                    | RelocationKind::PageOff12
            ) {
                continue;
            }
            let mut addend = i128::from(r.addend);
            if let Some((name, delta)) = targets.get(&r.symbol) {
                r.symbol = name.clone();
                addend += delta;
            }
            if let Some((name, delta)) = r.subtract_symbol.as_ref().and_then(|s| targets.get(s)) {
                r.subtract_symbol = Some(name.clone());
                addend -= delta;
            }
            r.addend = i64::try_from(addend)
                .map_err(|_| Error::new(0, "atom relocation addend exceeds 64 bits"))?;
        }
        Ok(())
    }
}
