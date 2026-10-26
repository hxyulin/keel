//! ELF section syntax and the metadata not exposed by object 0.36's writer.
use super::Section;
use keel_asm_ir::x86;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Declaration<Name = String> {
    pub name: Name,
    pub flags: Option<u64>,
    pub nobits: bool,
    pub entry_size: u64,
    pub group: Option<String>,
    pub section_type: Option<u32>,
    pub unique: Option<u64>,
}

pub(super) fn validate(
    name: &str,
    flags: u64,
    nobits: bool,
    entry_size: u64,
    group: Option<&str>,
) -> Result<(), String> {
    if name.is_empty() || name.contains('\0') {
        return Err("invalid ELF section name".into());
    }
    if flags & !0x8020_0637 != 0 {
        return Err("unsupported ELF section flags".into());
    }
    if flags & 0x20 != 0 && flags & 0x10 == 0 {
        return Err("SHF_STRINGS requires SHF_MERGE (section flag M)".into());
    }
    if (flags & 0x10 != 0) != (entry_size != 0) {
        return Err("SHF_MERGE requires a positive entry size; entry sizes require flag M".into());
    }
    if nobits && flags & 0x30 != 0 {
        return Err("merge/string attributes are unsupported for NOBITS sections".into());
    }
    if (flags & 0x200 != 0) != group.is_some() {
        return Err("section flag G requires a COMDAT signature and comdat selection".into());
    }
    if group.is_some_and(|s| s.is_empty() || s.contains('\0')) {
        return Err("invalid COMDAT signature".into());
    }
    Ok(())
}

