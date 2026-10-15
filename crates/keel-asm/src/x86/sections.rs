//! ELF section syntax and the metadata not exposed by object 0.36's writer.
use super::Section;
use keel_asm_ir::x86;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Declaration {
    pub name: String,
    pub flags: Option<u64>,
    pub nobits: bool,
    pub entry_size: u64,
    pub group: Option<String>,
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
    if flags & !0x637 != 0 {
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
                _ => return Err(format!("unsupported ELF section flag `{}`", b as char)),
            };
        }
        flags = Some(f);
    }
    let mut nobits = matches!(name.as_str(), ".bss" | ".tbss");
    if let Some(t) = parts.get(2) {
        match *t {
            "@progbits" | "%progbits" => nobits = false,
            "@nobits" | "%nobits" => nobits = true,
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
        u64::try_from(x86::integer(size).map_err(|_| "invalid merge entry size")?)
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
) -> Result<(), String> {
    if groups.is_empty() && sections.iter().all(|s| s.entry_size == 0) {
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
    Ok(())
}
