//! Single-producer AMD64 SFrame v3, using the packed binutils-2_47 layout.
//! This preserves rows and index identities; merging and reordering are unsupported.
use super::{BASE, LIMIT, Section, Unit, end};
use crate::Result;
use object::SymbolSection;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const SHT_GNU_SFRAME: u32 = 0x6fff_fff4;
pub(super) const PT_GNU_SFRAME: u32 = 0x6474_e554;
const HEADER: usize = 28;
const INDEX: usize = 16;

struct Entry {
    field: usize,
    size: u32,
}
pub(super) struct Table {
    entries: Vec<Entry>,
}

fn word(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

/// Validate the name/type pairing even for nonallocated inputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_section(
    name: &str,
    kind: u32,
    flags: u64,
    alignment: u64,
    entsize: u64,
    link: u32,
    info: u32,
) -> Result<bool> {
    if name != ".sframe" && kind != SHT_GNU_SFRAME {
        return Ok(false);
    }
    if name != ".sframe"
        || kind != SHT_GNU_SFRAME
        || flags != 2
        || alignment < 8
        || !alignment.is_power_of_two()
        || alignment > 1024 * 1024
        || entsize != 0
        || link != 0
        || info != 0
    {
        return Err(".sframe requires SHT_GNU_SFRAME, SHF_ALLOC only, alignment 8..1MiB (power of two), and zero entsize/link/info".into());
    }
    Ok(true)
}