/// LLVM's `name` or `name.*` section-name prefix test.
fn has_prefix(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// The flags LLVM MC infers from a section name. It ORs them into explicit
/// flags too. Array types use the same prefix test as their allocation flags.
pub(super) fn default_flags(name: &str) -> u64 {
    if has_prefix(name, ".rodata") || name == ".rodata1" {
        2
    } else if has_prefix(name, ".text") || matches!(name, ".init" | ".fini") {
        6
    } else if array_type(name).is_some()
        || has_prefix(name, ".data")
        || name == ".data1"
        || has_prefix(name, ".bss")
    {
        3
    } else if has_prefix(name, ".tdata") || has_prefix(name, ".tbss") {
        0x403
    } else {
        0
    }
}

/// LLVM MC makes `.bss`/`.tbss` sections NOBITS unless a type is given.
pub(super) fn default_nobits(name: &str) -> bool {
    has_prefix(name, ".bss") || has_prefix(name, ".tbss")
}

fn array_type(name: &str) -> Option<u32> {
    [
        (".init_array", 14),
        (".fini_array", 15),
        (".preinit_array", 16),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| has_prefix(name, prefix).then_some(kind))
}

pub(super) fn parse(parts: &[&str]) -> Result<Declaration, String> {
    let name = if parts[0].starts_with('"') {
        x86::symbol(parts[0])?
    } else if !parts[0].is_empty()
        && parts[0]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        parts[0].to_string()
    } else {
        return Err("invalid ELF section name".into());
    };
    let mut flags = None;
    if let Some(s) = parts.get(1) {
        let mut f = 0;
        for b in x86::unquote(s)? {
            f |= match b {
                b'a' => 2,
                b'w' => 1,
                b'x' => 4,
                b'M' => 0x10,
                b'S' => 0x20,
                b'G' => 0x200,
                b'T' => 0x400,
                b'R' => 0x200000,
                _ => return Err(format!("unsupported ELF section flag `{}`", b as char)),
            };
        }
        flags = Some(f | default_flags(&name));
    }
    let mut nobits = default_nobits(&name);
    let mut section_type = array_type(&name);
    if let Some(t) = parts.get(2) {
        match *t {
            "@progbits" | "%progbits" => {
                nobits = false;
                section_type = Some(1);
            }
            "@nobits" | "%nobits" => {
                nobits = true;
                section_type = Some(8);
            }
            "@init_array" | "%init_array" => {
                nobits = false;
                section_type = Some(14);
            }
            "@fini_array" | "%fini_array" => {
                nobits = false;
                section_type = Some(15);
            }
            "@preinit_array" | "%preinit_array" => {
                nobits = false;
                section_type = Some(16);
            }
            "@unwind" | "%unwind" => {
                nobits = false;
                section_type = Some(0x7000_0001);
            }
            "@note" | "%note" => {
                nobits = false;
                section_type = Some(7);
            }
            _ => return Err(format!("unsupported section type `{t}`")),
        }
    }
    let f = flags.unwrap_or(0);
    let mut next = 3;
    let entry_size = if f & 0x10 != 0 {
        let size = parts
            .get(next)
            .ok_or("SHF_MERGE requires a positive entry size")?;
        next += 1;
        u64::try_from(x86::integer(size).map_err(|e| format!("invalid merge entry size: {e}"))?)
            .ok()
            .filter(|&n| n != 0)
            .ok_or("merge entry size must be a positive u64")?
    } else {
        0
    };
    let group = if f & 0x200 != 0 {
        let signature = parts.get(next).ok_or("missing COMDAT group signature")?;
        if signature.is_empty() {
            return Err("missing COMDAT group signature".into());
        }
        let signature =
            x86::symbol(signature).map_err(|e| format!("invalid COMDAT signature: {e}"))?;
        next += 1;
        match parts.get(next) {
            Some(&"comdat") => {}
            Some(other) => {
                return Err(format!(
                    "unsupported section group selection `{other}`; expected comdat"
                ));
            }
            None => return Err("section group requires explicit comdat selection".into()),
        }
        next += 1;
        Some(signature)
    } else {
        None
    };
    let unique = if parts.get(next) == Some(&"unique") {
        next += 1;
        let id = parts.get(next).ok_or("missing unique section id")?;
        next += 1;
        Some(u64::try_from(x86::integer(id)?).map_err(|_| "invalid unique section id")?)
    } else {
        None
    };
    if parts.len() > next {
        return Err(
            "unexpected section arguments (entry sizes require M; COMDAT requires G)".into(),
        );
    }
    // Default attributes are selected by the assembler for bare section switches.
    if flags.is_some() {
        validate(&name, f, nobits, entry_size, group.as_deref())?;
    }
    Ok(Declaration {
        name,
        flags,
        nobits,
        entry_size,
        group,
        section_type,
        unique,
    })
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// object 0.36 writes groups but omits associated RELA sections from membership,
/// and has no arbitrary sh_entsize API. Complete only those fields in our own
/// ELF64 little-endian output. No offsets in existing contents are moved.
pub(super) fn finalize(
    bytes: &mut Vec<u8>,
    sections: &[Section],
    groups: &BTreeMap<String, Vec<usize>>,
    synthetic_signatures: &BTreeSet<String>,
    addrsig: Option<&[String]>,
) -> Result<(), String> {
    // GNU retain is OS-specific; GNU ld recognizes it with ELFOSABI_GNU.
    if sections.iter().any(|s| s.flags & 0x200000 != 0) {
        bytes[7] = 3;
    }
    if groups.is_empty()
        && sections
            .iter()
            .all(|s| s.entry_size == 0 && matches!(s.section_type, 1 | 8))
        && addrsig.is_none()
    {
        return Ok(());
    }
    let shoff = u64_at(bytes, 40) as usize;
    let count = u16::from_le_bytes(bytes[60..62].try_into().unwrap()) as usize;
    let count = if count == 0 {
        u64_at(bytes, shoff + 32) as usize
    } else {
        count
    };
    // All assembler-owned sections are PROGBITS or NOBITS. Generated tables,
    // groups and relocation sections have distinct types. Preserve section
    // order rather than matching names: distinct groups may share a name.
    let indices: Vec<_> = (1..count)
        .filter(|&i| matches!(u32_at(bytes, shoff + 64 * i + 4), 1 | 8))
        .collect();
    if indices.len() != sections.len() {
        return Err("unexpected ELF writer section layout".into());
    }
    for (s, &i) in sections.iter().zip(&indices) {
        put64(bytes, shoff + 64 * i + 56, s.entry_size);
        bytes[shoff + 64 * i + 4..shoff + 64 * i + 8]
            .copy_from_slice(&s.section_type.to_le_bytes());
    }
    let group_headers: Vec<_> = (1..count)
        .filter(|&i| u32_at(bytes, shoff + 64 * i + 4) == 17)
        .collect();
    if group_headers.len() != groups.len() {
        return Err("unexpected ELF writer group layout".into());
    }
    let mut relocations: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 1..count {
        let header = shoff + 64 * i;
        if matches!(u32_at(bytes, header + 4), 4 | 9) {
            relocations
                .entry(u32_at(bytes, header + 44) as usize)
                .or_default()
                .push(i);
        }
    }
    for ((signature, members), &group_index) in groups.iter().zip(&group_headers) {
        let header = shoff + 64 * group_index;
        if synthetic_signatures.contains(signature) {
            if group_index >= 0xff00 {
                return Err(
                    "extended ELF section indexes for synthetic COMDAT signatures are unsupported"
                        .into(),
                );
            }
            // GAS/Clang define an otherwise undeclared signature in its .group
            // section, not in a member. The writer API cannot name that section.
            let symtab = shoff + 64 * u32_at(bytes, header + 40) as usize;
            let symbol =
                u64_at(bytes, symtab + 24) as usize + 24 * u32_at(bytes, header + 44) as usize;
            bytes[symbol + 6..symbol + 8].copy_from_slice(&(group_index as u16).to_le_bytes());
        }
        let mut entries = vec![1u32]; // GRP_COMDAT
        for &member in members {
            let i = indices[member];
            entries.push(i as u32);
            if let Some(relocations) = relocations.get(&i) {
                for &r in relocations {
                    entries.push(r as u32);
                    let header = shoff + 64 * r;
                    let flags = u64_at(bytes, header + 8);
                    put64(bytes, header + 8, flags | 0x200);
                }
            }
        }
        // Reuse the writer's group payload if it already has all members.
        if u64_at(bytes, header + 32) == (entries.len() * 4) as u64 {
            continue;
        }
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let offset = bytes.len() as u64;
        for entry in &entries {
            bytes.extend_from_slice(&entry.to_le_bytes());
        }
        put64(bytes, header + 24, offset);
        put64(bytes, header + 32, (entries.len() * 4) as u64);
    }
    if let Some(names) = addrsig {
        let symtab_index = (1..count)
            .find(|&i| u32_at(bytes, shoff + 64 * i + 4) == 2)
            .ok_or("missing symbol table for address signatures")?;
        let symtab = shoff + 64 * symtab_index;
        let strings_header = shoff + 64 * u32_at(bytes, symtab + 40) as usize;
        let strings = u64_at(bytes, strings_header + 24) as usize;
        let symbol_offset = u64_at(bytes, symtab + 24) as usize;
        let symbol_count = u64_at(bytes, symtab + 32) as usize / 24;
        let mut symbols = BTreeMap::new();
        for i in 1..symbol_count {
            let start = strings + u32_at(bytes, symbol_offset + i * 24) as usize;
            let len = bytes[start..]
                .iter()
                .position(|&b| b == 0)
                .ok_or("invalid ELF symbol name")?;
            symbols.insert(bytes[start..start + len].to_vec(), i);
        }
        let mut payload = Vec::new();
        let mut seen = BTreeSet::new();
        for name in names {
            if let Some(&index) = symbols.get(name.as_bytes())
                && seen.insert(index)
            {
                super::metadata::leb(&mut payload, index as i128, false)?;
            }
        }
        let index = sections
            .iter()
            .position(|s| s.name == ".llvm_addrsig")
            .ok_or("missing address signature section")?;
        let header = shoff + 64 * indices[index];
        let start = bytes.len();
        bytes.extend(payload);
        put64(bytes, header + 24, start as u64);
        let size = (bytes.len() - start) as u64;
        put64(bytes, header + 32, size);
        bytes[header + 40..header + 44].copy_from_slice(&(symtab_index as u32).to_le_bytes());
    }
    Ok(())
}
