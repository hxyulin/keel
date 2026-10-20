use crate::*;
use read::Reader;
use write::u32le;

const ENTRIES_PER_PAGE: usize = 511;

// All arrays and regular pages are 4-byte aligned. Keep sizing and emission on
// the same checked format layout; no final address or encoding affects widths.
struct Layout {
    index: usize,
    lsda: usize,
    pages: usize,
    page_count: usize,
    size: usize,
}
impl Layout {
    fn new(entries: usize, personalities: usize, lsdas: usize) -> Result<Self> {
        let page_count = entries.div_ceil(ENTRIES_PER_PAGE);
        let add = |base: usize, count: usize, width: usize| {
            base.checked_add(count.checked_mul(width).ok_or("unwind size overflow")?)
                .ok_or("unwind size overflow")
        };
        let index = add(28, personalities, 4)?;
        let lsda = add(
            index,
            page_count.checked_add(1).ok_or("unwind size overflow")?,
            12,
        )?;
        let pages = add(lsda, lsdas, 8)?;
        let size = add(add(pages, page_count, 8)?, entries, 8)?;
        u32::try_from(size).map_err(|_| "unwind section exceeds 32-bit offsets")?;
        if size > 512 * 1024 * 1024 {
            return fail("unwind section exceeds 512 MiB");
        }
        Ok(Self {
            index,
            lsda,
            pages,
            page_count,
            size,
        })
    }
}
#[derive(Clone)]
struct Entry {
    start: u32,
    end: u32,
    encoding: u32,
    lsda: u32,
}
impl Linker {
    // This bound includes one gap per pair of input records, all possible
    // personalities and LSDA entries. It is independent of final addresses.
    fn unwind_counts(&self) -> Result<(usize, usize, usize)> {
        let mut records = 0usize;
        let mut lsdas = 0usize;
        let mut personalities = BTreeSet::new();
        for (oi, o) in self.objects.iter().enumerate() {
            for s in &o.sections {
                if s.segment != "__LD" || s.name != "__compact_unwind" {
                    continue;
                }
                if s.data.len() % 32 != 0 {
                    return fail("compact unwind size is not a multiple of 32");
                }
                records = records
                    .checked_add(s.data.len() / 32)
                    .ok_or("unwind record count overflow")?;
                for at in (0..s.data.len()).step_by(32) {
                    lsdas = lsdas
                        .checked_add(usize::from(Reader(&s.data).u32(at + 12)? & 0x40000000 != 0))
                        .ok_or("unwind LSDA count overflow")?;
                }
                for rel in &s.relocs {
                    if rel.offset % 32 == 16 {
                        personalities.insert(self.got_key(oi, rel.symbol)?);
                    }
                }
            }
        }
        Ok((records, personalities.len(), lsdas))
    }

    pub(crate) fn unwind_bound(&self) -> Result<usize> {
        let (records, personalities, _) = self.unwind_counts()?;
        let ranges = records
            .checked_add(records.saturating_sub(1))
            .ok_or("unwind range count overflow")?;
        // Count every record as a potential LSDA: unusual relocations may
        // change its flag/pointer during provisional layout.
        Ok(Layout::new(ranges, personalities, records)?.size)
    }

    // Reuse a successful size calculation only for ordinary pointer records.
    // Lengths and encoding/LSDA flags cannot change in this subset, and the
    // personality keys are fixed. Only relocated function starts affect the
    // range/gap count. Input addends and section-relative bases remain fixed
    // while layout changes output addresses; compare every start's target.
    // Differences, instruction relocations and writes to other fields retain
    // the full relocation-based planner on every provisional layout.
    pub(crate) fn unwind_plan_key(&self) -> Option<Vec<u64>> {
        let mut starts = Vec::new();
        for (oi, o) in self.objects.iter().enumerate() {
            for s in &o.sections {
                if s.segment != "__LD" || s.name != "__compact_unwind" {
                    continue;
                }
                // UNSIGNED in a TLV descriptor can subtract a moving TLS
                // template base as well as adding the symbol target.
                if s.flags & 255 == 0x13 {
                    return None;
                }
                for r in &s.relocs {
                    if r.kind != 0 || r.pc || r.width != 8 || !matches!(r.offset % 32, 0 | 16 | 24)
                    {
                        return None;
                    }
                    if r.offset % 32 == 0 {
                        starts.push(self.reloc_target(oi, r).ok()?.1);
                    }
                }
            }
        }
        Some(starts)
    }

