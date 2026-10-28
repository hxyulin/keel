//! ELF64 partial linking. Contributions keep their identities until final layout.
//!
//! Unlike executable linking, undefined symbols, common storage and relocations
//! remain in the output. No instruction or address relocation is applied here.
use super::{Executable, Machine, input, put16, put32, put64};
use crate::{Input, Result};
use bbtree::unsync::BBTreeMap as BTreeMap;
use object::read::elf::{ElfFile64, FileHeader, SectionHeader};
use object::{Object, ObjectSection, ObjectSymbol, SymbolFlags, SymbolSection};

const LIMIT: u64 = 512 * 1024 * 1024;

struct Section {
    name: Vec<u8>,
    kind: u32,
    flags: u64,
    alignment: u64,
    size: u64,
    data: Vec<u8>,
    link: u32,
    info: u32,
    entsize: u64,
}
struct Symbol {
    name: Vec<u8>,
    info: u8,
    other: u8,
    section: u16,
    value: u64,
    size: u64,
}
impl Symbol {
    fn rank(&self) -> u8 {
        if self.section == 0 {
            0
        } else if self.info >> 4 == 2 {
            1
        } else if self.section == object::elf::SHN_COMMON {
            2
        } else {
            3
        }
    }
}
fn aligned(value: u64, alignment: u64) -> Result<u64> {
    if !alignment.is_power_of_two() {
        return Err(format!("invalid ELF section alignment {alignment}"));
    }
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .filter(|v| *v <= LIMIT)
        .ok_or_else(|| "relocatable ELF exceeds 512 MiB limit".into())
}
fn string(table: &mut Vec<u8>, value: &[u8]) -> Result<u32> {
    let index = u32::try_from(table.len()).map_err(|_| "ELF string table is too large")?;
    table.extend_from_slice(value);
    table.push(0);
    Ok(index)
}

