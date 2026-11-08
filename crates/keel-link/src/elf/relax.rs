//! Opt-in, size-preserving x86-64 GOT-load relaxation before GOT planning.
use super::{Globals, Section, Unit, dynamic, selected};
use crate::Result;
use object::{SymbolSection, elf};
use std::collections::BTreeMap;

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
        sections[section].data[opcode] = 0x8d;
        units[ui].relocs[ri].kind = elf::R_X86_64_PC32;
    }
    Ok(())
}

fn prefix(byte: u8) -> bool {
    matches!(byte, 0x26 | 0x2e | 0x36 | 0x3e | 0x40..=0x4f | 0x64..=0x67 | 0xf0 | 0xf2 | 0xf3)
}