    // Relocate only metadata into scratch payloads using the real relocation
    // validator. Restore every original payload even on error: final relocation
    // must apply addends exactly once, after layout has been fixed.
    pub(crate) fn planned_unwind_size(&mut self) -> Result<usize> {
        let mut originals = Vec::new();
        let result = (|| {
            for oi in 0..self.objects.len() {
                for si in 0..self.objects[oi].sections.len() {
                    let s = &self.objects[oi].sections[si];
                    if s.segment == "__LD" && s.name == "__compact_unwind" {
                        originals.push((oi, si, s.data.clone()));
                        self.relocate_section(oi, si)?;
                    }
                }
            }
            let (_, personalities, lsdas) = self.unwind_counts()?;
            let mut entries = Vec::new();
            for o in &self.objects {
                for s in &o.sections {
                    if s.segment == "__LD" && s.name == "__compact_unwind" {
                        let r = Reader(&s.data);
                        for at in (0..s.data.len()).step_by(32) {
                            let (start, end) = function_range(r.u64(at)?, r.u32(at + 8)?)?;
                            entries.push(Entry {
                                start,
                                end,
                                encoding: 0,
                                lsda: 0,
                            });
                        }
                    }
                }
            }
            Ok(Layout::new(function_ranges(entries)?.len(), personalities, lsdas)?.size)
        })();
        for (oi, si, data) in originals {
            self.objects[oi].sections[si].data = data;
        }
        result
    }

    pub(crate) fn unwind(&self) -> Result<Vec<u8>> {
        let mut fdes = BTreeMap::new();
        for o in &self.objects {
            for s in &o.sections {
                if s.name != "__eh_frame" {
                    continue;
                }
                let output = &self.outputs[s.output.ok_or("eh_frame discarded")?];
                // Membership is by offset after filtering: an earlier foreign
                // relocation at the same offset must not hide a native one.
                let fde_starts: BTreeSet<_> = s
                    .relocs
                    .iter()
                    .filter(|rel| rel.kind == 1 && rel.width == 8)
                    .map(|rel| rel.offset)
                    .collect();
                let mut at = 0;
                while at < s.data.len() {
                    let r = Reader(&s.data);
                    let length = r.u32(at)? as usize;
                    if length == 0 {
                        at += 4;
                        continue;
                    }
                    if length == 0xffffffff || length < 4 || at + 4 + length > s.data.len() {
                        return fail("unsupported/malformed DWARF FDE length");
                    }
                    let cie = r.u32(at + 4)?;
                    if cie != 0 {
                        // Keel emits explicit pointer differences with native
                        // width PC-relative FDE starts (DW_EH_PE_pcrel).
                        // Never silently copy implicit assembler FDE fixups.
                        if !fde_starts.contains(&(at + 8)) {
                            return fail(format!(
                                "{}: implicit/foreign eh_frame FDE fixups require the system linker",
                                o.name
                            ));
                        }
                        let cie_at = (at + 4)
                            .checked_sub(cie as usize)
                            .ok_or("invalid FDE CIE pointer")?;
                        validate_cie(&s.data, cie_at)?;
                        if length < 21 {
                            return fail("truncated native-width FDE");
                        }
                        let field = output.address + s.offset as u64 + at as u64 + 8;
                        let start = field.wrapping_add(r.u64(at + 8)?);
                        let offset = u32::try_from(
                            s.offset
                                .checked_add(at)
                                .ok_or("DWARF FDE offset overflow")?,
                        )
                        .map_err(|_| "DWARF unwind FDE offset exceeds 32 bits")?;
                        if offset >= 1 << 24 {
                            return fail("DWARF unwind FDE offset exceeds 24 bits");
                        }
                        fdes.insert(start, offset);
                    }
                    at += 4 + length;
                }
            }
        }
        let (ranges, personalities) = self.unwind_entries(&fdes)?;
        let sentinel = ranges.last().map_or(0, |e| e.end);
        let layout = Layout::new(
            ranges.len(),
            personalities.len(),
            ranges.iter().filter(|e| e.lsda != 0).count(),
        )?;
        let personality_off = 28;
        let index_off = layout.index;
        let lsda_off = layout.lsda;
        let pages_off = layout.pages;
        let page_count = layout.page_count;
        let mut out = Vec::with_capacity(layout.size);
        for n in [
            1,
            28,
            0,
            personality_off as u32,
            personalities.len() as u32,
            index_off as u32,
            (page_count + 1) as u32,
        ] {
            u32le(&mut out, n);
        }
        for p in personalities {
            u32le(&mut out, p);
        }
        let mut lsda_cursor = lsda_off;
        let mut page_cursor = pages_off;
        for chunk in ranges.chunks(ENTRIES_PER_PAGE) {
            for n in [chunk[0].start, page_cursor as u32, lsda_cursor as u32] {
                u32le(&mut out, n);
            }
            lsda_cursor += chunk.iter().filter(|e| e.lsda != 0).count() * 8;
            page_cursor += 8 + chunk.len() * 8;
        }
        for n in [sentinel, 0, lsda_cursor as u32] {
            u32le(&mut out, n);
        }
        for e in &ranges {
            if e.lsda != 0 {
                u32le(&mut out, e.start);
                u32le(&mut out, e.lsda);
            }
        }
        out.resize(pages_off, 0);
        for chunk in ranges.chunks(ENTRIES_PER_PAGE) {
            u32le(&mut out, 2);
            out.extend_from_slice(&8u16.to_le_bytes());
            out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            for e in chunk {
                u32le(&mut out, e.start);
                u32le(&mut out, e.encoding);
            }
        }
        if out.len() != layout.size {
            return fail("internal unwind serialization size mismatch");
        }
        Ok(out)
    }

