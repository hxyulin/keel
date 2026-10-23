//! Bounded .eh_frame record parsing and a runtime-discoverable search index.
//! CFI instructions remain opaque; this is not an unwind instruction validator.
use super::{BASE, Section, Unit, align, end, put32};
use crate::Result;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Pointer {
    offset: usize,
    encoding: u8,
}
impl Pointer {
    fn width(self) -> Result<usize> {
        match self.encoding & 15 {
            0 | 4 | 12 => Ok(8),
            2 | 10 => Ok(2),
            3 | 11 => Ok(4),
            _ => Err(format!("unsupported encoded pointer {:#x}", self.encoding)),
        }
    }
    fn value(self, data: &[u8], address: u64) -> Result<u64> {
        let width = self.width()?;
        let mut bytes = [0; 8];
        bytes[..width].copy_from_slice(&data[self.offset..self.offset + width]);
        let raw = u64::from_le_bytes(bytes);
        let value = if self.encoding & 15 >= 10 {
            i128::from(((raw << (64 - width * 8)) as i64) >> (64 - width * 8))
        } else {
            i128::from(raw)
        };
        let base = if self.encoding & 0x70 == 0x10 {
            address + self.offset as u64
        } else {
            0
        };
        u64::try_from(i128::from(base) + value).map_err(|_| "unwind pointer overflow".into())
    }
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).ok_or("unwind record overflow")?;
        let bytes = self
            .data
            .get(self.at..end)
            .ok_or("truncated unwind record")?;
        self.at = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn leb(&mut self, signed: bool) -> Result<u64> {
        let mut value = 0;
        for shift in (0..70).step_by(7) {
            let b = self.byte()?;
            if shift == 63
                && if signed {
                    !matches!(b, 0 | 0x7f)
                } else {
                    b > 1
                }
            {
                return Err("unwind LEB128 overflow".into());
            }
            value |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("unwind LEB128 overflow".into())
    }
    fn augmentation(&mut self) -> Result<Self> {
        let length = usize::try_from(self.leb(false)?).map_err(|_| "unwind length overflow")?;
        let start = self.at;
        self.take(length)?;
        Ok(Self {
            data: &self.data[..self.at],
            at: start,
        })
    }
    fn pointer(&mut self, encoding: u8, indirect: bool) -> Result<Pointer> {
        if !matches!(encoding & 0x70, 0 | 0x10) || (!indirect && encoding & 0x80 != 0) {
            return Err(format!("unsupported encoded pointer {encoding:#x}"));
        }
        let pointer = Pointer {
            offset: self.at,
            encoding,
        };
        self.take(pointer.width()?)?;
        Ok(pointer)
    }
}
struct Cie {
    encoding: u8,
    lsda: u8,
    augmented: bool,
}
struct Frame {
    record: usize,
    pc: Pointer,
    range: Pointer,
}
pub(super) struct Frames {
    frames: Vec<Frame>,
    fields: BTreeMap<u64, Pointer>,
    last_record: Option<usize>,
}
impl Frames {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut result = Self {
            frames: Vec::new(),
            fields: BTreeMap::new(),
            last_record: None,
        };
        let mut cies = BTreeMap::new();
        let mut at = 0;
        while at < data.len() {
            let record = at;
            let mut r = Reader { data, at };
            let length = r.word()? as usize;
            if length == 0 {
                if data[r.at..].iter().any(|b| *b != 0) {
                    return Err("nonzero data after unwind terminator".into());
                }
                break;
            }
            if length == 0xffff_ffff {
                return Err("DWARF64 unwind records are unsupported".into());
            }
            result.last_record = Some(record);
            at =
                r.at.checked_add(length)
                    .filter(|end| *end <= data.len())
                    .ok_or("truncated unwind record")?;
            if !at.is_multiple_of(4) {
                return Err("unaligned unwind record".into());
            }
            r.data = &data[..at];
            let id = r.word()? as usize;
            if id == 0 {
                let version = r.byte()?;
                if !matches!(version, 1 | 3 | 4) {
                    return Err(format!("unsupported unwind CIE version {version}"));
                }
                let start = r.at;
                while r.byte()? != 0 {}
                let augmentation = &data[start..r.at - 1];
                if version == 4 && (r.byte()? != 8 || r.byte()? != 0) {
                    return Err("unsupported unwind address/segment size".into());
                }
                if r.leb(false)? == 0 {
                    return Err("zero unwind code alignment".into());
                }
                r.leb(true)?;
                if version == 1 {
                    r.byte()?;
                } else {
                    r.leb(false)?;
                }
                let mut cie = Cie {
                    encoding: 0,
                    lsda: 0xff,
                    augmented: !augmentation.is_empty(),
                };
                if cie.augmented {
                    if augmentation[0] != b'z' {
                        return Err("unsupported unwind CIE augmentation".into());
                    }
                    let mut aug = r.augmentation()?;
                    let mut seen = Vec::new();
                    for &ch in &augmentation[1..] {
                        if seen.contains(&ch) {
                            return Err("duplicate unwind CIE augmentation".into());
                        }
                        seen.push(ch);
                        match ch {
                            b'R' => {
                                cie.encoding = aug.byte()?;
                            }
                            b'L' => {
                                cie.lsda = aug.byte()?;
                            }
                            b'P' => {
                                let encoding = aug.byte()?;
                                result.field(aug.pointer(encoding, true)?);
                            }
                            b'S' => {}
                            _ => return Err("unsupported unwind CIE augmentation".into()),
                        }
                    }
                    if aug.at != aug.data.len() {
                        return Err("invalid unwind CIE augmentation length".into());
                    }
                }
                // Validate encodings even when there is no FDE using this CIE.
                let mut probe = Reader {
                    data: &[0; 8],
                    at: 0,
                };
                probe.pointer(cie.encoding, false)?;
                if cie.lsda != 0xff {
                    probe.at = 0;
                    probe.pointer(cie.lsda, true)?;
                }
                cies.insert(record, cie);
            } else {
                let cie = record
                    .checked_add(4)
                    .and_then(|p| p.checked_sub(id))
                    .and_then(|p| cies.get(&p))
                    .ok_or("invalid unwind FDE CIE reference")?;
                let pc = r.pointer(cie.encoding, false)?;
                let range = r.pointer(cie.encoding & 15, false)?;
                result.field(pc);
                if cie.augmented {
                    let mut aug = r.augmentation()?;
                    if cie.lsda != 0xff {
                        result.field(aug.pointer(cie.lsda, true)?);
                    }
                    if aug.at != aug.data.len() {
                        return Err("invalid unwind FDE augmentation length".into());
                    }
                }
                result.frames.push(Frame { record, pc, range });
            }
        }
        Ok(result)
    }
    fn field(&mut self, pointer: Pointer) {
        self.fields.insert(pointer.offset as u64, pointer);
    }
    pub fn relocation(&self, offset: u64, kind: u32) -> Result<()> {
        let field = self
            .fields
            .get(&offset)
            .ok_or("unwind relocation must address an encoded pointer field")?;
        let supported = match (field.encoding & 0x70, field.width()?) {
            (0, 8) => kind == object::elf::R_X86_64_64,
            (0, 4) => matches!(kind, object::elf::R_X86_64_32 | object::elf::R_X86_64_32S),
            (0x10, 4) => kind == object::elf::R_X86_64_PC32,
            (0x10, 8) => kind == object::elf::R_X86_64_PC64,
            _ => false,
        };
        if !supported {
            return Err("unwind relocation disagrees with pointer encoding".into());
        }
        Ok(())
    }
}

