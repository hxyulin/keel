//! MH_OBJECT: one __TEXT,__text section, symbols, and arm64 instruction relocations.
//! Mach-O field layout follows Apple's loader.h / nlist.h. No host structs
//! are serialized, so the result is identical on non-Apple hosts.
use super::{Assembly, Error, RelocationKind};
use std::collections::BTreeMap;

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn u64le(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn name(out: &mut Vec<u8>, name: &str) {
    let mut bytes = [0; 16];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    out.extend_from_slice(&bytes);
}
fn size(value: usize) -> Result<u32, Error> {
    value
        .try_into()
        .map_err(|_| Error::new(0, "Mach-O object exceeds the 32-bit file-offset limit"))
}
fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

pub(super) fn write(assembly: &Assembly) -> Result<Vec<u8>, Error> {
    if assembly.text_alignment < 2
        || assembly.text_alignment > 16
        || !assembly.text.len().is_multiple_of(4)
    {
        return Err(Error::new(0, "invalid A64 text alignment or size"));
    }
    size(assembly.text.len())?;
    let mut symbols: Vec<_> = assembly.symbols.iter().collect();
    let group = |s: &super::Symbol| {
        if s.offset.is_none() {
            2
        } else if s.global {
            1
        } else {
            0
        }
    };
    symbols.sort_by(|a, b| (group(a), &a.name).cmp(&(group(b), &b.name)));
    let nsyms = size(symbols.len())?;
    if nsyms > 1 << 24 {
        return Err(Error::new(
            0,
            "Mach-O relocation symbol index exceeds 24 bits",
        ));
    }
    let nlocals = size(symbols.iter().filter(|s| group(s) == 0).count())?;
    let ndefined = size(symbols.iter().filter(|s| group(s) == 1).count())?;
    let nundefined = nsyms - nlocals - ndefined;
    let mut strings = vec![0];
    let mut entries = Vec::new();
    let mut indices = BTreeMap::new();
    for (index, symbol) in symbols.into_iter().enumerate() {
        if symbol
            .offset
            .is_some_and(|o| o > assembly.text.len() as u64)
            || symbol.name.is_empty()
            || symbol.name.contains('\0')
            || (symbol.offset.is_none() && !symbol.global)
            || indices.insert(symbol.name.as_str(), index as u32).is_some()
        {
            return Err(Error::new(0, "invalid or duplicate text symbol"));
        }
        u32le(&mut entries, size(strings.len())?);
        entries.extend_from_slice(&[
            if symbol.offset.is_none() {
                0x01
            } else if symbol.global {
                0x0f
            } else {
                0x0e
            },
            u8::from(symbol.offset.is_some()),
            0,
            0,
        ]); // N_UNDF | N_EXT or N_SECT (optionally external), section 0/1, desc 0
        u64le(&mut entries, symbol.offset.unwrap_or(0));
        strings.extend_from_slice(symbol.name.as_bytes());
        strings.push(0);
    }
    let mut relocations: Vec<_> = assembly.relocations.iter().collect();
    relocations.sort_by_key(|r| std::cmp::Reverse(r.offset));
    let mut records = Vec::new();
    let mut previous = None;
    for relocation in relocations {
        if !relocation.offset.is_multiple_of(4)
            || relocation.offset as usize + 4 > assembly.text.len()
            || relocation.offset > i32::MAX as u32
            || previous == Some(relocation.offset)
        {
            return Err(Error::new(
                0,
                "invalid or duplicate instruction relocation offset",
            ));
        }
        previous = Some(relocation.offset);
        if !(-(1i64 << 23)..(1i64 << 23)).contains(&relocation.addend)
            || (!relocation.kind.accepts_addend() && relocation.addend != 0)
            || (relocation.kind == RelocationKind::Branch26 && relocation.addend % 4 != 0)
        {
            return Err(Error::new(0, "invalid relocation addend"));
        }
        let index = indices.get(relocation.symbol.as_str()).ok_or_else(|| {
            Error::new(
                0,
                format!(
                    "relocation references missing symbol `{}`",
                    relocation.symbol
                ),
            )
        })?;
        // ARM64_RELOC_ADDEND precedes its matching instruction relocation.
        if relocation.addend != 0 {
            u32le(&mut records, relocation.offset);
            u32le(
                &mut records,
                ((relocation.addend as u32) & 0x00ff_ffff) | (2 << 25) | (10 << 28),
            );
        }
        u32le(&mut records, relocation.offset);
        u32le(
            &mut records,
            index
                | (u32::from(relocation.kind.pc_relative()) << 24)
                | (2 << 25)
                | (1 << 27)
                | ((relocation.kind as u32) << 28),
        );
    }
    let nreloc = size(records.len() / 8)?;
    let strsize = size(strings.len())?;
    const COMMAND_SIZE: usize = 152 + 24 + 80 + 24;
    let textoff = align(32 + COMMAND_SIZE, 1 << assembly.text_alignment);
    let reloff = align(textoff + assembly.text.len(), 4);
    let symoff = align(reloff + records.len(), 8);
    let stroff = symoff + entries.len();
    size(stroff + strings.len())?;
    let mut out = Vec::new();
    for value in [0xfeed_facf, 0x0100_000c, 0, 1, 4, COMMAND_SIZE as u32, 0, 0] {
        u32le(&mut out, value);
    }
    // LC_SEGMENT_64 + one section_64. Object segments have an empty name.
    u32le(&mut out, 0x19);
    u32le(&mut out, 152);
    name(&mut out, "");
    u64le(&mut out, 0);
    u64le(&mut out, assembly.text.len() as u64);
    u64le(&mut out, textoff as u64);
    u64le(&mut out, assembly.text.len() as u64);
    for value in [7, 7, 1, 0] {
        u32le(&mut out, value);
    }
    name(&mut out, "__text");
    name(&mut out, "__TEXT");
    u64le(&mut out, 0);
    u64le(&mut out, assembly.text.len() as u64);
    for value in [
        size(textoff)?,
        assembly.text_alignment,
        if nreloc == 0 { 0 } else { size(reloff)? },
        nreloc,
        0x8000_0400 | if nreloc == 0 { 0 } else { 0x200 },
        0,
        0,
        0,
    ] {
        u32le(&mut out, value);
    }
    // LC_SYMTAB
    for value in [2, 24, size(symoff)?, nsyms, size(stroff)?, strsize] {
        u32le(&mut out, value);
    }
    // LC_DYSYMTAB: local definitions, external definitions, then undefineds.
    for value in [
        0xb,
        80,
        0,
        nlocals,
        nlocals,
        ndefined,
        nlocals + ndefined,
        nundefined,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ] {
        u32le(&mut out, value);
    }
    // LC_BUILD_VERSION: macOS 11.0, the first arm64 macOS release.
    for value in [0x32, 24, 1, 11 << 16, 0, 0] {
        u32le(&mut out, value);
    }
    debug_assert_eq!(out.len(), 32 + COMMAND_SIZE);
    out.resize(textoff, 0);
    out.extend_from_slice(&assembly.text);
    out.resize(reloff, 0);
    out.extend_from_slice(&records);
    out.resize(symoff, 0);
    out.extend_from_slice(&entries);
    out.extend_from_slice(&strings);
    Ok(out)
}