    fn unwind_entries(&self, fdes: &BTreeMap<u64, u32>) -> Result<(Vec<Entry>, Vec<u32>)> {
        let mut personalities = Vec::new();
        let mut entries = Vec::new();
        for (oi, o) in self.objects.iter().enumerate() {
            for s in &o.sections {
                if s.segment != "__LD" || s.name != "__compact_unwind" {
                    continue;
                }
                if s.data.len() % 32 != 0 {
                    return fail("compact unwind size is not a multiple of 32");
                }
                let r = Reader(&s.data);
                // Preserve find's first-match semantics, including malformed
                // duplicates. Validation still happens in record order below.
                let mut personality_relocs = BTreeMap::new();
                for rel in &s.relocs {
                    if rel.offset % 32 == 16 {
                        personality_relocs.entry(rel.offset).or_insert(rel);
                    }
                }
                for at in (0..s.data.len()).step_by(32) {
                    let address = r.u64(at)?;
                    let (start, end) = function_range(address, r.u32(at + 8)?)?;
                    let mut encoding = r.u32(at + 12)?;
                    if encoding & 0x30000000 != 0 {
                        return fail("input unwind encoding already has personality index");
                    }
                    if encoding & 0x0f000000 == 0x03000000 {
                        encoding = (encoding & !0xffffff)
                            | *fdes
                                .get(&address)
                                .ok_or("DWARF-mode compact unwind entry has no relocated FDE")?;
                    } else if !matches!(encoding & 0x0f000000, 0 | 0x02000000 | 0x04000000) {
                        return fail("unknown arm64 compact unwind mode");
                    }
                    let personality = personality_relocs.get(&(at + 16));
                    if let Some(rel) = personality {
                        if !rel.external || rel.kind != 0 || rel.width != 8 {
                            return fail("unsupported compact personality relocation");
                        }
                        let got = self.outputs[self.got_section].address
                            + self.got[&self.got_key(oi, rel.symbol)?] as u64 * 8;
                        let offset = u32::try_from(got - BASE)
                            .map_err(|_| "personality GOT offset exceeds 32 bits")?;
                        let index = if let Some(i) = personalities.iter().position(|v| *v == offset)
                        {
                            i
                        } else {
                            personalities.push(offset);
                            personalities.len() - 1
                        };
                        if index >= 3 {
                            return fail(
                                "more than three compact unwind personalities require DWARF mode",
                            );
                        }
                        encoding |= ((index + 1) as u32) << 28;
                    }
                    let lsda = r.u64(at + 24)?;
                    if (lsda != 0) != (encoding & 0x40000000 != 0) {
                        return fail("compact unwind LSDA flag/pointer mismatch");
                    }
                    let lsda = if lsda == 0 {
                        0
                    } else {
                        u32::try_from(lsda.checked_sub(BASE).ok_or("LSDA below image")?)
                            .map_err(|_| "LSDA offset overflow")?
                    };
                    entries.push(Entry {
                        start,
                        end,
                        encoding,
                        lsda,
                    });
                }
            }
        }
        Ok((function_ranges(entries)?, personalities))
    }
}

