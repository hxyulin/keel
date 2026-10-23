//! Collection over original contribution IDs, before output sections exist.
use crate::{
    read::{Reader, Reloc},
    *,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    Section(usize, usize),
    Record(usize, usize, usize),
    Common(String),
}
#[derive(Clone)]
struct Record {
    start: usize,
    end: usize,
    cie: Option<usize>,
}
struct Graph<'a> {
    linker: &'a Linker,
    records: BTreeMap<(usize, usize), Vec<Record>>,
    owners: BTreeMap<Node, Vec<Node>>,
    demanded: BTreeSet<(usize, usize)>,
}
impl Graph<'_> {
    fn at(&self, oi: usize, si: usize, offset: usize) -> Result<Node> {
        if let Some(records) = self.records.get(&(oi, si)) {
            let record = records
                .iter()
                .find(|r| offset >= r.start && offset < r.end)
                .ok_or("dead_strip: reference outside an unwind record")?;
            Ok(Node::Record(oi, si, record.start))
        } else {
            Ok(Node::Section(oi, si))
        }
    }
    fn symbol(&mut self, oi: usize, si: usize) -> Result<Option<Node>> {
        let s = self.linker.objects[oi]
            .symbols
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        self.demanded.insert((oi, si));
        let (oi, si) = if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            match self.linker.globals.get(&s.name) {
                Some(Target::Definition(o, s)) => (*o, *s),
                Some(Target::Header) => return Ok(None),
                None if self
                    .linker
                    .objects
                    .iter()
                    .flat_map(|o| &o.symbols)
                    .any(|c| c.name == s.name && c.kind & 0xe == 0 && c.value != 0) =>
                {
                    return Ok(Some(Node::Common(s.name.clone())));
                }
                None => return Ok(None), // imports and synthesized boundaries resolve after GC
                _ => return fail("dead_strip: unexpected pre-layout symbol target"),
            }
        } else {
            (oi, si)
        };
        let s = &self.linker.objects[oi].symbols[si];
        if s.kind & 0xe == 2 {
            return Ok(None);
        }
        let section = &self.linker.objects[oi].sections[s.section - 1];
        Ok(Some(self.at(
            oi,
            s.section - 1,
            (s.value - section.address) as usize,
        )?))
    }
    fn target(&mut self, oi: usize, r: &Reloc, data: &[u8], pointer: bool) -> Result<Option<Node>> {
        if r.external {
            return self.symbol(oi, r.symbol);
        }
        let si = r
            .symbol
            .checked_sub(1)
            .ok_or("relocation section ordinal out of range")?;
        let section = self.linker.objects[oi]
            .sections
            .get(si)
            .ok_or("relocation section ordinal out of range")?;
        // Local pointer payloads carry original object addresses, not symbol IDs.
        // At section granularity instruction addends cannot change the target node.
        let offset = if pointer {
            Reader(data)
                .u64(r.offset)?
                .checked_sub(section.address)
                .ok_or("dead_strip: local pointer precedes its input section")? as usize
        } else {
            0
        };
        if self.records.contains_key(&(oi, si)) {
            return fail(
                "dead_strip: local section relocation into unwind metadata is unsupported; use explicit symbol differences",
            );
        }
        if pointer && offset > section.size {
            return fail("dead_strip: local pointer outside its input section");
        }
        Ok(Some(self.at(oi, si, offset)?))
    }
    /// Decode prefix/pair records before interpreting symbol fields. ADDEND's
    /// 24-bit symbol field is a signed number, never an edge.
    fn edges(&mut self, oi: usize, si: usize, begin: usize, end: usize) -> Result<Vec<Node>> {
        let s = &self.linker.objects[oi].sections[si];
        let mut edges = Vec::new();
        let mut i = 0;
        while i < s.relocs.len() {
            let mut r = &s.relocs[i];
            i += 1;
            if r.offset < begin || r.offset >= end {
                continue;
            }
            if r.kind == 10 {
                if r.external || r.pc || r.width != 4 {
                    return fail("invalid ARM64_RELOC_ADDEND");
                }
                let next = s.relocs.get(i).ok_or("unpaired ADDEND")?;
                if next.offset != r.offset || !matches!(next.kind, 2..=4) {
                    return fail("ADDEND must precede matching branch/page relocation");
                }
                r = next;
                i += 1;
            }
            if r.kind == 1 {
                let positive = s.relocs.get(i).ok_or("unpaired SUBTRACTOR")?;
                if !r.external
                    || r.pc
                    || positive.kind != 0
                    || positive.offset != r.offset
                    || positive.width != r.width
                    || positive.pc
                    || !matches!(r.width, 4 | 8)
                {
                    return fail("invalid SUBTRACTOR/UNSIGNED pair");
                }
                // Native FDE differences use a local metadata anchor. It is
                // arithmetic, not a liveness dependency on the anchor's record.
                let anchor = self.linker.objects[oi]
                    .symbols
                    .get(r.symbol)
                    .ok_or("invalid subtractor symbol")?;
                if !(self.records.contains_key(&(oi, si))
                    && anchor.kind & 0xe == 0xe
                    && anchor.kind & 1 == 0
                    && anchor.section == si + 1)
                {
                    if let Some(n) = self.target(oi, r, &s.data, false)? {
                        edges.push(n);
                    }
                }
                if let Some(n) = self.target(oi, positive, &s.data, false)? {
                    edges.push(n);
                }
                i += 1;
            } else {
                if r.kind > 9 {
                    return fail(format!(
                        "dead_strip: unsupported ARM64 relocation {}",
                        r.kind
                    ));
                }
                if let Some(n) = self.target(oi, r, &s.data, r.kind == 0 && r.width == 8)? {
                    edges.push(n);
                }
            }
        }
        Ok(edges)
    }
    fn owner(&mut self, oi: usize, si: usize, field: usize, fde: bool) -> Result<Option<Node>> {
        let s = &self.linker.objects[oi].sections[si];
        let matches: Vec<_> = s.relocs.iter().filter(|r| r.offset == field).collect();
        let rel = if fde {
            if matches.len() != 2
                || matches[0].kind != 1
                || matches[1].kind != 0
                || !matches[1].external
                || !matches[0].external
                || matches[0].pc
                || matches[1].pc
                || matches[0].width != 8
                || matches[1].width != 8
            {
                return fail(
                    "dead_strip: implicit/foreign eh_frame FDE ownership requires native-width SUBTRACTOR/UNSIGNED fixups",
                );
            }
            // The native pcrel field may subtract its own label or a section
            // anchor with an offset folded into the payload. Rebuild remaps both.
            let negative = self.linker.objects[oi]
                .symbols
                .get(matches[0].symbol)
                .ok_or("invalid FDE subtractor symbol")?;
            if negative.kind & 0xe != 0xe || negative.kind & 1 != 0 || negative.section != si + 1 {
                return fail(
                    "dead_strip: FDE subtractor must be a local anchor in its own section",
                );
            }
            matches[1]
        } else {
            if matches.len() != 1 || matches[0].kind != 0 || matches[0].pc || matches[0].width != 8
            {
                return fail(
                    "dead_strip: compact unwind ownership requires one UNSIGNED function pointer",
                );
            }
            matches[0]
        };
        if rel.external {
            let symbol = self.linker.objects[oi]
                .symbols
                .get(rel.symbol)
                .ok_or("invalid unwind symbol")?;
            if symbol.kind & 0xe == 0xe
                && symbol.kind & 1 != 0
                && !matches!(self.linker.globals.get(&symbol.name), Some(Target::Definition(o, s)) if *o == oi && *s == rel.symbol)
            {
                return Ok(None); // metadata for a coalesced-away weak definition
            }
        }
        let owner = self
            .target(oi, rel, &s.data, !fde && !rel.external)?
            .ok_or("dead_strip: unwind owner is not an input definition")?;
        if let Node::Section(o, sec) = owner {
            let code = &self.linker.objects[o].sections[sec];
            if code.segment == "__TEXT"
                && code.flags & 0x80000400 != 0
                && !matches!(code.flags & 255, 1 | 0x12)
            {
                let raw = Reader(&s.data).u64(field)?;
                let start = if rel.external {
                    let original = &self.linker.objects[oi].symbols[rel.symbol];
                    let sym = if original.kind & 1 != 0 || original.kind & 0xe == 0 {
                        let Some(Target::Definition(o, id)) =
                            self.linker.globals.get(&original.name)
                        else {
                            return fail("dead_strip: unwind owner is not an input definition");
                        };
                        &self.linker.objects[*o].symbols[*id]
                    } else {
                        original
                    };
                    let raw = if fde {
                        let negative = &self.linker.objects[oi].symbols[matches[0].symbol];
                        raw.wrapping_add(s.address + field as u64)
                            .wrapping_sub(negative.value)
                    } else {
                        raw
                    };
                    sym.value.wrapping_add(raw).checked_sub(code.address)
                } else {
                    raw.checked_sub(code.address)
                }
                .ok_or("dead_strip: unwind function precedes owner section")?;
                let aliases: Vec<_> = self.linker.objects[o]
                    .symbols
                    .iter()
                    .enumerate()
                    .filter(|(_, sym)| {
                        sym.section == sec + 1
                            && sym.value == code.address + start
                            && sym.kind & 1 != 0
                    })
                    .collect();
                if !aliases.is_empty() && aliases.iter().all(|(id, sym)| !matches!(self.linker.globals.get(&sym.name), Some(Target::Definition(winner, s)) if *winner == o && *s == *id)) {
                    return Ok(None);
                }
                let length = if fde {
                    Reader(&s.data).u64(field + 8)?
                } else {
                    Reader(&s.data).u32(field + 8)? as u64
                };
                if length == 0
                    || start
                        .checked_add(length)
                        .is_none_or(|end| end > code.size as u64)
                {
                    return fail(
                        "dead_strip: unwind function range crosses its input section contribution",
                    );
                }
                return Ok(Some(owner));
            }
        }
        fail("dead_strip: unwind owner must be file-backed executable __TEXT")
    }
}
impl Linker {
    pub(crate) fn collect(&mut self) -> Result<BTreeSet<String>> {
        let mut g = Graph {
            linker: self,
            records: BTreeMap::new(),
            owners: BTreeMap::new(),
            demanded: BTreeSet::new(),
        };
        // Parse record extents first, so all symbol references can be mapped to
        // records even when a CIE or an FDE is shared or appears out of order.
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, s) in o.sections.iter().enumerate() {
                let compact = s.segment == "__LD" && s.name == "__compact_unwind";
                if !compact && s.name != "__eh_frame" {
                    continue;
                }
                if o.symbols
                    .iter()
                    .any(|sym| sym.section == si + 1 && sym.kind & 1 != 0)
                {
                    return fail("dead_strip: external symbols in unwind metadata are unsupported");
                }
                let mut records = Vec::new();
                let mut at = 0;
                while at < s.data.len() {
                    let (end, cie) = if compact {
                        (at + 32, None)
                    } else {
                        let length = Reader(&s.data).u32(at)? as usize;
                        if length == 0 {
                            at += 4;
                            continue;
                        }
                        if length == 0xffffffff || length < 4 {
                            return fail("dead_strip: unsupported/malformed DWARF FDE length");
                        }
                        let ptr = Reader(&s.data).u32(at + 4)? as usize;
                        if ptr != 0 && length < 21 {
                            return fail("dead_strip: truncated native-width FDE");
                        }
                        (
                            at.checked_add(4 + length).ok_or("unwind size overflow")?,
                            if ptr == 0 {
                                None
                            } else {
                                Some((at + 4).checked_sub(ptr).ok_or("invalid FDE CIE pointer")?)
                            },
                        )
                    };
                    if end > s.data.len() {
                        return fail("dead_strip: truncated unwind record");
                    }
                    records.push(Record {
                        start: at,
                        end,
                        cie,
                    });
                    at = end;
                }
                // A fixup belongs to one original record in its entirety.
                // Otherwise dropping its starting record can silently remove
                // a write into a surviving neighbor (or reinterpret it after
                // compaction). Validate dead records too, before pruning.
                for rel in &s.relocs {
                    let record = records
                        .iter()
                        .find(|r| rel.offset >= r.start && rel.offset < r.end)
                        .ok_or("dead_strip: relocation outside an unwind record")?;
                    if rel
                        .offset
                        .checked_add(rel.width)
                        .is_none_or(|end| end > record.end)
                    {
                        return fail("dead_strip: relocation crosses an unwind record boundary");
                    }
                }
                g.records.insert((oi, si), records);
            }
        }
        let mut pending = Vec::new();
        let mut losing_owners = BTreeSet::new();

        for (&(oi, si), records) in &g.records.clone() {
            let compact = self.objects[oi].sections[si].name == "__compact_unwind";
            for r in records {
                if !compact && r.cie.is_none() {
                    continue;
                }
                let Some(owner) = g
                    .owner(oi, si, r.start + if compact { 0 } else { 8 }, !compact)
                    .map_err(|e| format!("{}: {e}", self.objects[oi].name))?
                else {
                    let field = r.start + if compact { 0 } else { 8 };
                    let section = &self.objects[oi].sections[si];
                    let rel = section
                        .relocs
                        .iter()
                        .find(|rel| rel.offset == field && rel.kind == 0)
                        .unwrap();
                    let original_section = if rel.external {
                        self.objects[oi].symbols[rel.symbol].section - 1
                    } else {
                        rel.symbol - 1
                    };
                    losing_owners.insert(Node::Section(oi, original_section));
                    continue;
                };
                g.owners
                    .entry(owner)
                    .or_default()
                    .push(Node::Record(oi, si, r.start));
                if let Some(cie) = r.cie {
                    let c = records
                        .iter()
                        .find(|c| c.start == cie && c.cie.is_none())
                        .ok_or("invalid FDE CIE pointer")?;
                    crate::unwind::validate_cie(&self.objects[oi].sections[si].data, c.start)?;
                }
            }
        }
        // Ownership discovery must not turn metadata's dead references into
        // import/common demand. Edges are demanded only when visited below.
        g.demanded.clear();
        let main = self
            .globals
            .get("_main")
            .ok_or("missing entry symbol `_main`")?;
        if let Target::Definition(o, s) = *main {
            pending.extend(g.symbol(o, s)?);
        } else {
            return fail("_main is not a text definition");
        }
        for name in &self.options.required_exports {
            if name.is_empty() || name.contains('\0') {
                return fail("required export must be a nonempty symbol name without NUL");
            }
            let definition = match self.globals.get(name) {
                Some(Target::Definition(o, s)) => Some((*o, *s)),
                // Commons merge visibility across every declaration: any
                // public common makes the selected common public.
                _ => self
                    .objects
                    .iter()
                    .enumerate()
                    .flat_map(|(oi, o)| {
                        o.symbols.iter().enumerate().map(move |(si, s)| (oi, si, s))
                    })
                    .filter(|(_, _, s)| s.name == *name && s.kind & 0xe == 0 && s.value != 0)
                    .min_by_key(|(_, _, s)| s.kind & 0x10)
                    .map(|(oi, si, _)| (oi, si)),
            };
            let Some((o, s)) = definition else {
                return fail(format!(
                    "required export `{name}` has no selected-input definition (required exports do not extract archive members)"
                ));
            };
            if self.objects[o].symbols[s].kind & 0x10 != 0 {
                return fail(format!("required export `{name}` is private extern"));
            }
            pending.extend(g.symbol(o, s)?);
        }
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, s) in o.sections.iter().enumerate() {
                if s.segment == "__DWARF" {
                    continue;
                }
                if s.flags & 0x10000000 != 0 || matches!(s.flags & 255, 9 | 0xa) {
                    if let Some(records) = g.records.get(&(oi, si)) {
                        pending.extend(records.iter().map(|r| Node::Record(oi, si, r.start)));
                    } else {
                        pending.push(Node::Section(oi, si));
                    }
                }
            }
            for (si, sym) in o.symbols.iter().enumerate() {
                if sym.desc & 0x20 != 0 {
                    pending.extend(g.symbol(oi, si)?);
                }
            }
        }
        let mut live = BTreeSet::new();
        let mut commons = BTreeSet::new();
        while let Some(node) = pending.pop() {
            if !live.insert(node.clone()) {
                continue;
            }
            if let Some(records) = g.owners.get(&node) {
                pending.extend(records.iter().cloned());
            }
            match node {
                Node::Section(oi, si) => {
                    let s = &self.objects[oi].sections[si];
                    if s.segment == "__DWARF" {
                        return fail("dead_strip: live reference into debug metadata");
                    }
                    let edges = g.edges(oi, si, 0, s.size).map_err(|e| {
                        format!("{}: {},{}: {e}", self.objects[oi].name, s.segment, s.name)
                    })?;
                    if edges.iter().any(|n| matches!(n, Node::Record(..))) {
                        return fail(
                            "dead_strip: ordinary input relocation into moving unwind metadata is unsupported",
                        );
                    }
                    pending.extend(edges);
                }
                Node::Record(oi, si, at) => {
                    let r = g.records[&(oi, si)]
                        .iter()
                        .find(|r| r.start == at)
                        .unwrap()
                        .clone();
                    pending.extend(g.edges(oi, si, r.start, r.end)?);
                    if let Some(cie) = r.cie {
                        pending.push(Node::Record(oi, si, cie));
                    }
                }
                Node::Common(name) => {
                    commons.insert(name);
                }
            }
        }
        if losing_owners.iter().any(|owner| live.contains(owner)) {
            return fail(
                "dead_strip: a live indivisible code contribution contains a losing weak unwind owner; split input sections or use the system linker",
            );
        }
        let demanded = std::mem::take(&mut g.demanded);
        let records = std::mem::take(&mut g.records);
        drop(g);
        // Rebuild metadata before sizing, preserving original contribution IDs.
        // Remap local metadata symbols and FDE backward CIE pointers; section
        // relocations into these moving records were rejected above.
        for ((oi, si), records) in records {
            let object = &mut self.objects[oi];
            let old_symbols = object.symbols.clone();
            let section = &mut object.sections[si];
            let mut mapping = BTreeMap::new();
            let mut data = Vec::new();
            for r in &records {
                if live.contains(&Node::Record(oi, si, r.start)) {
                    mapping.insert(r.start, data.len());
                    data.extend_from_slice(&section.data[r.start..r.end]);
                }
            }
            for r in &records {
                if let (Some(&at), Some(cie)) = (mapping.get(&r.start), r.cie) {
                    let cie = *mapping
                        .get(&cie)
                        .ok_or("dead_strip: live FDE lost its CIE")?;
                    let delta =
                        u32::try_from(at + 4 - cie).map_err(|_| "FDE CIE offset overflow")?;
                    data[at + 4..at + 8].copy_from_slice(&delta.to_le_bytes());
                }
            }
            let old_relocs = section.relocs.clone();
            section.relocs.retain_mut(|rel| {
                let Some(r) = records
                    .iter()
                    .find(|r| rel.offset >= r.start && rel.offset < r.end)
                else {
                    return false;
                };
                let Some(&at) = mapping.get(&r.start) else {
                    return false;
                };
                rel.offset = at + rel.offset - r.start;
                true
            });
            section.size = data.len();
            section.data = data;
            for sym in &mut object.symbols {
                if sym.section != si + 1 {
                    continue;
                }
                let offset = (sym.value - section.address) as usize;
                sym.value = section.address
                    + records
                        .iter()
                        .find_map(|r| {
                            (offset >= r.start && offset < r.end)
                                .then(|| mapping.get(&r.start).map(|at| at + offset - r.start))
                                .flatten()
                        })
                        .unwrap_or(section.size) as u64;
            }
            for rel in old_relocs.iter().filter(|r| r.kind == 1) {
                let Some(record) = records
                    .iter()
                    .find(|r| rel.offset >= r.start && rel.offset < r.end)
                else {
                    continue;
                };
                let Some(&at) = mapping.get(&record.start) else {
                    continue;
                };
                let old = &old_symbols[rel.symbol];
                if old.kind & 0xe != 0xe || old.kind & 1 != 0 || old.section != si + 1 {
                    return fail(
                        "dead_strip: unwind difference requires a local anchor in its own section",
                    );
                }
                let new_field = at + rel.offset - record.start;
                let raw = if rel.width == 8 {
                    Reader(&section.data).u64(new_field)?
                } else {
                    Reader(&section.data).u32(new_field)? as u64
                };
                let value = raw
                    .wrapping_add(rel.offset as u64)
                    .wrapping_sub(new_field as u64)
                    .wrapping_add(object.symbols[rel.symbol].value)
                    .wrapping_sub(old.value);
                section.data[new_field..new_field + rel.width]
                    .copy_from_slice(&value.to_le_bytes()[..rel.width]);
            }
        }
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, section) in o.sections.iter().enumerate() {
                let metadata_live = live
                    .iter()
                    .any(|n| matches!(n, Node::Record(o,s,_) if *o == oi && *s == si));
                if !live.contains(&Node::Section(oi, si)) && !metadata_live {
                    self.discarded.insert((oi, si));
                }
                if metadata_live && section.data.is_empty() {
                    return fail("dead_strip: empty live metadata");
                }
            }
        }
        self.globals.retain(|_, t| !matches!(t, Target::Definition(o, s) if self.objects[*o].symbols[*s].section != 0 && self.discarded.contains(&(*o, self.objects[*o].symbols[*s].section - 1))));
        self.weak_names
            .retain(|name| self.globals.contains_key(name));
        self.demanded = Some(demanded);
        Ok(commons)
    }
}