impl Table {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER {
            return Err(
                ".sframe: truncated 28-byte v3 header (zero-byte sections are invalid)".into(),
            );
        }
        if data[..2] != [0xe2, 0xde] || data[2] != 3 {
            return Err(".sframe: expected little-endian SFrame version 3 magic/header".into());
        }
        if data[3] & !5 != 0 || data[3] & 4 == 0 {
            return Err(
                ".sframe: unsupported v3 flags; require PCREL, allow only PCREL|SORTED".into(),
            );
        }
        if data[4..8] != [3, 0, 0xf8, 0] {
            return Err(
                ".sframe: require AMD64 LE, fixed FP=0, RA=-8, and no auxiliary header".into(),
            );
        }
        let count = u64::from(word(data, 8));
        let rows = u64::from(word(data, 12));
        let fre_len = u64::from(word(data, 16));
        let index_len = count
            .checked_mul(INDEX as u64)
            .ok_or(".sframe: index size overflow")?;
        let fre_start = end(HEADER as u64, index_len).map_err(|e| format!(".sframe: {e}"))?;
        let size = end(fre_start, fre_len).map_err(|e| format!(".sframe: {e}"))?;
        if word(data, 20) != 0
            || u64::from(word(data, 24)) != index_len
            || size != data.len() as u64
            || size > LIMIT
        {
            return Err(".sframe: require packed index/FRE offsets and exact section size (no gaps or trailing data)".into());
        }
        // At least five attribute bytes per function and two bytes per row.
        // Bound counts before allocating or iterating over untrusted values.
        if rows < count
            || count * 5 + rows * 2 > fre_len
            || (count == 0 && (rows != 0 || fre_len != 0))
        {
            return Err(".sframe: inconsistent or excessive function/row counts".into());
        }
        let mut entries = Vec::new();
        let mut blocks = Vec::new();
        for i in 0..count as usize {
            let field = HEADER + INDEX * i;
            let size = word(data, field + 8);
            let block = u64::from(word(data, field + 12));
            if size == 0 || block >= fre_len {
                return Err(
                    ".sframe: positive function size and in-bounds FRE block required".into(),
                );
            }
            entries.push(Entry { field, size });
            blocks.push((block as usize, i));
        }
        // FRE order need not follow index order. Sharing, overlaps, and gaps
        // are rejected by requiring each parsed block to consume its own span.
        blocks.sort_unstable();
        if blocks.first().is_some_and(|&(off, _)| off != 0) {
            return Err(".sframe: gap before first FRE block".into());
        }
        let fre = &data[fre_start as usize..];
        let mut total_rows = 0u64;
        for (i, &(start, entry)) in blocks.iter().enumerate() {
            let stop = blocks.get(i + 1).map_or(fre.len(), |b| b.0);
            let block = fre
                .get(start..stop)
                .ok_or(".sframe: invalid FRE block bounds")?;
            if block.len() < 5 {
                return Err(".sframe: shared, overlapping, or truncated FRE attributes".into());
            }
            let count = u16::from_le_bytes(block[..2].try_into().unwrap()) as usize;
            let info = block[2];
            if info & 0xf0 != 0 || info & 15 > 2 || block[3] != 0 || block[4] != 0 {
                return Err(".sframe: unsupported attributes (require DEFAULT, incrementing PC, nonsignal; FLEX/PCMASK/PAUTH/reserved bits unsupported)".into());
            }
            let pc_width = 1usize << info;
            if count == 0 || count > (block.len() - 5) / (pc_width + 1) {
                return Err(".sframe: invalid or truncated per-function row count".into());
            }
            let mut at = 5;
            let mut previous = None;
            for _ in 0..count {
                let row = block
                    .get(at..at + pc_width + 1)
                    .ok_or(".sframe: truncated FRE row within its block")?;
                let mut pc = [0; 4];
                pc[..pc_width].copy_from_slice(&row[..pc_width]);
                let pc = u32::from_le_bytes(pc);
                if previous.map_or(pc != 0, |p| pc <= p) || pc >= entries[entry].size {
                    return Err(".sframe: row PCs must start at zero, increase strictly, and lie inside the function".into());
                }
                previous = Some(pc);
                let info = row[pc_width];
                let words = usize::from((info >> 1) & 15);
                let width = (info >> 5) & 3;
                if info & 0x80 != 0 || words > 2 || width == 3 {
                    return Err(
                        ".sframe: invalid DEFAULT AMD64 row data count/width or RA mangling".into(),
                    );
                }
                // N=0 explicitly means RA undefined, including crt1's PC4 row.
                at += pc_width + 1 + (words << width);
                if at > block.len() {
                    return Err(".sframe: truncated FRE data words within their block".into());
                }
            }
            if at != block.len() {
                return Err(".sframe: FRE block has gaps, overlaps, or trailing data".into());
            }
            total_rows += count as u64;
        }
        if total_rows != rows {
            return Err(".sframe: total FRE count disagrees with function attributes".into());
        }
        // A complete 28-byte header with all counts/lengths zero is retained.
        Ok(Self { entries })
    }

    pub fn relocation(&self, offset: u64, kind: u32, seen: &mut BTreeSet<u64>) -> Result<()> {
        if kind != object::elf::R_X86_64_PC64
            || offset < HEADER as u64
            || !(offset - HEADER as u64).is_multiple_of(INDEX as u64)
            || (offset - HEADER as u64) / INDEX as u64 >= self.entries.len() as u64
        {
            return Err(
                ".sframe: require PC64 relocation only at an index start-address field".into(),
            );
        }
        if !seen.insert(offset) {
            return Err(".sframe: duplicate index start-address relocation".into());
        }
        Ok(())
    }

    pub fn require_relocations(&self, seen: &BTreeSet<u64>) -> Result<()> {
        if seen.len() != self.entries.len() {
            return Err(".sframe: missing required PC64 index start-address relocation".into());
        }
        Ok(())
    }

    /// Resolve original symbol identity before undefined symbols can become imports.
    pub fn targets(
        &self,
        section: usize,
        units: &[Unit],
        sections: &[Section],
        globals: &BTreeMap<String, (usize, usize)>,
    ) -> Result<Vec<(usize, u64)>> {
        let mut targets = vec![(0, 0); self.entries.len()];
        for (ui, unit) in units.iter().enumerate() {
            for r in unit.relocs.iter().filter(|r| r.section == section) {
                let symbol = unit
                    .symbols
                    .get(&r.symbol)
                    .ok_or(".sframe: invalid target symbol")?;
                let (u, s) = if symbol.global() {
                    globals.get(&symbol.name).copied().unwrap_or((ui, r.symbol))
                } else {
                    (ui, r.symbol)
                };
                let symbol = &units[u].symbols[&s];
                if symbol.section == SymbolSection::Undefined {
                    return Err(format!(
                        ".sframe: undefined or imported code target `{}` is unsupported",
                        symbol.name
                    ));
                }
                let out = symbol
                    .section
                    .index()
                    .and_then(|i| units[u].sections.get(&i.0))
                    .ok_or(
                        ".sframe: target must be retained native executable input section code",
                    )?;
                if sections[*out].flags & 4 == 0 {
                    return Err(".sframe: non-code relocation target".into());
                }
                let i = (r.offset as usize - HEADER) / INDEX;
                let pc = i128::from(symbol.value) + i128::from(r.addend);
                let finish = pc + i128::from(self.entries[i].size);
                if pc < 0 || finish > i128::from(sections[*out].size) {
                    return Err(
                        ".sframe: target code range is outside its executable input section".into(),
                    );
                }
                targets[i] = (*out, pc as u64);
            }
        }
        Ok(targets)
    }

    /// Independently decode relocated, field-relative PCs; never sort raw deltas.
    pub fn finish(
        &self,
        bytes: &mut [u8],
        section: usize,
        sections: &[Section],
        targets: &[(usize, u64)],
    ) -> Result<()> {
        let section = &sections[section];
        let data = &mut bytes[section.offset as usize..(section.offset + section.size) as usize];
        let mut previous_end = None;
        for (entry, &(target, relative)) in self.entries.iter().zip(targets) {
            let p = BASE + section.offset + entry.field as u64;
            let d = i64::from_le_bytes(data[entry.field..entry.field + 8].try_into().unwrap());
            let pc = u64::try_from(i128::from(p) + i128::from(d))
                .map_err(|_| ".sframe: relocated PC overflow")?;
            let finish = pc
                .checked_add(u64::from(entry.size))
                .ok_or(".sframe: relocated code range overflow")?;
            let code = &sections[target];
            let start = BASE + code.offset;
            if code.flags & 4 == 0
                || pc != start + relative
                || pc < start
                || finish > start + code.size
            {
                return Err(
                    ".sframe: relocated PC/range disagrees with native executable input code"
                        .into(),
                );
            }
            if previous_end.is_some_and(|end| pc < end) {
                return Err(".sframe: resolved PCs are unordered or overlapping; single-input reordering is unsupported".into());
            }
            previous_end = Some(finish);
        }
        data[3] |= 1; // Size-neutral, only after proving resolved order and ranges.
        Ok(())
    }
}
