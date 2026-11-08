//! Version 3 semantic link-unit cache. Integers are fixed-width little-endian.
//! Absolute byte ranges form contiguous regions in header order. Target IDs
//! use bit 31 for atoms and the remaining bits for an atom or linkage symbol ID.
use crate::{Atom, Definition, Fixup, FixupKind as K, Object, Section, Symbol, Target, Unwind};
use zerocopy::byteorder::little_endian::{I64, U16, U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};
const MAGIC: [u8; 8] = *b"KEELOBJ\0";
const VERSION: u32 = 3;
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct Range {
    offset: U64,
    count: U64,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct Header {
    magic: [u8; 8],
    version: U32,
    target: U32,
    sections: Range,
    atoms: Range,
    symbols: Range,
    fixups: Range,
    unwind: Range,
    strings: Range,
    payloads: Range,
    minimum: U32,
    flags: U32,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct SectionRecord {
    segment: Range,
    name: Range,
    size: U64,
    alignment: U32,
    flags: U32,
    data: Range,
    fixups: Range,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct AtomRecord {
    section: U32,
    flags: U32,
    offset: U32,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct SymbolRecord {
    name: Range,
    value: U64,
    index: U32,
    flags: U16,
    tag: u8,
    alignment: u8,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct FixupRecord {
    offset: U32,
    target: U32,
    addend: I64,
    aux: U32,
    kind: U16,
    reserved: U16,
}
#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct UnwindRecord {
    owner: U32,
    length: U32,
    encoding: U32,
    personality: U32,
    lsda: U32,
    flags: U32,
}
fn count(n: u64) -> Result<usize, String> {
    usize::try_from(n).map_err(|_| "cache range exceeds host address space".into())
}
fn index(n: usize) -> Result<U32, String> {
    u32::try_from(n)
        .map(U32::new)
        .map_err(|_| "cache index exceeds 32 bits".into())
}
fn range(offset: usize, count: usize) -> Range {
    Range {
        offset: U64::new(offset as u64),
        count: U64::new(count as u64),
    }
}
fn bytes(source: &[u8], r: Range) -> Result<&[u8], String> {
    let start = count(r.offset.get())?;
    let end = start
        .checked_add(count(r.count.get())?)
        .ok_or("cache range overflow")?;
    source
        .get(start..end)
        .ok_or_else(|| "cache range outside file".into())
}
fn table<T: FromBytes + KnownLayout + Immutable + Unaligned>(
    source: &[u8],
    r: Range,
) -> Result<&[T], String> {
    let n = count(r.count.get())?
        .checked_mul(size_of::<T>())
        .ok_or("cache table overflow")?;
    <[T]>::ref_from_bytes(bytes(
        source,
        Range {
            offset: r.offset,
            count: U64::new(n as u64),
        },
    )?)
    .map_err(|_| "invalid cache table layout".into())
}
fn text(source: &[u8], r: Range) -> Result<&str, String> {
    let s = std::str::from_utf8(bytes(source, r)?).map_err(|_| "cache name is not UTF-8")?;
    if s.contains('\0') {
        return Err("cache name contains NUL".into());
    }
    Ok(s)
}
fn target(id: u32) -> Target {
    if id & 0x80000000 != 0 {
        Target::Atom(id & 0x7fffffff)
    } else {
        Target::Symbol(id)
    }
}
fn target_id(t: Target) -> Result<U32, String> {
    let (n, tag) = match t {
        Target::Atom(n) => (n, 0x80000000),
        Target::Symbol(n) => (n, 0),
    };
    if n >= 0x80000000 {
        return Err("cache target exceeds 31 bits".into());
    }
    Ok(U32::new(n | tag))
}
fn fixup(r: &FixupRecord) -> Result<Fixup, String> {
    let aux = r.aux.get();
    let kind = match r.kind.get() {
        0 => K::Pointer64,
        1 => K::Branch26,
        2 => K::Page21,
        3 => K::PageOff12,
        4 => K::GotPage21,
        5 => K::GotPageOff12,
        6 => K::PointerToGot,
        7 => K::TlvpPage21,
        8 => K::TlvpPageOff12,
        9 => K::Difference32 { minus: target(aux) },
        10 => K::Difference64 { minus: target(aux) },
        11 => K::PagePair { low_offset: aux },
        12 => K::GotPagePair { low_offset: aux },
        13 => K::TlvpPagePair { low_offset: aux },
        _ => return Err("unknown cached fixup operation".into()),
    };
    if r.reserved.get() != 0 || (r.kind.get() < 9 && aux != 0) {
        return Err("invalid cached fixup fields".into());
    }
    Ok(Fixup {
        offset: r.offset.get(),
        target: target(r.target.get()),
        addend: r.addend.get(),
        kind,
    })
}
#[derive(Debug, Clone, Copy)]
pub struct Cache<'a> {
    source: &'a [u8],
    header: &'a Header,
    sections: &'a [SectionRecord],
    atoms: &'a [AtomRecord],
    symbols: &'a [SymbolRecord],
}
#[derive(Debug)]
pub struct SectionRef<'a> {
    pub segment: &'a str,
    pub name: &'a str,
    pub size: usize,
    pub alignment: u32,
    pub flags: u32,
    pub data: &'a [u8],
    fixups: &'a [FixupRecord],
}
impl SectionRef<'_> {
    pub fn fixups(&self) -> impl ExactSizeIterator<Item = Fixup> + '_ {
        self.fixups
            .iter()
            .map(|r| fixup(r).expect("validated fixup"))
    }
}
#[derive(Debug)]
pub struct SymbolRef<'a> {
    pub name: &'a str,
    pub definition: Definition,
    pub private: bool,
    pub weak_definition: bool,
    pub weak_reference: bool,
    pub weak_can_be_hidden: bool,
    pub no_dead_strip: bool,
}
impl SymbolRef<'_> {
    pub fn to_symbol(&self) -> Symbol {
        Symbol {
            name: self.name.into(),
            definition: self.definition,
            private: self.private,
            weak_definition: self.weak_definition,
            weak_reference: self.weak_reference,
            weak_can_be_hidden: self.weak_can_be_hidden,
            no_dead_strip: self.no_dead_strip,
        }
    }
}
fn definition(r: &SymbolRecord) -> Result<Definition, String> {
    match r.tag {
        0 if r.value.get() == 0 && r.index.get() == 0 && r.alignment == 0 => {
            Ok(Definition::Undefined)
        }
        1 if r.value.get() != 0 && r.alignment <= 15 && r.index.get() == 0 => {
            Ok(Definition::Common {
                size: r.value.get(),
                alignment: r.alignment,
            })
        }
        2 if r.value.get() == 0 && r.alignment == 0 => Ok(Definition::Atom(r.index.get() as usize)),
        _ => Err("invalid cached symbol definition".into()),
    }
}
impl<'a> Cache<'a> {
    /// Borrow a semantic cache, validating version, layout and all references.
    ///
    /// # Errors
    /// Returns an error for malformed, truncated or incompatible entries.
    pub fn parse(source: &'a [u8]) -> Result<Self, String> {
        let (h, _) = Header::ref_from_prefix(source).map_err(|_| "truncated cache header")?;
        if h.magic != MAGIC || h.version.get() != VERSION || h.target.get() != 1 {
            return Err("unsupported Keel object cache version or target".into());
        }
        if h.flags.get() & !1 != 0 {
            return Err("unknown cache flags".into());
        }
        let mut next = size_of::<Header>();
        for (r, stride) in [
            (h.sections, size_of::<SectionRecord>()),
            (h.atoms, size_of::<AtomRecord>()),
            (h.symbols, size_of::<SymbolRecord>()),
            (h.fixups, size_of::<FixupRecord>()),
            (h.unwind, size_of::<UnwindRecord>()),
            (h.strings, 1),
            (h.payloads, 1),
        ] {
            if count(r.offset.get())? != next {
                return Err("noncontiguous cache regions".into());
            }
            next = next
                .checked_add(
                    count(r.count.get())?
                        .checked_mul(stride)
                        .ok_or("cache region overflow")?,
                )
                .ok_or("cache region overflow")?;
        }
        if next != source.len() {
            return Err("truncated cache or trailing bytes".into());
        }
        let cache = Self {
            source,
            header: h,
            sections: table(source, h.sections)?,
            atoms: table(source, h.atoms)?,
            symbols: table(source, h.symbols)?,
        };
        let within = |r: Range, outer: Range, stride: usize| -> Result<(), String> {
            let start = count(r.offset.get())?;
            let base = count(outer.offset.get())?;
            let end = start
                .checked_add(
                    count(r.count.get())?
                        .checked_mul(stride)
                        .ok_or("cache range overflow")?,
                )
                .ok_or("cache range overflow")?;
            let limit = base
                .checked_add(
                    count(outer.count.get())?
                        .checked_mul(stride)
                        .ok_or("cache range overflow")?,
                )
                .ok_or("cache range overflow")?;
            if start < base || end > limit || (start - base) % stride != 0 {
                return Err("cache reference outside region".into());
            }
            Ok(())
        };
        let valid_target = |t| match t {
            Target::Atom(n) => (n as usize) < cache.atoms.len(),
            Target::Symbol(n) => (n as usize) < cache.symbols.len(),
        };
        let (mut next_payload, mut next_fixup) = (h.payloads.offset.get(), h.fixups.offset.get());
        for s in cache.sections {
            for n in [s.segment, s.name] {
                within(n, h.strings, 1)?;
                text(source, n)?;
            }
            within(s.data, h.payloads, 1)?;
            within(s.fixups, h.fixups, size_of::<FixupRecord>())?;
            if s.data.offset.get() != next_payload || s.fixups.offset.get() != next_fixup {
                return Err("overlapping or unordered section ranges".into());
            }
            next_payload += s.data.count.get();
            next_fixup += s.fixups.count.get() * size_of::<FixupRecord>() as u64;
            let size = count(s.size.get())?;
            let zero = matches!(s.flags.get() & 255, 1 | 0x12);
            if count(s.data.count.get())? != if zero { 0 } else { size } {
                return Err("invalid cached section size".into());
            }
            let mut previous_end = 0usize;
            for r in table::<FixupRecord>(source, s.fixups)? {
                let f = fixup(r)?;
                if (f.offset as usize) < previous_end {
                    return Err("cache fixups must be ordered and nonoverlapping".into());
                }
                previous_end = if let Some(low) = f.kind.low_offset() {
                    (low as usize).checked_add(4)
                } else {
                    (f.offset as usize).checked_add(f.kind.width())
                }
                .ok_or("cached fixup range overflow")?;
                if zero
                    || !valid_target(f.target)
                    || (f.offset as usize)
                        .checked_add(f.kind.width())
                        .is_none_or(|n| n > size)
                    || f.kind.low_offset().is_some_and(|n| {
                        n as usize + 4 > size || f.offset.checked_add(4) != Some(n)
                    })
                    || matches!(f.kind,K::Difference32{minus}|K::Difference64{minus} if !valid_target(minus))
                {
                    return Err("invalid cached fixup target or location".into());
                }
            }
        }
        if next_payload != h.payloads.offset.get() + h.payloads.count.get()
            || next_fixup
                != h.fixups.offset.get() + h.fixups.count.get() * size_of::<FixupRecord>() as u64
        {
            return Err("unreferenced cache records".into());
        }
        for a in cache.atoms {
            if a.flags.get() & !3 != 0
                || cache
                    .sections
                    .get(a.section.get() as usize)
                    .is_none_or(|s| a.offset.get() as u64 > s.size.get())
            {
                return Err("invalid cached atom".into());
            }
        }
        for s in cache.symbols {
            within(s.name, h.strings, 1)?;
            if text(source, s.name)?.is_empty() || s.flags.get() & !31 != 0 {
                return Err("invalid cached linkage name or flags".into());
            }
            if let Definition::Atom(n) = definition(s)?
                && n >= cache.atoms.len()
            {
                return Err("invalid cached symbol atom".into());
            }
        }
        for u in table::<UnwindRecord>(source, h.unwind)? {
            if u.owner.get() as usize >= cache.atoms.len()
                || u.flags.get() & !3 != 0
                || (u.flags.get() & 1 != 0 && !valid_target(target(u.personality.get())))
                || (u.flags.get() & 2 != 0 && !valid_target(target(u.lsda.get())))
                || (u.flags.get() & 1 == 0 && u.personality.get() != 0)
                || (u.flags.get() & 2 == 0 && (u.lsda.get() != 0))
            {
                return Err("invalid cached unwind reference".into());
            }
        }
        Ok(cache)
    }
    pub fn minimum(&self) -> u32 {
        self.header.minimum.get()
    }
    pub fn subsections(&self) -> bool {
        self.header.flags.get() & 1 != 0
    }
    pub fn atoms(&self) -> impl ExactSizeIterator<Item = Atom> + '_ {
        self.atoms.iter().map(|a| Atom {
            section: a.section.get(),
            offset: a.offset.get(),
            boundary: a.flags.get() & 1 != 0,
            no_dead_strip: a.flags.get() & 2 != 0,
        })
    }
    pub fn sections(&self) -> impl ExactSizeIterator<Item = SectionRef<'a>> + '_ {
        self.sections.iter().map(|s| SectionRef {
            segment: text(self.source, s.segment).expect("validated segment"),
            name: text(self.source, s.name).expect("validated section name"),
            size: s.size.get() as usize,
            alignment: s.alignment.get(),
            flags: s.flags.get(),
            data: bytes(self.source, s.data).expect("validated payload"),
            fixups: table(self.source, s.fixups).expect("validated fixups"),
        })
    }
    pub fn symbols(&self) -> impl ExactSizeIterator<Item = SymbolRef<'a>> + '_ {
        self.symbols.iter().map(|s| {
            let f = s.flags.get();
            SymbolRef {
                name: text(self.source, s.name).expect("validated name"),
                definition: definition(s).expect("validated definition"),
                private: f & 1 != 0,
                weak_definition: f & 2 != 0,
                weak_reference: f & 4 != 0,
                weak_can_be_hidden: f & 8 != 0,
                no_dead_strip: f & 16 != 0,
            }
        })
    }
    pub fn unwind(&self) -> impl ExactSizeIterator<Item = Unwind> + '_ {
        table::<UnwindRecord>(self.source, self.header.unwind)
            .expect("validated unwind records")
            .iter()
            .map(|u| Unwind {
                owner: u.owner.get(),
                length: u.length.get(),
                encoding: u.encoding.get(),
                personality: (u.flags.get() & 1 != 0).then(|| target(u.personality.get())),
                lsda: (u.flags.get() & 2 != 0).then(|| target(u.lsda.get())),
            })
    }
    pub fn to_object(&self) -> Object {
        Object {
            sections: self
                .sections()
                .map(|s| Section {
                    segment: s.segment.into(),
                    name: s.name.into(),
                    size: s.size,
                    alignment: s.alignment,
                    flags: s.flags,
                    data: s.data.to_vec(),
                    fixups: s.fixups().collect(),
                })
                .collect(),
            atoms: self.atoms().collect(),
            unwind: self.unwind().collect(),
            symbols: self.symbols().map(|s| s.to_symbol()).collect(),
            minimum: self.minimum(),
            subsections: self.subsections(),
        }
    }
}
pub(crate) fn encode(object: &Object) -> Result<Vec<u8>, String> {
    fn region(end: &mut usize, n: usize, stride: usize) -> Result<Range, String> {
        let r = range(*end, n);
        *end = end
            .checked_add(n.checked_mul(stride).ok_or("cache size overflow")?)
            .ok_or("cache size overflow")?;
        Ok(r)
    }
    let nfixups = object.sections.iter().try_fold(0usize, |n, s| {
        n.checked_add(s.fixups.len()).ok_or("too many fixups")
    })?;
    let mut end = size_of::<Header>();
    let sections_range = region(&mut end, object.sections.len(), size_of::<SectionRecord>())?;
    let atoms_range = region(&mut end, object.atoms.len(), size_of::<AtomRecord>())?;
    let symbols_range = region(&mut end, object.symbols.len(), size_of::<SymbolRecord>())?;
    let fixups_range = region(&mut end, nfixups, size_of::<FixupRecord>())?;
    let unwind_range = region(&mut end, object.unwind.len(), size_of::<UnwindRecord>())?;
    let mut names = Vec::new();
    let string_base = end;
    let mut name = |s: &str| {
        let r = range(string_base + names.len(), s.len());
        names.extend_from_slice(s.as_bytes());
        r
    };
    let mut sections = Vec::with_capacity(object.sections.len());
    let mut fixups = Vec::with_capacity(nfixups);
    let mut payload_len = 0usize;
    for s in &object.sections {
        let first = fixups.len();
        for f in &s.fixups {
            let (kind, aux) = match f.kind {
                K::Pointer64 => (0, 0),
                K::Branch26 => (1, 0),
                K::Page21 => (2, 0),
                K::PageOff12 => (3, 0),
                K::GotPage21 => (4, 0),
                K::GotPageOff12 => (5, 0),
                K::PointerToGot => (6, 0),
                K::TlvpPage21 => (7, 0),
                K::TlvpPageOff12 => (8, 0),
                K::Difference32 { minus } => (9, target_id(minus)?.get()),
                K::Difference64 { minus } => (10, target_id(minus)?.get()),
                K::PagePair { low_offset } => (11, low_offset),
                K::GotPagePair { low_offset } => (12, low_offset),
                K::TlvpPagePair { low_offset } => (13, low_offset),
            };
            fixups.push(FixupRecord {
                offset: U32::new(f.offset),
                target: target_id(f.target)?,
                addend: I64::new(f.addend),
                aux: U32::new(aux),
                kind: U16::new(kind),
                reserved: U16::new(0),
            });
        }
        sections.push(SectionRecord {
            segment: name(&s.segment),
            name: name(&s.name),
            size: U64::new(s.size as u64),
            alignment: U32::new(s.alignment),
            flags: U32::new(s.flags),
            data: range(payload_len, s.data.len()),
            fixups: range(
                count(fixups_range.offset.get())? + first * size_of::<FixupRecord>(),
                s.fixups.len(),
            ),
        });
        payload_len = payload_len
            .checked_add(s.data.len())
            .ok_or("cache payload overflow")?;
    }
    let atoms = object
        .atoms
        .iter()
        .map(|a| {
            Ok(AtomRecord {
                section: U32::new(a.section),
                offset: U32::new(a.offset),
                flags: U32::new(u32::from(a.boundary) | u32::from(a.no_dead_strip) << 1),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let symbols = object
        .symbols
        .iter()
        .map(|s| {
            let (tag, value, idx, alignment) = match s.definition {
                Definition::Undefined => (0, 0, 0, 0),
                Definition::Common { size, alignment } => (1, size, 0, alignment),
                Definition::Atom(n) => (2, 0, index(n)?.get(), 0),
            };
            let flags = u16::from(s.private)
                | (u16::from(s.weak_definition) << 1)
                | (u16::from(s.weak_reference) << 2)
                | (u16::from(s.weak_can_be_hidden) << 3)
                | (u16::from(s.no_dead_strip) << 4);
            Ok(SymbolRecord {
                name: name(&s.name),
                tag,
                value: U64::new(value),
                index: U32::new(idx),
                alignment,
                flags: U16::new(flags),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let unwind = object
        .unwind
        .iter()
        .map(|u| {
            Ok(UnwindRecord {
                owner: U32::new(u.owner),
                length: U32::new(u.length),
                encoding: U32::new(u.encoding),
                flags: U32::new(
                    u32::from(u.personality.is_some()) | u32::from(u.lsda.is_some()) << 1,
                ),
                personality: u
                    .personality
                    .map(target_id)
                    .transpose()?
                    .unwrap_or(U32::new(0)),
                lsda: u.lsda.map(target_id).transpose()?.unwrap_or(U32::new(0)),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let strings_range = region(&mut end, names.len(), 1)?;
    let payloads_range = region(&mut end, payload_len, 1)?;
    for s in &mut sections {
        s.data.offset = U64::new(s.data.offset.get() + payloads_range.offset.get());
    }
    let header = Header {
        magic: MAGIC,
        version: U32::new(VERSION),
        target: U32::new(1),
        sections: sections_range,
        atoms: atoms_range,
        symbols: symbols_range,
        fixups: fixups_range,
        unwind: unwind_range,
        strings: strings_range,
        payloads: payloads_range,
        minimum: U32::new(object.minimum),
        flags: U32::new(u32::from(object.subsections)),
    };
    let mut out = Vec::with_capacity(end);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(sections.as_bytes());
    out.extend_from_slice(atoms.as_bytes());
    out.extend_from_slice(symbols.as_bytes());
    out.extend_from_slice(fixups.as_bytes());
    out.extend_from_slice(unwind.as_bytes());
    out.extend_from_slice(&names);
    for s in &object.sections {
        out.extend_from_slice(&s.data);
    }
    Ok(out)
}
