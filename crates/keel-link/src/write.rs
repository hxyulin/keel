use crate::*;
use read::{Reader, Reloc};

// Most object relocation lists run monotonically through the section (often
// backwards). Keep those disjoint ranges in a compact vector. Arbitrary input
// order falls back once to an interval tree, without changing validation order.
#[derive(Default)]
struct RelocationRanges {
    ordered: Vec<(usize, usize)>,
    descending: bool,
    unordered: Option<BTreeMap<usize, usize>>,
}
impl RelocationRanges {
    fn insert(&mut self, start: usize, end: usize) -> bool {
        if start == end {
            return true;
        }
        if self.unordered.is_none() {
            let append = match self.ordered.as_slice() {
                [] => true,
                &[(first, last)] => {
                    if end <= first {
                        self.descending = true;
                        true
                    } else if start >= last {
                        true
                    } else {
                        return false;
                    }
                }
                ranges => {
                    let &(last_start, last_end) = ranges.last().unwrap();
                    if self.descending {
                        end <= last_start
                    } else {
                        start >= last_end
                    }
                }
            };
            if append {
                self.ordered.push((start, end));
                return true;
            }
            self.unordered = Some(std::mem::take(&mut self.ordered).into_iter().collect());
        }
        let ranges = self.unordered.as_mut().unwrap();
        // Existing ranges are disjoint. The last one starting before this end
        // is the only predecessor that can intersect the new half-open range.
        if ranges
            .range(..end)
            .next_back()
            .is_some_and(|(_, &last)| last > start)
        {
            return false;
        }
        ranges.insert(start, end);
        true
    }
}

