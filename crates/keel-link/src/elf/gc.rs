//! Reachability uses original contribution indices until one final compaction.
use super::{Result, Section, Symbol, Unit, sframe, unwind};
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::SymbolSection;

type Key = (usize, usize);

// An allocated symbol ID cannot equal usize::MAX. Raw keys still may.
const NO_SYMBOL: usize = usize::MAX;

/// Raw ELF indices can be sparse, including after COMDAT pruning. Use direct
/// lookup only when its allocation is bounded by twice the actual symbol count;
/// otherwise binary-search the actual keys. Neither path validates symbols.
enum SymbolLookup {
    Dense(Vec<usize>),
    Sparse(Vec<(usize, usize)>),
}
impl SymbolLookup {
    fn new(keys: Vec<(usize, usize)>) -> Self {
        let count = keys.last().and_then(|(key, _)| key.checked_add(1));
        if let Some(count) = count.filter(|&count| count <= keys.len().saturating_mul(2)) {
            let mut ids = vec![NO_SYMBOL; count];
            for (key, id) in keys {
                ids[key] = id;
            }
            Self::Dense(ids)
        } else {
            Self::Sparse(keys)
        }
    }

    fn get(&self, key: usize) -> Option<usize> {
        match self {
            Self::Dense(ids) => ids.get(key).copied().filter(|&id| id != NO_SYMBOL),
            Self::Sparse(keys) => keys
                .binary_search_by_key(&key, |&(key, _)| key)
                .ok()
                .map(|at| keys[at].1),
        }
    }
}

struct SelectedSymbol<'a> {
    symbol: &'a Symbol,
    canonical: usize,
    target: Option<usize>,
}

/// Dense IDs preserve (unit, raw symbol) order. Prebinding existing symbols is
/// infallible: missing relocation symbols are still diagnosed only on demand,
/// and unselected section mappings remain absent rather than being validated.
fn selected_symbols<'a>(
    units: &'a [Unit],
    globals: &BTreeMap<String, Key>,
) -> (Vec<SymbolLookup>, Vec<SelectedSymbol<'a>>) {
    let mut selected = Vec::with_capacity(units.iter().map(|unit| unit.symbols.len()).sum());
    let lookups: Vec<_> = units
        .iter()
        .map(|unit| {
            // Reuse the same bounded raw-index strategy for section mappings.
            // Common dense inputs then avoid one tree search per symbol.
            let sections = SymbolLookup::new(
                unit.sections
                    .iter()
                    .map(|(&raw, &target)| (raw, target))
                    .collect(),
            );
            SymbolLookup::new(
                unit.symbols
                    .iter()
                    .map(|(&raw, symbol)| {
                        let id = selected.len();
                        selected.push(SelectedSymbol {
                            symbol,
                            canonical: id,
                            target: match symbol.section {
                                SymbolSection::Section(input) => sections.get(input.0),
                                _ => None,
                            },
                        });
                        (raw, id)
                    })
                    .collect(),
            )
        })
        .collect();
    for entry in &mut selected {
        if entry.symbol.global()
            && let Some(&(unit, raw)) = globals.get(&entry.symbol.name)
        {
            entry.canonical = lookups[unit].get(raw).expect("selected global symbol");
        }
    }
    (lookups, selected)
}

/// Section indices are the selected contribution indices used by edges and
/// unit mappings. Mark on enqueue so cycles and duplicate edges visit once.
struct LiveSections {
    marked: Vec<bool>,
    pending: Vec<usize>,
}
impl LiveSections {
    fn new(count: usize) -> Self {
        Self {
            marked: vec![false; count],
            pending: Vec::new(),
        }
    }

    fn insert(&mut self, section: usize) {
        if !std::mem::replace(&mut self.marked[section], true) {
            self.pending.push(section);
        }
    }

    fn contains(&self, section: &usize) -> bool {
        self.marked[*section]
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.marked
            .iter()
            .enumerate()
            .filter_map(|(section, &live)| live.then_some(section))
    }

    fn as_set(&self) -> BTreeSet<usize> {
        self.iter().collect()
    }
}

