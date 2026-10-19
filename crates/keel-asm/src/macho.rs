//! Multi-section MH_OBJECT with instruction, pointer, and difference relocations.
//! Serialize fields explicitly so host layout never affects the object format.
use super::{Assembly, Error, RelocationKind, SectionContents, SectionKind};
use std::collections::{BTreeMap, BTreeSet};

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
fn align(value: u64, alignment: u64) -> Result<u64, Error> {
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .ok_or_else(|| Error::new(0, "section layout overflow"))
}
struct View<'a> {
    segment: &'a str,
    name: &'a str,
    alignment: u32,
    flags: u32,
    data: &'a [u8],
    size: u64,
    zero: bool,
}
struct Layout {
    id: usize,
    address: u64,
    offset: u32,
    reloff: u32,
    records: Vec<u8>,
}

pub(super) fn write(assembly: &Assembly) -> Result<Vec<u8>, Error> {
    if assembly.text_alignment < 2 || assembly.text_alignment > 16 || assembly.sections.len() > 254
    {
        return Err(Error::new(0, "invalid text alignment or section count"));
    }
    let mut views = vec![View {
        segment: "__TEXT",
        name: "__text",
        alignment: assembly.text_alignment,
        flags: 0x8000_0400,
        data: &assembly.text,
        size: assembly.text.len() as u64,
        zero: false,
    }];
    let mut names = BTreeSet::from([("__TEXT", "__text")]);
    for section in &assembly.sections {
        if section.segment.is_empty()
            || section.name.is_empty()
            || section.segment.len() > 16
            || section.name.len() > 16
            || section.segment.contains('\0')
            || section.name.contains('\0')
            || section.alignment > 16
            || !names.insert((section.segment.as_str(), section.name.as_str()))
            || section.kind.is_zero_fill()
                != matches!(section.contents, SectionContents::ZeroFill(_))
        {
            return Err(Error::new(0, "invalid or duplicate Mach-O section"));
        }
        views.push(View {
            segment: &section.segment,
            name: &section.name,
            alignment: section.alignment,
            flags: section.kind.flags(),
            data: section.data(),
            size: section.size(),
            zero: section.kind.is_zero_fill(),
        });
    }
    // All file-backed sections precede zero-fill sections in VM layout.
    // Symbols keep their parser IDs; this mapping supplies on-disk ordinals.
    let mut order: Vec<_> = (0..views.len()).collect();
    order.sort_by_key(|id| views[*id].zero);
    let segment_size = 72 + views.len() * 80;
    let has_loh = !assembly.linker_optimization_hints.is_empty();
    let command_size = segment_size + 24 + 80 + 24 + if has_loh { 16 } else { 0 };
    let mut file = (32 + command_size) as u64;
    let mut vm = 0;
    let mut layouts = Vec::new();
    let mut ordinals = vec![0; views.len()];
    let mut addresses = vec![0; views.len()];
    let mut section_offsets = vec![0; views.len()];
    let mut first_file = None;
    for id in order {
        let view = &views[id];
        vm = align(vm, 1 << view.alignment)?;
        let address = vm;
        vm = vm
            .checked_add(view.size)
            .ok_or_else(|| Error::new(0, "section address overflow"))?;
        let offset = if view.zero {
            0
        } else {
            file = align(file, 1 << view.alignment)?;
            let offset = u32::try_from(file)
                .map_err(|_| Error::new(0, "section file offset exceeds 32 bits"))?;
            first_file.get_or_insert(file);
            file = file
                .checked_add(view.size)
                .ok_or_else(|| Error::new(0, "section file size overflow"))?;
            offset
        };
        ordinals[id] = (layouts.len() + 1) as u8;
        addresses[id] = address;
        section_offsets[id] = offset;
        layouts.push(Layout {
            id,
            address,
            offset,
            reloff: 0,
            records: Vec::new(),
        });
    }
    let data_end = file;
    let first_file = first_file.unwrap_or(data_end);
    let mut all_names = BTreeSet::new();
    for symbol in &assembly.symbols {
        if symbol.name.is_empty()
            || symbol.name.contains('\0')
            || !all_names.insert(symbol.name.as_str())
            || (symbol.offset.is_none() && !symbol.global)
            || (symbol.private_extern && !symbol.global)
            || symbol.offset.is_some() != symbol.section.is_some()
            || symbol.common.is_some_and(|(size, alignment)| {
                size == 0 || alignment > 15 || symbol.section.is_some() || !symbol.global
            })
        {
            return Err(Error::new(0, "invalid or duplicate symbol"));
        }
        if let Some(section) = symbol.section
            && views
                .get(section)
                .is_none_or(|v| symbol.offset.unwrap() > v.size)
        {
            return Err(Error::new(0, "invalid symbol section or offset"));
        }
    }
    let definitions: BTreeMap<_, _> = assembly
        .symbols
        .iter()
        .filter_map(|s| Some((s.name.as_str(), (s.section?, s.offset?))))
        .collect();
    let temporary: BTreeSet<_> = assembly
        .symbols
        .iter()
        .filter(|s| !s.global && s.name.starts_with('L'))
        .map(|s| s.name.as_str())
        .collect();
    // Absolute local pointers can use a section ordinal and original address,
    // so no interior temporary label has to become a linker function entry.
    let section_target = |r: &super::Relocation| {
        if r.kind != RelocationKind::Unsigned
            || r.subtract_symbol.is_some()
            || !(temporary.contains(r.symbol.as_str())
                || views
                    .get(r.section)
                    .is_some_and(|v| v.flags & 0x0200_0000 != 0))
        {
            return None;
        }
        definitions.get(r.symbol.as_str()).copied()
    };
    // Darwin temporary labels are not independent function entries. Keeping
    // interior L labels in nlist makes ld generate spurious no-unwind ranges.
    let referenced: BTreeSet<_> = assembly
        .relocations
        .iter()
        .flat_map(|r| {
            let target = if section_target(r).is_some() {
                None
            } else {
                Some(r.symbol.as_str())
            };
            target.into_iter().chain(r.subtract_symbol.as_deref())
        })
        .collect();
    let mut symbols: Vec<_> = assembly
        .symbols
        .iter()
        .filter(|s| s.global || !s.name.starts_with('L') || referenced.contains(s.name.as_str()))
        .collect();
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
        if symbol.name.is_empty()
            || symbol.name.contains('\0')
            || (symbol.offset.is_none() && !symbol.global)
            || symbol.offset.is_some() != symbol.section.is_some()
            || symbol.common.is_some_and(|(size, alignment)| {
                size == 0 || alignment > 15 || symbol.section.is_some() || !symbol.global
            })
            || indices.insert(symbol.name.as_str(), index as u32).is_some()
        {
            return Err(Error::new(0, "invalid or duplicate symbol"));
        }
        let (ordinal, value) = if let Some(section) = symbol.section {
            let view = views
                .get(section)
                .ok_or_else(|| Error::new(0, "invalid symbol section"))?;
            let offset = symbol.offset.unwrap();
            if offset > view.size {
                return Err(Error::new(0, "symbol offset exceeds its section"));
            }
            (
                ordinals[section],
                addresses[section]
                    .checked_add(offset)
                    .ok_or_else(|| Error::new(0, "symbol address overflow"))?,
            )
        } else {
            (0, symbol.common.map_or(0, |c| c.0))
        };
        u32le(&mut entries, size(strings.len())?);
        entries.extend_from_slice(&[
            (if ordinal == 0 {
                1
            } else if symbol.global {
                0xf
            } else {
                0xe
            }) | if symbol.private_extern { 0x10 } else { 0 },
            ordinal,
            0,
            if let Some((_, alignment)) = symbol.common {
                alignment
            } else if symbol.section.is_some() && !symbol.global && symbol.name.starts_with('L') {
                2
            } else {
                0
            },
        ]);
        u64le(&mut entries, value);
        strings.extend_from_slice(symbol.name.as_bytes());
        strings.push(0);
    }
    for layout in &mut layouts {
        let view = &views[layout.id];
        let mut relocations: Vec<_> = assembly
            .relocations
            .iter()
            .filter(|r| r.section == layout.id)
            .collect();
        relocations.sort_by_key(|r| std::cmp::Reverse(r.offset));
        let mut previous_end = view.size;
        for r in relocations {
            let instruction = !matches!(
                r.kind,
                RelocationKind::Unsigned | RelocationKind::PointerToGot
            );
            let length = match r.size {
                4 => 2,
                8 if !instruction => 3,
                _ => return Err(Error::new(0, "invalid relocation width")),
            };
            if view.zero
                || u64::from(r.offset) + u64::from(r.size) > previous_end
                || r.offset > i32::MAX as u32
                || (instruction && (r.section != 0 || !r.offset.is_multiple_of(4)))
                || (instruction && r.subtract_symbol.is_some())
                || (r.kind == RelocationKind::Unsigned
                    && r.size == 4
                    && r.subtract_symbol.is_none())
                || (r.kind == RelocationKind::PointerToGot
                    && (r.size != 4 || r.subtract_symbol.is_some() || r.addend != 0))
            {
                return Err(Error::new(0, "invalid or overlapping relocation location"));
            }
            previous_end = u64::from(r.offset);
            if instruction
                && (!(-(1i64 << 23)..(1i64 << 23)).contains(&r.addend)
                    || (!r.kind.accepts_addend() && r.addend != 0)
                    || (r.kind == RelocationKind::Branch26 && r.addend % 4 != 0))
            {
                return Err(Error::new(0, "invalid instruction relocation addend"));
            }
            let local = section_target(r);
            let index = if local.is_some() {
                0
            } else {
                *indices.get(r.symbol.as_str()).ok_or_else(|| {
                    Error::new(
                        0,
                        format!("relocation references missing symbol `{}`", r.symbol),
                    )
                })?
            };
            if let Some(subtract) = &r.subtract_symbol {
                let index = *indices
                    .get(subtract.as_str())
                    .ok_or_else(|| Error::new(0, "difference references missing symbol"))?;
                u32le(&mut layout.records, r.offset);
                u32le(
                    &mut layout.records,
                    index | (length << 25) | (1 << 27) | (1 << 28),
                );
            } else if instruction && r.addend != 0 {
                u32le(&mut layout.records, r.offset);
                u32le(
                    &mut layout.records,
                    ((r.addend as u32) & 0x00ff_ffff) | (2 << 25) | (10 << 28),
                );
            }
            let (index, external) =
                local.map_or((index, 1), |(section, _)| (u32::from(ordinals[section]), 0));
            u32le(&mut layout.records, r.offset);
            u32le(
                &mut layout.records,
                index
                    | (u32::from(r.kind.pc_relative()) << 24)
                    | (length << 25)
                    | (external << 27)
                    | ((r.kind as u32) << 28),
            );
        }
        if !layout.records.is_empty() {
            file = align(file, 8)?;
            layout.reloff = u32::try_from(file)
                .map_err(|_| Error::new(0, "relocation table exceeds 32 bits"))?;
            file = file
                .checked_add(layout.records.len() as u64)
                .ok_or_else(|| Error::new(0, "relocation size overflow"))?;
        }
    }
    if assembly
        .relocations
        .iter()
        .any(|r| r.section >= views.len())
    {
        return Err(Error::new(0, "invalid relocation section"));
    }
    let mut loh = Vec::new();
    for hint in &assembly.linker_optimization_hints {
        let count = hint.argument_count()?;
        uleb(&mut loh, u64::from(hint.kind));
        uleb(&mut loh, count as u64);
        for offset in &hint.offsets[..count] {
            if *offset % 4 != 0 || *offset >= assembly.text.len() as u64 {
                return Err(Error::new(0, "invalid LOH text offset"));
            }
            uleb(&mut loh, addresses[0] + offset);
        }
    }
    let lohoff = align(file, 8)?;
    loh.resize(align(loh.len() as u64, 8)? as usize, 0);
    if has_loh {
        file = lohoff + loh.len() as u64;
    }
    let symoff = align(file, 8)?;
    let stroff = symoff
        .checked_add(entries.len() as u64)
        .ok_or_else(|| Error::new(0, "symbol table size overflow"))?;
    let end = stroff
        .checked_add(strings.len() as u64)
        .ok_or_else(|| Error::new(0, "string table size overflow"))?;
    if end > u64::from(u32::MAX) {
        return Err(Error::new(0, "Mach-O file exceeds 32 bits"));
    }
    let mut out = Vec::new();
    let flags = (if assembly
        .sections
        .iter()
        .any(|s| s.kind == SectionKind::ThreadVariables)
    {
        0x800000
    } else {
        0
    }) | if assembly.subsections_via_symbols {
        0x2000
    } else {
        0
    };
    for value in [
        0xfeed_facf,
        0x0100_000c,
        0,
        1,
        if has_loh { 5 } else { 4 },
        command_size as u32,
        flags,
        0,
    ] {
        u32le(&mut out, value);
    }
    // One unnamed LC_SEGMENT_64, containing all sections in object VM order.
    u32le(&mut out, 0x19);
    u32le(&mut out, segment_size as u32);
    name(&mut out, "");
    u64le(&mut out, 0);
    u64le(&mut out, vm);
    u64le(&mut out, first_file);
    u64le(&mut out, data_end - first_file);
    for value in [7, 7, views.len() as u32, 0] {
        u32le(&mut out, value);
    }
    for layout in &layouts {
        let view = &views[layout.id];
        name(&mut out, view.name);
        name(&mut out, view.segment);
        u64le(&mut out, layout.address);
        u64le(&mut out, view.size);
        for value in [
            layout.offset,
            view.alignment,
            layout.reloff,
            size(layout.records.len() / 8)?,
            view.flags | if layout.records.is_empty() { 0 } else { 0x200 },
            0,
            0,
            0,
        ] {
            u32le(&mut out, value);
        }
    }
    for value in [
        2,
        24,
        symoff as u32,
        nsyms,
        stroff as u32,
        size(strings.len())?,
    ] {
        u32le(&mut out, value);
    }
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
    let build = assembly.build_version;
    for value in [0x32, 24, build.platform, build.minimum, build.sdk, 0] {
        u32le(&mut out, value);
    }
    if has_loh {
        for value in [0x2e, 16, size(lohoff as usize)?, size(loh.len())?] {
            u32le(&mut out, value);
        }
    }
    debug_assert_eq!(out.len(), 32 + command_size);
    for layout in &layouts {
        if views[layout.id].zero {
            continue;
        }
        out.resize(layout.offset as usize, 0);
        out.extend_from_slice(views[layout.id].data);
    }
    // Pointer/difference addends belong in their data words, unlike the
    // explicit ADDEND records paired with instruction relocations.
    for r in &assembly.relocations {
        if matches!(
            r.kind,
            RelocationKind::Unsigned | RelocationKind::PointerToGot
        ) {
            let start = section_offsets[r.section] as usize + r.offset as usize;
            let value = section_target(r).map_or(r.addend as u64, |(section, offset)| {
                (r.addend as u64)
                    .wrapping_add(addresses[section])
                    .wrapping_add(offset)
            });
            out[start..start + usize::from(r.size)]
                .copy_from_slice(&value.to_le_bytes()[..usize::from(r.size)]);
        }
    }
    for layout in &layouts {
        if layout.records.is_empty() {
            continue;
        }
        out.resize(layout.reloff as usize, 0);
        out.extend_from_slice(&layout.records);
    }
    if has_loh {
        out.resize(lohoff as usize, 0);
        out.extend_from_slice(&loh);
    }
    out.resize(symoff as usize, 0);
    out.extend_from_slice(&entries);
    out.extend_from_slice(&strings);
    Ok(out)
}

fn uleb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        out.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}