/// GDB reads one .eh_frame section rather than the runtime search index.
/// Merge validated contributions before layout, retaining their CIE/FDE offsets
/// within each contribution. Extend the last record with DW_CFA_nop bytes over
/// any terminator/alignment gap so discovery can reach the next contribution.
/// No CFI instruction, pointer field, or intra-contribution CIE reference moves.
pub(super) fn merge(
    sections: &mut Vec<Section>,
    units: &mut [Unit],
    frames: &mut BTreeMap<usize, Frames>,
) -> Result<Vec<usize>> {
    let mut remap: Vec<_> = (0..sections.len()).collect();
    if frames.len() < 2 {
        return Ok(remap);
    }
    let mut merged = Section {
        name: ".eh_frame".into(),
        flags: 2,
        kind: object::elf::SHT_PROGBITS,
        align: 4,
        size: 0,
        bss: false,
        data: Vec::new(),
        offset: 0,
    };
    let mut bases = vec![0; sections.len()];
    let mut last = None;
    for (&index, records) in frames.iter() {
        let section = &sections[index];
        // Leading empty/terminator-only contributions need a record whose CFI
        // can absorb their zero bytes. Keep their symbols/addressable offsets.
        if last.is_none() && records.last_record.is_none() {
            merged
                .data
                .extend_from_slice(&[12, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0x78, 16, 0, 0, 0]);
            last = Some(0);
        }
        // Even an empty contribution becomes the end of the preceding record.
        // Its input alignment may be 1 or 2, but every rewritten record length
        // must still span a multiple of four bytes.
        let base = align(merged.data.len() as u64, section.align.max(4))?;
        merged.data.resize(base as usize, 0);
        if let Some(record) = last {
            let length = u32::try_from(base as usize - record - 4)
                .map_err(|_| "merged unwind record length overflow")?;
            put32(&mut merged.data, record, length);
        }
        bases[index] = base;
        end(base, section.size)?;
        merged.data.extend_from_slice(&section.data);
        if let Some(record) = records.last_record {
            last = Some(base as usize + record);
        }
        merged.align = merged.align.max(section.align);
    }
    merged.size = merged.data.len() as u64;
    let records = Frames::parse(&merged.data)?;
    let first = *frames.first_key_value().unwrap().0;
    let mut output = Vec::with_capacity(sections.len() - frames.len() + 1);
    let mut merged = Some(merged);
    for (index, section) in std::mem::take(sections).into_iter().enumerate() {
        if frames.contains_key(&index) {
            remap[index] = first;
            if index == first {
                output.push(merged.take().unwrap());
            }
        } else {
            remap[index] = output.len();
            output.push(section);
        }
    }
    for unit in units {
        for symbol in unit.symbols.values_mut() {
            if let object::SymbolSection::Section(index) = symbol.section
                && let Some(&output) = unit.sections.get(&index.0)
            {
                symbol.value += bases[output];
            }
        }
        for output in unit.sections.values_mut() {
            *output = remap[*output];
        }
        for reloc in &mut unit.relocs {
            reloc.offset += bases[reloc.section];
            reloc.section = remap[reloc.section];
        }
    }
    *sections = output;
    frames.clear();
    frames.insert(first, records);
    Ok(remap)
}