/// Original (unit, relocation) indices, including every relocation at a field.
/// Do not canonicalize here: owner/range validation must precede edge errors.
struct UnwindRelocations(BTreeMap<(usize, u64), Vec<Key>>);
impl UnwindRelocations {
    fn new(units: &[Unit], frames: &BTreeMap<usize, unwind::Frames>) -> Self {
        let mut index = BTreeMap::<_, Vec<_>>::new();
        if frames.is_empty() {
            return Self(index);
        }
        for (ui, unit) in units.iter().enumerate() {
            for (ri, r) in unit.relocs.iter().enumerate() {
                if frames.contains_key(&r.section) {
                    index
                        .entry((r.section, r.offset))
                        .or_default()
                        .push((ui, ri));
                }
            }
        }
        Self(index)
    }

    fn offsets(&self, section: usize) -> BTreeSet<u64> {
        self.0
            .range((section, 0)..=(section, u64::MAX))
            .map(|(&(_, offset), _)| offset)
            .collect()
    }

    fn first(&self, section: usize, offset: u64) -> Option<Key> {
        self.0.get(&(section, offset))?.first().copied()
    }

    fn edges(&self, section: usize, pc: u64, fields: &[u64]) -> Vec<Key> {
        let mut relocs = Vec::new();
        for &offset in fields {
            if offset != pc
                && let Some(indices) = self.0.get(&(section, offset))
            {
                relocs.extend_from_slice(indices);
            }
        }
        // Field order can differ from relocation order. Preserve the original
        // unit/relocation traversal, including which malformed symbol errs first.
        relocs.sort_unstable();
        relocs
    }
}

/// Cache the original live-relocation scan up to its first invalid symbol.
/// For each queried target, a preceding reference still wins over that error;
/// targets first referenced after the error must report the error instead.
struct ReferenceTargets {
    sections: BTreeSet<usize>,
    error: Option<String>,
}
impl ReferenceTargets {
    fn new(targets: impl Iterator<Item = Result<Option<usize>>>) -> Self {
        let mut index = Self {
            sections: BTreeSet::new(),
            error: None,
        };
        for target in targets {
            match target {
                Ok(Some(section)) => {
                    index.sections.insert(section);
                }
                Ok(None) => {}
                Err(error) => {
                    index.error = Some(error);
                    break;
                }
            }
        }
        index
    }

    fn contains(&self, section: usize) -> Result<bool> {
        if self.sections.contains(&section) {
            Ok(true)
        } else if let Some(error) = &self.error {
            Err(error.clone())
        } else {
            Ok(false)
        }
    }
}

