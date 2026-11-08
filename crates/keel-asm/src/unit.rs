//! Native semantic handoff, independent of Mach-O file layout and nlist tables.
use crate::fxhash::FxHashMap;
use crate::{Assembly, Error, RelocationKind, SectionContents};
use keel_object::{Atom, Definition, Fixup, FixupKind as K, Object, Section, Symbol, Target};
use std::collections::BTreeSet;

pub(crate) fn prepare(assembly: Assembly) -> Result<Object, Error> {
    let Assembly {
        text,
        text_alignment,
        sections,
        symbols,
        relocations,
        build_version,
        subsections_via_symbols,
        ..
    } = assembly;
    if build_version.platform != 1 || !(2..=16).contains(&text_alignment) {
        return Err(Error::new(
            0,
            "invalid AArch64 Darwin unit target or alignment",
        ));
    }
    let mut output = vec![Section {
        segment: "__TEXT".into(),
        name: "__text".into(),
        size: text.len(),
        alignment: text_alignment,
        flags: 0x80000400,
        data: text,
        fixups: Vec::new(),
    }];
    let mut names = BTreeSet::from([("__TEXT".to_string(), "__text".to_string())]);
    for s in sections {
        if s.segment.is_empty()
            || s.name.is_empty()
            || s.segment.len() > 16
            || s.name.len() > 16
            || s.segment.contains('\0')
            || s.name.contains('\0')
            || s.alignment > 16
            || !names.insert((s.segment.clone(), s.name.clone()))
            || s.kind.is_zero_fill() != matches!(s.contents, SectionContents::ZeroFill(_))
        {
            return Err(Error::new(0, "invalid or duplicate native section"));
        }
        let size =
            usize::try_from(s.size()).map_err(|_| Error::new(0, "native section too large"))?;
        let data = match s.contents {
            SectionContents::Bytes(bytes) => bytes,
            SectionContents::ZeroFill(_) => Vec::new(),
        };
        output.push(Section {
            segment: s.segment,
            name: s.name,
            size,
            alignment: s.alignment,
            flags: s.kind.flags() | if s.contains_instructions { 0x400 } else { 0 },
            data,
            fixups: Vec::new(),
        });
    }
    if symbols.len() >= 0x80000000 || output.iter().any(|s| s.size > 512 * 1024 * 1024) {
        return Err(Error::new(0, "native link unit exceeds size limits"));
    }
    let mut by_name = FxHashMap::default();
    by_name.reserve(symbols.len());
    for (i, s) in symbols.iter().enumerate() {
        if s.name.is_empty()
            || s.name.contains('\0')
            || by_name.insert(s.name.as_str(), i).is_some()
            || s.offset.is_some() != s.section.is_some()
            || (s.offset.is_none() && !s.global)
            || (s.private_extern && !s.global)
            || (s.weak_reference && (s.offset.is_some() || !s.global || s.common.is_some()))
            || (s.weak_can_be_hidden && !s.weak_definition)
            || (s.weak_definition && (!s.global || s.section.is_none()))
            || s.common.is_some_and(|(size, align)| {
                size == 0 || align > 15 || s.section.is_some() || !s.global
            })
        {
            return Err(Error::new(0, "invalid or duplicate native symbol"));
        }
        if let Some(section) = s.section
            && output
                .get(section)
                .is_none_or(|sec| s.offset.expect("defined symbol offset") > sec.size as u64)
        {
            return Err(Error::new(0, "invalid native symbol location"));
        }
    }
    let lookup = |name: &str| {
        by_name
            .get(name)
            .copied()
            .ok_or_else(|| Error::new(0, format!("relocation references missing symbol `{name}`")))
    };
    let mut needed: Vec<_> = symbols
        .iter()
        .map(|s| {
            s.global || s.no_dead_strip || (!s.name.starts_with('L') && !s.name.starts_with("ltmp"))
        })
        .collect();
    let references = relocations
        .iter()
        .map(|r| {
            let positive = lookup(&r.symbol)?;
            needed[positive] = true;
            let minus = r.subtract_symbol.as_deref().map(lookup).transpose()?;
            if let Some(i) = minus {
                needed[i] = true;
            }
            Ok((positive as u32, minus.map_or(u32::MAX, |i| i as u32)))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    drop(by_name);
    let mut atoms = Vec::with_capacity(needed.iter().filter(|n| **n).count());
    let mut named = Vec::new();
    let mut targets = Vec::with_capacity(symbols.len());
    for (i, s) in symbols.into_iter().enumerate() {
        if !needed[i] {
            targets.push(Target::Atom(u32::MAX));
            continue;
        }
        let definition = if let Some(section) = s.section {
            let offset = s.offset.expect("defined symbol has an offset");
            let id = atoms.len();
            atoms.push(Atom {
                section: section as u32,
                offset: offset as u32,
                boundary: !s.name.starts_with('L') && !s.name.starts_with("ltmp"),
                no_dead_strip: s.no_dead_strip,
            });
            Definition::Atom(id)
        } else if let Some((size, alignment)) = s.common {
            Definition::Common { size, alignment }
        } else {
            Definition::Undefined
        };
        targets.push(if s.global {
            let id = named.len();
            named.push(Symbol {
                name: s.name,
                definition,
                private: s.private_extern,
                weak_definition: s.weak_definition,
                weak_reference: s.weak_reference,
                weak_can_be_hidden: s.weak_can_be_hidden,
                no_dead_strip: s.no_dead_strip,
            });
            Target::Symbol(id as u32)
        } else {
            let Definition::Atom(id) = definition else {
                unreachable!()
            };
            Target::Atom(id as u32)
        });
    }
    let section_sizes: Vec<_> = output.iter().map(|s| s.size).collect();
    for (r, (positive, minus)) in relocations.into_iter().zip(references) {
        let positive = positive as usize;
        let minus = (minus != u32::MAX).then_some(minus as usize);
        let section = output
            .get_mut(r.section)
            .ok_or_else(|| Error::new(0, "invalid native fixup section"))?;
        if r.offset as usize + r.size as usize > section.data.len() || !matches!(r.size, 4 | 8) {
            return Err(Error::new(0, "native fixup outside file-backed section"));
        }
        let mut target = targets[positive];
        // Unwind ownership always follows this object's definition, including
        // a losing weak copy, rather than global symbol selection.
        if (section.name == "__compact_unwind" || section.name == "__eh_frame")
            && !(section.name == "__compact_unwind" && r.offset % 32 == 16)
            && r.kind != RelocationKind::PointerToGot
            && let Target::Symbol(i) = target
            && let Definition::Atom(id) = named[i as usize].definition
        {
            target = Target::Atom(id as u32);
        }
        let mut addend = r.addend;
        if r.kind == RelocationKind::Unsigned
            && minus.is_none()
            && r.addend != 0
            && (r.symbol.starts_with('L') || section.flags & 0x02000000 != 0)
            && let Target::Atom(id) = target
        {
            let a = atoms[id as usize];
            let offset = (a.offset as u64).wrapping_add(r.addend as u64);
            if offset > section_sizes[a.section as usize] as u64 {
                return Err(Error::new(0, "local pointer target outside its section"));
            }
            target = Target::Atom(
                u32::try_from(atoms.len()).map_err(|_| Error::new(0, "too many native atoms"))?,
            );
            atoms.push(Atom {
                section: a.section,
                offset: offset as u32,
                boundary: false,
                no_dead_strip: false,
            });
            addend = 0;
        }
        let kind = if let Some(minus) = minus {
            if r.kind != RelocationKind::Unsigned {
                return Err(Error::new(0, "invalid native difference"));
            }
            let minus = targets[minus];
            match r.size {
                4 => K::Difference32 { minus },
                8 => K::Difference64 { minus },
                _ => unreachable!(),
            }
        } else {
            match r.kind {
                RelocationKind::Unsigned if r.size == 8 => K::Pointer64,
                RelocationKind::Branch26 => K::Branch26,
                RelocationKind::Page21 => K::Page21,
                RelocationKind::PageOff12 => K::PageOff12,
                RelocationKind::GotPage21 => K::GotPage21,
                RelocationKind::GotPageOff12 => K::GotPageOff12,
                RelocationKind::PointerToGot => K::PointerToGot,
                RelocationKind::TlvpPage21 => K::TlvpPage21,
                RelocationKind::TlvpPageOff12 => K::TlvpPageOff12,
                _ => return Err(Error::new(0, "invalid native pointer width")),
            }
        };
        if kind.width() != r.size as usize {
            return Err(Error::new(0, "invalid native fixup width"));
        }
        section.fixups.push(Fixup {
            offset: r.offset,
            target,
            addend,
            kind,
        });
    }
    for (section_id, section) in output.iter_mut().enumerate() {
        section.fixups.sort_unstable_by_key(|f| f.offset);
        let mut previous_end = 0;
        for f in &section.fixups {
            if (f.offset as usize) < previous_end {
                return Err(Error::new(0, "overlapping native fixups"));
            }
            previous_end = f.offset as usize + f.kind.width();
        }
        let mut boundaries: Vec<_> = atoms
            .iter()
            .filter(|a| a.boundary && a.section as usize == section_id)
            .map(|a| a.offset)
            .collect();
        boundaries.sort_unstable();
        // A pair must stay within one independently movable source atom.
        let mut combined = Vec::with_capacity(section.fixups.len());
        let mut i = 0;
        while i < section.fixups.len() {
            let mut f = section.fixups[i];
            if let Some(low) = section.fixups.get(i + 1)
                && low.offset == f.offset + 4
                && low.target == f.target
                && low.addend == f.addend
                && boundaries.binary_search(&low.offset).is_err()
            {
                f.kind = match (f.kind, low.kind) {
                    (K::Page21, K::PageOff12) => K::PagePair {
                        low_offset: low.offset,
                    },
                    (K::GotPage21, K::GotPageOff12) => K::GotPagePair {
                        low_offset: low.offset,
                    },
                    (K::TlvpPage21, K::TlvpPageOff12) => K::TlvpPagePair {
                        low_offset: low.offset,
                    },
                    _ => f.kind,
                };
                if f.kind.low_offset().is_some() {
                    i += 1;
                }
            }
            combined.push(f);
            i += 1;
        }
        section.fixups = combined;
    }
    let unwind = extract_unwind(&mut output, &mut atoms)?;
    Ok(Object {
        sections: output,
        atoms,
        unwind,
        symbols: named,
        minimum: build_version.minimum,
        subsections: subsections_via_symbols,
    })
}

fn extract_unwind(
    sections: &mut Vec<Section>,
    atoms: &mut [Atom],
) -> Result<Vec<keel_object::Unwind>, Error> {
    let Some(id) = sections
        .iter()
        .position(|s| s.segment == "__LD" && s.name == "__compact_unwind")
    else {
        return Ok(Vec::new());
    };
    let s = &sections[id];
    if atoms.iter().any(|a| a.section as usize == id)
        || s.flags & 0x10000000 != 0
        || !s.data.len().is_multiple_of(32)
        || s.fixups
            .iter()
            .any(|f| f.kind != K::Pointer64 || !matches!(f.offset % 32, 0 | 16 | 24))
    {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(s.data.len() / 32);
    let mut fixups = s.fixups.iter().peekable();
    for offset in (0..s.data.len()).step_by(32) {
        let mut refs = [None; 3];
        while let Some(f) = fixups.peek().filter(|f| (f.offset as usize) < offset + 32) {
            let slot = match f.offset % 32 {
                0 => 0,
                16 => 1,
                24 => 2,
                _ => unreachable!(),
            };
            refs[slot] = Some((f.target, f.addend));
            fixups.next();
        }
        let Some((Target::Atom(owner), addend)) = refs[0] else {
            return Ok(Vec::new());
        };
        if refs[1].is_some_and(|(_, a)| a != 0)
            || refs[2].is_some_and(|(_, a)| a != 0)
            || addend != 0
        {
            return Ok(Vec::new());
        }
        let word =
            |at| u32::from_le_bytes(s.data[at..at + 4].try_into().expect("bounded compact word"));
        // Preserve exotic explicit compact payloads on the ordinary path.
        if (refs[1].is_none() && s.data[offset + 16..offset + 24].iter().any(|b| *b != 0))
            || (refs[2].is_none() && s.data[offset + 24..offset + 32].iter().any(|b| *b != 0))
        {
            return Ok(Vec::new());
        }
        result.push(keel_object::Unwind {
            owner,
            length: word(offset + 8),
            encoding: word(offset + 12),
            personality: refs[1].map(|(t, _)| t),
            lsda: refs[2].map(|(t, _)| t),
        });
    }
    sections.remove(id);
    for a in atoms {
        if a.section as usize > id {
            a.section -= 1;
        }
    }
    Ok(result)
}
