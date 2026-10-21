//! Linker atoms and temporary-label ownership. A label at a function's end
//! belongs to that function even when the next function starts at the same
//! offset; retain source order rather than inferring all ownership by address.
use super::data::Location;
use crate::fxhash::{FxHashMap, FxHashSet};
use crate::{Assembly, Error, RelocationKind};
use bumpalo::Bump;
use std::collections::BTreeMap;

/// Atoms are identified by an index into `names`, so ownership checks
/// compare integers rather than (often long, shared-prefix) names.
pub(super) struct Atoms<'a> {
    arena: &'a Bump,
    names: Vec<&'a str>,
    /// Atom starts by (section, offset), sorted once collected.
    boundaries: Vec<((usize, u64), u32)>,
    owners: FxHashMap<&'a str, u32>,
    defaults: BTreeMap<usize, u32>,
}
impl<'a> Atoms<'a> {
    pub(super) fn new(
        arena: &'a Bump,
        order: &[(&'a str, Location)],
        globals: &FxHashMap<&'a str, usize>,
        labels: &mut super::Labels<'a>,
        prefix: &str,
        enabled: bool,
    ) -> Self {
        let mut result = Self {
            arena,
            names: Vec::new(),
            boundaries: Vec::new(),
            owners: FxHashMap::default(),
            defaults: BTreeMap::new(),
        };
        if !enabled {
            return result;
        }
        result.owners.reserve(order.len());
        let mut boundaries = BTreeMap::new();
        let mut current = BTreeMap::new();
        for (name, location) in order {
            if !name.starts_with('L') || globals.contains_key(name) {
                // Aliases at one address share an atom.
                let next = result.names.len() as u32;
                let owner = *boundaries
                    .entry((location.section, location.offset))
                    .or_insert(next);
                if owner == next {
                    result.names.push(*name);
                }
                // A leading temporary label can precede its public alias.
                // Both denote the initial atom at offset zero.
                if location.offset == 0
                    && let Some(default) = result.defaults.remove(&location.section)
                {
                    for previous in result.owners.values_mut() {
                        if *previous == default {
                            *previous = owner;
                        }
                    }
                }
                current.insert(location.section, owner);
            }
            let owner = current
                .get(&location.section)
                .cloned()
                .unwrap_or_else(|| result.default(location.section, prefix));
            result.owners.insert(*name, owner);
        }
        result.boundaries = boundaries.into_iter().collect();
        for (section, &atom) in &result.defaults {
            labels.insert(
                result.names[atom as usize],
                Location {
                    section: *section,
                    offset: 0,
                },
            );
        }
        result
    }
    fn default(&mut self, section: usize, prefix: &str) -> u32 {
        *self.defaults.entry(section).or_insert_with(|| {
            self.names.push(
                bumpalo::format!(in self.arena, "ltmp_{}section{}", prefix, section)
                    .into_bump_str(),
            );
            self.names.len() as u32 - 1
        })
    }
    fn owner(&self, name: &str, location: Location) -> Option<u32> {
        self.owners.get(name).copied().or_else(|| self.at(location))
    }
    fn at(&self, location: Location) -> Option<u32> {
        let key = (location.section, location.offset);
        let end = self.boundaries.partition_point(|&(start, _)| start <= key);
        end.checked_sub(1)
            .map(|i| self.boundaries[i])
            .filter(|&((section, _), _)| section == location.section)
            .map(|(_, atom)| atom)
            .or_else(|| self.defaults.get(&location.section).copied())
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
        labels: &mut super::Labels<'a>,
        globals: &FxHashMap<&'a str, usize>,
        prefix: &str,
    ) -> Result<(), Error> {
        // Each name resolves independently of the others, so visit order
        // does not matter; skip only repeats.
        let mut seen = FxHashSet::default();
        let names = a
            .relocations
            .iter()
            .flat_map(|r| std::iter::once(&r.symbol).chain(r.subtract_symbol.iter()))
            .map(String::as_str)
            .filter(|name| seen.insert(*name));
        let mut targets = FxHashMap::default();
        for name in names {
            if !name.starts_with('L') || globals.contains_key(&name) {
                continue;
            }
            let Some(location) = labels.get(&name).copied() else {
                continue;
            };
            let owner = self
                .owner(name, location)
                .unwrap_or_else(|| self.default(location.section, prefix));
            let owner = self.names[owner as usize];
            let base = labels.get(&owner).copied().unwrap_or(Location {
                section: location.section,
                offset: 0,
            });
            labels.entry(owner).or_insert(base);
            targets.insert(
                &*self.arena.alloc_str(name),
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
            if let Some((name, delta)) = targets.get(r.symbol.as_str()) {
                r.symbol = name.to_string();
                addend += delta;
            }
            if let Some((name, delta)) = r
                .subtract_symbol
                .as_ref()
                .and_then(|s| targets.get(s.as_str()))
            {
                r.subtract_symbol = Some(name.to_string());
                addend -= delta;
            }
            r.addend = i64::try_from(addend)
                .map_err(|_| Error::new(0, "atom relocation addend exceeds 64 bits"))?;
        }
        Ok(())
    }
}
