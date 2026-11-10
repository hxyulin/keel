//! Opt-in, size-preserving x86-64 GOT-load relaxation before GOT planning.
use super::{Globals, Section, Unit, dynamic, selected};
use crate::Result;
use object::{SymbolSection, elf};
use std::collections::{BTreeMap, BTreeSet};

/// Replace canonical RIP-relative 64-bit MOV loads with LEA for definitions
/// inside this executable. Its default layout fits in 512 MiB, so any two
/// section/common addresses are within PC32 reach. Absolute, undefined and
/// imported symbols keep GOT addressing, including undefined weak symbols.
pub(super) fn got_loads(
    sections: &mut [Section],
    units: &mut [Unit],
    globals: &Globals,
    imports: &BTreeMap<String, dynamic::Import>,
) -> Result<()> {
    let selected = selected::Index::new(units, globals, imports);
    let mut edits = Vec::new();
    for (ui, unit) in units.iter().enumerate() {
        let mut fields = BTreeMap::<usize, Vec<(u64, u64, usize)>>::new();
        for (ri, r) in unit.relocs.iter().enumerate() {
            fields.entry(r.section).or_default().push((
                r.offset,
                r.offset + super::reloc::x86_64::width(r.kind)?,
                ri,
            ));
        }
        for ranges in fields.values_mut() {
            ranges.sort_unstable();
        }
        for (ri, r) in unit.relocs.iter().enumerate() {
            if r.kind != elf::R_X86_64_REX_GOTPCRELX || r.addend != -4 {
                continue;
            }
            let selection = selected.get(ui, r.symbol)?;
            if selection.import.is_some() {
                continue;
            }
            let defined = match selection.definition.section {
                SymbolSection::Common => true,
                SymbolSection::Section(index) => units[selection.key.0]
                    .sections
                    .get(&index.0)
                    .is_some_and(|&index| selection.definition.value <= sections[index].size),
                _ => false,
            };
            let section = &sections[r.section];
            if !defined || section.flags & u64::from(elf::SHF_EXECINSTR) == 0 {
                continue;
            }
            let Some(start) = r.offset.checked_sub(3) else {
                continue;
            };
            let start = start as usize;
            let data = &section.data;
            // REX.W, MOV r64,[RIP+disp32]. REX.R may select r8..r15;
            // REX.X/B and extra legacy prefixes are deliberately excluded.
            if !matches!(data[start], 0x48 | 0x4c)
                || data[start + 1] != 0x8b
                || data[start + 2] & 0xc7 != 0x05
                || start.checked_sub(1).is_some_and(|i| prefix(data[i]))
            {
                continue;
            }
            let ranges = &fields[&r.section];
            let first =
                ranges.partition_point(|&(begin, _, _)| begin < (start as u64).saturating_sub(7));
            if ranges[first..]
                .iter()
                .take_while(|&&(begin, _, _)| begin < r.offset + 4)
                .any(|&(_, end, index)| index != ri && end > start as u64)
            {
                continue;
            }
            edits.push((ui, ri, r.section, start + 1));
        }
    }
    for (ui, ri, section, opcode) in edits {
        sections[section].data.to_mut()[opcode] = 0x8d;
        units[ui].relocs[ri].kind = elf::R_X86_64_PC32;
    }
    Ok(())
}

fn prefix(byte: u8) -> bool {
    matches!(byte, 0x26 | 0x2e | 0x36 | 0x3e | 0x40..=0x4f | 0x64..=0x67 | 0xf0 | 0xf2 | 0xf3)
}

/// Relax the ABI's padded GD sequence without changing section sizes. Other
/// instruction forms keep their resolver call and ordinary dynamic TLS records.
pub(super) fn tls_general(
    sections: &mut [Section],
    units: &mut [Unit],
    globals: &Globals,
) -> Result<()> {
    let imports = BTreeMap::new();
    let selected = selected::Index::new(units, globals, &imports);
    let mut edits = Vec::new();
    for (ui, unit) in units.iter().enumerate() {
        for (ri, r) in unit.relocs.iter().enumerate() {
            if r.kind != elf::R_X86_64_TLSGD || r.addend != -4 {
                continue;
            }
            let Some(start) = r.offset.checked_sub(4) else {
                continue;
            };
            let section = &sections[r.section];
            let Some(code) = section.data.get(start as usize..start as usize + 16) else {
                continue;
            };
            if section.flags & u64::from(elf::SHF_EXECINSTR) == 0
                || code[..4] != [0x66, 0x48, 0x8d, 0x3d]
                || code[8..12] != [0x66, 0x66, 0x48, 0xe8]
            {
                continue;
            }
            let Some((ci, call)) = unit.relocs.iter().enumerate().find(|(_, call)| {
                call.section == r.section
                    && call.offset == start + 12
                    && matches!(call.kind, elf::R_X86_64_PLT32 | elf::R_X86_64_PC32)
                    && call.addend == -4
                    && unit.symbols.get(&call.symbol).is_some_and(|s| {
                        s.global()
                            && s.name == "__tls_get_addr"
                            && s.section == SymbolSection::Undefined
                    })
            }) else {
                continue;
            };
            // Reject other relocations touching either instruction.
            if unit.relocs.iter().enumerate().any(|(i, other)| {
                i != ri
                    && i != ci
                    && other.section == r.section
                    && other.offset < start + 16
                    && other.offset.saturating_add(8) > start
            }) {
                continue;
            }
            let definition = selected.get(ui, r.symbol)?.definition;
            if definition.info & 15 != elf::STT_TLS {
                continue;
            }
            let local = matches!(definition.section, SymbolSection::Section(_));
            if !local && definition.section != SymbolSection::Undefined {
                continue;
            }
            edits.push((ui, ri, ci, call.symbol, r.section, start as usize, local));
        }
    }
    let mut consumed = vec![BTreeSet::new(); units.len()];
    for (ui, ri, ci, resolver, section, start, local) in edits {
        // mov %fs:0,%rax; lea tpoff(%rax),%rax (or add a TP-offset GOT load).
        sections[section].data.to_mut()[start..start + 12].copy_from_slice(&[
            0x64,
            0x48,
            0x8b,
            0x04,
            0x25,
            0,
            0,
            0,
            0,
            0x48,
            if local { 0x8d } else { 0x03 },
            if local { 0x80 } else { 0x05 },
        ]);
        let unit = &mut units[ui];
        let r = &mut unit.relocs[ri];
        r.offset = start as u64 + 12;
        r.kind = if local {
            elf::R_X86_64_TPOFF32
        } else {
            elf::R_X86_64_GOTTPOFF
        };
        r.addend = if local { 0 } else { -4 };
        // Remove consumed calls after all stable-index edits.
        consumed[ui].insert(ci);
        if !unit
            .relocs
            .iter()
            .enumerate()
            .any(|(i, r)| !consumed[ui].contains(&i) && r.symbol == resolver)
        {
            unit.symbols.remove(&resolver);
        }
    }
    for (unit, consumed) in units.iter_mut().zip(consumed) {
        let mut index = 0;
        unit.relocs.retain(|_| {
            let keep = !consumed.contains(&index);
            index += 1;
            keep
        });
    }
    Ok(())
}