pub(super) fn prepare(
    sections: &mut Vec<Section>,
    frames: &BTreeMap<usize, Frames>,
) -> Result<Option<usize>> {
    if frames.is_empty() {
        return Ok(None);
    }
    let count: usize = frames.values().map(|f| f.frames.len()).sum();
    let size = end(
        12,
        (count as u64)
            .checked_mul(8)
            .ok_or("too many unwind FDEs")?,
    )?;
    let header = sections.len();
    sections.push(Section {
        name: ".eh_frame_hdr".into(),
        flags: 2,
        kind: 1,
        align: 4,
        size,
        bss: false,
        data: vec![0; size as usize],
        offset: 0,
    });
    Ok(Some(header))
}
/// Final layout gives sections disjoint intervals, but registration grouping
/// can change their address order. Keep boundaries separate: an FDE must fit
/// in one section even when two executable sections are adjacent.
struct ExecutableRanges(Vec<(u64, u64)>);
impl ExecutableRanges {
    fn new(sections: &[Section]) -> Self {
        let mut ranges: Vec<_> = sections
            .iter()
            .filter(|s| s.flags & 4 != 0 && s.size != 0)
            // Layout has already bounded offset + size to the image limit.
            .map(|s| (BASE + s.offset, BASE + s.offset + s.size))
            .collect();
        ranges.sort_unstable();
        Self(ranges)
    }

    fn contains(&self, pc: u64, end: u64) -> bool {
        let i = self.0.partition_point(|&(start, _)| start <= pc);
        i != 0 && pc < self.0[i - 1].1 && end <= self.0[i - 1].1
    }
}

