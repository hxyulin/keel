//! Collection over original contribution IDs, before output sections exist.
//!
//! Each section splits into units, the collector's nodes. Unwind metadata
//! splits into records. A section of an object with
//! MH_SUBSECTIONS_VIA_SYMBOLS splits into atoms that start at its symbols.
//! Any other section is one atom.
use crate::{
    read::{self, Fixup, Operation, Reader, Reloc, Section},
    *,
};

/// Unit `ui` of a section, or a common.
#[derive(Clone, Copy)]
enum Node<'a> {
    Unit(usize, usize, usize),
    Common(&'a str),
    Unwind(usize, usize),
}
/// Where a reference lands, before it is mapped to a node.
#[derive(Clone, Copy)]
enum Place<'a> {
    /// An offset in section `.1` of object `.0`.
    In(usize, usize, usize),
    Common(&'a str),
}
#[derive(Clone)]
struct Unit {
    start: usize,
    end: usize,
    /// An FDE's CIE, by offset.
    cie: Option<usize>,
}
/// A section's units, with its relocations bucketed by unit.
struct Parts {
    /// Sorted by start and disjoint. Atoms tile the section; records may
    /// leave gaps.
    units: Vec<Unit>,
    records: bool,
    /// Whether the section may split into atoms at symbols.
    split: bool,
    buckets: Buckets,
}
/// Native fixups are disjoint and ordered, so each unit needs only a range.
/// Foreign records may be unordered and include prefixes and subtractor pairs.
enum Buckets {
    Ordered(Vec<std::ops::Range<usize>>),
    Indexed {
        relocs: Vec<Vec<u32>>,
        fixups: Vec<Vec<u32>>,
    },
}
impl Buckets {
    fn relocation(&self, unit: usize, index: usize) -> Option<usize> {
        match self {
            Self::Ordered(ranges) => {
                let range = &ranges[unit];
                (index < range.len()).then(|| range.start + index)
            }
            Self::Indexed { relocs, .. } => relocs[unit].get(index).map(|&i| i as usize),
        }
    }
    fn fixup(&self, unit: usize, index: usize) -> Option<usize> {
        match self {
            Self::Ordered(_) => self.relocation(unit, index),
            Self::Indexed { fixups, .. } => fixups[unit].get(index).map(|&i| i as usize),
        }
    }
}
impl Parts {
    fn whole(s: &Section, split: bool) -> Self {
        Self {
            units: vec![Unit {
                start: 0,
                end: s.size,
                cie: None,
            }],
            records: false,
            split,
            buckets: Buckets::Ordered(Vec::new()),
        }
    }
    /// The unit whose bytes contain `offset`.
    fn find(&self, offset: usize) -> Option<usize> {
        let i = self.units.partition_point(|u| u.start <= offset);
        i.checked_sub(1).filter(|&i| offset < self.units[i].end)
    }
    /// The atom a symbol at `offset` belongs to: the last one starting at or
    /// before it, so a label at an atom's end belongs to the next atom.
    fn atom(&self, offset: usize) -> usize {
        self.units
            .partition_point(|u| u.start <= offset)
            .saturating_sub(1)
    }
    fn starting_at(&self, start: usize) -> Option<usize> {
        self.units.binary_search_by_key(&start, |u| u.start).ok()
    }
    /// Bucket `s`'s relocations by unit. A fixup belongs to one unit in its
    /// entirety. Otherwise dropping its unit could silently remove a write
    /// into a surviving neighbor, or reinterpret it after compaction. Dead
    /// units are validated too, before pruning.
    fn bucket(&mut self, s: &Section) -> Result<()> {
        // Native ingestion validates disjoint, ascending fixups before GC.
        let ordered = s.relocs.iter().all(|r| r.operation != Operation::Foreign);
        if ordered {
            let mut ranges = vec![0..0; self.units.len()];
            let mut ui = 0;
            for (at, rel) in s.relocs.iter().enumerate() {
                let offset = rel.offset as usize;
                while ui < self.units.len() && offset >= self.units[ui].end {
                    ui += 1;
                }
                let found = self.units.get(ui).filter(|u| offset >= u.start);
                self.validate_relocation(found, rel)?;
                let range = &mut ranges[ui];
                if range.start == range.end {
                    range.start = at;
                }
                range.end = at + 1;
            }
            self.buckets = Buckets::Ordered(ranges);
        } else {
            let mut relocs = vec![Vec::new(); self.units.len()];
            let mut fixups = vec![Vec::new(); self.units.len()];
            let mut next_fixup = 0;
            for (at, rel) in s.relocs.iter().enumerate() {
                let found = self.find(rel.offset as usize);
                self.validate_relocation(found.map(|ui| &self.units[ui]), rel)?;
                let ui = found.expect("validated relocation belongs to a unit");
                relocs[ui].push(at as u32);
                // Prefix/pair records consume the next relocation too.
                if at == next_fixup {
                    fixups[ui].push(at as u32);
                    let paired = rel.operation == Operation::Foreign
                        && matches!(rel.kind, read::ADDEND | read::SUBTRACTOR)
                        && read::fixups(&s.relocs[at..], 0..usize::MAX)
                            .next()
                            .is_some_and(|f| f.is_ok());
                    next_fixup = at + 1 + paired as usize;
                }
            }
            self.buckets = Buckets::Indexed { relocs, fixups };
        }
        Ok(())
    }
    fn validate_relocation(&self, unit: Option<&Unit>, rel: &Reloc) -> Result<()> {
        let Some(unit) = unit else {
            return fail(if self.records {
                "dead_strip: relocation outside an unwind record"
            } else {
                "dead_strip: relocation outside its section"
            });
        };
        if (rel.offset as usize)
            .checked_add(rel.width as usize)
            .is_none_or(|end| end > unit.end)
        {
            return fail(if self.records {
                "dead_strip: relocation crosses an unwind record boundary"
            } else {
                "dead_strip: relocation crosses a linker atom"
            });
        }
        if rel.operation == Operation::PagePair
            && (rel.aux as usize + 4 > unit.end || (rel.aux as usize) < unit.start)
        {
            return fail("dead_strip: paired relocation crosses a linker atom");
        }
        Ok(())
    }
}
/// Whether a symbol names the start of an atom: a definition that is not an
/// alternate entry or a temporary label.
fn starts_atom(s: &Symbol) -> bool {
    s.kind & 0xe == 0xe && s.desc & 0x200 == 0 && !temporary_label(&s.name)
}
/// Assembler-local names: `L` labels and `ltmp` section anchors. They start
/// no atom and get no debug-map entry.
pub(crate) fn temporary_label(name: &str) -> bool {
    name.starts_with('L') || name.starts_with("ltmp")
}
/// Whether section `s` of an object may split into atoms at its symbols.
/// Unwind metadata splits into records instead. Module initializer and
/// no-dead-strip sections are roots as a whole, and DWARF is never linked.
fn splits(o: &Object, s: &Section) -> bool {
    o.subsections
        && s.segment != "__DWARF"
        && s.flags & 0x10000000 == 0
        && !matches!(s.flags & 255, 9 | 0xa)
}
struct Graph<'a, 'data> {
    linker: &'a Linker<'data>,
    /// `parts[object][section]`.
    parts: Vec<Vec<Parts>>,
    /// `first[object][section]` is the node id of the section's first unit.
    first: Vec<Vec<usize>>,
    /// Common node ids, after every section's nodes.
    common_ids: BTreeMap<&'a str, usize>,
    nodes: usize,
    unwind_first: Vec<usize>,
    /// The records owned by each code atom, by node id.
    owners: Vec<Vec<Node<'a>>>,
    demanded: Vec<Vec<bool>>,
    /// Filled lazily after atom boundaries and global winners are final.
    /// The outer option distinguishes an unseen symbol from no input edge.
    targets: Vec<Vec<Option<Option<Node<'a>>>>>,
}
impl<'a, 'data> Graph<'a, 'data> {
    fn records(&self, oi: usize, si: usize) -> bool {
        self.parts[oi][si].records
    }
    fn id(&self, node: Node<'_>) -> usize {
        match node {
            Node::Unit(oi, si, ui) => self.first[oi][si] + ui,
            Node::Common(name) => self.common_ids[name],
            Node::Unwind(o, r) => self.unwind_first[o] + r,
        }
    }
    fn node(&self, place: Place<'a>) -> Result<Node<'a>> {
        match place {
            Place::Common(name) => Ok(Node::Common(name)),
            Place::In(oi, si, offset) => {
                let parts = &self.parts[oi][si];
                let ui = if parts.records {
                    parts
                        .find(offset)
                        .ok_or("dead_strip: reference outside an unwind record")?
                } else {
                    parts.atom(offset)
                };
                Ok(Node::Unit(oi, si, ui))
            }
        }
    }
    fn symbol(&mut self, oi: usize, si: usize) -> Result<Option<Node<'a>>> {
        let cached = self.targets[oi]
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        if let Some(node) = cached {
            return Ok(*node);
        }
        let node = self
            .symbol_place(oi, si)?
            .map(|p| self.node(p))
            .transpose()?;
        self.targets[oi][si] = Some(node);
        Ok(node)
    }
    fn symbol_place(&mut self, oi: usize, si: usize) -> Result<Option<Place<'a>>> {
        let linker = self.linker;
        let s = linker.objects[oi]
            .symbols
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        self.demanded[oi][si] = true;
        let (oi, si) = if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            match linker.globals.get(&s.name) {
                Some(Target::Definition(o, s)) => (*o, *s),
                Some(Target::Header) => return Ok(None),
                None if self.common_ids.contains_key(s.name.as_str()) => {
                    return Ok(Some(Place::Common(s.name.as_str())));
                }
                None => return Ok(None), // imports and synthesized boundaries resolve after GC
                _ => return fail("dead_strip: unexpected pre-layout symbol target"),
            }
        } else {
            (oi, si)
        };
        Ok(self.definition(oi, si))
    }
    fn definition(&self, oi: usize, si: usize) -> Option<Place<'a>> {
        let s = &self.linker.objects[oi].symbols[si];
        if s.kind & 0xe == 2 {
            return None;
        }
        let section = &self.linker.objects[oi].sections[s.section - 1];
        Some(Place::In(
            oi,
            s.section - 1,
            (s.value - section.address) as usize,
        ))
    }
    fn target(
        &mut self,
        oi: usize,
        r: &Reloc,
        data: &[u8],
        pointer: bool,
    ) -> Result<Option<Place<'a>>> {
        if r.external {
            return self.symbol_place(oi, r.symbol as usize);
        }
        let si = (r.symbol as usize)
            .checked_sub(1)
            .ok_or("relocation section ordinal out of range")?;
        let section = self.linker.objects[oi]
            .sections
            .get(si)
            .ok_or("relocation section ordinal out of range")?;
        // Local pointer payloads carry original object addresses, not symbol
        // IDs. Other section-relative fields reach only unsplit sections, where
        // the addend cannot change the target node.
        let offset = if pointer {
            Reader(data)
                .u64(r.offset as usize)?
                .checked_sub(section.address)
                .ok_or("dead_strip: local pointer precedes its input section")? as usize
        } else {
            0
        };
        if self.records(oi, si) {
            return fail(
                "dead_strip: local section relocation into unwind metadata is unsupported; use explicit symbol differences",
            );
        }
        if pointer && offset > section.size {
            return fail("dead_strip: local pointer outside its input section");
        }
        if !pointer && self.parts[oi][si].units.len() > 1 {
            return fail(
                "dead_strip: section-relative difference into a section split at symbols is unsupported",
            );
        }
        Ok(Some(Place::In(oi, si, offset)))
    }
    /// Where a relocation in section `si` lands. Unwind metadata names its
    /// own object's copy of a definition, also when that copy is a losing
    /// weak one (see Linker::unwind_target).
    fn edge(
        &mut self,
        oi: usize,
        si: usize,
        r: &Reloc,
        data: &[u8],
        pointer: bool,
    ) -> Result<Option<Place<'a>>> {
        if self.records(oi, si)
            && r.external
            && self.linker.objects[oi]
                .symbols
                .get(r.symbol as usize)
                .is_some_and(|s| s.kind & 0xe == 0xe)
        {
            return Ok(self.definition(oi, r.symbol as usize));
        }
        self.target(oi, r, data, pointer)
    }
    /// Ordinary external references resolve once per symbol. Metadata's own
    /// definitions bypass global winners, so they retain their ownership path.
    fn edge_node(
        &mut self,
        oi: usize,
        si: usize,
        r: &Reloc,
        data: &[u8],
        pointer: bool,
    ) -> Result<Option<Node<'a>>> {
        if r.external
            && !(self.records(oi, si)
                && self.linker.objects[oi]
                    .symbols
                    .get(r.symbol as usize)
                    .is_some_and(|s| s.kind & 0xe == 0xe))
        {
            return self.symbol(oi, r.symbol as usize);
        }
        self.edge(oi, si, r, data, pointer)?
            .map(|p| self.node(p))
            .transpose()
    }
    /// Decode prefix/pair records before interpreting symbol fields. ADDEND's
    /// 24-bit symbol field is a signed number, never an edge.
    fn edges(&mut self, oi: usize, si: usize, ui: usize, edges: &mut Vec<Node<'a>>) -> Result<()> {
        let linker = self.linker;
        let s = &linker.objects[oi].sections[si];
        let records = self.records(oi, si);
        edges.clear();
        let mut i = 0;
        while let Some(at) = self.parts[oi][si].buckets.fixup(ui, i) {
            i += 1;
            let fixup = read::fixups(&s.relocs[at..], 0..usize::MAX)
                .next()
                .expect("bucketed fixup exists");
            match fixup? {
                Fixup::Difference { minus, plus } => {
                    // Native FDE differences use a local metadata anchor. It is
                    // arithmetic, not a liveness dependency on the anchor's record.
                    let anchor = linker.objects[oi]
                        .symbols
                        .get(minus.symbol as usize)
                        .ok_or("invalid subtractor symbol")?;
                    if !(records
                        && anchor.kind & 0xe == 0xe
                        && anchor.kind & 1 == 0
                        && anchor.section == si + 1)
                        && let Some(node) = self.edge_node(oi, si, minus, &s.data, false)?
                    {
                        edges.push(node);
                    }
                    if let Some(node) = self.edge_node(oi, si, plus, &s.data, false)? {
                        edges.push(node);
                    }
                }
                Fixup::Native(r) => {
                    if r.operation == Operation::Difference {
                        let mut minus = r.clone();
                        minus.symbol = r.aux;
                        minus.kind = 1;
                        let anchor = &linker.objects[oi].symbols[minus.symbol as usize];
                        if !(records
                            && anchor.kind & 0xe == 0xe
                            && anchor.kind & 1 == 0
                            && anchor.section == si + 1)
                            && let Some(node) = self.edge_node(oi, si, &minus, &s.data, false)?
                        {
                            edges.push(node);
                        }
                    }
                    if let Some(node) = self.edge_node(oi, si, r, &s.data, false)? {
                        edges.push(node);
                    }
                }
                Fixup::Single { reloc: r, .. } => {
                    if r.kind > 9 {
                        return fail(format!(
                            "dead_strip: unsupported ARM64 relocation {}",
                            r.kind
                        ));
                    }
                    if let Some(node) = self.edge_node(
                        oi,
                        si,
                        r,
                        &s.data,
                        r.kind == read::UNSIGNED && (r.width as usize) == 8,
                    )? {
                        edges.push(node);
                    }
                }
            }
        }
        Ok(())
    }
    /// The function range that record `ri` describes: its code section and
    /// the byte range in it.
    fn owner(
        &mut self,
        oi: usize,
        si: usize,
        ri: usize,
        field: usize,
        fde: bool,
    ) -> Result<(usize, usize, usize, usize)> {
        let linker = self.linker;
        let s = &linker.objects[oi].sections[si];
        let matches: Vec<_> = (0..)
            .map_while(|i| self.parts[oi][si].buckets.relocation(ri, i))
            .map(|at| &s.relocs[at])
            .filter(|r| (r.offset as usize) == field)
            .collect();
        let native_difference =
            fde && matches.len() == 1 && matches[0].operation == Operation::Difference;
        let negative_id = if native_difference {
            matches[0].aux as usize
        } else {
            matches.first().map_or(0, |r| r.symbol as usize)
        };
        let rel = if fde {
            if !native_difference
                && (matches.len() != 2
                    || matches[0].kind != 1
                    || matches[1].kind != 0
                    || !matches[1].external
                    || !matches[0].external
                    || matches[0].pc
                    || matches[1].pc
                    || (matches[0].width as usize) != 8
                    || (matches[1].width as usize) != 8)
            {
                return fail(
                    "dead_strip: implicit/foreign eh_frame FDE ownership requires native-width SUBTRACTOR/UNSIGNED fixups",
                );
            }
            // The native pcrel field may subtract its own label or a section
            // anchor with an offset folded into the payload. Rebuild remaps both.
            let negative = linker.objects[oi]
                .symbols
                .get(negative_id)
                .ok_or("invalid FDE subtractor symbol")?;
            if negative.kind & 0xe != 0xe || negative.kind & 1 != 0 || negative.section != si + 1 {
                return fail(
                    "dead_strip: FDE subtractor must be a local anchor in its own section",
                );
            }
            if native_difference {
                matches[0]
            } else {
                matches[1]
            }
        } else {
            if matches.len() != 1
                || matches[0].kind != 0
                || matches[0].pc
                || (matches[0].width as usize) != 8
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
        if let Place::In(o, sec, _) = owner
            && !self.records(o, sec)
        {
            let code = &linker.objects[o].sections[sec];
            if code.segment == "__TEXT"
                && code.flags & 0x80000400 != 0
                && !matches!(code.flags & 255, 1 | 0x12)
            {
                let raw = if rel.operation != Operation::Foreign {
                    rel.addend as u64
                } else {
                    Reader(&s.data).u64(field)?
                };
                let start = if rel.external {
                    let original = &linker.objects[oi].symbols[rel.symbol as usize];
                    let sym = if original.kind & 0xe == 0 {
                        let Some(Target::Definition(o, id)) = linker.globals.get(&original.name)
                        else {
                            return fail("dead_strip: unwind owner is not an input definition");
                        };
                        &linker.objects[*o].symbols[*id]
                    } else {
                        original
                    };
                    let raw = if fde {
                        let negative = &linker.objects[oi].symbols[negative_id];
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
                let Some(end) = start
                    .checked_add(length)
                    .filter(|&end| length != 0 && end <= code.size as u64)
                else {
                    return fail(
                        "dead_strip: unwind function range crosses its input section contribution",
                    );
                };
                return Ok((o, sec, start as usize, end as usize));
            }
        }
        fail("dead_strip: unwind owner must be file-backed executable __TEXT")
    }
}
/// Split an unwind metadata section into records.
fn records(s: &Section, compact: bool) -> Result<Vec<Unit>> {
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
        records.push(Unit {
            start: at,
            end,
            cie,
        });
        at = end;
    }
    Ok(records)
}
/// Split a section into atoms at `starts`, which are sorted and unique and
/// include 0, after removing the boundaries strictly inside each of `joined`.
fn atoms(s: &Section, mut starts: Vec<usize>, joined: &mut [(usize, usize)]) -> Vec<Unit> {
    joined.sort_unstable();
    let mut j = 0;
    let mut cover = 0;
    starts.retain(|&b| {
        while j < joined.len() && joined[j].0 < b {
            cover = cover.max(joined[j].1);
            j += 1;
        }
        b >= cover
    });
    let mut units: Vec<_> = starts
        .iter()
        .map(|&start| Unit {
            start,
            end: 0,
            cie: None,
        })
        .collect();
    for i in 0..units.len() {
        units[i].end = units.get(i + 1).map_or(s.size, |u| u.start);
    }
    units
}
impl Linker<'_> {
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
        let mut parts = Vec::with_capacity(self.objects.len());
        for o in &self.objects {
            let mut sections = Vec::with_capacity(o.sections.len());
            for (si, s) in o.sections.iter().enumerate() {
                let compact = s.segment == "__LD" && s.name == "__compact_unwind";
                if !compact && s.name != "__eh_frame" {
                    sections.push(Parts::whole(s, splits(o, s)));
                    continue;
                }
                if o.symbols
                    .iter()
                    .any(|sym| sym.section == si + 1 && sym.kind & 1 != 0)
                {
                    return fail("dead_strip: external symbols in unwind metadata are unsupported");
                }
                let mut p = Parts {
                    units: records(s, compact)?,
                    records: true,
                    split: false,
                    buckets: Buckets::Ordered(Vec::new()),
                };
                p.bucket(s)?;
                sections.push(p);
            }
            parts.push(sections);
        }
        let mut g = Graph {
            linker: self,
            parts,
            first: Vec::new(),
            common_ids: BTreeMap::new(),
            nodes: 0,
            unwind_first: Vec::new(),
            owners: Vec::new(),
            targets: Vec::new(),
            demanded: self
                .objects
                .iter()
                .map(|o| vec![false; o.symbols.len()])
                .collect(),
        };
        // Each record's function range. A range joins every atom it covers,
        // so a record and its function stay together.
        let mut owned = Vec::new();
        let mut joined: Vec<Vec<Vec<(usize, usize)>>> = self
            .objects
            .iter()
            .map(|o| vec![Vec::new(); o.sections.len()])
            .collect();
        for oi in 0..self.objects.len() {
            for si in 0..self.objects[oi].sections.len() {
                if !g.records(oi, si) {
                    continue;
                }
                let compact = self.objects[oi].sections[si].name == "__compact_unwind";
                let units = g.parts[oi][si].units.clone();
                for (ri, r) in units.iter().enumerate() {
                    if !compact && r.cie.is_none() {
                        continue;
                    }
                    let (o, sec, start, end) = g
                        .owner(oi, si, ri, r.start + if compact { 0 } else { 8 }, !compact)
                        .map_err(|e| format!("{}: {e}", self.objects[oi].name))?;
                    if g.parts[o][sec].split {
                        joined[o][sec].push((start, end));
                    }
                    owned.push((o, sec, start, Node::Unit(oi, si, ri)));
                    if let Some(cie) = r.cie {
                        let c = g.parts[oi][si]
                            .starting_at(cie)
                            .filter(|&c| units[c].cie.is_none())
                            .ok_or("invalid FDE CIE pointer")?;
                        crate::unwind::validate_cie(
                            &self.objects[oi].sections[si].data,
                            units[c].start,
                        )?;
                    }
                }
            }
        }
        for (oi, o) in self.objects.iter().enumerate() {
            for (ui, u) in o.unwind.iter().enumerate() {
                let sym = o.symbols.get(u.owner).ok_or("invalid unwind owner atom")?;
                let si = sym
                    .section
                    .checked_sub(1)
                    .ok_or("unwind owner is not a definition")?;
                let sec = o.sections.get(si).ok_or("invalid unwind owner section")?;
                let start = sym.value;
                let end = start
                    .checked_add(u.length as u64)
                    .ok_or("unwind range overflow")?;
                if sec.segment != "__TEXT"
                    || sec.flags & 0x80000400 == 0
                    || matches!(sec.flags & 255, 1 | 0x12)
                    || u.length == 0
                    || end > sec.size as u64
                {
                    return fail("native unwind owner range is outside executable section");
                }
                if g.parts[oi][si].split {
                    joined[oi][si].push((start as usize, end as usize));
                }
                owned.push((oi, si, start as usize, Node::Unwind(oi, ui)));
            }
        }
        // Ownership discovery must not turn metadata's dead references into
        // import/common demand. Edges are demanded only when visited below.
        for demanded in &mut g.demanded {
            demanded.fill(false);
        }
        for (oi, o) in self.objects.iter().enumerate() {
            let mut starts = vec![vec![0]; o.sections.len()];
            for sym in &o.symbols {
                if starts_atom(sym) {
                    let s = &o.sections[sym.section - 1];
                    starts[sym.section - 1].push((sym.value - s.address) as usize);
                }
            }
            for (si, s) in o.sections.iter().enumerate() {
                let p = &mut g.parts[oi][si];
                if p.records {
                    continue;
                }
                if p.split {
                    let mut starts = std::mem::take(&mut starts[si]);
                    starts.sort_unstable();
                    starts.dedup();
                    p.units = atoms(s, starts, &mut joined[oi][si]);
                    // dyld walks thread-local descriptors in 24-byte steps, so
                    // they split only where no padding can enter between them.
                    if s.flags & 255 == 0x13
                        && (s.alignment > 3 || p.units.iter().any(|u| u.start % 24 != 0))
                    {
                        *p = Parts::whole(s, false);
                    }
                }
                if s.segment != "__DWARF" {
                    p.bucket(s)
                        .map_err(|e| format!("{}: {},{}: {e}", o.name, s.segment, s.name))?;
                }
            }
        }
        let mut nodes = 0;
        g.first = g
            .parts
            .iter()
            .map(|sections| {
                sections
                    .iter()
                    .map(|p| {
                        let id = nodes;
                        nodes += p.units.len();
                        id
                    })
                    .collect()
            })
            .collect();
        g.unwind_first = self
            .objects
            .iter()
            .map(|o| {
                let first = nodes;
                nodes += o.unwind.len();
                first
            })
            .collect();
        g.common_ids = commons.into_iter().zip(nodes..).collect();
        nodes += g.common_ids.len();
        g.nodes = nodes;
        g.targets = self
            .objects
            .iter()
            .map(|o| vec![None; o.symbols.len()])
            .collect();
        g.owners = vec![Vec::new(); nodes];
        for (o, sec, start, record) in owned {
            let id = g.id(Node::Unit(o, sec, g.parts[o][sec].atom(start)));
            g.owners[id].push(record);
        }
        let mut pending = Vec::new();
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
                    let units = g.parts[oi][si].units.len();
                    pending.extend((0..units).map(|ui| Node::Unit(oi, si, ui)));
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
        let mut edges = Vec::new();
        while let Some(node) = pending.pop() {
            let id = g.id(node);
            if std::mem::replace(&mut live[id], true) {
                continue;
            }
            pending.extend_from_slice(&g.owners[id]);
            match node {
                Node::Unit(oi, si, ui) if g.records(oi, si) => {
                    g.edges(oi, si, ui, &mut edges)?;
                    pending.append(&mut edges);
                    if let Some(cie) = g.parts[oi][si].units[ui].cie {
                        let c = g.parts[oi][si].starting_at(cie).unwrap();
                        pending.push(Node::Unit(oi, si, c));
                    }
                }
                Node::Unit(oi, si, ui) => {
                    let s = &self.objects[oi].sections[si];
                    if s.segment == "__DWARF" {
                        return fail("dead_strip: live reference into debug metadata");
                    }
                    g.edges(oi, si, ui, &mut edges).map_err(|e| {
                        format!("{}: {},{}: {e}", self.objects[oi].name, s.segment, s.name)
                    })?;
                    if edges
                        .iter()
                        .any(|n| matches!(*n, Node::Unit(o, sec, _) if g.records(o, sec)))
                    {
                        return fail(
                            "dead_strip: ordinary input relocation into moving unwind metadata is unsupported",
                        );
                    }
                    pending.append(&mut edges);
                }
                Node::Common(name) => {
                    commons.insert(name.to_string());
                }
                Node::Unwind(oi, ui) => {
                    let u = &self.objects[oi].unwind[ui];
                    for si in [u.personality, u.lsda].into_iter().flatten() {
                        pending.extend(g.symbol(oi, si)?);
                    }
                }
            }
        }
        let unwind_first = std::mem::take(&mut g.unwind_first);
        let demanded = std::mem::take(&mut g.demanded);
        let parts = std::mem::take(&mut g.parts);
        let first = std::mem::take(&mut g.first);
        drop(g);
        for (oi, o) in self.objects.iter_mut().enumerate() {
            let mut i = 0;
            o.unwind.retain(|_| {
                let keep = live[unwind_first[oi] + i];
                i += 1;
                keep
            });
        }
        // Rebuild metadata before sizing, preserving original contribution IDs.
        // Remap local metadata symbols and FDE backward CIE pointers; section
        // relocations into these moving records were rejected above.
        for (oi, sections) in parts.iter().enumerate() {
            for (si, m) in sections.iter().enumerate() {
                if !m.records {
                    continue;
                }
                let records = &m.units;
                let object = &mut self.objects[oi];
                let old_symbols = object.symbols.clone();
                let section = &mut object.sections[si];
                let mut mapping = vec![None; records.len()];
                let mut data = Vec::new();
                for (ri, r) in records.iter().enumerate() {
                    if live[first[oi][si] + ri] {
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
                    let Some(ri) = m.find(rel.offset as usize) else {
                        return false;
                    };
                    let Some(at) = mapping[ri] else {
                        return false;
                    };
                    let old_field = rel.offset as usize;
                    rel.offset = (at + (rel.offset as usize) - records[ri].start) as u32;
                    if rel.operation == Operation::Difference {
                        let old = &old_symbols[rel.aux as usize];
                        let old_anchor = (old.value - section.address) as usize;
                        let new_anchor = m
                            .find(old_anchor)
                            .and_then(|ri| {
                                mapping[ri].map(|at| at + old_anchor - records[ri].start)
                            })
                            .unwrap_or(data.len());
                        rel.addend = rel
                            .addend
                            .wrapping_add(old_field as i64)
                            .wrapping_sub((rel.offset as usize) as i64)
                            .wrapping_add(new_anchor as i64)
                            .wrapping_sub(old_anchor as i64);
                    }
                    true
                });
                section.size = data.len();
                section.data = data.into();
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
                    let Some(ri) = m.find(rel.offset as usize) else {
                        continue;
                    };
                    let Some(at) = mapping[ri] else {
                        continue;
                    };
                    let old = &old_symbols[rel.symbol as usize];
                    if old.kind & 0xe != 0xe || old.kind & 1 != 0 || old.section != si + 1 {
                        return fail(
                            "dead_strip: unwind difference requires a local anchor in its own section",
                        );
                    }
                    let new_field = at + (rel.offset as usize) - records[ri].start;
                    let raw = if (rel.width as usize) == 8 {
                        Reader(&section.data).u64(new_field)?
                    } else {
                        Reader(&section.data).u32(new_field)? as u64
                    };
                    let value = raw
                        .wrapping_add((rel.offset as usize) as u64)
                        .wrapping_sub(new_field as u64)
                        .wrapping_add(object.symbols[rel.symbol as usize].value)
                        .wrapping_sub(old.value);
                    section.data.to_mut()[new_field..new_field + (rel.width as usize)]
                        .copy_from_slice(&value.to_le_bytes()[..(rel.width as usize)]);
                }
            }
        }
        for (oi, o) in self.objects.iter_mut().enumerate() {
            for (si, section) in o.sections.iter_mut().enumerate() {
                let id = first[oi][si];
                let p = &parts[oi][si];
                let units = &live[id..id + p.units.len()];
                let any_live = units.contains(&true);
                if !any_live {
                    self.discarded.insert((oi, si));
                }
                if p.records && any_live && section.data.is_empty() {
                    return fail("dead_strip: empty live metadata");
                }
                // A section whose atoms all live is placed whole, as before.
                if !p.records && any_live && units.contains(&false) {
                    section.atoms = p
                        .units
                        .iter()
                        .zip(units)
                        .map(|(u, &live)| read::Atom {
                            start: u.start,
                            end: u.end,
                            live,
                            offset: 0,
                        })
                        .collect();
                }
            }
        }
        self.globals.retain(|_, t| match *t {
            Target::Definition(o, s) => {
                let symbol = &self.objects[o].symbols[s];
                symbol.section == 0 || {
                    let section = &self.objects[o].sections[symbol.section - 1];
                    !self.discarded.contains(&(o, symbol.section - 1))
                        && section.live_at((symbol.value - section.address) as usize)
                }
            }
            _ => true,
        });
        self.weak_names
            .retain(|name| self.globals.contains_key(name));
        self.demanded = Some(demanded);
        Ok(commons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(size: usize, relocs: Vec<Reloc>) -> Section<'static> {
        Section {
            segment: "__TEXT".into(),
            name: "__text".into(),
            address: 0,
            size,
            alignment: 2,
            flags: 0x80000400,
            data: vec![0; size].into(),
            relocs,
            output: None,
            offset: 0,
            atoms: Vec::new(),
        }
    }
    fn symbol(name: &str, kind: u8, desc: u16) -> Symbol {
        Symbol {
            name: name.into(),
            kind,
            section: 1,
            desc,
            value: 0,
        }
    }
    fn bounds(units: &[Unit]) -> Vec<(usize, usize)> {
        units.iter().map(|u| (u.start, u.end)).collect()
    }

    #[test]
    fn atoms_start_at_symbols_that_are_not_alternate_entries_or_temporaries() {
        assert!(starts_atom(&symbol("_f", 0xf, 0)));
        assert!(starts_atom(&symbol("_static", 0xe, 0)));
        // Clang's linker-private strings keep their own atoms.
        assert!(starts_atom(&symbol("l_.str", 0xe, 0)));
        assert!(!starts_atom(&symbol("Lloop", 0xe, 0)));
        assert!(!starts_atom(&symbol("ltmp0", 0xe, 0)));
        assert!(!starts_atom(&symbol("_alt", 0xf, 0x200)));
        assert!(!starts_atom(&symbol("_undefined", 1, 0)));
        assert!(!starts_atom(&symbol("_absolute", 3, 0)));
    }

    #[test]
    fn unwind_ranges_join_the_atoms_they_cover() {
        let s = section(48, Vec::new());
        // Content before the first symbol is an atom of its own, and a symbol
        // at the section end makes a zero-size atom there.
        assert_eq!(
            bounds(&atoms(&s, vec![0, 8, 16, 24, 48], &mut [])),
            [(0, 8), (8, 16), (16, 24), (24, 48), (48, 48)]
        );
        // A boundary strictly inside a range goes; one at either end stays.
        assert_eq!(
            bounds(&atoms(&s, vec![0, 8, 16, 24], &mut [(8, 24)])),
            [(0, 8), (8, 24), (24, 48)]
        );
        assert_eq!(
            bounds(&atoms(&s, vec![0, 8, 16, 24], &mut [(8, 24), (4, 9)])),
            [(0, 24), (24, 48)]
        );
        // Overlapping ranges, given out of order, join through both.
        assert_eq!(
            bounds(&atoms(&s, vec![0, 8, 16, 24, 32], &mut [(12, 30), (0, 14)])),
            [(0, 32), (32, 48)]
        );
    }

    #[test]
    fn labels_belong_to_the_atom_that_starts_at_or_before_them() {
        let s = section(32, Vec::new());
        let mut parts = Parts::whole(&s, true);
        parts.units = atoms(&s, vec![0, 16, 32], &mut []);
        assert_eq!(parts.atom(15), 0);
        // A label at an atom's end belongs to the next atom.
        assert_eq!(parts.atom(16), 1);
        assert_eq!(parts.atom(32), 2);
        // Relocations never land in the zero-size atom.
        assert_eq!(parts.find(31), Some(1));
        assert_eq!(parts.find(32), None);
    }

    #[test]
    fn a_relocation_may_not_cross_an_atom() {
        let reloc = |offset| Reloc {
            offset,
            symbol: 0,
            external: true,
            pc: false,
            width: 8,
            kind: read::UNSIGNED,
            ..Reloc::default()
        };
        for operation in [Operation::Foreign, Operation::Single] {
            let reloc = |offset| Reloc {
                operation,
                ..reloc(offset)
            };
            let s = section(32, vec![reloc(8), reloc(12)]);
            let mut parts = Parts::whole(&s, true);
            parts.units = atoms(&s, vec![0, 16], &mut []);
            let error = parts.bucket(&s).unwrap_err();
            assert!(
                error.to_string().contains("crosses a linker atom"),
                "{operation:?}: {error}"
            );
            let s = section(32, vec![reloc(8)]);
            parts.bucket(&s).unwrap();
            assert_eq!(parts.buckets.relocation(0, 0), Some(0));
            assert_eq!(parts.buckets.relocation(0, 1), None);
            assert_eq!(parts.buckets.relocation(1, 0), None);
        }
    }
}
