//! Bounded, exact ELF merge-record pooling after selection and GC.
use super::{Globals, Section, Symbol, Unit};
use object::SymbolSection;
use std::collections::BTreeMap;

// Bound auxiliary maps independently of the executable's 512 MiB size limit.
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 131_072;

struct Entry {
    old: u64,
    len: u64,
    new: u64,
}
struct Map {
    entries: Vec<Entry>,
    output: usize,
}
impl Map {
    fn entry(&self, offset: u64) -> Option<&Entry> {
        let index = self.entries.partition_point(|e| e.old <= offset);
        let entry = self.entries.get(index.checked_sub(1)?)?;
        (offset <= entry.old + entry.len).then_some(entry)
    }
    fn offset(&self, offset: u64) -> Option<u64> {
        self.entry(offset).map(|e| e.new + offset - e.old)
    }
}

fn records(section: &Section, budget: &mut usize) -> Option<Map> {
    let (width, strings) = section.merge?;
    let width = usize::try_from(width).ok().filter(|&w| w != 0)?;
    if section.kind != object::elf::SHT_PROGBITS
        || section.flags & 7 != 2
        || !section.name.starts_with(".rodata")
        || !(width as u64).is_multiple_of(section.align)
        || section.data.is_empty()
        || !section.data.len().is_multiple_of(width)
    {
        return None;
    }
    let mut entries = Vec::new();
    let mut start = 0;
    for (i, unit) in section.data.chunks_exact(width).enumerate() {
        if !strings || unit.iter().all(|&b| b == 0) {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            let end = (i + 1) * width;
            entries.push(Entry {
                old: start as u64,
                len: (end - start) as u64,
                new: 0,
            });
            start = end;
        }
    }
    if start != section.data.len() {
        return None; // Unterminated wide strings stay byte-for-byte intact.
    }
    Some(Map { entries, output: 0 })
}

fn section(unit: &Unit, symbol: &Symbol) -> Option<usize> {
    match symbol.section {
        SymbolSection::Section(index) => unit.sections.get(&index.0).copied(),
        _ => None,
    }
}
fn selected<'a>(
    units: &'a [Unit],
    globals: &Globals,
    ui: usize,
    si: usize,
) -> Option<(usize, &'a Symbol)> {
    let symbol = units[ui].symbols.get(&si)?;
    let (ui, si) = if symbol.global() {
        globals.get(&symbol.name).copied().unwrap_or((ui, si))
    } else {
        (ui, si)
    };
    Some((ui, &units[ui].symbols[&si]))
}

/// A non-section symbol's addend is arithmetic on its address, not an input
/// section offset. Keep it within its record (including a sized one-past).
/// Only absolute section-symbol relocations interpret S+A as a section offset.
/// LLD makes the same distinction in ELF/Symbols.cpp, getSymVA().
fn target(map: &Map, symbol: &Symbol, addend: i64, kind: u32) -> Option<u64> {
    if addend == 0 {
        return map.offset(symbol.value);
    }
    let absolute = matches!(
        kind,
        object::elf::R_X86_64_64 | object::elf::R_X86_64_32 | object::elf::R_X86_64_32S
    );
    if !absolute {
        return None;
    }
    let offset = symbol.value.checked_add_signed(addend)?;
    if symbol.info & 15 == object::elf::STT_SECTION {
        return map.offset(offset);
    }
    let entry = map.entry(symbol.value)?;
    if offset < entry.old || offset > entry.old + entry.len {
        return None;
    }
    Some(entry.new + offset - entry.old)
}

pub(super) fn constants(sections: &mut [Section], units: &mut [Unit], globals: &Globals) {
    let bytes: usize = sections
        .iter()
        .filter(|s| s.merge.is_some())
        .map(|s| s.data.len())
        .sum();
    if bytes > MAX_BYTES {
        return;
    }
    let mut budget = MAX_ENTRIES;
    let mut maps: Vec<_> = sections.iter().map(|s| records(s, &mut budget)).collect();
    // If the cap was reached, leave the entire pass unchanged, rather than
    // making admission depend on where a malformed or large section appeared.
    if budget == 0 {
        return;
    }
    for unit in units.iter() {
        for reloc in &unit.relocs {
            maps[reloc.section] = None; // Equality would need relocation semantics.
        }
        for symbol in unit.symbols.values() {
            let Some(index) = section(unit, symbol) else {
                continue;
            };
            let Some(map) = &maps[index] else {
                continue;
            };
            let valid = map.entry(symbol.value).is_some_and(|entry| {
                symbol
                    .value
                    .checked_add(symbol.size)
                    .is_some_and(|end| end <= entry.old + entry.len)
            });
            if !valid {
                maps[index] = None;
            }
        }
    }
    for (ui, unit) in units.iter().enumerate() {
        for reloc in &unit.relocs {
            let Some((owner, symbol)) = selected(units, globals, ui, reloc.symbol) else {
                continue;
            };
            let Some(index) = section(&units[owner], symbol) else {
                continue;
            };
            if let Some(map) = &maps[index]
                && target(map, symbol, reloc.addend, reloc.kind).is_none()
            {
                maps[index] = None;
            }
        }
    }
    // Compatibility includes alignment and GNU_RETAIN, so pooling cannot weaken
    // either contract. Every record length is a multiple of the pool alignment.
    let mut pools = BTreeMap::new();
    let mut unique = BTreeMap::new();
    let mut data: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for (index, map) in maps.iter_mut().enumerate() {
        let Some(map) = map else {
            continue;
        };
        let section = &sections[index];
        let key = (section.flags, section.align, section.merge);
        let output = *pools.entry(key).or_insert(index);
        map.output = output;
        let bytes = data.entry(output).or_default();
        for entry in &mut map.entries {
            let record = &section.data[entry.old as usize..(entry.old + entry.len) as usize];
            let key = (output, record.to_vec());
            entry.new = *unique.entry(key).or_insert_with(|| {
                let offset = bytes.len() as u64;
                bytes.extend_from_slice(record);
                offset
            });
        }
    }
    // Symbols retain original values until every relocation has been adjusted.
    for ui in 0..units.len() {
        for ri in 0..units[ui].relocs.len() {
            let reloc = &units[ui].relocs[ri];
            let Some((owner, symbol)) = selected(units, globals, ui, reloc.symbol) else {
                continue;
            };
            let Some(index) = section(&units[owner], symbol) else {
                continue;
            };
            let Some(map) = &maps[index] else {
                continue;
            };
            let value = map.offset(symbol.value).expect("admitted symbol offset");
            let target =
                target(map, symbol, reloc.addend, reloc.kind).expect("admitted relocation");
            units[ui].relocs[ri].addend = target as i64 - value as i64;
        }
    }
    for unit in units {
        for symbol in unit.symbols.values_mut() {
            if let SymbolSection::Section(index) = symbol.section
                && let Some(&index) = unit.sections.get(&index.0)
                && let Some(map) = &maps[index]
            {
                symbol.value = map.offset(symbol.value).expect("admitted symbol offset");
            }
        }
        for index in unit.sections.values_mut() {
            if let Some(map) = &maps[*index] {
                *index = map.output;
            }
        }
    }
    for (index, map) in maps.iter().enumerate() {
        if map.is_some() {
            sections[index].data = data.remove(&index).unwrap_or_default();
            sections[index].size = sections[index].data.len() as u64;
        }
    }
}