pub(super) fn collect(
    sections: &mut Vec<Section>,
    units: &mut [Unit],
    frames: &mut BTreeMap<usize, unwind::Frames>,
    globals: &mut BTreeMap<String, Key>,
    entry: &str,
    dynamic: bool,
    sframe: &mut Option<(usize, sframe::Table)>,
    sframe_targets: &mut Option<sframe::Bindings>,
) -> Result<Vec<Option<usize>>> {
    let (lookups, selected) = selected_symbols(units, globals);
    let canonical = |u: usize, s: usize| -> Result<usize> {
        let id = lookups[u].get(s).ok_or("invalid GC relocation symbol")?;
        Ok(selected[id].canonical)
    };
    let target = |id: usize| selected[id].target;
    let mut edges = vec![Vec::<usize>::new(); sections.len()];
    let mut live = LiveSections::new(sections.len());
    let mut symbols = vec![false; selected.len()];
    if let Some(&(u, s)) = globals.get(entry) {
        symbols[lookups[u].get(s).expect("entry symbol")] = true;
    }
    // Match the existing dynamic-export policy, including common symbols.
    if dynamic {
        for &(u, s) in globals.values() {
            let id = lookups[u].get(s).expect("export symbol");
            if matches!(selected[id].symbol.other & 3, 0 | 3) {
                symbols[id] = true;
            }
        }
    }
    for (i, s) in sections.iter().enumerate() {
        if s.flags & 0x200000 != 0
            || matches!(
                s.name.as_str(),
                ".init" | ".fini" | ".tm_clone_table" | ".note.ABI-tag"
            )
            || matches!(s.kind, 14..=16)
        {
            live.insert(i);
        }
    }
    // Share each group's membership list rather than copying it per member.
    let mut section_groups = vec![Vec::new(); sections.len()];
    let mut groups = Vec::new();
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            if !frames.contains_key(&r.section)
                && sframe.as_ref().is_none_or(|(i, _)| *i != r.section)
            {
                edges[r.section].push(canonical(ui, r.symbol)?);
            }
        }
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (&input, &group) in &u.groups {
            if let Some(&s) = u.sections.get(&input) {
                members.entry(group).or_default().push(s);
            }
        }
        for members in members.into_values() {
            let group = groups.len();
            for &s in &members {
                section_groups[s].push(group);
            }
            groups.push(Some(members));
        }
    }
    // Metadata has an owner, never an unconditional edge to its code. CIE
    // personality and FDE LSDA references become edges only from that owner.
    let relocations = UnwindRelocations::new(units, frames);
    let mut owners = BTreeMap::new();
    for (&section, records) in frames.iter() {
        let mut list = Vec::new();
        for (record, pc, range, fields) in records.owners(&sections[section].data)? {
            let (ui, ri) = relocations
                .first(section, pc)
                .ok_or("--gc-sections requires a relocation for every FDE initial location")?;
            let r = &units[ui].relocs[ri];
            let resolved = canonical(ui, r.symbol)?;
            let raw = lookups[ui].get(r.symbol).expect("resolved owner symbol");
            let original = if target(raw).is_some() { raw } else { resolved };
            let owner =
                target(original).ok_or("--gc-sections requires a selected section FDE owner")?;
            // Validate the original code range before deciding whether to drop
            // its FDE. Layout-time validation cannot see discarded metadata.
            let start =
                u64::try_from(i128::from(selected[original].symbol.value) + i128::from(r.addend))
                    .map_err(|_| "unwind FDE range is outside executable section data")?;
            if sections[owner].flags & 4 == 0
                || start >= sections[owner].size
                || start
                    .checked_add(range)
                    .is_none_or(|end| end > sections[owner].size)
            {
                return Err("unwind FDE range is outside executable section data".into());
            }
            let rebound = target(resolved) != Some(owner);
            for (ui, ri) in relocations.edges(section, pc, &fields) {
                edges[owner].push(canonical(ui, units[ui].relocs[ri].symbol)?);
            }
            list.push((record, owner, rebound));
        }
        owners.insert(section, list);
    }
    // Index actual mappings, not raw ELF indices: sparse input indices must
    // never determine allocation size. A unit may contribute several unwind
    // sections (or several mappings to one), so deduplicate per unit.
    let mut frame_units = BTreeMap::<usize, Vec<usize>>::new();
    if !frames.is_empty() {
        for (ui, unit) in units.iter().enumerate() {
            let mut contributed = BTreeSet::new();
            for &section in unit.sections.values() {
                if frames.contains_key(&section) && contributed.insert(section) {
                    frame_units.entry(section).or_default().push(ui);
                }
            }
        }
    }
    // Only relocation edges trigger synthesized boundary retention, including
    // when the undefined symbol was already seeded by entry/export policy.
    // Remove each name after expansion: start/stop aliases and duplicate edges
    // must not repeatedly scan all selected contributions.
    let mut boundaries = BTreeMap::<&str, Vec<usize>>::new();
    for (section, contribution) in sections.iter().enumerate() {
        if super::bounds::ordinary(contribution) {
            boundaries
                .entry(&contribution.name)
                .or_default()
                .push(section);
        }
    }
    let mut pending_symbols: Vec<_> = symbols
        .iter()
        .enumerate()
        .filter_map(|(id, &marked)| marked.then_some(id))
        .collect();
    loop {
        if let Some(key) = pending_symbols.pop() {
            if let Some(section) = target(key) {
                live.insert(section);
            }
        } else if let Some(section) = live.pending.pop() {
            for &group in &section_groups[section] {
                if let Some(members) = groups[group].take() {
                    for member in members {
                        live.insert(member);
                    }
                }
            }
            for &key in &edges[section] {
                if !std::mem::replace(&mut symbols[key], true) {
                    pending_symbols.push(key);
                }
                let symbol = selected[key].symbol;
                if symbol.section == SymbolSection::Undefined {
                    if let Some(group) = symbol
                        .name
                        .strip_prefix("__start_")
                        .or_else(|| symbol.name.strip_prefix("__stop_"))
                    {
                        // GNU default: a live boundary reference retains every
                        // selected ordinary contribution with this name.
                        if let Some(members) = boundaries.remove(group) {
                            for member in members {
                                live.insert(member);
                            }
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    // Keep constant-time membership through validation and compaction. Only
    // SFrame needs an ordered-set adapter at its existing API boundary.
    let mut references = None;
    for section in owners
        .keys()
        .copied()
        .chain(sframe.as_ref().map(|(i, _)| *i))
    {
        if dynamic
            && globals.values().any(|&key| {
                target(lookups[key.0].get(key.1).expect("export symbol")) == Some(section)
                    && matches!(units[key.0].symbols[&key.1].other & 3, 0 | 3)
            })
        {
            return Err(format!(
                "--gc-sections exports into rewritten {} are unsupported",
                sections[section].name
            ));
        }
        // Input references to rewritten records require addend translation as
        // well as symbol translation. Keep this existing bounded limitation.
        // DWARF sections become live only after compaction below. Their CIE
        // personality/FDE LSDA edges already belong to live code owners, so
        // consult those edges too before SFrame anchors can be reset/pruned.
        if sframe.as_ref().is_some_and(|(i, _)| *i == section)
            && live
                .iter()
                .any(|owner| edges[owner].iter().any(|&key| target(key) == Some(section)))
        {
            return Err("--gc-sections references into rewritten .sframe are unsupported".into());
        }
        // Build only when this validation is reached, after export/SFrame
        // checks. Cache targets in input traversal order and defer the first
        // symbol error so each metadata section keeps its original precedence.
        // Retain only rewritten targets, bounding this cache by metadata count.
        let references = references.get_or_insert_with(|| {
            let owners = &owners;
            let sframe = &sframe;
            ReferenceTargets::new(units.iter().enumerate().flat_map(|(ui, u)| {
                u.relocs
                    .iter()
                    .filter(|r| live.contains(&r.section))
                    .map(move |r| {
                        canonical(ui, r.symbol).map(|key| {
                            target(key).filter(|section| {
                                owners.contains_key(section)
                                    || sframe.as_ref().is_some_and(|(i, _)| i == section)
                            })
                        })
                    })
            }))
        });
        if references.contains(section)? {
            return Err(format!(
                "--gc-sections references into rewritten {} are unsupported",
                sections[section].name
            ));
        }
    }
    // Release symbol borrows before metadata mutates/removes input symbols.
    drop(selected);
    // Boundary names borrow sections, which metadata rewriting mutates below.
    drop(boundaries);
    for (&section, list) in &owners {
        if list
            .iter()
            .any(|(_, owner, rebound)| *rebound && live.contains(owner))
        {
            return Err("--gc-sections cannot rewrite a live FDE whose code symbol resolves to a different contribution".into());
        }
        let dead = list
            .iter()
            .filter(|(_, owner, _)| !live.contains(owner))
            .map(|(record, _, _)| *record)
            .collect();
        let relocated = relocations.offsets(section);
        let filtered = frames[&section].compact(&sections[section].data, &dead, &relocated)?;
        // Relocations belong to the unit that owns their source contribution.
        // Retain only in contributing units instead of rescanning every input
        // for each unwind section. No saved relocation index is used after a
        // retain: earlier compactions may have shifted positions within a unit.
        for &ui in frame_units.get(&section).into_iter().flatten() {
            let u = &mut units[ui];
            u.relocs.retain_mut(|r| {
                if r.section != section {
                    return true;
                }
                if let Some(at) = filtered.offset(r.offset) {
                    r.offset = at;
                    true
                } else {
                    false
                }
            });
            let mappings = &u.sections;
            u.symbols.retain(|_,s| {
                if !matches!(s.section, SymbolSection::Section(i) if mappings.get(&i.0) == Some(&section)) { return true; }
                if s.info & 15 == 3 { s.value = 0; s.size = 0; return true; }
                if let Some(at) = filtered.offset(s.value) {
                    if s.size > 0 && filtered.offset(s.value+s.size-1) != Some(at+s.size-1) { return false; }
                    s.value = at; true
                } else { false }
            });
        }
        sections[section].data = filtered.data;
        sections[section].size = sections[section].data.len() as u64;
        if sections[section].size > 0 {
            live.insert(section);
        }
    }
    if let Some((section, table)) = sframe {
        // The table itself is metadata, not a root for its code. Keep an empty
        // header so the producer/section/program-header contract stays explicit.
        let targets = sframe_targets
            .as_mut()
            .ok_or(".sframe: missing original bindings")?;
        let indices = table.live_indices(targets, &live.as_set());
        table.reconstruct(targets, indices, *section, sections, units)?;
        live.insert(*section);
        for u in units.iter_mut() {
            let mappings = &u.sections;
            u.symbols.retain(|_, s| {
                if !matches!(s.section, SymbolSection::Section(i) if mappings.get(&i.0) == Some(section)) { return true; }
                if s.info & 15 == 3 { s.value = 0; s.size = 0; true } else { false }
            });
        }
    }
    let mut remap = vec![None; sections.len()];
    let mut output = Vec::new();
    for (i, s) in std::mem::take(sections).into_iter().enumerate() {
        if live.contains(&i) {
            remap[i] = Some(output.len());
            output.push(s);
        }
    }
    // Preserve raw live relocation indices as well as canonical winners.
    for (ui, u) in units.iter().enumerate() {
        for r in u.relocs.iter().filter(|r| remap[r.section].is_some()) {
            if let Some(id) = lookups[ui].get(r.symbol) {
                symbols[id] = true;
            }
        }
    }
    // Metadata rewriting may already have removed symbols. Start this bitmap
    // empty and record only symbols that survive the final retain pass.
    let mut retained_symbols = vec![false; symbols.len()];
    for (ui, u) in units.iter_mut().enumerate() {
        u.relocs.retain_mut(|r| {
            if let Some(at) = remap[r.section] {
                r.section = at;
                true
            } else {
                false
            }
        });
        u.sections.retain(|_, at| {
            if let Some(new) = remap[*at] {
                *at = new;
                true
            } else {
                false
            }
        });
        let retained_sections = SymbolLookup::new(
            u.sections
                .iter()
                .map(|(&raw, &target)| (raw, target))
                .collect(),
        );
        u.symbols.retain(|&si, s| {
            let id = lookups[ui].get(si).expect("original symbol identity");
            let keep = match s.section {
                SymbolSection::Section(i) => {
                    if retained_sections.get(i.0).is_some() || u.consumed_properties.contains(&i.0)
                    {
                        true
                    } else if s.global() && symbols[id] {
                        s.section = SymbolSection::Undefined;
                        true
                    } else {
                        false
                    }
                }
                SymbolSection::Undefined | SymbolSection::Common => symbols[id],
                _ => true,
            };
            retained_symbols[id] = keep;
            keep
        });
    }
    globals.retain(|_, key| {
        let (u, raw) = *key;
        retained_symbols[lookups[u].get(raw).expect("selected global identity")]
    });
    *frames = std::mem::take(frames)
        .into_iter()
        .filter_map(|(i, _)| remap[i].map(|at| Ok((at, unwind::Frames::parse(&output[at].data)?))))
        .collect::<Result<_>>()?;
    if let Some((section, _)) = sframe {
        *section = remap[*section].ok_or(".sframe: metadata was discarded")?;
        sframe_targets.as_mut().unwrap().remap(&remap)?;
    }
    *sections = output;
    Ok(remap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::Reloc;

    #[test]
    fn sparse_raw_weak_identity_survives_dense_marking() {
        let section = |name: &str| Section {
            name: name.into(),
            flags: 6,
            kind: 1,
            align: 1,
            size: 1,
            bss: false,
            data: vec![0xc3],
            offset: 0,
        };
        let symbol = |name: &str, input, info| Symbol {
            name: name.into(),
            section: SymbolSection::Section(object::SectionIndex(input)),
            value: 0,
            size: 1,
            info,
            other: 0,
        };
        let mut sections = vec![
            section(".text"),
            section(".text.weak"),
            section(".text.strong"),
        ];
        let mut units = [
            Unit {
                name: "weak.o".into(),
                sections: BTreeMap::from([(usize::MAX - 1, 0), (usize::MAX, 1)]),
                symbols: BTreeMap::from([
                    (usize::MAX - 1, symbol("_start", usize::MAX - 1, 0x12)),
                    (usize::MAX, symbol("choice", usize::MAX, 0x22)),
                ]),
                relocs: vec![Reloc {
                    section: 0,
                    offset: 0,
                    symbol: usize::MAX,
                    kind: object::elf::R_X86_64_64,
                    addend: 0,
                }],
                consumed_properties: BTreeSet::new(),
                groups: BTreeMap::new(),
            },
            Unit {
                name: "strong.o".into(),
                sections: BTreeMap::from([(usize::MAX, 2)]),
                symbols: BTreeMap::from([
                    (usize::MAX, symbol("choice", usize::MAX, 0x12)),
                    // Prebinding must not diagnose an unreferenced, unmapped symbol.
                    (usize::MAX - 1, symbol("unused", 123, 0x12)),
                ]),
                relocs: vec![],
                consumed_properties: BTreeSet::new(),
                groups: BTreeMap::new(),
            },
        ];
        let mut globals = BTreeMap::from([
            ("_start".into(), (0, usize::MAX - 1)),
            ("choice".into(), (1, usize::MAX)),
            ("unused".into(), (1, usize::MAX - 1)),
        ]);
        let remap = collect(
            &mut sections,
            &mut units,
            &mut BTreeMap::new(),
            &mut globals,
            "_start",
            false,
            &mut None,
            &mut None,
        )
        .unwrap();
        assert_eq!(remap, [Some(0), None, Some(1)]);
        assert_eq!(units[0].relocs[0].symbol, usize::MAX);
        assert_eq!(
            units[0].symbols[&usize::MAX].section,
            SymbolSection::Undefined
        );
        assert_eq!(
            units[1].symbols[&usize::MAX].section,
            SymbolSection::Section(object::SectionIndex(usize::MAX))
        );
        assert!(!units[1].symbols.contains_key(&(usize::MAX - 1)));
        assert_eq!(globals["choice"], (1, usize::MAX));
    }

    #[test]
    fn symbol_lookup_is_bounded_and_preserves_holes() {
        for keys in [vec![], vec![0, 2, 3], vec![0, 1000], vec![usize::MAX]] {
            let pairs: Vec<_> = keys
                .iter()
                .enumerate()
                .map(|(id, &key)| (key, id))
                .collect();
            let index = SymbolLookup::new(pairs.clone());
            match &index {
                SymbolLookup::Dense(ids) => assert!(ids.len() <= pairs.len() * 2),
                SymbolLookup::Sparse(entries) => assert_eq!(entries.len(), pairs.len()),
            }
            for raw in [0, 1, 2, 3, 1000, usize::MAX - 1, usize::MAX] {
                assert_eq!(
                    index.get(raw),
                    pairs
                        .iter()
                        .find(|&&(key, _)| key == raw)
                        .map(|&(_, id)| id)
                );
            }
        }
    }

    #[test]
    fn prebinding_matches_lazy_symbol_and_section_selection() {
        let symbol = |name: &str, section, info| Symbol {
            name: name.into(),
            section,
            info,
            value: u64::MAX,
            size: u64::MAX,
            other: 0,
        };
        let mut units = vec![];
        for (sparse, raw_section) in [(false, 1), (true, usize::MAX)] {
            let offset = if sparse { usize::MAX - 5 } else { 0 };
            units.push(Unit {
                name: "bindings.o".into(),
                sections: BTreeMap::from([(raw_section, 0)]),
                symbols: BTreeMap::from([
                    (
                        offset,
                        symbol(
                            "choice",
                            SymbolSection::Section(object::SectionIndex(raw_section)),
                            0x22,
                        ),
                    ),
                    (offset + 1, symbol("choice", SymbolSection::Absolute, 0)),
                    (offset + 2, symbol("common", SymbolSection::Common, 0x11)),
                    (offset + 3, symbol("choice", SymbolSection::Undefined, 0x12)),
                    // An invalid unused definition is not validated by prebinding.
                    (
                        offset + 4,
                        symbol(
                            "unused",
                            SymbolSection::Section(object::SectionIndex(123)),
                            0x12,
                        ),
                    ),
                ]),
                relocs: vec![],
                consumed_properties: BTreeSet::new(),
                groups: BTreeMap::new(),
            });
        }
        let globals = BTreeMap::from([
            ("choice".into(), (1, usize::MAX - 5)),
            ("common".into(), (0, 2)),
        ]);
        let (lookups, selected) = selected_symbols(&units, &globals);
        for (ui, unit) in units.iter().enumerate() {
            for (&raw, symbol) in &unit.symbols {
                let key = if symbol.global() {
                    globals.get(&symbol.name).copied().unwrap_or((ui, raw))
                } else {
                    (ui, raw)
                };
                let id = lookups[ui].get(raw).unwrap();
                let winner = lookups[key.0].get(key.1).unwrap();
                assert_eq!(selected[id].canonical, winner);
                assert!(std::ptr::eq(selected[id].symbol, symbol));
                let expected = match units[key.0].symbols[&key.1].section {
                    SymbolSection::Section(input) => units[key.0].sections.get(&input.0).copied(),
                    _ => None,
                };
                assert_eq!(selected[winner].target, expected);
            }
            assert!(lookups[ui].get(5000).is_none());
        }
    }

    #[test]
    fn sparse_alias_mappings_rewrite_a_contributing_unit_only_once() {
        let code = usize::MAX;
        let dead = code - 1;
        let frame = code - 2;
        let alias = code - 3;
        let mut data = Vec::new();
        for word in [12u32, 0] {
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.extend_from_slice(&[1, 0, 1, 120, 16, 0, 0, 0]);
        for cie in [20u32, 44] {
            data.extend_from_slice(&20u32.to_le_bytes());
            data.extend_from_slice(&cie.to_le_bytes());
            data.extend_from_slice(&0u64.to_le_bytes());
            data.extend_from_slice(&1u64.to_le_bytes());
        }
        let section = |name: &str, flags, data: Vec<u8>| Section {
            name: name.into(),
            flags,
            kind: 1,
            align: 8,
            size: data.len() as u64,
            bss: false,
            data,
            offset: 0,
        };
        let mut frames = BTreeMap::from([(2, unwind::Frames::parse(&data).unwrap())]);
        let mut sections = vec![
            section(".text", 6, vec![0xc3]),
            section(".text.dead", 6, vec![0xc3]),
            section(".eh_frame", 2, data),
        ];
        let symbol = |name: &str, input, value, size, info| super::super::Symbol {
            name: name.into(),
            section: SymbolSection::Section(object::SectionIndex(input)),
            value,
            size,
            info,
            other: 0,
        };
        let mut units = [Unit {
            name: "sparse.o".into(),
            sections: BTreeMap::from([(code, 0), (dead, 1), (frame, 2), (alias, 2)]),
            symbols: BTreeMap::from([
                (1, symbol("_start", code, 0, 1, 0x12)),
                (2, symbol("dead", dead, 0, 1, 2)),
                (3, symbol("record", alias, 40, 24, 0)),
                (4, symbol("", frame, 0, 64, 3)),
            ]),
            relocs: [(24, 2), (48, 1)]
                .into_iter()
                .map(|(offset, symbol)| Reloc {
                    section: 2,
                    offset,
                    symbol,
                    kind: object::elf::R_X86_64_64,
                    addend: 0,
                })
                .collect(),
            consumed_properties: BTreeSet::new(),
            groups: BTreeMap::new(),
        }];
        let mut globals = BTreeMap::from([("_start".into(), (0, 1))]);
        let remap = collect(
            &mut sections,
            &mut units,
            &mut frames,
            &mut globals,
            "_start",
            false,
            &mut None,
            &mut None,
        )
        .unwrap();
        assert_eq!(remap, [Some(0), None, Some(1)]);
        assert_eq!(sections[1].size, 40);
        assert_eq!(
            units[0].sections,
            BTreeMap::from([(code, 0), (frame, 1), (alias, 1)])
        );
        assert_eq!(units[0].relocs.len(), 1);
        assert_eq!(
            (units[0].relocs[0].section, units[0].relocs[0].offset),
            (1, 24)
        );
        assert_eq!(
            (units[0].symbols[&3].value, units[0].symbols[&3].size),
            (16, 24)
        );
        assert_eq!(
            (units[0].symbols[&4].value, units[0].symbols[&4].size),
            (0, 0)
        );
        assert!(!units[0].symbols.contains_key(&2));
        assert!(frames.contains_key(&1));
    }

    #[test]
    fn reference_index_preserves_each_original_scan_and_stops_at_first_error() {
        // Include sparse selected indices and multiple distinct errors. Each
        // query must match a fresh fallible scan, even when another target had
        // a reference before the error.
        let choices = [
            Ok(None),
            Ok(Some(0)),
            Ok(Some(usize::MAX)),
            Err("first".into()),
            Err("second".into()),
        ];
        for mut pattern in 0..choices.len().pow(4) {
            let targets: Vec<_> = (0..4)
                .map(|_| {
                    let target = choices[pattern % choices.len()].clone();
                    pattern /= choices.len();
                    target
                })
                .collect();
            let index = ReferenceTargets::new(targets.iter().cloned());
            for section in [0, 1, usize::MAX] {
                let mut expected = Ok(false);
                for target in &targets {
                    match target {
                        Ok(Some(at)) if *at == section => {
                            expected = Ok(true);
                            break;
                        }
                        Err(error) => {
                            expected = Err(error.clone());
                            break;
                        }
                        _ => {}
                    }
                }
                assert_eq!(index.contains(section), expected, "{targets:?} / {section}");
            }
        }
        let targets = std::iter::once(Err("invalid GC relocation symbol".into())).chain(
            std::iter::once_with(|| panic!("validated past first error")),
        );
        assert_eq!(
            ReferenceTargets::new(targets).contains(0),
            Err("invalid GC relocation symbol".into())
        );
    }

    #[test]
    fn unwind_index_preserves_first_owner_and_all_edges_in_input_order() {
        let unit = |locations: &[(usize, u64)]| Unit {
            name: "index.o".into(),
            sections: BTreeMap::new(),
            symbols: BTreeMap::new(),
            relocs: locations
                .iter()
                .map(|&(section, offset)| Reloc {
                    section,
                    offset,
                    // Building the index must not resolve symbols early.
                    symbol: usize::MAX,
                    kind: object::elf::R_X86_64_64,
                    addend: 0,
                })
                .collect(),
            consumed_properties: BTreeSet::new(),
            groups: BTreeMap::new(),
        };
        let units = [
            unit(&[(2, 40), (2, 32), (2, 16), (2, 40), (2, 32), (3, 16)]),
            unit(&[(2, 32), (2, 40), (2, 16), (2, 48)]),
        ];
        let frames = BTreeMap::from([(2, unwind::Frames::parse(&[]).unwrap())]);
        let index = UnwindRelocations::new(&units, &frames);
        assert_eq!(index.offsets(2), BTreeSet::from([16, 32, 40, 48]));
        assert!(index.offsets(3).is_empty());
        assert_eq!(index.first(2, 32), Some((0, 1)));
        assert_eq!(index.first(2, 48), Some((1, 3)));
        assert_eq!(index.first(2, 24), None);
        assert_eq!(index.first(3, 16), None);
        // Duplicates at non-PC fields in both units are all retained; every
        // duplicate at the PC is excluded, and unrelated fields stay excluded.
        assert_eq!(
            index.edges(2, 32, &[16, 32, 40]),
            [(0, 0), (0, 2), (0, 3), (1, 1), (1, 2)]
        );
    }
}