fn function_range(address: u64, length: u32) -> Result<(u32, u32)> {
    let start = u32::try_from(
        address
            .checked_sub(BASE)
            .ok_or("unwind function below image")?,
    )
    .map_err(|_| "unwind address exceeds 32 bits")?;
    let end = start
        .checked_add(length)
        .ok_or("unwind function range overflow")?;
    Ok((start, end))
}

fn function_ranges(mut entries: Vec<Entry>) -> Result<Vec<Entry>> {
    entries.sort_by_key(|e| e.start);
    let mut ranges: Vec<Entry> = Vec::new();
    for e in entries {
        if let Some(prev) = ranges.last() {
            if e.start < prev.end {
                return fail("overlapping compact unwind functions");
            }
            if e.start > prev.end {
                ranges.push(Entry {
                    start: prev.end,
                    end: e.start,
                    encoding: 0,
                    lsda: 0,
                });
            }
        }
        ranges.push(e);
    }
    Ok(ranges)
}

// Keel's supported CIE contract: version 1, AArch64 LR=30, code/data
// alignment 1/-8, native-width PC-relative FDE pointers. Personality uses an
// indirect PC-relative signed 32-bit GOT reference. Reject other encodings
// until they have their own relocation and runtime coverage.
fn validate_cie(bytes: &[u8], at: usize) -> Result<()> {
    let r = Reader(bytes);
    let len = r.u32(at)? as usize;
    if len < 9 || r.u32(at + 4)? != 0 {
        return fail("invalid FDE CIE");
    }
    let data = r.bytes(at + 8, len - 4)?;
    if data.first() != Some(&1) {
        return fail("only Keel's version-1 CIE contract is supported");
    }
    let nul = data[1..]
        .iter()
        .position(|b| *b == 0)
        .ok_or("unterminated CIE augmentation")?
        + 1;
    let aug = &data[1..nul];
    if aug.first() != Some(&b'z') {
        return fail("unsupported CIE augmentation");
    }
    let mut cursor = nul + 1;
    if data.get(cursor..cursor + 3) != Some(&[1, 0x78, 30][..]) {
        return fail("unsupported CIE alignment or return register");
    }
    cursor += 3;
    let augmentation_len = *data.get(cursor).ok_or("missing CIE augmentation size")? as usize;
    if augmentation_len >= 128 {
        return fail("extended CIE augmentation length unsupported");
    }
    cursor += 1;
    let end = cursor + augmentation_len;
    for &kind in &aug[1..] {
        let expected = match kind {
            b'P' => 0x9b,
            b'L' | b'R' => 0x10,
            _ => return fail("unsupported CIE augmentation field"),
        };
        if data.get(cursor) != Some(&expected) {
            return fail("unsupported CIE pointer encoding");
        }
        cursor += if kind == b'P' { 5 } else { 1 };
        if cursor > end {
            return fail("invalid CIE augmentation length");
        }
    }
    if !aug.contains(&b'R') || cursor != end || end > data.len() {
        return fail("invalid CIE augmentation layout");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personality_index_preserves_first_match_and_record_error_order() {
        let bytes = keel_asm::assemble_object(".text\n.globl _main\n_main:\nret\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n.long 0x02001000\n.quad _p0\n.quad 0\n").unwrap();
        let mut l = prepare_linker(
            &[Input {
                name: "index.o",
                bytes: &bytes,
            }],
            Options {
                dylibs: vec![Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: BTreeSet::from(["_p0".into()]),
                    weak_exports: BTreeSet::new(),
                }],
                ..Options::default()
            },
        )
        .unwrap();
        l.outputs[l.got_section].address = BASE + 0x1000;
        let si = l.objects[0]
            .sections
            .iter()
            .position(|s| s.name == "__compact_unwind")
            .unwrap();
        let s = &mut l.objects[0].sections[si];
        s.data[..8].copy_from_slice(&(BASE + 4).to_le_bytes());
        let valid = s.relocs.iter().find(|r| r.offset == 16).unwrap().clone();
        let mut invalid = valid.clone();
        invalid.width = 4;
        s.relocs = vec![valid.clone(), invalid.clone()];
        let expected = l.unwind_entries(&BTreeMap::new()).unwrap();
        assert_eq!(expected.1, vec![0x1000]);
        l.objects[0].sections[si].relocs.reverse();
        assert_eq!(
            l.unwind_entries(&BTreeMap::new()).err().unwrap(),
            "unsupported compact personality relocation"
        );
        l.objects[0].sections[si].data[12..16].copy_from_slice(&0x05000000u32.to_le_bytes());
        assert_eq!(
            l.unwind_entries(&BTreeMap::new()).err().unwrap(),
            "unknown arm64 compact unwind mode"
        );
        l.objects[0].sections[si].data[12..16].copy_from_slice(&0x02001000u32.to_le_bytes());
        l.objects[0].sections[si].relocs = vec![valid];
        assert_eq!(l.unwind_entries(&BTreeMap::new()).unwrap().1, expected.1);
    }

    #[test]
    fn fde_index_filters_before_matching_duplicate_offsets() {
        let bytes = keel_asm::assemble_object(
            ".text\n.globl _main\n_main:\n.cfi_startproc\n.cfi_def_cfa x28,16\nret\n.cfi_endproc\n",
        )
        .unwrap();
        let mut l = prepare_linker(
            &[Input {
                name: "fde.o",
                bytes: &bytes,
            }],
            Options {
                dylibs: vec![Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: BTreeSet::new(),
                    weak_exports: BTreeSet::new(),
                }],
                ..Options::default()
            },
        )
        .unwrap();
        let si = l.objects[0]
            .sections
            .iter()
            .position(|s| s.name == "__eh_frame")
            .unwrap();
        let s = &mut l.objects[0].sections[si];
        let valid = s
            .relocs
            .iter()
            .find(|r| r.kind == 1 && r.width == 8)
            .unwrap()
            .clone();
        let mut foreign = valid.clone();
        foreign.kind = 0;
        s.relocs.insert(0, foreign.clone());
        // This fixture's compact record needs a relocated address, but the FDE
        // membership check runs first. Compare its later error with the oracle.
        let with_duplicate = l.unwind().err().unwrap();
        assert!(!with_duplicate.contains("implicit/foreign"));
        l.objects[0].sections[si].relocs.retain(|r| r.kind != 1);
        assert!(l.unwind().err().unwrap().contains("implicit/foreign"));
        foreign.kind = 1;
        foreign.width = 4;
        l.objects[0].sections[si].relocs.push(foreign);
        assert!(l.unwind().err().unwrap().contains("implicit/foreign"));
    }

    #[test]
    fn plan_key_tracks_function_targets_but_not_lsda_addresses() {
        let bytes = keel_asm::assemble_object(".text\n.globl _main\n_main:\nret\n.data\n_lsda:\n.quad 0\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n.long 0x42001000\n.quad _p0\n.quad _lsda\n").unwrap();
        let mut l = prepare_linker(
            &[Input {
                name: "key.o",
                bytes: &bytes,
            }],
            Options {
                dylibs: vec![Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: BTreeSet::from(["_p0".into()]),
                    weak_exports: BTreeSet::new(),
                }],
                ..Options::default()
            },
        )
        .unwrap();
        let key = l.unwind_plan_key().unwrap();
        assert_eq!(key.len(), 1);
        let data = l.outputs.iter().position(|s| s.name == "__data").unwrap();
        l.outputs[data].address += PAGE as u64;
        l.outputs[l.got_section].address += PAGE as u64;
        assert_eq!(l.unwind_plan_key().unwrap(), key);
        let text = l.outputs.iter().position(|s| s.name == "__text").unwrap();
        l.outputs[text].address += PAGE as u64;
        assert_ne!(l.unwind_plan_key().unwrap(), key);

        let metadata = l.objects[0]
            .sections
            .iter()
            .position(|s| s.name == "__compact_unwind")
            .unwrap();
        let original = l.objects[0].sections[metadata].relocs[0].clone();
        // Encoding/length writes, differences and instruction fixups must use
        // the complete planner even when all function target addresses match.
        for (offset, kind, width, pc) in [
            (8, 0, 8, false),
            (12, 0, 8, false),
            (0, 1, 8, false),
            (0, 0, 4, false),
            (0, 0, 8, true),
        ] {
            let r = &mut l.objects[0].sections[metadata].relocs[0];
            *r = original.clone();
            r.offset = offset;
            r.kind = kind;
            r.width = width;
            r.pc = pc;
            assert!(l.unwind_plan_key().is_none());
        }
    }

    #[test]
    fn size_and_function_range_overflows_are_errors() {
        assert!(Layout::new(usize::MAX, 0, 0).is_err());
        assert!(Layout::new(0, usize::MAX, 0).is_err());
        assert!(Layout::new(0, 0, usize::MAX).is_err());
        assert!(Layout::new(1 << 29, 0, 0).is_err());
        assert!(function_range(BASE - 1, 0).is_err());
        assert!(function_range(BASE + u32::MAX as u64, 1).is_err());
    }
}