pub(super) fn emit(
    bytes: &mut [u8],
    sections: &[Section],
    frames: &BTreeMap<usize, Frames>,
    header: usize,
) -> Result<()> {
    let executable = ExecutableRanges::new(sections);
    let mut entries = Vec::new();
    for (&index, records) in frames {
        let section = &sections[index];
        let address = BASE + section.offset;
        let data = &bytes[section.offset as usize..(section.offset + section.size) as usize];
        for f in &records.frames {
            let pc = f.pc.value(data, address)?;
            let range = f.range.value(data, address)?;
            let end = pc.checked_add(range).ok_or("unwind FDE range overflow")?;
            if !executable.contains(pc, end) {
                return Err("unwind FDE range is outside executable section data".into());
            }
            entries.push((pc, address + f.record as u64));
        }
    }
    entries.sort_unstable();
    if entries.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("duplicate unwind FDE start address is unsupported".into());
    }
    let section = &sections[header];
    let address = BASE + section.offset;
    let out = &mut bytes[section.offset as usize..(section.offset + section.size) as usize];
    // version 1, pcrel sdata4 eh_frame pointer, udata4 count, datarel sdata4 table.
    out[..4].copy_from_slice(&[1, 0x1b, 3, 0x3b]);
    let relative = |value: u64, base: u64| -> Result<u32> {
        i32::try_from(i128::from(value) - i128::from(base))
            .map(|v| v as u32)
            .map_err(|_| "unwind search index displacement overflow".into())
    };
    let first = frames.first_key_value().unwrap().0;
    put32(
        out,
        4,
        relative(BASE + sections[*first].offset, address + 4)?,
    );
    put32(
        out,
        8,
        u32::try_from(entries.len()).map_err(|_| "too many unwind FDEs")?,
    );
    for (i, (pc, fde)) in entries.into_iter().enumerate() {
        put32(out, 12 + i * 8, relative(pc, address)?);
        put32(out, 16 + i * 8, relative(fde, address)?);
    }
    Ok(())
}