/// Combine selected ELF64 objects without assigning final addresses.
///
/// Archives use the ordinary lazy selection rules, without an executable entry
/// root. Shared libraries are not accepted. Separate input contributions remain
/// separate output sections, including equal names; final scripts aggregate them.
///
/// # Errors
/// Returns an error for malformed input, conflicting strong definitions,
/// unsupported ELF metadata, REL relocations, or output exceeding 512 MiB.
pub fn link(inputs: &[Input<'_>], machine: Machine) -> Result<Executable> {
    let prepared = input::prepare(inputs, "")?;
    link_prepared(&prepared, machine)
}

/// Partially link an ordered selection prepared without an executable entry.
/// The same immutable selection can be reused without repeating preparation.
///
/// # Errors
/// Returns ordinary partial-link errors, or an error when the selection was
/// prepared with an executable entry root.
pub fn link_selected(
    selection: &crate::elf_selection::Selection<'_>,
    machine: Machine,
) -> Result<Executable> {
    if !selection.entry.is_empty() {
        return Err("partial linking requires an ELF selection prepared without an entry".into());
    }
    link_prepared(&selection.prepared, machine)
}

fn link_prepared(prepared: &super::Prepared<'_>, machine: Machine) -> Result<Executable> {
    let inputs = &prepared.inputs;
    if !prepared.libraries.is_empty() {
        return Err("shared libraries cannot be partially linked".into());
    }
    let mut sections = Vec::<Section>::new();
    let mut section_maps = Vec::new();
    let mut files = Vec::new();
    let mut total_data = 0u64;
    for input in inputs {
        let file =
            ElfFile64::<object::Endianness>::parse(input.bytes).map_err(|e| e.to_string())?;
        let architecture = match machine {
            Machine::X86_64 => object::Architecture::X86_64,
            Machine::Aarch64 => object::Architecture::Aarch64,
        };
        if file.architecture() != architecture
            || !file.is_little_endian()
            || file.kind() != object::ObjectKind::Relocatable
        {
            return Err(format!(
                "{}: expected a little-endian {architecture:?} ELF64 relocatable object",
                input.name
            ));
        }
        let header = file.elf_header();
        if !matches!(header.e_ident().os_abi, 0 | 3) || header.e_flags(file.endian()) != 0 {
            return Err(format!(
                "{}: unsupported ELF OS ABI or machine flags",
                input.name
            ));
        }
        let symbol_table = file.elf_symbol_table().section().0;
        let symbol_strings = if symbol_table == 0 {
            0
        } else {
            file.section_by_index(object::SectionIndex(symbol_table))
                .map_err(|e| e.to_string())?
                .elf_section_header()
                .sh_link(file.endian()) as usize
        };
        let section_strings = usize::from(header.e_shstrndx(file.endian()));
        if section_strings == usize::from(object::elf::SHN_XINDEX) {
            return Err(
                "extended input section-name indices are unsupported in partial links".into(),
            );
        }
        let mut map = BTreeMap::new();
        for section in file.sections() {
            let index = section.index().0;
            if input.groups.discarded.contains(&index) {
                continue;
            }
            let raw = section.elf_section_header();
            let kind = raw.sh_type(file.endian());
            if kind == object::elf::SHT_GROUP
                && input.groups.membership.iter().any(|(member, group)| {
                    *group == index && input.groups.discarded.contains(member)
                })
            {
                continue;
            }
            if kind == object::elf::SHT_REL {
                return Err("REL relocations are unsupported; use RELA".into());
            }
            if kind == object::elf::SHT_SYMTAB
                || index == symbol_strings
                || index == section_strings
            {
                continue;
            }
            if !matches!(
                kind,
                object::elf::SHT_PROGBITS
                    | object::elf::SHT_STRTAB
                    | object::elf::SHT_NOBITS
                    | object::elf::SHT_NOTE
                    | object::elf::SHT_RELA
                    | object::elf::SHT_GROUP
                    | object::elf::SHT_INIT_ARRAY
                    | object::elf::SHT_FINI_ARRAY
                    | object::elf::SHT_PREINIT_ARRAY
                    | object::elf::SHT_X86_64_UNWIND
            ) {
                return Err(format!("unsupported partial-link section type {kind:#x}"));
            }
            if kind == object::elf::SHT_RELA
                && (raw.sh_link(file.endian()) as usize != file.elf_symbol_table().section().0
                    || raw.sh_info(file.endian()) == 0
                    || raw.sh_entsize(file.endian()) != 24)
            {
                return Err("malformed RELA section association".into());
            }
            let alignment = section.align().max(1);
            aligned(0, alignment)?;
            let original = section.data().map_err(|e| e.to_string())?;
            let data = input
                .groups
                .frames
                .get(&index)
                .map_or(original, |f| f.data.as_slice());
            let size = if input.groups.frames.contains_key(&index) {
                data.len() as u64
            } else {
                section.size()
            };
            if size > LIMIT || (kind != object::elf::SHT_NOBITS && size != data.len() as u64) {
                return Err("invalid partial-link section size".into());
            }
            total_data = total_data
                .checked_add(data.len() as u64)
                .filter(|v| *v <= LIMIT)
                .ok_or("relocatable ELF exceeds 512 MiB limit")?;
            map.insert(index, sections.len() + 1);
            sections.push(Section {
                name: section.name_bytes().map_err(|e| e.to_string())?.to_vec(),
                kind,
                flags: raw.sh_flags(file.endian()),
                alignment,
                size,
                data: data.to_vec(),
                link: raw.sh_link(file.endian()),
                info: raw.sh_info(file.endian()),
                entsize: raw.sh_entsize(file.endian()),
            });
        }
        section_maps.push(map);
        files.push(file);
    }
    // ELF reserves the upper section-index range for special indices. Extended
    // indices need SHT_SYMTAB_SHNDX, which this writer deliberately diagnoses.
    if sections.len() + 4 >= usize::from(object::elf::SHN_LORESERVE) {
        return Err("too many partial-link sections".into());
    }
    let symtab = sections.len() as u32 + 1;
    let mut symbols = Vec::<Symbol>::new();
    let mut symbol_maps = Vec::new();
    let mut globals = BTreeMap::<Vec<u8>, usize>::new();
    for (ui, file) in files.iter().enumerate() {
        let mut map = BTreeMap::new();
        for s in file.symbols() {
            let raw_section = s.section();
            let discarded = matches!(raw_section, SymbolSection::Section(i) if inputs[ui].groups.discarded.contains(&i.0));
            if discarded || inputs[ui].groups.dead_symbol(s.index().0, raw_section) {
                // Surviving group signatures can be defined in SHT_GROUP itself.
                if !matches!(raw_section, SymbolSection::Section(i) if section_maps[ui].contains_key(&i.0) && file.section_by_index(i).is_ok_and(|g| g.elf_section_header().sh_type(file.endian()) == object::elf::SHT_GROUP))
                {
                    continue;
                }
            }
            let section = match raw_section {
                SymbolSection::Undefined | SymbolSection::None => 0,
                SymbolSection::Absolute => object::elf::SHN_ABS,
                SymbolSection::Common => object::elf::SHN_COMMON,
                SymbolSection::Section(i) => *section_maps[ui]
                    .get(&i.0)
                    .ok_or("symbol refers to unsupported section")?
                    as u16,
                _ => return Err("unsupported partial-link symbol section".into()),
            };
            let SymbolFlags::Elf {
                st_info: info,
                st_other: other,
            } = s.flags()
            else {
                return Err("invalid ELF symbol flags".into());
            };
            if !matches!(info >> 4, 0..=2) {
                return Err("unsupported partial-link symbol binding".into());
            }
            let candidate = Symbol {
                name: s.name_bytes().map_err(|e| e.to_string())?.to_vec(),
                info,
                other,
                section,
                value: s.address(),
                size: s.size(),
            };
            let index = if info >> 4 != 0 {
                if let Some(&index) = globals.get(&candidate.name) {
                    let existing = &mut symbols[index];
                    if existing.rank() == 3 && candidate.rank() == 3 {
                        return Err(format!(
                            "duplicate strong symbol `{}`",
                            String::from_utf8_lossy(&candidate.name)
                        ));
                    }
                    let visibility_rank = |v: u8| match v & 3 {
                        1 => 3,
                        2 => 2,
                        3 => 1,
                        _ => 0,
                    };
                    let visibility =
                        if visibility_rank(existing.other) > visibility_rank(candidate.other) {
                            existing.other & 3
                        } else {
                            candidate.other & 3
                        };
                    if candidate.rank() > existing.rank() {
                        *existing = candidate;
                    } else if candidate.section == object::elf::SHN_COMMON
                        && existing.section == object::elf::SHN_COMMON
                    {
                        existing.size = existing.size.max(candidate.size);
                        existing.value = existing.value.max(candidate.value);
                    } else if candidate.rank() == 0
                        && existing.rank() == 0
                        && candidate.info >> 4 == 1
                    {
                        existing.info = candidate.info;
                    }
                    existing.other = (existing.other & !3) | visibility;
                    index
                } else {
                    let index = symbols.len();
                    globals.insert(candidate.name.clone(), index);
                    symbols.push(candidate);
                    index
                }
            } else {
                let index = symbols.len();
                symbols.push(candidate);
                index
            };
            map.insert(s.index().0, index);
        }
        symbol_maps.push(map);
    }
    let mut order = (0..symbols.len()).collect::<Vec<_>>();
    order.sort_by_key(|i| symbols[*i].info >> 4 != 0);
    let first_global = 1 + order
        .iter()
        .take_while(|i| symbols[**i].info >> 4 == 0)
        .count();
    let mut symbol_indices = vec![0u32; symbols.len()];
    for (i, original) in order.iter().enumerate() {
        symbol_indices[*original] = i as u32 + 1;
    }
    for (ui, file) in files.iter().enumerate() {
        let map = &section_maps[ui];
        for section in file.sections() {
            let Some(&index) = map.get(&section.index().0) else {
                continue;
            };
            let out = &mut sections[index - 1];
            if out.kind == object::elf::SHT_RELA {
                let target = out.info as usize;
                out.info = *map
                    .get(&target)
                    .ok_or("RELA target section was discarded")? as u32;
                out.link = symtab;
                let original = std::mem::take(&mut out.data);
                if !original.len().is_multiple_of(24) {
                    return Err("malformed ELF RELA size".into());
                }
                for row in original.as_chunks::<24>().0 {
                    let offset =
                        u64::from_le_bytes(row[..8].try_into().expect("fixed relocation offset"));
                    let Some(offset) = inputs[ui].groups.offset(target, offset) else {
                        continue;
                    };
                    let info =
                        u64::from_le_bytes(row[8..16].try_into().expect("fixed relocation info"));
                    let old_symbol = (info >> 32) as usize;
                    let symbol = if old_symbol == 0 {
                        0
                    } else if let Some(i) = symbol_maps[ui].get(&old_symbol) {
                        symbol_indices[*i]
                    } else {
                        let s = file
                            .symbol_by_index(object::SymbolIndex(old_symbol))
                            .map_err(|e| e.to_string())?;
                        if s.is_global() || s.is_weak() {
                            *globals
                                .get(s.name_bytes().map_err(|e| e.to_string())?)
                                .map(|i| &symbol_indices[*i])
                                .ok_or("relocation refers to discarded definition")?
                        } else {
                            return Err("relocation refers to discarded local definition".into());
                        }
                    };
                    out.data.extend_from_slice(&offset.to_le_bytes());
                    out.data.extend_from_slice(
                        &((u64::from(symbol) << 32) | (info & 0xffff_ffff)).to_le_bytes(),
                    );
                    out.data.extend_from_slice(&row[16..24]);
                }
                out.size = out.data.len() as u64;
            } else if out.kind == object::elf::SHT_GROUP {
                out.link = symtab;
                let signature = *symbol_maps[ui]
                    .get(&(out.info as usize))
                    .ok_or("missing COMDAT signature")?;
                out.info = symbol_indices[signature];
                let mut data = Vec::new();
                for (i, row) in out.data.as_chunks::<4>().0.iter().enumerate() {
                    let value = u32::from_le_bytes(*row);
                    let value = if i == 0 {
                        value
                    } else {
                        *map.get(&(value as usize)).ok_or("missing COMDAT member")? as u32
                    };
                    data.extend_from_slice(&value.to_le_bytes());
                }
                out.data = data;
            } else {
                if out.link != 0 {
                    out.link =
                        *map.get(&(out.link as usize))
                            .ok_or("unsupported ELF section link")? as u32;
                }
                if out.flags & u64::from(object::elf::SHF_INFO_LINK) != 0 && out.info != 0 {
                    out.info = *map
                        .get(&(out.info as usize))
                        .ok_or("unsupported ELF section info link")?
                        as u32;
                }
            }
        }
    }
    let mut strings = vec![0];
    let mut symbol_data = vec![0; (symbols.len() + 1) * 24];
    for (i, original) in order.into_iter().enumerate() {
        let s = &symbols[original];
        let at = (i + 1) * 24;
        put32(&mut symbol_data, at, string(&mut strings, &s.name)?);
        symbol_data[at + 4] = s.info;
        symbol_data[at + 5] = s.other;
        put16(&mut symbol_data, at + 6, s.section);
        put64(&mut symbol_data, at + 8, s.value);
        put64(&mut symbol_data, at + 16, s.size);
    }
    sections.push(Section {
        name: b".symtab".to_vec(),
        kind: object::elf::SHT_SYMTAB,
        flags: 0,
        alignment: 8,
        size: symbol_data.len() as u64,
        data: symbol_data,
        link: symtab + 1,
        info: first_global as u32,
        entsize: 24,
    });
    sections.push(Section {
        name: b".strtab".to_vec(),
        kind: object::elf::SHT_STRTAB,
        flags: 0,
        alignment: 1,
        size: strings.len() as u64,
        data: strings,
        link: 0,
        info: 0,
        entsize: 0,
    });
    let mut names = vec![0];
    let mut name_indices = Vec::new();
    for section in &sections {
        name_indices.push(string(&mut names, &section.name)?);
    }
    name_indices.push(string(&mut names, b".shstrtab")?);
    sections.push(Section {
        name: b".shstrtab".to_vec(),
        kind: object::elf::SHT_STRTAB,
        flags: 0,
        alignment: 1,
        size: names.len() as u64,
        data: names,
        link: 0,
        info: 0,
        entsize: 0,
    });
    let mut cursor = 64u64;
    let mut offsets = Vec::new();
    for section in &sections {
        cursor = aligned(cursor, section.alignment)?;
        offsets.push(cursor);
        if section.kind != object::elf::SHT_NOBITS {
            cursor = cursor
                .checked_add(section.size)
                .filter(|v| *v <= LIMIT)
                .ok_or("relocatable ELF exceeds 512 MiB limit")?;
        }
    }
    let shoff = aligned(cursor, 8)?;
    let total = shoff
        .checked_add((sections.len() as u64 + 1) * 64)
        .filter(|v| *v <= LIMIT)
        .ok_or("relocatable ELF exceeds 512 MiB limit")?;
    let mut bytes = vec![0; total as usize];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut bytes, 16, object::elf::ET_REL);
    put16(
        &mut bytes,
        18,
        match machine {
            Machine::X86_64 => object::elf::EM_X86_64,
            Machine::Aarch64 => object::elf::EM_AARCH64,
        },
    );
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 40, shoff);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 58, 64);
    put16(&mut bytes, 60, sections.len() as u16 + 1);
    put16(&mut bytes, 62, sections.len() as u16);
    for (i, section) in sections.iter().enumerate() {
        let at = shoff as usize + (i + 1) * 64;
        put32(&mut bytes, at, name_indices[i]);
        put32(&mut bytes, at + 4, section.kind);
        put64(&mut bytes, at + 8, section.flags);
        put64(&mut bytes, at + 24, offsets[i]);
        put64(&mut bytes, at + 32, section.size);
        put32(&mut bytes, at + 40, section.link);
        put32(&mut bytes, at + 44, section.info);
        put64(&mut bytes, at + 48, section.alignment);
        put64(&mut bytes, at + 56, section.entsize);
        if section.kind != object::elf::SHT_NOBITS {
            bytes[offsets[i] as usize..offsets[i] as usize + section.data.len()]
                .copy_from_slice(&section.data);
        }
    }
    Ok(Executable {
        bytes,
        warnings: Vec::new(),
    })
}
