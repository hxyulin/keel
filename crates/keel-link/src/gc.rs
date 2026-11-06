//! Collection over original contribution IDs, before output sections exist.
use crate::{
    read::{self, Fixup, Reader, Reloc},
    *,
};

/// A collector node. A record is named by its index in its section's
/// [`Meta::records`].
#[derive(Clone, Copy)]
enum Node<'a> {
    Section(usize, usize),
    Record(usize, usize, usize),
    Common(&'a str),
}
#[derive(Clone)]
struct Record {
    start: usize,
    end: usize,
    cie: Option<usize>,
}
/// An unwind metadata section split into records.
struct Meta {
    /// Sorted by start, and disjoint.
    records: Vec<Record>,
    /// Each record's relocations, as indexes in their original order.
    relocs: Vec<Vec<u32>>,
    /// The relocations among those that start a fixup.
    fixups: Vec<Vec<u32>>,
}
impl Meta {
    fn find(&self, offset: usize) -> Option<usize> {
        let i = self.records.partition_point(|r| r.start <= offset);
        i.checked_sub(1).filter(|&i| offset < self.records[i].end)
    }
    fn starting_at(&self, start: usize) -> Option<usize> {
        self.records.binary_search_by_key(&start, |r| r.start).ok()
    }
}
struct Graph<'a> {
    linker: &'a Linker,
    /// `meta[object][section]` for unwind metadata sections.
    meta: Vec<Vec<Option<Meta>>>,
    /// `first[object][section]` is the section's node id. Its records follow.
    first: Vec<Vec<usize>>,
    /// Common node ids, after every section's nodes.
    common_ids: BTreeMap<&'a str, usize>,
    nodes: usize,
    /// The records owned by each code node, by node id.
    owners: Vec<Vec<Node<'a>>>,
    demanded: BTreeSet<(usize, usize)>,
}
impl<'a> Graph<'a> {
    fn meta(&self, oi: usize, si: usize) -> Option<&Meta> {
        self.meta[oi][si].as_ref()
    }
    fn id(&self, node: Node<'_>) -> usize {
        match node {
            Node::Section(oi, si) => self.first[oi][si],
            Node::Record(oi, si, ri) => self.first[oi][si] + 1 + ri,
            Node::Common(name) => self.common_ids[name],
        }
    }
    fn at(&self, oi: usize, si: usize, offset: usize) -> Result<Node<'a>> {
        if let Some(meta) = self.meta(oi, si) {
            let ri = meta
                .find(offset)
                .ok_or("dead_strip: reference outside an unwind record")?;
            Ok(Node::Record(oi, si, ri))
        } else {
            Ok(Node::Section(oi, si))
        }
    }
    fn symbol(&mut self, oi: usize, si: usize) -> Result<Option<Node<'a>>> {
        let linker = self.linker;
        let s = linker.objects[oi]
            .symbols
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        self.demanded.insert((oi, si));
        let (oi, si) = if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            match linker.globals.get(&s.name) {
                Some(Target::Definition(o, s)) => (*o, *s),
                Some(Target::Header) => return Ok(None),
                None if self.common_ids.contains_key(s.name.as_str()) => {
                    return Ok(Some(Node::Common(s.name.as_str())));
                }
                None => return Ok(None), // imports and synthesized boundaries resolve after GC
                _ => return fail("dead_strip: unexpected pre-layout symbol target"),
            }
        } else {
            (oi, si)
        };
        self.definition(oi, si)
    }
    fn definition(&self, oi: usize, si: usize) -> Result<Option<Node<'a>>> {
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
    fn target(
        &mut self,
        oi: usize,
        r: &Reloc,
        data: &[u8],
        pointer: bool,
    ) -> Result<Option<Node<'a>>> {
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
        if self.meta(oi, si).is_some() {
            return fail(
                "dead_strip: local section relocation into unwind metadata is unsupported; use explicit symbol differences",
            );
        }
        if pointer && offset > section.size {
            return fail("dead_strip: local pointer outside its input section");
        }
        Ok(Some(self.at(oi, si, offset)?))
    }
    /// The target of a relocation in section `si`. Unwind metadata names its
    /// own object's copy of a definition, also when that copy is a losing
    /// weak one (see Linker::unwind_target).
    fn edge(
        &mut self,
        oi: usize,
        si: usize,
        r: &Reloc,
        data: &[u8],
        pointer: bool,
    ) -> Result<Option<Node<'a>>> {
        if self.meta(oi, si).is_some()
            && r.external
            && self.linker.objects[oi]
                .symbols
                .get(r.symbol)
                .is_some_and(|s| s.kind & 0xe == 0xe)
        {
            return self.definition(oi, r.symbol);
        }
        self.target(oi, r, data, pointer)
    }
    /// Decode prefix/pair records before interpreting symbol fields. ADDEND's
    /// 24-bit symbol field is a signed number, never an edge.
    fn edges(
        &mut self,
        oi: usize,
        si: usize,
        fixups: impl Iterator<Item = Result<Fixup<'a>>>,
    ) -> Result<Vec<Node<'a>>> {
        let s = &self.linker.objects[oi].sections[si];
        let metadata = self.meta(oi, si).is_some();
        let mut edges = Vec::new();
        for fixup in fixups {
            match fixup? {
                Fixup::Difference { minus, plus } => {
                    // Native FDE differences use a local metadata anchor. It is
                    // arithmetic, not a liveness dependency on the anchor's record.
                    let anchor = self.linker.objects[oi]
                        .symbols
                        .get(minus.symbol)
                        .ok_or("invalid subtractor symbol")?;
                    if !(metadata
                        && anchor.kind & 0xe == 0xe
                        && anchor.kind & 1 == 0
                        && anchor.section == si + 1)
                        && let Some(n) = self.edge(oi, si, minus, &s.data, false)?
                    {
                        edges.push(n);
                    }
                    if let Some(n) = self.edge(oi, si, plus, &s.data, false)? {
                        edges.push(n);
                    }
                }
                Fixup::Single { reloc: r, .. } => {
                    if r.kind > 9 {
                        return fail(format!(
                            "dead_strip: unsupported ARM64 relocation {}",
                            r.kind
                        ));
                    }
                    if let Some(n) =
                        self.edge(oi, si, r, &s.data, r.kind == read::UNSIGNED && r.width == 8)?
                    {
                        edges.push(n);
                    }
                }
            }
        }
        Ok(edges)
    }
    /// The fixups that start inside record `ri`, decoded from the section's
    /// whole relocation list.
    fn record_fixups(&self, oi: usize, si: usize, ri: usize) -> Vec<Result<Fixup<'a>>> {
        let linker = self.linker;
        let relocs = &linker.objects[oi].sections[si].relocs;
        self.meta(oi, si).unwrap().fixups[ri]
            .iter()
            .map(|&at| {
                read::fixups(&relocs[at as usize..], 0..usize::MAX)
                    .next()
                    .unwrap()
            })
            .collect()
    }
    fn owner(
        &mut self,
        oi: usize,
        si: usize,
        ri: usize,
        field: usize,
        fde: bool,
    ) -> Result<Node<'a>> {
        let s = &self.linker.objects[oi].sections[si];
        let matches: Vec<_> = self.meta(oi, si).unwrap().relocs[ri]
            .iter()
            .map(|&at| &s.relocs[at as usize])
            .filter(|r| r.offset == field)
            .collect();
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
        let owner = self
            .edge(oi, si, rel, &s.data, !fde && !rel.external)?
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
                    let sym = if original.kind & 0xe == 0 {
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
                return Ok(owner);
            }
        }
        fail("dead_strip: unwind owner must be file-backed executable __TEXT")
    }
}
impl Linker {
    pub(crate) fn collect(&mut self) -> Result<BTreeSet<String>> {
        let commons: BTreeSet<&str> = self
            .objects
            .iter()
            .flat_map(|o| &o.symbols)
            .filter(|c| c.kind & 0xe == 0 && c.value != 0)
            .map(|c| c.name.as_str())
            .collect();
        // Parse record extents first, so all symbol references can be mapped to
        // records even when a CIE or an FDE is shared or appears out of order.
        let mut meta = Vec::with_capacity(self.objects.len());
        for o in &self.objects {
            let mut sections = Vec::with_capacity(o.sections.len());
            for (si, s) in o.sections.iter().enumerate() {
                let compact = s.segment == "__LD" && s.name == "__compact_unwind";
                if !compact && s.name != "__eh_frame" {
                    sections.push(None);
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
                let mut m = Meta {
                    relocs: vec![Vec::new(); records.len()],
                    fixups: vec![Vec::new(); records.len()],
                    records,
                };
                // A fixup belongs to one original record in its entirety.
                // Otherwise dropping its starting record can silently remove
                // a write into a surviving neighbor (or reinterpret it after
                // compaction). Validate dead records too, before pruning.
                let mut next_fixup = 0;
                for (at, rel) in s.relocs.iter().enumerate() {
                    let ri = m
                        .find(rel.offset)
                        .ok_or("dead_strip: relocation outside an unwind record")?;
                    if rel
                        .offset
                        .checked_add(rel.width)
                        .is_none_or(|end| end > m.records[ri].end)
                    {
                        return fail("dead_strip: relocation crosses an unwind record boundary");
                    }
                    m.relocs[ri].push(at as u32);
                    // Decode once in list order, as an ADDEND prefix or a
                    // SUBTRACTOR pair takes the next relocation with it.
                    if at == next_fixup {
                        m.fixups[ri].push(at as u32);
                        let paired = matches!(rel.kind, read::ADDEND | read::SUBTRACTOR)
                            && read::fixups(&s.relocs[at..], 0..usize::MAX)
                                .next()
                                .is_some_and(|f| f.is_ok());
                        next_fixup = at + 1 + paired as usize;
                    }
                }
                sections.push(Some(m));
            }
            meta.push(sections);
        }
        let mut nodes = 0;
        let first: Vec<Vec<usize>> = meta
            .iter()
            .map(|sections| {
                sections
                    .iter()
                    .map(|m| {
                        let id = nodes;
                        nodes += 1 + m.as_ref().map_or(0, |m| m.records.len());
                        id
                    })
                    .collect()
            })
            .collect();
        let common_ids: BTreeMap<_, _> = commons.into_iter().zip(nodes..).collect();
        nodes += common_ids.len();
        let mut g = Graph {
            linker: self,
            meta,
            first,
            common_ids,
            nodes,
            owners: vec![Vec::new(); nodes],
            demanded: BTreeSet::new(),
        };
        let mut pending = Vec::new();
        for oi in 0..g.meta.len() {
            for si in 0..g.meta[oi].len() {
                let Some(m) = g.meta(oi, si) else {
                    continue;
                };
                let compact = self.objects[oi].sections[si].name == "__compact_unwind";
                let records = m.records.clone();
                for (ri, r) in records.iter().enumerate() {
                    if !compact && r.cie.is_none() {
                        continue;
                    }
                    let owner = g
                        .owner(oi, si, ri, r.start + if compact { 0 } else { 8 }, !compact)
                        .map_err(|e| format!("{}: {e}", self.objects[oi].name))?;
                    let id = g.id(owner);
                    g.owners[id].push(Node::Record(oi, si, ri));
                    if let Some(cie) = r.cie {
                        let c = g
                            .meta(oi, si)
                            .unwrap()
                            .starting_at(cie)
                            .filter(|&c| records[c].cie.is_none())
                            .ok_or("invalid FDE CIE pointer")?;
                        crate::unwind::validate_cie(
                            &self.objects[oi].sections[si].data,
                            records[c].start,
                        )?;
                    }
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
        // Each common's declarations, in input order.
        let mut common_decls: BTreeMap<&str, Vec<(usize, usize)>> = BTreeMap::new();
        if !self.options.required_exports.is_empty() {
            for (oi, o) in self.objects.iter().enumerate() {
                for (si, s) in o.symbols.iter().enumerate() {
                    if s.kind & 0xe == 0 && s.value != 0 {
                        common_decls
                            .entry(s.name.as_str())
                            .or_default()
                            .push((oi, si));
                    }
                }
            }
        }
        for name in &self.options.required_exports {
            if name.is_empty() || name.contains('\0') {
                return fail("required export must be a nonempty symbol name without NUL");
            }
            let definition = match self.globals.get(name) {
                Some(Target::Definition(o, s)) => Some((*o, *s)),
                // Commons merge visibility across every declaration: any
                // public common makes the selected common public.
                _ => common_decls.get(name.as_str()).and_then(|decls| {
                    decls
                        .iter()
                        .copied()
                        .min_by_key(|&(oi, si)| self.objects[oi].symbols[si].kind & 0x10)
                }),
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
                    if let Some(m) = g.meta(oi, si) {
                        pending.extend((0..m.records.len()).map(|ri| Node::Record(oi, si, ri)));
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
        let mut live = vec![false; g.nodes];
        let mut commons = BTreeSet::new();
        while let Some(node) = pending.pop() {
            let id = g.id(node);
            if std::mem::replace(&mut live[id], true) {
                continue;
            }
            pending.extend_from_slice(&g.owners[id]);
            match node {
                Node::Section(oi, si) => {
                    let s = &self.objects[oi].sections[si];
                    if s.segment == "__DWARF" {
                        return fail("dead_strip: live reference into debug metadata");
                    }
                    let edges = g
                        .edges(oi, si, read::fixups(&s.relocs, 0..s.size))
                        .map_err(|e| {
                            format!("{}: {},{}: {e}", self.objects[oi].name, s.segment, s.name)
                        })?;
                    if edges.iter().any(|n| matches!(n, Node::Record(..))) {
                        return fail(
                            "dead_strip: ordinary input relocation into moving unwind metadata is unsupported",
                        );
                    }
                    pending.extend(edges);
                }
                Node::Record(oi, si, ri) => {
                    let fixups = g.record_fixups(oi, si, ri);
                    pending.extend(g.edges(oi, si, fixups.into_iter())?);
                    if let Some(cie) = g.meta(oi, si).unwrap().records[ri].cie {
                        let c = g.meta(oi, si).unwrap().starting_at(cie).unwrap();
                        pending.push(Node::Record(oi, si, c));
                    }
                }
                Node::Common(name) => {
                    commons.insert(name.to_string());
                }
            }
        }
        let demanded = std::mem::take(&mut g.demanded);
        let meta = std::mem::take(&mut g.meta);
        let first = std::mem::take(&mut g.first);
        drop(g);
        // Rebuild metadata before sizing, preserving original contribution IDs.
        // Remap local metadata symbols and FDE backward CIE pointers; section
        // relocations into these moving records were rejected above.
        for (oi, sections) in meta.iter().enumerate() {
            for (si, m) in sections.iter().enumerate() {
                let Some(m) = m else {
                    continue;
                };
                let records = &m.records;
                let object = &mut self.objects[oi];
                let old_symbols = object.symbols.clone();
                let section = &mut object.sections[si];
                let mut mapping = vec![None; records.len()];
                let mut data = Vec::new();
                for (ri, r) in records.iter().enumerate() {
                    if live[first[oi][si] + 1 + ri] {
                        mapping[ri] = Some(data.len());
                        data.extend_from_slice(&section.data[r.start..r.end]);
                    }
                }
                for (ri, r) in records.iter().enumerate() {
                    if let (Some(at), Some(cie)) = (mapping[ri], r.cie) {
                        let cie = m
                            .starting_at(cie)
                            .and_then(|c| mapping[c])
                            .ok_or("dead_strip: live FDE lost its CIE")?;
                        let delta =
                            u32::try_from(at + 4 - cie).map_err(|_| "FDE CIE offset overflow")?;
                        data[at + 4..at + 8].copy_from_slice(&delta.to_le_bytes());
                    }
                }
                let old_relocs = section.relocs.clone();
                section.relocs.retain_mut(|rel| {
                    let Some(ri) = m.find(rel.offset) else {
                        return false;
                    };
                    let Some(at) = mapping[ri] else {
                        return false;
                    };
                    rel.offset = at + rel.offset - records[ri].start;
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
                        + m.find(offset)
                            .and_then(|ri| mapping[ri].map(|at| at + offset - records[ri].start))
                            .unwrap_or(section.size) as u64;
                }
                for rel in old_relocs.iter().filter(|r| r.kind == 1) {
                    let Some(ri) = m.find(rel.offset) else {
                        continue;
                    };
                    let Some(at) = mapping[ri] else {
                        continue;
                    };
                    let old = &old_symbols[rel.symbol];
                    if old.kind & 0xe != 0xe || old.kind & 1 != 0 || old.section != si + 1 {
                        return fail(
                            "dead_strip: unwind difference requires a local anchor in its own section",
                        );
                    }
                    let new_field = at + rel.offset - records[ri].start;
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
        }
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, section) in o.sections.iter().enumerate() {
                let id = first[oi][si];
                let records = meta[oi][si].as_ref().map_or(0, |m| m.records.len());
                let metadata_live = live[id + 1..id + 1 + records].contains(&true);
                if !live[id] && !metadata_live {
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
