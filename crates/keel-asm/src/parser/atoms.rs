//! Linker atoms and temporary-label ownership. A label at a function's end
//! belongs to that function even when the next function starts at the same
//! offset; retain source order rather than inferring all ownership by address.
use super::data::Location;
use crate::fxhash::{FxHashMap, FxHashSet};
use crate::{Assembly, Error, RelocationKind};
use bumpalo::Bump;
use std::cell::Cell;
use std::collections::BTreeMap;

/// Atoms are identified by an index into `names`, so ownership checks
/// compare integers rather than (often long, shared-prefix) names.
pub(super) struct Atoms<'a> {
    arena: &'a Bump,
    names: Vec<&'a str>,
    /// Atom starts by (section, offset), sorted once collected.
    boundaries: Vec<((usize, u64), u32)>,
    /// Labels owned by an atom other than the one at their address. Labels
    /// keep source-order ownership, so only one at a later atom's start (a
    /// function's end label, say) can differ; the rest need no lookup.
    exceptions: FxHashMap<&'a str, u32>,
    defaults: BTreeMap<usize, u32>,
    /// The previous search's result: queries come in source order, so most
    /// fall in the same atom as the one before.
    hint: Cell<usize>,
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
            exceptions: FxHashMap::default(),
            defaults: BTreeMap::new(),
            hint: Cell::new(0),
        };
        if !enabled {
            return result;
        }
        // Each label's owner, in source order.
        let mut owners = Vec::with_capacity(order.len());
        // Temporary names are rarely global: skip their lookups when none is.
        let temporary_globals = globals.keys().any(|name| name.starts_with('L'));
        let mut boundaries = FxHashMap::default();
        let mut current = BTreeMap::new();
        for (name, location) in order {
            if !name.starts_with('L') || temporary_globals && globals.contains_key(name) {
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
                    for previous in &mut owners {
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
            owners.push(owner);
        }
        // Only a temporary label can precede the atom starting at its address.
        for ((name, location), owner) in order.iter().zip(owners) {
            if name.starts_with('L')
                && boundaries
                    .get(&(location.section, location.offset))
                    .is_some_and(|&atom| atom != owner)
            {
                result.exceptions.insert(*name, owner);
            }
        }
        result.boundaries = boundaries.into_iter().collect();
        result.boundaries.sort_unstable();
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
    /// The number of atoms starting at or before `key`.
    fn search(&self, key: (usize, u64)) -> usize {
        let hint = self.hint.get();
        if hint.checked_sub(1).is_none_or(|i| self.boundaries[i].0 <= key)
            && self.boundaries.get(hint).is_none_or(|&(start, _)| key < start)
        {
            return hint;
        }
        let end = self.boundaries.partition_point(|&(start, _)| start <= key);
        self.hint.set(end);
        end
    }
    fn owner(&self, name: &str, location: Location) -> Option<u32> {
        let key = (location.section, location.offset);
        let end = self.search(key);
        if end > 0
            && self.boundaries[end - 1].0 == key
            && let Some(&owner) = self.exceptions.get(name)
        {
            return Some(owner);
        }
        self.preceding(end, location.section)
    }
    /// The atom of the boundary before `end`, if in `section`.
    fn preceding(&self, end: usize, section: usize) -> Option<u32> {
        end.checked_sub(1)
            .map(|i| self.boundaries[i])
            .filter(|&((s, _), _)| s == section)
            .map(|(_, atom)| atom)
            .or_else(|| self.defaults.get(&section).copied())
    }
    pub(super) fn same(&self, a: (&str, Location), b: (&str, Location)) -> bool {
        a.1.section == b.1.section && self.owner(a.0, a.1) == self.owner(b.0, b.1)
    }
    pub(super) fn contains(&self, point: Location, target: (&str, Location)) -> bool {
        if point.section != target.1.section {
            return false;
        }
        let end = self.search((point.section, point.offset));
        // A target strictly inside the point's atom, as most are, has its
        // owner without another search: no other atom starts there.
        let target_key = (target.1.section, target.1.offset);
        if end.checked_sub(1).is_some_and(|i| self.boundaries[i].0 < target_key)
            && self.boundaries.get(end).is_none_or(|&(start, _)| target_key < start)
        {
            return true;
        }
        self.preceding(end, point.section) == self.owner(target.0, target.1)
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
        // does not matter; skip only repeats. Only temporary names rebase,
        // so the (mostly external) others are not hashed at all.
        let mut seen = FxHashSet::default();
        let names = a
            .relocations
            .iter()
            .flat_map(|r| std::iter::once(&r.symbol).chain(r.subtract_symbol.iter()))
            .map(String::as_str)
            .filter(|name| name.starts_with('L') && seen.insert(*name));
        let mut targets = FxHashMap::default();
        for name in names {
            if globals.contains_key(&name) {
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
            let target = |name: &str| {
                name.starts_with('L')
                    .then(|| targets.get(name))
                    .flatten()
            };
            if let Some((name, delta)) = target(&r.symbol) {
                r.symbol = name.to_string();
                addend += delta;
            }
            if let Some((name, delta)) = r.subtract_symbol.as_deref().and_then(target) {
                r.subtract_symbol = Some(name.to_string());
                addend -= delta;
            }
            r.addend = i64::try_from(addend)
                .map_err(|_| Error::new(0, "atom relocation addend exceeds 64 bits"))?;
        }
        Ok(())
    }
}