/// Compact only FDEs whose initial-location relocation targets discarded
/// COMDAT code, and their now unused CIEs. This is metadata selection, not GC.
pub(super) struct Filtered {
    pub data: Vec<u8>,
    ranges: Vec<(u64, u64, u64)>,
}
impl Filtered {
    pub fn offset(&self, offset: u64) -> Option<u64> {
        let i = self
            .ranges
            .partition_point(|&(start, _, _)| start <= offset);
        let &(start, end, base) = self.ranges.get(i.checked_sub(1)?)?;
        (offset < end).then(|| base + offset - start)
    }
}
pub(super) fn filter(
    section: &object::read::elf::ElfSection64<'_, '_, object::Endianness>,
    file: &object::read::elf::ElfFile64<'_, object::Endianness>,
    discarded: &std::collections::BTreeSet<usize>,
) -> Result<Option<Filtered>> {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SymbolSection};
    let data = section.data().map_err(|e| e.to_string())?;
    let parsed = Frames::parse(data).map_err(|e| format!(".eh_frame: {e}"))?;
    section.elf_linked_rela().map_err(|e| e.to_string())?;
    let mut relocs = BTreeMap::new();
    for (offset, r) in section.relocations() {
        // R_NONE has no target or pointer semantics. In particular its symbol
        // index must not cause a live FDE (and its CIE) to be discarded.
        if matches!(r.flags(), object::RelocationFlags::Elf { r_type: 0 }) {
            continue;
        }
        if relocs.insert(offset, r).is_some() {
            return Err("duplicate .eh_frame pointer relocation".into());
        }
    }
    let mut dead = std::collections::BTreeSet::new();
    for frame in &parsed.frames {
        if let Some(r) = relocs.get(&(frame.pc.offset as u64)) {
            if let object::RelocationFlags::Elf { r_type } = r.flags() {
                parsed.relocation(frame.pc.offset as u64, r_type)?;
            }
            let RelocationTarget::Symbol(i) = r.target() else {
                continue;
            };
            let symbol = file.symbol_by_index(i).map_err(|e| e.to_string())?;
            if matches!(symbol.section(), SymbolSection::Section(i) if discarded.contains(&i.0)) {
                dead.insert(frame.record);
            }
        }
    }
    if dead.is_empty() {
        return Ok(None);
    }
    Ok(Some(parsed.compact(
        data,
        &dead,
        &relocs.keys().copied().collect(),
    )?))
}
impl Frames {
    /// FDE owner PC fields and all relocation fields in the FDE and its CIE.
    pub(super) fn owners(&self, data: &[u8]) -> Result<Vec<(usize, u64, u64, Vec<u64>)>> {
        self.frames
            .iter()
            .map(|f| {
                let id = u32::from_le_bytes(data[f.record + 4..f.record + 8].try_into().unwrap())
                    as usize;
                let cie = f.record + 4 - id;
                let end = |at: usize| {
                    at + 4 + u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
                };
                let fields = self
                    .fields
                    // A referenced CIE precedes its FDE, so these disjoint
                    // ranges retain the original ascending field order.
                    .range(cie as u64..end(cie) as u64)
                    .chain(self.fields.range(f.record as u64..end(f.record) as u64))
                    .map(|(&offset, _)| offset)
                    .collect();
                Ok((
                    f.record,
                    f.pc.offset as u64,
                    f.range.value(data, 0)?,
                    fields,
                ))
            })
            .collect()
    }
    pub(super) fn compact(
        &self,
        data: &[u8],
        dead: &std::collections::BTreeSet<usize>,
        relocated: &std::collections::BTreeSet<u64>,
    ) -> Result<Filtered> {
        // CIE offsets come from validated FDEs, not a potentially sparse input
        // index. Store only referenced CIEs and avoid repeated tree insertion
        // for the common case of many adjacent FDEs sharing one CIE.
        let mut used_cies = Vec::new();
        for frame in &self.frames {
            if !dead.contains(&frame.record) {
                let id = u32::from_le_bytes(
                    data[frame.record + 4..frame.record + 8].try_into().unwrap(),
                ) as usize;
                let cie = frame.record + 4 - id;
                if used_cies.last() != Some(&cie) {
                    used_cies.push(cie);
                }
            }
        }
        used_cies.sort_unstable();
        used_cies.dedup();
        let mut filtered = Filtered {
            data: Vec::new(),
            ranges: Vec::new(),
        };
        let mut at = 0;
        while at + 4 <= data.len() {
            let start = at;
            let length = u32::from_le_bytes(data[start..start + 4].try_into().unwrap()) as usize;
            if length == 0 {
                break;
            }
            let end = start + 4 + length;
            let id = u32::from_le_bytes(data[start + 4..start + 8].try_into().unwrap()) as usize;
            at = end;
            if (id == 0 && used_cies.binary_search(&start).is_err()) || dead.contains(&start) {
                continue;
            }
            let base = filtered.data.len();
            // Kept CIEs already appear in the sorted output ranges. Reuse that
            // mapping instead of building a second record-to-base index.
            let cie = if id != 0 {
                filtered.offset((start + 4 - id) as u64)
            } else {
                None
            };
            filtered
                .ranges
                .push((start as u64, end as u64, base as u64));
            filtered.data.extend_from_slice(&data[start..end]);
            if id != 0 {
                put32(
                    &mut filtered.data,
                    base + 4,
                    (base + 4 - cie.unwrap() as usize) as u32,
                );
            }
        }
        // Both fields and kept ranges are ordered. Visit each range once,
        // retaining the original field validation and error order.
        let mut ranges = filtered.ranges.iter().peekable();
        for (&offset, pointer) in &self.fields {
            while ranges.peek().is_some_and(|&&(_, end, _)| end <= offset) {
                ranges.next();
            }
            if ranges.peek().is_some_and(|&&(start, _, _)| start <= offset)
                && !relocated.contains(&offset)
                && pointer.encoding & 0x70 != 0
                && data[pointer.offset..pointer.offset + pointer.width()?]
                    .iter()
                    .any(|b| *b != 0)
            {
                return Err(
                    "unrelocated relative pointer in filtered .eh_frame is unsupported".into(),
                );
            }
        }
        Frames::parse(&filtered.data)?;
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(offset: u64, size: u64, flags: u64) -> Section {
        Section {
            name: String::new(),
            flags,
            kind: object::elf::SHT_PROGBITS,
            align: 1,
            size,
            bss: false,
            data: Vec::new(),
            offset,
        }
    }

    #[test]
    fn executable_lookup_matches_section_scan_for_final_layouts() {
        // Deliberately not in address order, as with registration grouping.
        // Empty sections at a live start must not hide its nonempty interval.
        // Non-executable data, alignment gaps and adjacent code stay distinct.
        let sections = [
            section(40, 4, 6),
            section(12, 4, 6),
            section(8, 4, 6),
            section(8, 0, 6),
            section(10, 0, 6),
            section(16, 0, 6),
            section(16, 8, 2),
            section(28, 0, 6),
            section(32, 4, 6),
        ];
        for len in 0..=sections.len() {
            let sections = &sections[..len];
            let lookup = ExecutableRanges::new(sections);
            for offset in 0..=48 {
                let pc = BASE + offset;
                for range in 0..=48 {
                    let end = pc + range;
                    let expected = sections.iter().any(|s| {
                        s.flags & 4 != 0
                            && pc >= BASE + s.offset
                            && pc < BASE + s.offset + s.size
                            && end <= BASE + s.offset + s.size
                    });
                    assert_eq!(
                        lookup.contains(pc, end),
                        expected,
                        "sections={len} offset={offset} range={range}"
                    );
                }
            }
            for pc in [0, BASE - 1, u64::MAX] {
                assert!(!lookup.contains(pc, pc));
            }
        }
    }

    // Frozen pre-optimization algorithm: independent output/error oracle and
    // same-build baseline for the ignored local performance comparison.
    fn compact_reference(
        parsed: &Frames,
        data: &[u8],
        dead: &std::collections::BTreeSet<usize>,
        relocated: &std::collections::BTreeSet<u64>,
    ) -> Result<Filtered> {
        let mut records = Vec::new();
        let mut used_cies = std::collections::BTreeSet::new();
        let mut at = 0;
        while at + 4 <= data.len() {
            let length = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            if length == 0 {
                break;
            }
            let end = at + 4 + length;
            let id = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize;
            if id != 0 && !dead.contains(&at) {
                used_cies.insert(at + 4 - id);
            }
            records.push((at, end, id));
            at = end;
        }
        let mut filtered = Filtered {
            data: Vec::new(),
            ranges: Vec::new(),
        };
        let mut bases = BTreeMap::new();
        for (start, end, id) in records {
            if (id == 0 && !used_cies.contains(&start)) || dead.contains(&start) {
                continue;
            }
            let base = filtered.data.len();
            filtered
                .ranges
                .push((start as u64, end as u64, base as u64));
            bases.insert(start, base);
            filtered.data.extend_from_slice(&data[start..end]);
            if id != 0 {
                let cie = bases[&(start + 4 - id)];
                put32(&mut filtered.data, base + 4, (base + 4 - cie) as u32);
            }
        }
        for (&offset, pointer) in &parsed.fields {
            if filtered.offset(offset).is_some()
                && !relocated.contains(&offset)
                && pointer.encoding & 0x70 != 0
                && data[pointer.offset..pointer.offset + pointer.width()?]
                    .iter()
                    .any(|b| *b != 0)
            {
                return Err(
                    "unrelocated relative pointer in filtered .eh_frame is unsupported".into(),
                );
            }
        }
        Frames::parse(&filtered.data)?;
        Ok(filtered)
    }

    fn compact_fixture(count: usize, cies: usize, relative: bool) -> Vec<u8> {
        fn record(data: &mut Vec<u8>, body: &[u8]) -> usize {
            let start = data.len();
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(body);
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
            let length = (data.len() - start - 4) as u32;
            put32(data, start, length);
            start
        }
        let mut data = Vec::new();
        let encoding = if relative { 0x1b } else { 0x03 };
        let mut starts = Vec::new();
        for _ in 0..cies {
            // Personality and LSDA fields exercise ownership shared by FDEs.
            starts.push(record(
                &mut data,
                &[
                    0, 0, 0, 0, 1, b'z', b'P', b'L', b'R', 0, 1, 120, 16, 7, encoding, 0, 0, 0, 0,
                    encoding, encoding, // Opaque CFI bytes must survive unchanged.
                    0xff, 0x80, 0x12,
                ],
            ));
        }
        for i in 0..count {
            let cie = starts[i % cies];
            let id = (data.len() + 4 - cie) as u32;
            let mut body = id.to_le_bytes().to_vec();
            body.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 0, 0xff, 0x81]);
            record(&mut data, &body);
        }
        data.extend_from_slice(&[0; 12]);
        data
    }

    #[test]
    fn compact_matches_reference_for_shared_cies_fields_and_sparse_dead_records() {
        for relative in [false, true] {
            let original = compact_fixture(8, 4, relative);
            let parsed = Frames::parse(&original).unwrap();
            for mask in 0..256 {
                let mut dead: std::collections::BTreeSet<_> = parsed
                    .frames
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, f)| f.record)
                    .collect();
                // Unrelated sparse indices never determine an allocation size.
                dead.insert(usize::MAX);
                for nonzero in [false, true] {
                    let mut data = original.clone();
                    if nonzero {
                        for pointer in parsed.fields.values() {
                            data[pointer.offset] = 1;
                        }
                    }
                    for relocations in [0, 1, 2] {
                        let relocated = parsed
                            .fields
                            .keys()
                            .enumerate()
                            .filter(|(i, _)| relocations == 2 || (relocations == 1 && i % 2 == 0))
                            .map(|(_, &offset)| offset)
                            .collect();
                        let expected = compact_reference(&parsed, &data, &dead, &relocated);
                        let actual = parsed.compact(&data, &dead, &relocated);
                        match (actual, expected) {
                            (Ok(actual), Ok(expected)) => {
                                assert_eq!(actual.data, expected.data);
                                assert_eq!(actual.ranges, expected.ranges);
                                for offset in 0..=data.len() as u64 + 4 {
                                    assert_eq!(actual.offset(offset), expected.offset(offset));
                                }
                                assert_eq!(actual.offset(u64::MAX), None);
                            }
                            (Err(actual), Err(expected)) => assert_eq!(actual, expected),
                            _ => panic!(
                                "compaction result differs: relative={relative} mask={mask} nonzero={nonzero} relocations={relocations}"
                            ),
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "local compaction microbenchmark; run in release mode with --nocapture"]
    fn compact_local_comparison() {
        use std::{hint::black_box, time::Instant};
        for cies in [1, 64, 4096] {
            let data = compact_fixture(4096, cies, true);
            let parsed = Frames::parse(&data).unwrap();
            let dead = parsed
                .frames
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 4 != 0)
                .map(|(_, f)| f.record)
                .collect();
            let relocated = parsed.fields.keys().copied().collect();
            let expected = compact_reference(&parsed, &data, &dead, &relocated).unwrap();
            let actual = parsed.compact(&data, &dead, &relocated).unwrap();
            assert_eq!(actual.data, expected.data);
            assert_eq!(actual.ranges, expected.ranges);
            let mut samples = [Vec::new(), Vec::new()];
            for round in 0..21 {
                for candidate in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let start = Instant::now();
                    for _ in 0..100 {
                        let output = if candidate == 0 {
                            compact_reference(&parsed, &data, &dead, &relocated)
                        } else {
                            parsed.compact(&data, &dead, &relocated)
                        }
                        .unwrap();
                        black_box(output);
                    }
                    samples[candidate].push(start.elapsed().as_secs_f64() * 10.0);
                }
            }
            for values in &mut samples {
                values.sort_by(f64::total_cmp);
            }
            eprintln!(
                "4096 FDEs {cies} CIEs, ms/compact baseline median={:.4} candidate={:.4}; ranges {:.4}..{:.4} / {:.4}..{:.4}",
                samples[0][10],
                samples[1][10],
                samples[0][0],
                samples[0][20],
                samples[1][0],
                samples[1][20]
            );
        }
    }
}