pub(crate) fn u32le(v: &mut Vec<u8>, n: u32) {
    v.extend_from_slice(&n.to_le_bytes());
}
pub(crate) fn u64le(v: &mut Vec<u8>, n: u64) {
    v.extend_from_slice(&n.to_le_bytes());
}
pub(crate) fn uleb(v: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        v.push(b | if n == 0 { 0 } else { 128 });
        if n == 0 {
            break;
        }
    }
}
fn sleb(v: &mut Vec<u8>, mut n: i64) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        let end = (n == 0 && b & 64 == 0) || (n == -1 && b & 64 != 0);
        v.push(b | if end { 0 } else { 128 });
        if end {
            break;
        }
    }
}
fn name(v: &mut Vec<u8>, s: &str) {
    let mut n = [0; 16];
    n[..s.len()].copy_from_slice(s.as_bytes());
    v.extend_from_slice(&n);
}
fn cmd(v: &mut Vec<u8>, kind: u32, values: &[u32]) {
    u32le(v, kind);
    u32le(v, 8 + values.len() as u32 * 4);
    for x in values {
        u32le(v, *x);
    }
}
fn string_cmd(v: &mut Vec<u8>, kind: u32, s: &str, dylib: bool) {
    let header = if dylib { 24 } else { 12 };
    let size = align(header + s.len() + 1, 8);
    u32le(v, kind);
    u32le(v, size as u32);
    u32le(v, header as u32);
    if dylib {
        for n in [0, 0x10000, 0x10000] {
            u32le(v, n);
        }
    }
    v.extend_from_slice(s.as_bytes());
    v.resize(v.len() + size - header - s.len(), 0);
}
struct Segment {
    name: &'static str,
    address: u64,
    vm: usize,
    file: usize,
    size: usize,
    prot: u32,
    sections: Vec<usize>,
}
impl Segment {
    fn write(&self, v: &mut Vec<u8>, out: &[OutputSection]) {
        u32le(v, 0x19);
        u32le(v, (72 + self.sections.len() * 80) as u32);
        name(v, self.name);
        for n in [
            self.address,
            self.vm as u64,
            self.file as u64,
            self.size as u64,
        ] {
            u64le(v, n);
        }
        for n in [self.prot, self.prot, self.sections.len() as u32, 0] {
            u32le(v, n);
        }
        for &id in &self.sections {
            let s = &out[id];
            name(v, &s.name);
            name(v, &s.segment);
            u64le(v, s.address);
            u64le(v, s.size as u64);
            for n in [
                if matches!(s.flags & 255, 1 | 0x12) {
                    0
                } else {
                    s.file as u32
                },
                s.alignment,
                0,
                0,
                s.flags,
                if s.name == "__got" {
                    0
                } else if s.name == "__stubs" {
                    out.iter().find(|s| s.name == "__got").unwrap().size as u32 / 8
                } else {
                    0
                },
                if s.name == "__stubs" { 12 } else { 0 },
                0,
            ] {
                u32le(v, n);
            }
        }
    }
}
impl Linker {
    pub(crate) fn emit_to<W: std::io::Write + std::io::Seek>(
        mut self,
        output: &mut W,
        reserve: impl FnOnce(&mut W, usize),
    ) -> Result<LinkArtifacts> {
        // Plan before committing any relocations. Gap entries can change when
        // later sections move as unwind shrinks. Iterate provisional layouts,
        // with a bounded fallback for unusual address-dependent inputs.
        let bound = self.unwind_bound()?;
        self.outputs[self.unwind_section].size = bound;
        let mut settled = false;
        let mut previous_plan = None;
        for _ in 0..8 {
            self.layout()?;
            let key = self.unwind_plan_key();
            let planned = if let (Some((previous_key, size)), Some(key)) = (&previous_plan, &key)
                && previous_key == key
            {
                Ok(*size)
            } else {
                self.planned_unwind_size()
            };
            let Ok(planned) = planned else {
                // A provisional shrink can create overlapping ranges. Restore
                // the bound and let final serialization validate that layout.
                break;
            };
            previous_plan = key.map(|key| (key, planned));
            if planned > bound {
                return fail("internal unwind plan exceeds bound");
            }
            if planned == self.outputs[self.unwind_section].size {
                settled = true;
                break;
            }
            self.outputs[self.unwind_section].size = planned;
        }
        // If sizing cannot settle, retain a proven bound before final layout.
        if !settled {
            self.outputs[self.unwind_section].size = bound;
        }
        let (mut segments, file, address) = self.layout()?;
        self.outputs[self.unwind_section].data = vec![0; self.outputs[self.unwind_section].size];
        self.relocate()?;
        self.make_got_stubs()?;
        let unwind = self.unwind()?;
        let u = &mut self.outputs[self.unwind_section];
        if unwind.len() > u.size || (settled && unwind.len() != u.size) {
            return fail("internal unwind plan exhausted");
        }
        u.data[..unwind.len()].copy_from_slice(&unwind);
        let mut linkedit = Vec::new();
        let mut ranges = Vec::new();
        let mut rebase = Vec::new();
        for &addr in &self.rebases {
            let (seg, off) = segment_offset(&segments, addr)?;
            rebase.push(0x11);
            rebase.push(0x20 | seg);
            uleb(&mut rebase, off);
            rebase.push(0x51);
        }
        rebase.push(0);
        let mut binding = Vec::new();
        for (&addr, &(import, addend)) in &self.binds {
            let (symbol, ordinal) = &self.imports[import];
            let (seg, off) = segment_offset(&segments, addr)?;
            if *ordinal < 0 {
                binding.push(0x30 | (*ordinal as u8 & 15));
            } else {
                binding.push(0x20);
                uleb(&mut binding, *ordinal as u64);
            }
            binding.push(0x40 | u8::from(self.weak_imports.contains(&import)));
            binding.extend_from_slice(symbol.as_bytes());
            binding.push(0);
            binding.push(0x51);
            binding.push(0x60);
            sleb(&mut binding, addend);
            binding.push(0x70 | seg);
            uleb(&mut binding, off);
            binding.push(0x90);
        }
        binding.push(0);
        let mut weak_binding = Vec::new();
        // dyld requires weak binding records in symbol-name order.
        for name in &self.weak_names {
            let strong = matches!(self.globals.get(name), Some(Target::Definition(o, s)) if self.objects[*o].symbols[*s].desc & 0x80 == 0);
            weak_binding.push(0x40 | if strong { 8 } else { 0 });
            weak_binding.extend_from_slice(name.as_bytes());
            weak_binding.push(0);
            for (&addr, (symbol, addend)) in &self.weak_binds {
                if symbol != name {
                    continue;
                }
                let (seg, off) = segment_offset(&segments, addr)?;
                weak_binding.push(0x51);
                weak_binding.push(0x60);
                sleb(&mut weak_binding, *addend);
                weak_binding.push(0x70 | seg);
                uleb(&mut weak_binding, off);
                weak_binding.push(0x90);
            }
        }
        weak_binding.push(0);
        let exports = self.exports()?;
        for data in [&rebase, &binding, &exports, &weak_binding] {
            let off = file + linkedit.len();
            ranges.push((off as u32, data.len() as u32));
            linkedit.extend_from_slice(data);
            linkedit.resize(align(linkedit.len(), 8), 0);
        }
        let mut strings = vec![0];
        let mut symbols = Vec::new();
        let debug_objects = self.debug_map(&mut symbols, &mut strings)?;
        let locals = (symbols.len() / 16) as u32;
        let mut indices = BTreeMap::new();
        let mut defined = 0;
        for (symbol, &target) in &self.globals {
            if matches!(target, Target::Import(_)) {
                continue;
            }
            let addr = self.address(target)?;
            let (kind, sect) = match target {
                Target::Definition(oi, si) if self.objects[oi].symbols[si].kind & 0xe == 2 => {
                    (3, 0)
                }
                Target::Definition(oi, si) => {
                    let symbol = &self.objects[oi].symbols[si];
                    let section = &self.objects[oi].sections[symbol.section - 1];
                    (
                        15 | (symbol.kind & 0x10),
                        self.outputs[section.output.ok_or("export in discarded section")?].ordinal,
                    )
                }
                Target::Common(_) => (
                    15 | if self.private_commons.contains(symbol) {
                        0x10
                    } else {
                        0
                    },
                    self.outputs[self.common_section].ordinal,
                ),
                Target::Header => (15, 1),
                Target::SectionBoundary(id, _) => (0x1f, self.outputs[id].ordinal),
                Target::Import(_) => unreachable!(),
            };
            indices.insert(symbol.clone(), locals + defined);
            defined += 1;
            let desc = match target {
                Target::Definition(o, s) => self.objects[o].symbols[s].desc & 0x80,
                _ => 0,
            };
            nlist(&mut symbols, &mut strings, symbol, kind, sect, desc, addr);
        }
        for (i, (symbol, ordinal)) in self.imports.iter().enumerate() {
            indices.insert(symbol.clone(), locals + defined + i as u32);
            if *ordinal > 253 {
                return fail("more than 253 dylib ordinals unsupported");
            }
            nlist(
                &mut symbols,
                &mut strings,
                symbol,
                1,
                0,
                ((*ordinal as u16 & 255) << 8)
                    | if self.weak_imports.contains(&i) {
                        0x40
                    } else {
                        0
                    },
                0,
            );
        }
        let symoff = file + linkedit.len();
        linkedit.extend_from_slice(&symbols);
        let stroff = file + linkedit.len();
        linkedit.extend_from_slice(&strings);
        linkedit.resize(align(linkedit.len(), 8), 0);
        let indirectoff = file + linkedit.len();
        let mut got: Vec<_> = self.got.iter().collect();
        got.sort_by_key(|(_, i)| **i);
        let mut stubs: Vec<_> = self.stubs.iter().collect();
        stubs.sort_by_key(|(_, i)| **i);
        for (symbol, _) in got.into_iter().chain(stubs) {
            u32le(&mut linkedit, *indices.get(symbol).unwrap_or(&0x80000000));
        }
        let indirectcount = (linkedit.len() + file - indirectoff) / 4;
        linkedit.resize(align(linkedit.len(), 16), 0);
        segments.push(Segment {
            name: "__LINKEDIT",
            address,
            vm: align(linkedit.len(), PAGE),
            file,
            size: linkedit.len(),
            prot: 1,
            sections: vec![],
        });
        let mut commands = Vec::new();
        for seg in &segments {
            seg.write(&mut commands, &self.outputs);
        }
        cmd(
            &mut commands,
            0x80000022,
            &[
                ranges[0].0,
                ranges[0].1,
                ranges[1].0,
                ranges[1].1,
                ranges[3].0,
                ranges[3].1,
                0,
                0,
                ranges[2].0,
                ranges[2].1,
            ],
        );
        cmd(
            &mut commands,
            2,
            &[
                symoff as u32,
                (symbols.len() / 16) as u32,
                stroff as u32,
                strings.len() as u32,
            ],
        );
        cmd(
            &mut commands,
            0xb,
            &[
                0,
                locals,
                locals,
                defined,
                locals + defined,
                self.imports.len() as u32,
                0,
                0,
                0,
                0,
                0,
                0,
                indirectoff as u32,
                indirectcount as u32,
                0,
                0,
                0,
                0,
            ],
        );
        string_cmd(&mut commands, 0xe, "/usr/lib/dyld", false);
        for dylib in &self.options.dylibs {
            string_cmd(&mut commands, 0xc, &dylib.install_name, true);
        }
        let main = *self
            .globals
            .get("_main")
            .ok_or("missing entry symbol `_main`")?;
        let entry = self.address(main)?;
        if !self.outputs.iter().any(|s| {
            s.segment == "__TEXT"
                && !matches!(s.flags & 255, 1 | 0x12)
                && s.flags & 0x80000400 != 0
                && entry >= s.address
                && entry < s.address + s.size as u64
        }) {
            return fail("_main is not a text definition");
        }
        u32le(&mut commands, 0x80000028);
        u32le(&mut commands, 24);
        u64le(&mut commands, entry - BASE);
        u64le(&mut commands, 0);
        cmd(
            &mut commands,
            0x32,
            &[1, self.options.minimum_macos, self.options.sdk_version, 0],
        );
        // Current dyld requires LC_UUID even for small executables. Fill a
        // deterministic content-derived UUID after assembling the image.
        let uuid_offset = 32 + commands.len() + 8;
        cmd(&mut commands, 0x1b, &[0, 0, 0, 0]);
        // Space after load commands lets the signer add LC_CODE_SIGNATURE without
        // shifting mapped code or invalidating PC-relative relocations.
        if 32 + commands.len() + 16 > PAGE {
            return fail("load commands exceed reserved header page");
        }
        let image_size = file
            .checked_add(linkedit.len())
            .ok_or("image size overflow")?;
        // The byte API can reserve exactly once; file sinks need no buffer.
        reserve(output, image_size);
        let mut bytes = Vec::with_capacity(32 + commands.len());
        let mut flags = 0x200085; // MH_NOUNDEFS | MH_DYLDLINK | MH_TWOLEVEL | MH_PIE
        if self
            .weak_names
            .iter()
            .any(|name| matches!(self.globals.get(name), Some(Target::Definition(_, _))))
        {
            // Also needed for NON_WEAK_DEFINITION records overriding dylib weak definitions.
            flags |= 0x8000; // MH_WEAK_DEFINES
        }
        if !self.weak_binds.is_empty() {
            flags |= 0x10000;
        } // MH_BINDS_TO_WEAK
        if self.outputs.iter().any(|s| s.flags & 255 == 0x13) {
            flags |= 0x800000;
        } // MH_HAS_TLV_DESCRIPTORS
        for n in [
            0xfeedfacf,
            0x100000c,
            0,
            2,
            (segments.len() + 7 + self.options.dylibs.len()) as u32,
            commands.len() as u32,
            flags,
            0,
        ] {
            u32le(&mut bytes, n);
        }
        bytes.extend_from_slice(&commands);
        // Serialize and hash in file order. Padding is emitted in bounded
        // chunks, so file output never allocates another complete image.
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let mut position = 0;
        let mut write = |data: &[u8]| -> Result<()> {
            output.write_all(data).map_err(|e| e.to_string())?;
            hash.update(data);
            position += data.len();
            Ok(())
        };
        write(&bytes)?;
        let mut sections: Vec<_> = self
            .outputs
            .iter()
            .filter(|s| !matches!(s.flags & 255, 1 | 0x12) && !s.data.is_empty())
            .collect();
        sections.sort_by_key(|s| s.file);
        // Keep padding bounded independently of section or image size.
        let zeros = [0u8; 16384];
        let mut at = bytes.len();
        for s in sections {
            if s.file < at {
                return fail("overlapping output sections");
            }
            let mut gap = s.file - at;
            while gap != 0 {
                let count = gap.min(zeros.len());
                write(&zeros[..count])?;
                gap -= count;
            }
            write(&s.data)?;
            at = s.file + s.data.len();
        }
        if at > file {
            return fail("section exceeds output layout");
        }
        let mut gap = file - at;
        while gap != 0 {
            let count = gap.min(zeros.len());
            write(&zeros[..count])?;
            gap -= count;
        }
        write(&linkedit)?;
        let digest = hash.finalize();
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&digest[..16]);
        uuid[6] = (uuid[6] & 15) | 0x80; // UUID version 8
        uuid[8] = (uuid[8] & 63) | 0x80;
        output
            .seek(std::io::SeekFrom::Start(uuid_offset as u64))
            .map_err(|e| e.to_string())?;
        output.write_all(&uuid).map_err(|e| e.to_string())?;
        output
            .seek(std::io::SeekFrom::Start(position as u64))
            .map_err(|e| e.to_string())?;
        Ok(LinkArtifacts {
            debug_objects,
            warnings: self.warnings,
        })
    }

    /// Darwin's debug map relocates object DWARF via N_OSO/N_FUN/N_GSYM
    /// records. Retain exact input snapshots, including archive members, so the
    /// reusable byte-oriented API never depends on diagnostic input names.
    fn debug_map(
        &mut self,
        symbols: &mut Vec<u8>,
        strings: &mut Vec<u8>,
    ) -> Result<Vec<DebugObject>> {
        let mut debug_objects = Vec::new();
        if let Some(directory) = &self.options.debug_directory {
            for oi in 0..self.objects.len() {
                let object = &self.objects[oi];
                let Some(bytes) = &object.debug_bytes else {
                    continue;
                };
                use sha2::{Digest, Sha256};
                let hash: String = Sha256::digest(bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let path = directory.join(format!("object-{oi:05}-{hash}.o"));
                nlist(symbols, strings, &object.name, 0x64, 0, 0, 0); // N_SO
                nlist(symbols, strings, path.to_str().unwrap(), 0x66, 0, 1, 0); // N_OSO; zero timestamp means no staleness check
                for (si, symbol) in object.symbols.iter().enumerate() {
                    let common =
                        symbol.kind & 0xe == 0 && symbol.kind & 1 != 0 && symbol.value != 0;
                    if common && self.globals.contains_key(&symbol.name) {
                        let target = self.symbol_target(oi, si)?;
                        let ordinal = match target {
                            Target::Common(_) => self.outputs[self.common_section].ordinal,
                            Target::Definition(o, sym) => {
                                let definition = &self.objects[o].symbols[sym];
                                if definition.section == 0 {
                                    0
                                } else {
                                    let section = &self.objects[o].sections[definition.section - 1];
                                    self.outputs[section
                                        .output
                                        .ok_or("debug common resolves to discarded section")?]
                                    .ordinal
                                }
                            }
                            Target::Header => 1,
                            Target::SectionBoundary(id, _) => self.outputs[id].ordinal,
                            Target::Import(_) => return fail("debug common resolves to an import"),
                        };
                        nlist(
                            symbols,
                            strings,
                            &symbol.name,
                            0x20,
                            ordinal,
                            0,
                            self.address(target)?,
                        );
                        continue;
                    }
                    if symbol.kind & 0xe != 0xe
                        || symbol.name.starts_with("ltmp")
                        || symbol.name.starts_with('L')
                    {
                        continue;
                    }
                    if symbol.kind & 1 != 0
                        && !matches!(self.globals.get(&symbol.name), Some(Target::Definition(o,s)) if *o == oi && *s == si)
                    {
                        continue;
                    }
                    let section = &object.sections[symbol.section - 1];
                    let Some(out) = section.output else {
                        continue;
                    };
                    let code = section.flags & 0x80000400 != 0;
                    nlist(
                        symbols,
                        strings,
                        &symbol.name,
                        if code {
                            0x24
                        } else if symbol.kind & 1 != 0 {
                            0x20
                        } else {
                            0x26
                        },
                        self.outputs[out].ordinal,
                        0,
                        self.symbol_address(oi, symbol)?,
                    );
                    if code {
                        let end = object
                            .symbols
                            .iter()
                            .filter(|s| {
                                s.section == symbol.section
                                    && s.value > symbol.value
                                    && !s.name.starts_with("ltmp")
                                    && !s.name.starts_with('L')
                            })
                            .map(|s| s.value)
                            .min()
                            .unwrap_or(section.address + section.size as u64);
                        nlist(symbols, strings, "", 0x24, 0, 0, end - symbol.value);
                    }
                }
                nlist(symbols, strings, "", 0x64, 1, 0, 0);
                debug_objects.push(DebugObject {
                    path,
                    bytes: self.objects[oi].debug_bytes.take().unwrap(),
                });
            }
        }
        Ok(debug_objects)
    }
    pub(crate) fn reloc_target(&self, oi: usize, r: &Reloc) -> Result<(Option<Target>, u64)> {
        if r.external {
            let t = self.symbol_target(oi, r.symbol)?;
            Ok((
                Some(t),
                if matches!(t, Target::Import(_)) {
                    0
                } else {
                    self.address(t)?
                },
            ))
        } else {
            let s = self.objects[oi]
                .sections
                .get(r.symbol.wrapping_sub(1))
                .ok_or("relocation section ordinal out of range")?;
            let out = &self.outputs[s.output.ok_or("reference to discarded section")?];
            Ok((None, out.address + s.offset as u64))
        }
    }
    fn layout(&mut self) -> Result<(Vec<Segment>, usize, u64)> {
        let mut segments = vec![Segment {
            name: "__PAGEZERO",
            address: 0,
            vm: BASE as usize,
            file: 0,
            size: 0,
            prot: 0,
            sections: vec![],
        }];
        let mut file = 0;
        let mut address = BASE;
        let mut ordinal = 1u16;
        for (seg, prot) in [("__TEXT", 5), ("__DATA", 3)] {
            let mut ids: Vec<_> = self
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, s)| s.segment == seg)
                .map(|(id, _)| id)
                .collect();
            ids.sort_by_key(|&id| {
                (
                    match self.outputs[id].flags & 255 {
                        0x11 => 1,
                        0x12 => 2,
                        1 => 3,
                        _ => 0,
                    },
                    id,
                )
            });
            let start = file;
            let vmstart = address;
            if seg == "__TEXT" {
                file = PAGE;
                address += PAGE as u64;
            }
            let mut file_end = file;
            let tls_alignment = ids
                .iter()
                .filter(|&&id| matches!(self.outputs[id].flags & 255, 0x11 | 0x12))
                .map(|&id| self.outputs[id].alignment)
                .max()
                .unwrap_or(0);
            let mut tls_started = false;
            for &id in &ids {
                let s = &mut self.outputs[id];
                if !tls_started && matches!(s.flags & 255, 0x11 | 0x12) {
                    // dyld uses offsets into a malloc-backed TLS template.
                    // Preserve section alignment relative to the template base,
                    // not only relative to the executable's image base.
                    s.alignment = s.alignment.max(tls_alignment);
                    tls_started = true;
                }
                let a = 1 << s.alignment;
                // Segment starts are at least 16 KiB aligned, and all section
                // alignments above that are handled in VM and file together.
                let pad = align(address as usize, a) - address as usize;
                address += pad as u64;
                s.address = address;
                s.ordinal = ordinal as u8;
                ordinal += 1;
                if !matches!(s.flags & 255, 1 | 0x12) {
                    file += pad;
                    s.file = file;
                    file += s.size;
                    file_end = file;
                }
                address += s.size as u64;
            }
            file = align(file_end, PAGE);
            address = align(address as usize, PAGE) as u64;
            segments.push(Segment {
                name: seg,
                address: vmstart,
                vm: (address - vmstart) as usize,
                file: start,
                size: file - start,
                prot,
                sections: ids,
            });
        }
        if ordinal > 256 {
            return fail("too many executable sections");
        }
        if file > 512 * 1024 * 1024 {
            return fail("basic executable exceeds 512 MiB file limit");
        }
        Ok((segments, file, address))
    }
    fn relocate(&mut self) -> Result<()> {
        for oi in 0..self.objects.len() {
            for si in 0..self.objects[oi].sections.len() {
                if self.discarded.contains(&(oi, si))
                    || self.objects[oi].sections[si].segment == "__DWARF"
                {
                    continue;
                }
                self.relocate_section(oi, si).map_err(|e| {
                    format!(
                        "{}: {},{}: {e}",
                        self.objects[oi].name,
                        self.objects[oi].sections[si].segment,
                        self.objects[oi].sections[si].name
                    )
                })?;
            }
        }
        Ok(())
    }
    pub(crate) fn relocate_section(&mut self, oi: usize, si: usize) -> Result<()> {
        // Relocation consumes this section's payload. Symbol resolution uses
        // section addresses and sizes, so it does not need the resident bytes.
        let mut data = std::mem::take(&mut self.objects[oi].sections[si].data);
        let s = &self.objects[oi].sections[si];
        let metadata = s.name == "__compact_unwind" && s.segment == "__LD";
        let mut i = 0;
        let mut patched = RelocationRanges::default();
        while i < s.relocs.len() {
            let mut r = s.relocs[i].clone();
            i += 1;
            let mut extra = 0i64;
            if r.kind == 10 {
                if r.external || r.pc || r.width != 4 {
                    return fail("invalid ARM64_RELOC_ADDEND");
                }
                extra = ((r.symbol as i32) << 8 >> 8) as i64;
                let next = s.relocs.get(i).ok_or("unpaired ADDEND")?;
                if next.offset != r.offset || !matches!(next.kind, 2..=4) {
                    return fail("ADDEND must precede matching branch/page relocation");
                }
                r = next.clone();
                i += 1;
            }
            let location = if metadata {
                0
            } else {
                self.outputs[s.output.ok_or("missing output section")?].address
                    + s.offset as u64
                    + r.offset as u64
            };
            let (target, mut value) = self.reloc_target(oi, &r)?;
            let mut difference = false;
            if r.kind == 1 {
                let unsigned = s.relocs.get(i).ok_or("unpaired SUBTRACTOR")?;
                if !r.external
                    || r.pc
                    || unsigned.kind != 0
                    || unsigned.offset != r.offset
                    || unsigned.width != r.width
                    || unsigned.pc
                {
                    return fail("invalid SUBTRACTOR/UNSIGNED pair");
                }
                let (positive_target, positive) = self.reloc_target(oi, unsigned)?;
                if matches!(positive_target, Some(Target::Import(_))) {
                    return fail("cannot form a static difference with an imported symbol");
                }
                if matches!(target, Some(Target::Import(_))) {
                    return fail("cannot subtract an imported symbol");
                }
                value = positive.wrapping_sub(value);
                r.kind = 0;
                i += 1;
                difference = true;
            }
            if !patched.insert(r.offset, r.offset + r.width) {
                return fail("overlapping relocation locations");
            }
            match r.kind {
                0 => {
                    if r.pc || !matches!(r.width, 4 | 8) || (!difference && r.width != 8) {
                        return fail("unsupported UNSIGNED relocation width/flags");
                    }
                    let raw = if r.width == 8 {
                        Reader(&data).u64(r.offset)?
                    } else {
                        Reader(&data).u32(r.offset)? as i32 as i64 as u64
                    };
                    if !r.external && !difference {
                        value = value.wrapping_sub(self.objects[oi].sections[r.symbol - 1].address);
                    }
                    value = value.wrapping_add(raw);
                    // The third word of a TLV descriptor is an offset in the
                    // image's contiguous TLS template, never an ASLR pointer.
                    let tls_offset = s.flags & 255 == 0x13 && r.offset % 24 == 16;
                    if tls_offset {
                        let template = self
                            .outputs
                            .iter()
                            .filter(|s| matches!(s.flags & 255, 0x11 | 0x12))
                            .map(|s| s.address)
                            .min()
                            .ok_or("TLV descriptor without template")?;
                        let target_section = if r.external {
                            match target {
                                Some(Target::Definition(o, sym)) => self.objects[o]
                                    .sections
                                    .get(self.objects[o].symbols[sym].section.wrapping_sub(1))
                                    .ok_or("TLV template needs a section definition")?,
                                _ => return fail("TLV template must be a local definition"),
                            }
                        } else {
                            &self.objects[oi].sections[r.symbol - 1]
                        };
                        if !matches!(target_section.flags & 255, 0x11 | 0x12) {
                            return fail("TLV offset must reference TLS template data");
                        }
                        value = value
                            .checked_sub(template)
                            .ok_or("TLV offset precedes template")?;
                    }
                    if !metadata && !difference && !tls_offset {
                        if let Some(Target::Import(import)) = target {
                            self.binds.insert(location, (import, raw as i64));
                            value = 0;
                        } else {
                            let absolute = matches!(target,Some(Target::Definition(o,s)) if self.objects[o].symbols[s].kind&0xe==2);
                            if !absolute {
                                if self.outputs[s.output.unwrap()].segment == "__TEXT" {
                                    return fail(
                                        "absolute pointer relocation in read-only __TEXT requires the system linker",
                                    );
                                }
                                self.rebases.insert(location);
                            }
                        }
                    }
                    if !metadata && !difference && !tls_offset && r.external {
                        let name = &self.objects[oi].symbols[r.symbol].name;
                        if target.is_some_and(|t| self.coalesces(t, name)) {
                            self.weak_binds.insert(location, (name.clone(), raw as i64));
                        }
                    }
                    if r.width == 4
                        && !((value as i64) >= i32::MIN as i64 && (value as i64) <= u32::MAX as i64)
                    {
                        return fail("32-bit difference relocation overflow");
                    }
                    data[r.offset..r.offset + r.width]
                        .copy_from_slice(&value.to_le_bytes()[..r.width]);
                }
                2..=9 => {
                    if !r.external || r.width != 4 {
                        return fail(
                            "instruction/GOT relocations require external symbol indexes and width 4",
                        );
                    }
                    if matches!(r.kind, 8 | 9)
                        && let Some(Target::Definition(o, sym)) = target
                    {
                        let definition = &self.objects[o].symbols[sym];
                        if self.objects[o]
                            .sections
                            .get(definition.section.wrapping_sub(1))
                            .is_none_or(|s| s.flags & 255 != 0x13)
                        {
                            return fail("TLVP relocation must reference a TLS descriptor");
                        }
                    }
                    let symbol = &self.objects[oi].symbols[r.symbol].name;
                    if matches!(r.kind, 2 | 3 | 5 | 7 | 8) != r.pc {
                        return fail("invalid PC-relative relocation flag");
                    }
                    let word = Reader(&data).u32(r.offset)?;
                    if matches!(r.kind, 5..=9) {
                        value = self.outputs[self.got_section].address
                            + *self
                                .got
                                .get(&self.got_key(oi, r.symbol)?)
                                .ok_or("missing GOT slot")? as u64
                                * 8;
                    } else if matches!(target, Some(Target::Import(_)))
                        || target.is_some_and(|t| self.coalesces(t, symbol))
                    {
                        if r.kind != 2 {
                            return fail(
                                "direct page reference to imported/coalesced symbol requires GOT relocation",
                            );
                        }
                        value = self.outputs[self.stub_section].address
                            + self.stubs[symbol] as u64 * 12;
                    }
                    let patched = match r.kind {
                        2 => {
                            if word & 0x7c000000 != 0x14000000 {
                                return fail("BRANCH26 does not address B/BL");
                            }
                            let encoded = (((word & 0x3ffffff) as i32) << 6 >> 4) as i64;
                            let delta = value as i64 + extra + encoded - location as i64;
                            if delta % 4 != 0 || !(-(1 << 27)..(1 << 27)).contains(&delta) {
                                return fail("BRANCH26 out of range (no branch islands yet)");
                            }
                            (word & 0xfc000000) | ((delta >> 2) as u32 & 0x3ffffff)
                        }
                        3 | 5 | 8 => {
                            if word & 0x9f000000 != 0x90000000 {
                                return fail("PAGE21 does not address ADRP");
                            }
                            let imm = ((word >> 29) & 3) | ((word >> 3) & 0x1ffffc);
                            let encoded = ((imm as i32) << 11 >> 11) as i64 * 4096;
                            let page = (((value as i64 + extra) & !4095)
                                - (location as i64 & !4095))
                                / 4096
                                + encoded / 4096;
                            if !(-(1 << 20)..(1 << 20)).contains(&page) {
                                return fail("ADRP out of range");
                            }
                            (word & !0x60ffffe0)
                                | (((page as u32) & 3) << 29)
                                | (((page as u32) & 0x1ffffc) << 3)
                        }
                        4 | 6 | 9 => {
                            let scale = if word & 0x3b000000 == 0x39000000 {
                                if word & 0x04800000 == 0x04800000 {
                                    4
                                } else {
                                    word >> 30
                                }
                            } else if word & 0x7f400000 == 0x11000000 {
                                0
                            } else {
                                return fail(
                                    "PAGEOFF12 requires unshifted ADD or unsigned-offset load/store",
                                );
                            };
                            let old = ((word >> 10) & 4095) << scale;
                            let off = ((value as i64 + extra + old as i64) as u64 & 4095) as u32;
                            if off & ((1 << scale) - 1) != 0 {
                                return fail("unaligned PAGEOFF12 target");
                            }
                            (word & !0x003ffc00) | ((off >> scale) << 10)
                        }
                        7 => {
                            // POINTER_TO_GOT has no embedded addend. Apple assemblers
                            // may leave nonzero placeholder words in LSDA/CIE data.
                            let delta = value as i64 - location as i64;
                            if i32::try_from(delta).is_err() {
                                return fail("POINTER_TO_GOT overflow");
                            }
                            delta as u32
                        }
                        _ => unreachable!(),
                    };
                    data[r.offset..r.offset + 4].copy_from_slice(&patched.to_le_bytes());
                }
                _ => {
                    return fail(format!(
                        "unsupported arm64 relocation type {} at {:#x} (unsupported TLS/authentication form)",
                        r.kind, r.offset
                    ));
                }
            }
        }
        if let Some(id) = s.output.filter(|_| !data.is_empty()) {
            self.outputs[id].data[s.offset..s.offset + data.len()].copy_from_slice(&data);
        }
        // Only the unwind builder reads relocated input payloads afterward.
        // All other bytes are now in their merged output sections.
        if metadata || s.name == "__eh_frame" {
            self.objects[oi].sections[si].data = data;
        }
        Ok(())
    }
    fn make_got_stubs(&mut self) -> Result<()> {
        for (symbol, &index) in &self.got {
            let addr = self.outputs[self.got_section].address + index as u64 * 8;
            let target = self.got_targets[symbol];
            let value = match target {
                Target::Import(i) => {
                    self.binds.insert(addr, (i, 0));
                    0
                }
                _ => {
                    if !matches!(target,Target::Definition(o,s) if self.objects[o].symbols[s].kind&0xe==2)
                    {
                        self.rebases.insert(addr);
                    }
                    self.address(target)?
                }
            };
            if self.coalesces(target, symbol) {
                self.weak_binds.insert(addr, (symbol.clone(), 0));
            }
            self.outputs[self.got_section].data[index * 8..index * 8 + 8]
                .copy_from_slice(&value.to_le_bytes());
        }
        for (symbol, &index) in &self.stubs {
            let addr = self.outputs[self.stub_section].address + index as u64 * 12;
            let got = self.outputs[self.got_section].address + self.got[symbol] as u64 * 8;
            let page = ((got & !4095) as i64 - (addr & !4095) as i64) / 4096;
            if !(-(1 << 20)..(1 << 20)).contains(&page) {
                return fail("stub ADRP out of range");
            }
            let adrp = 0x90000010 | (((page as u32) & 3) << 29) | (((page as u32) & 0x1ffffc) << 3);
            let ldr = 0xf9400210 | (((got as u32 & 4095) / 8) << 10);
            let out = &mut self.outputs[self.stub_section].data;
            for (i, w) in [adrp, ldr, 0xd61f0200].into_iter().enumerate() {
                out[index * 12 + i * 4..index * 12 + i * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
        }
        Ok(())
    }
    fn exports(&self) -> Result<Vec<u8>> {
        #[derive(Default)]
        struct Node {
            edges: BTreeMap<u8, usize>,
            terminal: Vec<u8>,
        }
        let mut nodes = vec![Node::default()];
        for (symbol, &target) in &self.globals {
            if matches!(target, Target::Import(_) | Target::SectionBoundary(_, _)) {
                continue;
            }
            if matches!(target, Target::Common(_)) && self.private_commons.contains(symbol) {
                continue;
            }
            if let Target::Definition(o, s) = target
                && self.objects[o].symbols[s].kind & 0x10 != 0
            {
                continue;
            }
            let absolute =
                matches!(target,Target::Definition(o,s) if self.objects[o].symbols[s].kind&0xe==2);
            let mut term = Vec::new();
            let tls = matches!(target, Target::Definition(o, sym) if self.objects[o].symbols[sym].section != 0 && self.objects[o].sections[self.objects[o].symbols[sym].section - 1].flags & 255 == 0x13);
            let weak = matches!(target, Target::Definition(o,s) if self.objects[o].symbols[s].desc & 0x80 != 0);
            uleb(
                &mut term,
                (if absolute {
                    2
                } else if tls {
                    1
                } else {
                    0
                }) | if weak { 4 } else { 0 },
            );
            uleb(
                &mut term,
                self.address(target)? - if absolute { 0 } else { BASE },
            );
            let mut at = 0;
            for b in symbol.bytes() {
                let next = if let Some(&id) = nodes[at].edges.get(&b) {
                    id
                } else {
                    let id = nodes.len();
                    nodes.push(Node::default());
                    nodes[at].edges.insert(b, id);
                    id
                };
                at = next;
            }
            nodes[at].terminal = term;
        }
        let mut offsets = vec![0; nodes.len()];
        loop {
            let mut out = Vec::new();
            let mut new = Vec::new();
            for n in &nodes {
                new.push(out.len());
                uleb(&mut out, n.terminal.len() as u64);
                out.extend_from_slice(&n.terminal);
                out.push(n.edges.len() as u8);
                for (&b, &id) in &n.edges {
                    out.extend_from_slice(&[b, 0]);
                    uleb(&mut out, offsets[id] as u64);
                }
            }
            if new == offsets {
                return Ok(out);
            }
            offsets = new;
        }
    }
}
fn segment_offset(segments: &[Segment], addr: u64) -> Result<(u8, u64)> {
    segments
        .iter()
        .enumerate()
        .find(|(_, s)| s.prot & 2 != 0 && addr >= s.address && addr < s.address + s.vm as u64)
        .map(|(i, s)| (i as u8, addr - s.address))
        .ok_or_else(|| "dyld fixup outside writable segment".into())
}
fn nlist(
    out: &mut Vec<u8>,
    strings: &mut Vec<u8>,
    symbol: &str,
    kind: u8,
    section: u8,
    desc: u16,
    value: u64,
) {
    u32le(out, strings.len() as u32);
    out.extend_from_slice(&[kind, section]);
    out.extend_from_slice(&desc.to_le_bytes());
    u64le(out, value);
    strings.extend_from_slice(symbol.as_bytes());
    strings.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_intervals_match_byte_oracle_in_arbitrary_order() {
        let choices: Vec<_> = (0..5)
            .flat_map(|at| [0, 1, 2, 4, 8].map(|width| (at, at + width)))
            .collect();
        for a in &choices {
            for b in &choices {
                for c in &choices {
                    let mut ranges = RelocationRanges::default();
                    let mut bytes = BTreeSet::new();
                    for &(start, end) in [a, b, c] {
                        let expected = (start..end).all(|byte| bytes.insert(byte));
                        assert_eq!(ranges.insert(start, end), expected, "{a:?}, {b:?}, {c:?}");
                        if !expected {
                            break;
                        }
                    }
                }
            }
        }
        for offsets in [
            vec![0, 8, 16, 24, 32],
            vec![32, 24, 16, 8, 0],
            vec![0, 32, 8, 24, 16],
            vec![32, 0, 16, 8, 24],
        ] {
            let mut ranges = RelocationRanges::default();
            for start in offsets {
                assert!(ranges.insert(start, start + 8));
            }
            for (start, end) in [(0, 1), (7, 9), (16, 24), (39, 41)] {
                assert!(!ranges.insert(start, end));
            }
            assert!(ranges.insert(40, 48));
        }
    }

    #[test]
    fn overlap_checks_follow_pair_validation_and_precede_width_checks() {
        let source = ".text\n.globl _main\n_main:\nret\n.data\n.quad _main\n.quad _main\n";
        let bytes = keel_asm::assemble_object(source).unwrap();
        let make = || {
            prepare_linker(
                &[Input {
                    name: "pairs.o",
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
            .unwrap()
        };
        let l = make();
        let si = l.objects[0]
            .sections
            .iter()
            .position(|s| s.name == "__data")
            .unwrap();
        let mut pointer = l.objects[0].sections[si].relocs[0].clone();
        pointer.offset = 0;
        let check = |relocs, expected| {
            let mut l = make();
            l.objects[0].sections[si].relocs = relocs;
            assert_eq!(l.relocate_section(0, si).err().unwrap(), expected);
        };
        let mut narrow = pointer.clone();
        narrow.width = 4;
        check(
            vec![pointer.clone(), narrow.clone()],
            "overlapping relocation locations",
        );
        narrow.offset = 8;
        check(
            vec![pointer.clone(), narrow],
            "unsupported UNSIGNED relocation width/flags",
        );
        let mut subtractor = pointer.clone();
        subtractor.kind = 1;
        check(
            vec![pointer.clone(), subtractor.clone()],
            "unpaired SUBTRACTOR",
        );
        check(
            vec![pointer.clone(), subtractor.clone(), pointer.clone()],
            "overlapping relocation locations",
        );
        let mut bad_unsigned = pointer.clone();
        bad_unsigned.pc = true;
        check(
            vec![pointer.clone(), subtractor, bad_unsigned],
            "invalid SUBTRACTOR/UNSIGNED pair",
        );
        let mut addend = pointer.clone();
        addend.kind = 10;
        addend.symbol = 0;
        addend.external = false;
        addend.width = 4;
        check(vec![pointer.clone(), addend.clone()], "unpaired ADDEND");
        let mut branch = pointer.clone();
        branch.kind = 2;
        branch.pc = true;
        branch.width = 4;
        check(
            vec![pointer.clone(), addend.clone(), branch.clone()],
            "overlapping relocation locations",
        );
        addend.width = 8;
        check(vec![pointer, addend, branch], "invalid ARM64_RELOC_ADDEND");
    }
}
