//! Single-producer AMD64 SFrame v3, using the packed binutils-2_47 layout.
//! Decode/validate -> owned records -> reconstruction plan -> code binding -> final fixups.
//! GC filters and orders records before layout; non-GC preserves the input order.
use super::{BASE, LIMIT, Section, Unit};
use crate::Result;
use object::SymbolSection;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const SHT_GNU_SFRAME: u32 = 0x6fff_fff4;
pub(super) const PT_GNU_SFRAME: u32 = 0x6474_e554;
const HEADER: usize = 28;
const INDEX: usize = 16;

/// Only constructed after the complete packed layout and all row semantics validate.
#[derive(Debug, PartialEq, Eq)]
struct Header {
    flags: u8,
    rows: u32,
    fre_len: u32,
}

/// Input index identity survives independently of physical FRE block order.
#[derive(Debug, PartialEq, Eq)]
struct Function {
    field: usize,
    size: u32,
    start: [u8; 8],
    fre_offset: u32,
    rows: Vec<Row>,
    // Includes the five attribute bytes and every exact row/data byte.
    block: Vec<u8>,
}
/// Byte span includes the PC, row info, and all signed data words.
#[derive(Debug, PartialEq, Eq)]
struct Row {
    pc: u32,
    bytes: std::ops::Range<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Table {
    header: Header,
    functions: Vec<Function>,
}

/// Both orders contain original function identities. No duplicate ownership.
struct Plan {
    indices: Vec<usize>,
    blocks: Vec<usize>,
}

/// Relocation fields follow selected record identities through GC and ordering.
struct Encoded {
    bytes: Vec<u8>,
    fields: Vec<PlannedField>,
}
#[derive(Debug, PartialEq, Eq)]
struct PlannedField {
    function: usize,
    original: usize,
    output: usize,
}

#[derive(Debug)]
#[allow(dead_code)] // Keep both identities for diagnostics and ownership audits.
struct CodeBinding {
    // The relocation's original identity and selected winner are distinct.
    relocation_unit: usize,
    relocation_symbol: usize,
    addend: i64,
    // Immutable pre-GC contribution identity, even when the winner differs.
    original_code: Option<(usize, i128)>,
    selected_unit: usize,
    selected_symbol: usize,
    section: usize,
    relative: u64,
}

pub(super) struct Bindings {
    functions: Vec<CodeBinding>,
}

impl Bindings {
    pub fn remap(&mut self, remap: &[Option<usize>]) -> Result<()> {
        for binding in &mut self.functions {
            binding.section =
                remap[binding.section].ok_or(".sframe: retained code was discarded")?;
        }
        Ok(())
    }
}

fn layout(count: u64, rows: u64, fre_len: u64) -> Result<(usize, usize)> {
    // Check representability and the linker limit before any output allocation.
    let indices = count
        .checked_mul(INDEX as u64)
        .ok_or(".sframe: index size overflow")?;
    let size = (HEADER as u64)
        .checked_add(indices)
        .and_then(|n| n.checked_add(fre_len))
        .ok_or(".sframe: section size overflow")?;
    if count > u32::MAX as u64
        || rows > u32::MAX as u64
        || fre_len > u32::MAX as u64
        || indices > u32::MAX as u64
        || size > LIMIT
    {
        return Err(".sframe: reconstruction counts/offsets exceed format or linker limits".into());
    }
    Ok((indices as usize, size as usize))
}

// Conservative working-set estimate, including owned rows, planning maps and
// simultaneous input/reconstruction/image bytes. This is a practical admission
// cap, not a claim about the allocator's maximum resident set.
const BUDGET: u64 = 64 * 1024 * 1024;
fn budget(count: u64, rows: u64, size: u64) -> Result<()> {
    let estimate = count
        .checked_mul(1024)
        .and_then(|n| rows.checked_mul(32).and_then(|r| n.checked_add(r)))
        .and_then(|n| size.checked_mul(4).and_then(|b| n.checked_add(b)));
    if estimate.is_none_or(|n| n > BUDGET) {
        return Err(".sframe: reconstruction working-set budget exceeds 64 MiB".into());
    }
    Ok(())
}
fn reserved<T>(count: usize) -> Result<Vec<T>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| ".sframe: allocation failed")?;
    Ok(result)
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
        let (index_len, size) = layout(count, rows, fre_len)?;
        let fre_start = HEADER + index_len;
        if word(data, 20) != 0 || word(data, 24) as usize != index_len || size != data.len() {
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
        budget(count, rows, size as u64)?;
        let mut functions = reserved(count as usize)?;
        let mut blocks = reserved(count as usize)?;
        for i in 0..count as usize {
            let field = HEADER + INDEX * i;
            let size = word(data, field + 8);
            let block = u64::from(word(data, field + 12));
            if size == 0 || block >= fre_len {
                return Err(
                    ".sframe: positive function size and in-bounds FRE block required".into(),
                );
            }
            functions.push(Function {
                field,
                size,
                start: data[field..field + 8].try_into().unwrap(),
                fre_offset: block as u32,
                rows: Vec::new(),
                block: Vec::new(),
            });
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
        // The budget used the header's row total. Bound all block declarations
        // by that total before reserving any owned rows; checking only
        // after decoding lets a lying header understate the working set.
        for (i, &(start, _)) in blocks.iter().enumerate() {
            let stop = blocks.get(i + 1).map_or(fre.len(), |b| b.0);
            let block = fre
                .get(start..stop)
                .filter(|b| b.len() >= 5)
                .ok_or(".sframe: shared, overlapping, or truncated FRE attributes")?;
            total_rows += u64::from(u16::from_le_bytes(block[..2].try_into().unwrap()));
        }
        if total_rows > rows {
            return Err(".sframe: total FRE count disagrees with function attributes".into());
        }
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
            let mut owned_rows = reserved(count)?;
            for _ in 0..count {
                let row_start = at;
                let row = block
                    .get(at..at + pc_width + 1)
                    .ok_or(".sframe: truncated FRE row within its block")?;
                let mut pc = [0; 4];
                pc[..pc_width].copy_from_slice(&row[..pc_width]);
                let pc = u32::from_le_bytes(pc);
                if previous.map_or(pc != 0, |p| pc <= p) || pc >= functions[entry].size {
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
                owned_rows.push(Row {
                    pc,
                    bytes: row_start..at,
                });
            }
            if at != block.len() {
                return Err(".sframe: FRE block has gaps, overlaps, or trailing data".into());
            }
            functions[entry].rows = owned_rows;
            let mut owned_block = reserved(block.len())?;
            owned_block.extend_from_slice(block);
            functions[entry].block = owned_block;
        }
        if total_rows != rows {
            return Err(".sframe: total FRE count disagrees with function attributes".into());
        }
        // A complete 28-byte header with all counts/lengths zero is retained.
        Ok(Self {
            header: Header {
                flags: data[3],
                rows: rows as u32,
                fre_len: fre_len as u32,
            },
            functions,
        })
    }

    fn identity_plan(&self) -> Plan {
        let indices: Vec<_> = (0..self.functions.len()).collect();
        let mut blocks = indices.clone();
        blocks.sort_unstable_by_key(|&i| self.functions[i].fre_offset);
        Plan { indices, blocks }
    }

    /// Encode validated owned records. Index and FRE order are independent;
    /// each selected identity must own exactly one block. Start fields are still
    /// input bytes: a changed plan requires remapping relocations before linking.
    fn encode(&self, plan: &Plan) -> Result<Encoded> {
        if plan.indices.len() > self.functions.len() || plan.blocks.len() != plan.indices.len() {
            return Err(".sframe: invalid reconstruction ownership count".into());
        }
        let mut selected = BTreeSet::new();
        let mut rows = 0u64;
        let mut fre_len = 0u64;
        for &i in &plan.indices {
            let function = self
                .functions
                .get(i)
                .ok_or(".sframe: invalid reconstruction identity")?;
            if !selected.insert(i) {
                return Err(".sframe: duplicate reconstruction index identity".into());
            }
            rows = rows
                .checked_add(function.rows.len() as u64)
                .ok_or(".sframe: row count overflow")?;
            fre_len = fre_len
                .checked_add(function.block.len() as u64)
                .ok_or(".sframe: FRE length overflow")?;
        }
        let (index_len, size) = layout(plan.indices.len() as u64, rows, fre_len)?;
        let mut offsets = BTreeMap::new();
        let mut offset = 0u32;
        for &i in &plan.blocks {
            if !selected.contains(&i) || offsets.insert(i, offset).is_some() {
                return Err(
                    ".sframe: duplicate or unselected reconstruction block identity".into(),
                );
            }
            offset = offset
                .checked_add(
                    u32::try_from(self.functions[i].block.len())
                        .map_err(|_| ".sframe: FRE length overflow")?,
                )
                .ok_or(".sframe: FRE offset overflow")?;
        }
        budget(plan.indices.len() as u64, rows, size as u64)?;
        let mut data = reserved(size)?;
        // SORTED cannot be asserted for a changed index sequence until fixups.
        let flags = if plan.indices.iter().copied().eq(0..self.functions.len()) {
            self.header.flags
        } else {
            self.header.flags & !1
        };
        data.extend_from_slice(&[0xe2, 0xde, 3, flags, 3, 0, 0xf8, 0]);
        for n in [
            plan.indices.len() as u32,
            rows as u32,
            fre_len as u32,
            0,
            index_len as u32,
        ] {
            data.extend_from_slice(&n.to_le_bytes());
        }
        let mut fields = reserved(plan.indices.len())?;
        for &i in &plan.indices {
            let function = &self.functions[i];
            fields.push(PlannedField {
                function: i,
                original: function.field,
                output: data.len(),
            });
            data.extend_from_slice(&function.start);
            data.extend_from_slice(&function.size.to_le_bytes());
            data.extend_from_slice(&offsets[&i].to_le_bytes());
        }
        for &i in &plan.blocks {
            data.extend_from_slice(&self.functions[i].block);
        }
        debug_assert_eq!(data.len(), size);
        Ok(Encoded {
            bytes: data,
            fields,
        })
    }

    pub fn rebuild(&self) -> Result<Vec<u8>> {
        let encoded = self.encode(&self.identity_plan())?;
        if encoded
            .fields
            .iter()
            .any(|field| field.original != field.output)
        {
            return Err(".sframe: identity reconstruction moved a relocation field".into());
        }
        Ok(encoded.bytes)
    }

    pub fn relocation(&self, offset: u64, kind: u32, seen: &mut BTreeSet<u64>) -> Result<()> {
        if kind != object::elf::R_X86_64_PC64
            || offset < HEADER as u64
            || !(offset - HEADER as u64).is_multiple_of(INDEX as u64)
            || (offset - HEADER as u64) / INDEX as u64 >= self.functions.len() as u64
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
        if seen.len() != self.functions.len() {
            return Err(".sframe: missing required PC64 index start-address relocation".into());
        }
        Ok(())
    }

    /// Resolve original symbol identity before undefined symbols can become imports.
    pub fn bind(
        &self,
        section: usize,
        units: &[Unit],
        sections: &[Section],
        globals: &BTreeMap<String, (usize, usize)>,
        validate_original: bool,
    ) -> Result<Bindings> {
        let mut targets: Vec<Option<CodeBinding>> =
            (0..self.functions.len()).map(|_| None).collect();
        let mut seen = BTreeSet::new();
        for (ui, unit) in units.iter().enumerate() {
            for r in unit.relocs.iter().filter(|r| r.section == section) {
                self.relocation(r.offset, r.kind, &mut seen)?;
                let symbol = unit
                    .symbols
                    .get(&r.symbol)
                    .ok_or(".sframe: invalid target symbol")?;
                // A losing definition must not hide malformed dead metadata.
                let original_code = symbol
                    .section
                    .index()
                    .and_then(|i| unit.sections.get(&i.0))
                    .map(|&section| (section, i128::from(symbol.value) + i128::from(r.addend)));
                if validate_original && let Some((original, pc)) = original_code {
                    let i = (r.offset as usize - HEADER) / INDEX;
                    if sections[original].flags & 4 == 0 {
                        return Err(".sframe: non-code relocation target".into());
                    }
                    if pc < 0
                        || pc + i128::from(self.functions[i].size)
                            > i128::from(sections[original].size)
                    {
                        return Err(".sframe: original target code range is outside its executable input section".into());
                    }
                }
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
                let finish = pc + i128::from(self.functions[i].size);
                if pc < 0 || finish > i128::from(sections[*out].size) {
                    return Err(
                        ".sframe: target code range is outside its executable input section".into(),
                    );
                }
                let binding = CodeBinding {
                    relocation_unit: ui,
                    relocation_symbol: r.symbol,
                    addend: r.addend,
                    original_code,
                    selected_unit: u,
                    selected_symbol: s,
                    section: *out,
                    relative: pc as u64,
                };
                if targets[i].replace(binding).is_some() {
                    return Err(".sframe: duplicate index start-address relocation".into());
                }
            }
        }
        Ok(Bindings {
            functions: targets
                .into_iter()
                .map(|target| {
                    target.ok_or_else(|| {
                        ".sframe: missing required PC64 index start-address relocation".into()
                    })
                })
                .collect::<Result<_>>()?,
        })
    }

    /// Select/reorder identities and translate every retained PC64 field before
    /// layout. Exact row blocks move once; no image-sized row clones are needed.
    pub fn reconstruct(
        &mut self,
        targets: &mut Bindings,
        indices: Vec<usize>,
        section: usize,
        sections: &mut [Section],
        units: &mut [Unit],
    ) -> Result<()> {
        let selected: BTreeSet<_> = indices.iter().copied().collect();
        let blocks: Vec<_> = self
            .identity_plan()
            .blocks
            .into_iter()
            .filter(|i| selected.contains(i))
            .collect();
        // Keep independent physical FRE order even when indices change.
        let encoded = self.encode(&Plan { indices, blocks })?;
        let fields: BTreeMap<_, _> = encoded
            .fields
            .iter()
            .map(|f| (f.original as u64, f.output as u64))
            .collect();
        for unit in units {
            unit.relocs.retain_mut(|r| {
                if r.section != section {
                    return true;
                }
                if let Some(&output) = fields.get(&r.offset) {
                    r.offset = output;
                    true
                } else {
                    false
                }
            });
        }
        let mut functions: Vec<_> = std::mem::take(&mut self.functions)
            .into_iter()
            .map(Some)
            .collect();
        let mut bindings: Vec<_> = std::mem::take(&mut targets.functions)
            .into_iter()
            .map(Some)
            .collect();
        let mut retained = reserved(encoded.fields.len())?;
        let mut bound = reserved(encoded.fields.len())?;
        for field in &encoded.fields {
            let mut function = functions[field.function]
                .take()
                .ok_or(".sframe: duplicate planned function")?;
            function.field = field.output;
            function.fre_offset = word(&encoded.bytes, field.output + 12);
            retained.push(function);
            bound.push(
                bindings[field.function]
                    .take()
                    .ok_or(".sframe: missing planned binding")?,
            );
        }
        self.functions = retained;
        targets.functions = bound;
        self.header.flags = encoded.bytes[3];
        self.header.rows = word(&encoded.bytes, 12);
        self.header.fre_len = word(&encoded.bytes, 16);
        sections[section].size = encoded.bytes.len() as u64;
        sections[section].data = encoded.bytes;
        Ok(())
    }

    pub fn live_indices(&self, targets: &Bindings, live: &BTreeSet<usize>) -> Vec<usize> {
        targets
            .functions
            .iter()
            .enumerate()
            .filter(|(_, b)| live.contains(&b.section))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn order(&self, targets: &Bindings, sections: &[Section]) -> Vec<usize> {
        let mut ranks = vec![0; sections.len()];
        let mut rank = 0;
        for permissions in [4, 5, 6] {
            for bss in [false, true] {
                for i in super::bounds::order(
                    sections,
                    super::lifecycle::order(sections, permissions, bss),
                ) {
                    ranks[i] = rank;
                    rank += 1;
                }
            }
        }
        let mut indices: Vec<_> = (0..self.functions.len()).collect();
        indices.sort_by_key(|&i| {
            (
                ranks[targets.functions[i].section],
                targets.functions[i].relative,
            )
        });
        indices
    }

    /// Independently decode relocated, field-relative PCs; never sort raw deltas.
    pub fn finish(
        &self,
        bytes: &mut [u8],
        section: usize,
        sections: &[Section],
        targets: &Bindings,
    ) -> Result<()> {
        if targets.functions.len() != self.functions.len() {
            return Err(".sframe: binding count disagrees with function count".into());
        }
        let section = &sections[section];
        let stop = section
            .offset
            .checked_add(section.size)
            .ok_or(".sframe: section bounds overflow")?;
        let data = bytes
            .get_mut(section.offset as usize..stop as usize)
            .ok_or(".sframe: section outside output image")?;
        if data.len() != HEADER + INDEX * self.functions.len() + self.header.fre_len as usize {
            return Err(".sframe: output size disagrees with validated layout".into());
        }
        let mut previous_end = None;
        for (entry, binding) in self.functions.iter().zip(&targets.functions) {
            let p = BASE
                .checked_add(section.offset)
                .and_then(|n| n.checked_add(entry.field as u64))
                .ok_or(".sframe: relocated field address overflow")?;
            let d = i64::from_le_bytes(data[entry.field..entry.field + 8].try_into().unwrap());
            let pc = u64::try_from(i128::from(p) + i128::from(d))
                .map_err(|_| ".sframe: relocated PC overflow")?;
            let finish = pc
                .checked_add(u64::from(entry.size))
                .ok_or(".sframe: relocated code range overflow")?;
            let code = &sections[binding.section];
            let start = BASE
                .checked_add(code.offset)
                .ok_or(".sframe: code address overflow")?;
            let expected = start
                .checked_add(binding.relative)
                .ok_or(".sframe: target address overflow")?;
            let code_end = start
                .checked_add(code.size)
                .ok_or(".sframe: code range overflow")?;
            if code.flags & 4 == 0 || pc != expected || pc < start || finish > code_end {
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

#[cfg(test)]
mod tests {
    use super::*;
    use object::{Object, ObjectSection};

    fn crt() -> Vec<u8> {
        let object =
            object::File::parse(&include_bytes!("../../tests/fixtures/sframe/crt1.o")[..]).unwrap();
        object
            .section_by_name(".sframe")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    }
    fn put(data: &mut [u8], at: usize, value: u32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn retained_installed_crt_roundtrip_owns_exact_blocks_and_rows() {
        let data = crt();
        let table = Table::parse(&data).unwrap();
        assert_eq!(
            table.header,
            Header {
                flags: 4,
                rows: 4,
                fre_len: 26
            }
        );
        assert_eq!(table.rebuild().unwrap(), data);
        assert_eq!(
            table
                .functions
                .iter()
                .map(|f| f.rows.len())
                .collect::<Vec<_>>(),
            [2, 1, 1]
        );
        for (i, function) in table.functions.iter().enumerate() {
            let start = 76 + function.fre_offset as usize;
            assert_eq!(function.block, data[start..start + function.block.len()]);
            assert_eq!(function.field, 28 + 16 * i);
            let mut stop = 5;
            for row in &function.rows {
                assert_eq!(row.bytes.start, stop);
                stop = row.bytes.end;
                assert_eq!(
                    &function.block[row.bytes.clone()],
                    &data[start + row.bytes.start..start + row.bytes.end]
                );
            }
            assert_eq!(stop, function.block.len());
        }
        assert_eq!(
            table.functions[0].rows[1],
            Row {
                pc: 4,
                bytes: 8..10
            }
        );
        assert_eq!(&table.functions[0].block[8..], &[4, 0]); // RA undefined
        assert_eq!(Table::parse(&table.rebuild().unwrap()).unwrap(), table);
    }

    #[test]
    fn independent_index_and_fre_permutations_roundtrip_exactly() {
        let mut data = crt();
        // Distinguish the otherwise equal eight-byte blocks and start fields.
        data[101] = 0x80;
        data[28..36].copy_from_slice(&i64::MIN.to_le_bytes());
        data[44..52].copy_from_slice(&i64::MAX.to_le_bytes());
        put(&mut data, 56, 18);
        put(&mut data, 72, 10);
        let table = Table::parse(&data).unwrap();
        assert_eq!(table.identity_plan().blocks, [0, 2, 1]);
        assert_eq!(table.rebuild().unwrap(), data);
        let encoded = table
            .encode(&Plan {
                indices: vec![2, 0, 1],
                blocks: vec![1, 2, 0],
            })
            .unwrap();
        let decoded = Table::parse(&encoded.bytes).unwrap();
        for (output, original) in [2, 0, 1].into_iter().enumerate() {
            assert_eq!(
                decoded.functions[output].block,
                table.functions[original].block
            );
            assert_eq!(
                decoded.functions[output].start,
                table.functions[original].start
            );
            assert_eq!(
                encoded.fields[output],
                PlannedField {
                    function: original,
                    original: 28 + 16 * original,
                    output: 28 + 16 * output
                }
            );
        }
        assert_eq!(decoded.rebuild().unwrap(), encoded.bytes);
    }

    #[test]
    fn filtering_recomputes_counts_offsets_and_clears_unproven_sorted() {
        let mut data = crt();
        data[3] = 5;
        let table = Table::parse(&data).unwrap();
        let encoded = table
            .encode(&Plan {
                indices: vec![2, 0],
                blocks: vec![0, 2],
            })
            .unwrap();
        assert_eq!(
            &encoded.bytes[8..28],
            &[2, 0, 0, 0, 3, 0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0]
        );
        assert_eq!(encoded.bytes[3], 4);
        assert_eq!(word(&encoded.bytes, 40), 10);
        assert_eq!(word(&encoded.bytes, 56), 0);
        let decoded = Table::parse(&encoded.bytes).unwrap();
        assert_eq!(decoded.functions[0].block, table.functions[2].block);
        assert_eq!(decoded.functions[1].block, table.functions[0].block);
        assert_eq!(decoded.rebuild().unwrap(), encoded.bytes);
        let empty = table
            .encode(&Plan {
                indices: vec![],
                blocks: vec![],
            })
            .unwrap();
        assert_eq!(empty.bytes.len(), HEADER);
        assert!(empty.fields.is_empty());
        assert_eq!(
            Table::parse(&empty.bytes).unwrap().rebuild().unwrap(),
            empty.bytes
        );
    }

    #[test]
    fn plans_reject_invalid_identity_and_nonexact_ownership() {
        let table = Table::parse(&crt()).unwrap();
        for (indices, blocks) in [
            (vec![0], vec![]),
            (vec![0, 0], vec![0, 1]),
            (vec![0, 1], vec![0, 0]),
            (vec![0], vec![1]),
            (vec![3], vec![3]),
            (vec![0, 1, 2, 3], vec![0, 1, 2, 3]),
        ] {
            assert!(table.encode(&Plan { indices, blocks }).is_err());
        }
    }

    #[test]
    fn layout_checks_overflow_and_limits_without_allocating() {
        assert_eq!(layout(0, 0, 0).unwrap(), (0, HEADER));
        assert!(layout(u64::MAX, 0, 0).is_err());
        assert!(layout(1, 1, u64::MAX).is_err());
        assert!(layout(0, u32::MAX as u64 + 1, 0).is_err());
        assert!(layout(0, 0, u32::MAX as u64 + 1).is_err());
        assert!(layout(u32::MAX as u64, 0, 0).is_err());
        assert!(layout(0, 0, LIMIT).is_err());
        assert_eq!(
            layout(1, 1, LIMIT - HEADER as u64 - INDEX as u64)
                .unwrap()
                .1,
            LIMIT as usize
        );
    }

    #[test]
    fn working_budget_checks_boundaries_and_bounded_stress() {
        assert!(budget(0, 0, BUDGET / 4).is_ok());
        assert!(budget(0, 0, BUDGET / 4 + 1).is_err());
        assert!(budget(u64::MAX, 0, 0).is_err());
        assert!(budget(0, u64::MAX, 0).is_err());
        // About 2.5 MiB of wire bytes can request far more owned/planning
        // storage. Reject before creating even the first Function or Row.
        let wire = |count: usize| {
            let mut data = vec![0xe2, 0xde, 3, 4, 3, 0, 0xf8, 0];
            for n in [
                count as u32,
                count as u32,
                (7 * count) as u32,
                0,
                (16 * count) as u32,
            ] {
                data.extend_from_slice(&n.to_le_bytes());
            }
            for i in 0..count {
                data.extend_from_slice(&[0; 8]);
                data.extend_from_slice(&1u32.to_le_bytes());
                data.extend_from_slice(&((7 * i) as u32).to_le_bytes());
            }
            for _ in 0..count {
                data.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0]);
            }
            data
        };
        let excessive = wire(110_000);
        assert!(Table::parse(&excessive).unwrap_err().contains("budget"));
        let data = wire(4096);
        let table = Table::parse(&data).unwrap();
        assert_eq!(table.rebuild().unwrap(), data);
        let indices: Vec<_> = (0..4096).step_by(2).rev().collect();
        let blocks: Vec<_> = (0..4096).step_by(2).collect();
        let encoded = table.encode(&Plan { indices, blocks }).unwrap();
        let retained = Table::parse(&encoded.bytes).unwrap();
        assert_eq!(retained.functions.len(), 2048);
        assert_eq!(retained.rebuild().unwrap(), encoded.bytes);
    }

    #[test]
    fn malformed_offsets_counts_and_block_ownership_never_roundtrip() {
        let data = crt();
        for len in 0..data.len() {
            assert!(Table::parse(&data[..len]).is_err(), "truncation {len}");
        }
        for (offset, value) in [
            (8, u32::MAX),
            (12, u32::MAX),
            (16, u32::MAX),
            (20, 1),
            (24, 49),
            (8, 0),
            (12, 3),
            (12, 5),
            (16, 25),
            (16, 27),
            (40, 1),
            (40, 26),
            (56, 0),
            (56, 9),
            (56, 11),
            (72, 10),
        ] {
            let mut bad = data.clone();
            put(&mut bad, offset, value);
            assert!(
                Table::parse(&bad).is_err(),
                "offset {offset}, value {value}"
            );
        }
        let mut trailing = data;
        trailing.push(0);
        assert!(Table::parse(&trailing).is_err());
    }

    #[test]
    fn declared_row_total_is_checked_before_owned_row_allocation() {
        let mut data = crt();
        // The header admits three rows, while the blocks request four. A bad
        // first PC must not hide the understated allocation request.
        put(&mut data, 12, 3);
        data[81] = 1;
        assert!(
            Table::parse(&data)
                .unwrap_err()
                .contains("total FRE count disagrees"),
        );
        // An understated total can admit wire bytes whose actual row storage
        // exceeds the working-set estimate. No near-cap allocation is needed.
        let count = 24;
        let actual_rows = count * u16::MAX as u64;
        let size = HEADER as u64 + count * (INDEX as u64 + 5) + actual_rows * 5;
        assert!(budget(count, count, size).is_ok());
        assert!(budget(count, actual_rows, size).is_err());
    }

    #[test]
    fn all_pc_and_signed_word_widths_retain_exact_rows() {
        for pc_type in 0..=2u8 {
            for width in 0..=2u8 {
                let mut data = crt()[..44].to_vec();
                let p = 1usize << pc_type;
                let w = 1usize << width;
                let mut block = vec![2, 0, pc_type, 0, 0];
                block.extend(vec![0; p]);
                block.push(5 | (width << 5));
                block.extend(vec![0xff; 2 * w]); // exact signed -1 words
                block.push(4);
                block.extend(vec![0; p - 1]);
                block.push(0); // RA undefined
                put(&mut data, 8, 1);
                put(&mut data, 12, 2);
                put(&mut data, 16, block.len() as u32);
                put(&mut data, 24, 16);
                data.extend_from_slice(&block);
                let table = Table::parse(&data).unwrap();
                assert_eq!(table.functions[0].block, block);
                assert_eq!(table.functions[0].rows[1].pc, 4);
                assert_eq!(table.rebuild().unwrap(), data);
            }
        }
    }

    #[test]
    fn binding_retains_original_local_identity_and_selected_global_winner() {
        use super::super::{Reloc, Symbol};
        let table = Table::parse(&crt()).unwrap();
        let single = table
            .encode(&Plan {
                indices: vec![2],
                blocks: vec![2],
            })
            .unwrap();
        let table = Table::parse(&single.bytes).unwrap();
        let section = |name: &str| Section {
            name: name.into(),
            flags: 6,
            kind: 1,
            align: 1,
            size: 100,
            bss: false,
            data: vec![0; 100],
            offset: 0,
        };
        let sections = vec![section("original"), section("winner")];
        let unit = |value: usize| Unit {
            name: "test".into(),
            sections: BTreeMap::from([(1, value / 8)]),
            symbols: BTreeMap::from([(
                7,
                Symbol {
                    name: "same".into(),
                    section: SymbolSection::Section(object::SectionIndex(1)),
                    value: value as u64,
                    size: 5,
                    info: 0x12,
                    other: 0,
                },
            )]),
            relocs: vec![],
            consumed_properties: BTreeSet::new(),
            groups: BTreeMap::new(),
        };
        let mut units = vec![unit(0), unit(8)];
        units[0].relocs.push(Reloc {
            section: 2,
            offset: 28,
            symbol: 7,
            kind: object::elf::R_X86_64_PC64,
            addend: 3,
        });
        let globals = BTreeMap::from([("same".into(), (1, 7))]);
        let bound = table.bind(2, &units, &sections, &globals, true).unwrap();
        let b = &bound.functions[0];
        assert_eq!(b.original_code, Some((0, 3)));
        assert_eq!(
            (b.relocation_unit, b.relocation_symbol, b.addend),
            (0, 7, 3)
        );
        assert_eq!(
            (b.selected_unit, b.selected_symbol, b.section, b.relative),
            (1, 7, 1, 11)
        );
        units[0].symbols.get_mut(&7).unwrap().info = 2; // local, same spelling
        let bound = table.bind(2, &units, &sections, &globals, true).unwrap();
        let b = &bound.functions[0];
        assert_eq!(
            (b.selected_unit, b.selected_symbol, b.section, b.relative),
            (0, 7, 0, 3)
        );
        units[0].relocs.clear();
        assert!(table.bind(2, &units, &sections, &globals, true).is_err());
    }
}
