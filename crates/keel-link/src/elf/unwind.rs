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
pub(super) fn emit(
    bytes: &mut [u8],
    sections: &[Section],
    frames: &BTreeMap<usize, Frames>,
    header: usize,
) -> Result<()> {
    let mut entries = Vec::new();
    for (&index, records) in frames {
        let section = &sections[index];
        let address = BASE + section.offset;
        let data = &bytes[section.offset as usize..(section.offset + section.size) as usize];
        for f in &records.frames {
            let pc = f.pc.value(data, address)?;
            let range = f.range.value(data, address)?;
            let end = pc.checked_add(range).ok_or("unwind FDE range overflow")?;
            if !sections.iter().any(|s| {
                s.flags & 4 != 0
                    && pc >= BASE + s.offset
                    && pc < BASE + s.offset + s.size
                    && end <= BASE + s.offset + s.size
            }) {
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
