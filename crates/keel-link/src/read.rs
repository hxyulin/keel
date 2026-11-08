use crate::{Error, Result, fail};
use std::borrow::Cow;

pub(crate) struct Reader<'a>(pub &'a [u8]);
impl<'a> Reader<'a> {
    pub fn bytes(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        self.0
            .get(at..at.checked_add(len).ok_or("offset overflow")?)
            .ok_or_else(|| "truncated Mach-O input".into())
    }
    pub fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap()))
    }
    pub fn u64(&self, at: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(at, 8)?.try_into().unwrap()))
    }
    pub fn string(&self, at: usize, len: usize) -> Result<String> {
        let bytes = self.bytes(at, len)?;
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(len);
        Ok(std::str::from_utf8(&bytes[..end])
            .map_err(|_| "non-UTF8 Mach-O name")?
            .into())
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Operation {
    #[default]
    Foreign,
    Single,
    Difference,
    PagePair,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct Reloc {
    pub offset: u32,
    pub symbol: u32,
    pub external: bool,
    pub pc: bool,
    pub width: u8,
    pub kind: u8,
    /// Zero for foreign Mach-O records; native operations carry explicit addends.
    pub operation: Operation,
    pub addend: i64,
    /// Subtracted symbol ID for a difference, low instruction offset for a pair.
    pub aux: u32,
}
#[derive(Debug, Clone)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: u8,
    pub section: usize,
    pub desc: u16,
    pub value: u64,
}
fn read_reloc(r: &Reader<'_>, at: usize) -> Result<Reloc> {
    let offset = r.u32(at)?;
    let bits = r.u32(at + 4)?;
    Ok(Reloc {
        offset,
        symbol: bits & 0xffffff,
        external: bits & (1 << 27) != 0,
        pc: bits & (1 << 24) != 0,
        width: 1 << ((bits >> 25) & 3),
        kind: (bits >> 28) as u8,
        ..Reloc::default()
    })
}
pub(crate) const UNSIGNED: u8 = object::macho::ARM64_RELOC_UNSIGNED;
pub(crate) const SUBTRACTOR: u8 = object::macho::ARM64_RELOC_SUBTRACTOR;
pub(crate) const ADDEND: u8 = object::macho::ARM64_RELOC_ADDEND;

/// One relocation after ARM64_RELOC_ADDEND prefixes and
/// SUBTRACTOR/UNSIGNED pairs are decoded and validated.
pub(crate) enum Fixup<'a> {
    Native(&'a Reloc),
    /// `addend` is the signed 24-bit value of a preceding ADDEND, or 0.
    Single {
        reloc: &'a Reloc,
        addend: i64,
    },
    /// SUBTRACTOR (`minus`) followed by UNSIGNED (`plus`) at the same field.
    Difference {
        minus: &'a Reloc,
        plus: &'a Reloc,
    },
}

/// Decode the fixups whose first record lies in `range`. Records outside it
/// are skipped one at a time without validation.
pub(crate) fn fixups(relocs: &[Reloc], range: std::ops::Range<usize>) -> Fixups<'_> {
    Fixups {
        relocs,
        at: 0,
        range,
    }
}
pub(crate) struct Fixups<'a> {
    relocs: &'a [Reloc],
    at: usize,
    range: std::ops::Range<usize>,
}
impl<'a> Fixups<'a> {
    fn decode(&mut self, r: &'a Reloc) -> Result<Fixup<'a>> {
        if r.operation != Operation::Foreign {
            return Ok(Fixup::Native(r));
        }
        match r.kind {
            ADDEND => {
                if r.external || r.pc || (r.width as usize) != 4 {
                    return fail("invalid ARM64_RELOC_ADDEND");
                }
                let next = self.relocs.get(self.at).ok_or("unpaired ADDEND")?;
                if next.offset != r.offset || !matches!(next.kind, 2..=4) {
                    return fail("ADDEND must precede matching branch/page relocation");
                }
                self.at += 1;
                Ok(Fixup::Single {
                    reloc: next,
                    addend: (((r.symbol as usize) as i32) << 8 >> 8) as i64,
                })
            }
            SUBTRACTOR => {
                let plus = self.relocs.get(self.at).ok_or("unpaired SUBTRACTOR")?;
                if !r.external
                    || r.pc
                    || plus.kind != UNSIGNED
                    || (plus.offset as usize) != (r.offset as usize)
                    || (plus.width as usize) != (r.width as usize)
                    || plus.pc
                    || !matches!(r.width as usize, 4 | 8)
                {
                    return fail("invalid SUBTRACTOR/UNSIGNED pair");
                }
                self.at += 1;
                Ok(Fixup::Difference { minus: r, plus })
            }
            _ => Ok(Fixup::Single {
                reloc: r,
                addend: 0,
            }),
        }
    }
}
impl<'a> Iterator for Fixups<'a> {
    type Item = Result<Fixup<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let r = self.relocs.get(self.at)?;
            self.at += 1;
            if self.range.contains(&(r.offset as usize)) {
                return Some(self.decode(r));
            }
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Section<'a> {
    pub segment: String,
    pub name: String,
    pub address: u64,
    pub size: usize,
    pub alignment: u32,
    pub flags: u32,
    pub data: Cow<'a, [u8]>,
    pub relocs: Vec<Reloc>,
    pub output: Option<usize>,
    pub offset: usize,
    /// The atoms of a section that dead stripping split, when some of them
    /// are dead. Empty when the section is placed whole.
    pub atoms: Vec<Atom>,
}
/// A byte range of an input section that the linker places on its own.
#[derive(Clone, Debug)]
pub(crate) struct Atom {
    pub start: usize,
    pub end: usize,
    pub live: bool,
    /// Its offset in the output section, once placed.
    pub offset: usize,
}
impl Section<'_> {
    /// The atom that a symbol at `offset` belongs to.
    fn atom(&self, offset: usize) -> Option<&Atom> {
        let i = self.atoms.partition_point(|a| a.start <= offset);
        self.atoms.get(i.checked_sub(1)?)
    }
    /// Whether the bytes or label at `offset` survive dead stripping.
    pub fn live_at(&self, offset: usize) -> bool {
        self.atom(offset).is_none_or(|a| a.live)
    }
    /// The output-section offset of input offset `offset`.
    pub fn place(&self, offset: usize) -> Result<usize> {
        match self.atom(offset) {
            None => Ok(self.offset + offset),
            Some(a) if a.live => Ok(a.offset + (offset - a.start)),
            Some(_) => fail("reference to a dead-stripped atom"),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Object<'a> {
    pub name: String,
    pub sections: Vec<Section<'a>>,
    pub symbols: Vec<Symbol>,
    pub unwind: Vec<NativeUnwind>,
    pub minimum: u32,
    pub debug_bytes: Option<Vec<u8>>,
    /// MH_SUBSECTIONS_VIA_SYMBOLS: no reference between two symbols' bytes
    /// in one section was resolved without a relocation.
    pub subsections: bool,
}
#[derive(Debug, Clone)]
pub(crate) struct NativeUnwind {
    pub owner: usize,
    pub length: u32,
    pub encoding: u32,
    pub personality: Option<usize>,
    pub lsda: Option<usize>,
}
impl<'a> Object<'a> {
    pub(crate) fn from_owned(name: String, object: keel_object::Object) -> Result<Self> {
        let keel_object::Object {
            sections,
            atoms,
            unwind,
            symbols,
            minimum,
            subsections,
        } = object;
        let (symbols, map) = native_symbols(&atoms, symbols)?;
        let unwind = unwind
            .into_iter()
            .map(|u| native_unwind(u, &map))
            .collect::<Result<_>>()?;
        let sections = sections
            .into_iter()
            .map(|s| {
                Ok(Section {
                    segment: s.segment,
                    name: s.name,
                    address: 0,
                    size: s.size,
                    alignment: s.alignment,
                    flags: s.flags,
                    data: Cow::Owned(s.data),
                    relocs: s
                        .fixups
                        .into_iter()
                        .map(|f| native_fixup(f, &map))
                        .collect::<Result<_>>()?,
                    output: None,
                    offset: 0,
                    atoms: Vec::new(),
                })
            })
            .collect::<Result<_>>()?;
        let result = Self {
            name,
            sections,
            symbols,
            unwind,
            minimum,
            subsections,
            debug_bytes: None,
        };
        validate_object(&result)?;
        Ok(result)
    }
    pub(crate) fn from_cache(name: String, cache: keel_object::Cache<'a>) -> Result<Self> {
        let atoms: Vec<_> = cache.atoms().collect();
        let (symbols, map) =
            native_symbols(&atoms, cache.symbols().map(|s| s.to_symbol()).collect())?;
        let unwind = cache
            .unwind()
            .map(|u| native_unwind(u, &map))
            .collect::<Result<_>>()?;
        let sections = cache
            .sections()
            .map(|s| {
                Ok(Section {
                    segment: s.segment.into(),
                    name: s.name.into(),
                    address: 0,
                    size: s.size,
                    alignment: s.alignment,
                    flags: s.flags,
                    data: Cow::Borrowed(s.data),
                    relocs: s
                        .fixups()
                        .map(|f| native_fixup(f, &map))
                        .collect::<Result<_>>()?,
                    output: None,
                    offset: 0,
                    atoms: Vec::new(),
                })
            })
            .collect::<Result<_>>()?;
        let result = Self {
            name,
            sections,
            symbols,
            unwind,
            minimum: cache.minimum(),
            subsections: cache.subsections(),
            debug_bytes: None,
        };
        validate_object(&result)?;
        Ok(result)
    }

    /// Attach owned payloads to an already validated archive summary. Reuse its
    /// symbols and section metadata rather than parsing/allocating them again.
    /// `bytes` must be the unchanged member used to construct this summary.
    pub(crate) fn materialize(&mut self, bytes: &[u8], retain_debug: bool) -> Result<()> {
        let r = Reader(bytes);
        let mut at = 32;
        let mut section_index = 0;
        for _ in 0..r.u32(16)? {
            if r.u32(at)? == 0x19 {
                for i in 0..r.u32(at + 64)? as usize {
                    let s = at + 72 + i * 80;
                    let section = &mut self.sections[section_index];
                    section_index += 1;
                    if !matches!(section.flags & 255, 1 | 0x12) && section.segment != "__DWARF" {
                        section.data = r
                            .bytes(r.u32(s + 48)? as usize, section.size)?
                            .to_vec()
                            .into();
                    }
                    let nr = r.u32(s + 60)? as usize;
                    let ro = r.u32(s + 56)? as usize;
                    section.relocs.reserve(nr);
                    for j in 0..nr {
                        section.relocs.push(read_reloc(&r, ro + j * 8)?);
                    }
                }
            }
            at += r.u32(at + 4)? as usize;
        }
        if retain_debug && self.sections.iter().any(|s| s.segment == "__DWARF") {
            self.debug_bytes = Some(bytes.to_vec());
        }
        Ok(())
    }

    pub fn parse(name: &str, bytes: &[u8], retain_debug: bool) -> Result<Self> {
        Self::read(name, bytes, retain_debug, true).map_err(|e| format!("{name}: {e}").into())
    }
    fn read(name: &str, bytes: &[u8], retain_debug: bool, materialize: bool) -> Result<Self> {
        let r = Reader(bytes);
        if r.u32(0)? != 0xfeedfacf || r.u32(4)? != 0x100000c || r.u32(12)? != 1 {
            return fail("expected a thin little-endian arm64 MH_OBJECT");
        }
        if r.u32(8)? & 0xffffff != 0 {
            return fail("arm64e/authenticated objects are unsupported");
        }
        let ncmds = r.u32(16)? as usize;
        let end = 32usize
            .checked_add(r.u32(20)? as usize)
            .ok_or("command overflow")?;
        r.bytes(32, end - 32)?;
        let mut at = 32;
        let mut sections = Vec::new();
        let mut symtab = None;
        let mut minimum = 11 << 16;
        for _ in 0..ncmds {
            let cmd = r.u32(at)?;
            let size = r.u32(at + 4)? as usize;
            if size < 8 || !size.is_multiple_of(8) || at + size > end {
                return fail("invalid load command bounds");
            }
            if matches!(cmd, 0x24 | 0x29) && size != 16 {
                return fail("invalid load command size");
            }
            match cmd {
                0x19 => {
                    let count = r.u32(at + 64)? as usize;
                    if size != 72 + count * 80 {
                        return fail("invalid segment section count");
                    }
                    for i in 0..count {
                        let s = at + 72 + i * 80;
                        let flags = r.u32(s + 64)?;
                        let len: usize =
                            r.u64(s + 40)?.try_into().map_err(|_| "section too large")?;
                        let address = r.u64(s + 32)?;
                        address
                            .checked_add(len as u64)
                            .ok_or("section address overflow")?;
                        let alignment = r.u32(s + 52)?;
                        if alignment > 16 {
                            return fail("section alignment exceeds 64 KiB");
                        }
                        let kind = flags & 255;
                        if kind == 0x15 {
                            return fail(
                                "legacy TLS initializer pointer sections are unsupported; use compiler-generated guarded TLS initialization",
                            );
                        }
                        if !matches!(
                            kind,
                            0 | 1 | 2 | 3 | 4 | 5 | 9 | 0xb | 0xe | 0x11 | 0x12 | 0x13
                        ) {
                            return fail(format!(
                                "unsupported section type {kind:#x} in {} (unsupported TLS/ObjC/indirect section form)",
                                r.string(s, 16)?
                            ));
                        }
                        let zero = kind == 1 || kind == 0x12;
                        if len > 512 * 1024 * 1024 {
                            return fail("section exceeds basic linker's 512 MiB limit");
                        }
                        let segment = r.string(s + 16, 16)?;
                        let data = if zero {
                            Vec::new()
                        } else {
                            // DWARF is omitted from the image. Its exact bytes, when
                            // requested, live in the object snapshot instead.
                            let data = r.bytes(r.u32(s + 48)? as usize, len)?;
                            if segment == "__DWARF" || !materialize {
                                Vec::new()
                            } else {
                                data.to_vec()
                            }
                        };
                        let nr = r.u32(s + 60)? as usize;
                        let ro = r.u32(s + 56)? as usize;
                        r.bytes(ro, nr.checked_mul(8).ok_or("relocation overflow")?)?;
                        let mut relocs = Vec::new();
                        for j in 0..nr {
                            let relocation = read_reloc(&r, ro + j * 8)?;
                            if (relocation.offset as usize) & 0x80000000 != 0 {
                                return fail("scattered/negative relocations are unsupported");
                            }
                            if (relocation.offset as usize) + (relocation.width as usize) > len
                                || zero
                            {
                                return fail("relocation outside file-backed section");
                            }
                            if materialize {
                                relocs.push(relocation);
                            }
                        }
                        sections.push(Section {
                            name: r.string(s, 16)?,
                            segment,
                            address,
                            size: len,
                            alignment,
                            flags,
                            data: Cow::Owned(data),
                            relocs,
                            output: None,
                            offset: 0,
                            atoms: Vec::new(),
                        });
                    }
                }
                2 => {
                    if size != 24 || symtab.is_some() {
                        return fail("invalid/duplicate symbol table");
                    }
                    symtab = Some((
                        r.u32(at + 8)? as usize,
                        r.u32(at + 12)? as usize,
                        r.u32(at + 16)? as usize,
                        r.u32(at + 20)? as usize,
                    ));
                }
                0x32 => {
                    if size < 24 || r.u32(at + 8)? != 1 {
                        return fail("only macOS build versions are supported");
                    }
                    minimum = r.u32(at + 12)?;
                }
                0x24 => {
                    minimum = r.u32(at + 8)?;
                }
                0xb | 0x2e | 0x29 => {} // dysymtab, optimization hints, data-in-code (checked below)
                _ => return fail(format!("unsupported object load command {cmd:#x}")),
            }
            if cmd == 0x29 && r.u32(at + 12)? != 0 {
                return fail("LC_DATA_IN_CODE requires the system linker");
            }
            at += size;
        }
        if at != end {
            return fail("load command size mismatch");
        }
        let (so, count, st, sl) = symtab.ok_or("missing LC_SYMTAB")?;
        r.bytes(so, count.checked_mul(16).ok_or("symbol overflow")?)?;
        let strings = Reader(r.bytes(st, sl)?);
        let mut symbols = Vec::new();
        for i in 0..count {
            let s = so + i * 16;
            let strx = r.u32(s)? as usize;
            if strx >= sl {
                return fail("invalid string index");
            }
            let kind = r.bytes(s + 4, 1)?[0];
            let section = r.bytes(s + 5, 1)?[0] as usize;
            let desc = u16::from_le_bytes(r.bytes(s + 6, 2)?.try_into().unwrap());
            if kind & 0xe0 != 0 || !matches!(kind & 0xe, 0 | 2 | 0xe) {
                return fail("STABS/indirect symbols are unsupported");
            }
            if kind & 0xe != 0 && desc & 0x100 != 0 {
                return fail("resolver symbols require the system linker");
            }
            if desc & 0x80 != 0 && kind & 0xe != 0 && (kind & 0xe != 0xe || kind & 1 == 0) {
                return fail(
                    "weak definitions require external section symbols; local/absolute weak definitions are unsupported",
                );
            }
            let value = r.u64(s + 8)?;
            if kind & 0xe != 0xe && section != 0 {
                return fail("undefined/absolute symbol has a section ordinal");
            }
            if kind & 0xe == 0 && kind & 1 == 0 {
                return fail("non-external undefined symbol");
            }
            if !strings.bytes(strx, sl - strx)?.contains(&0) {
                return fail("unterminated symbol name");
            }
            if kind & 0xe == 0xe {
                let sec = sections
                    .get(section.wrapping_sub(1))
                    .ok_or("symbol has invalid section")?;
                if value < sec.address || value - sec.address > sec.size as u64 {
                    return fail("symbol outside its section");
                }
            }
            symbols.push(Symbol {
                name: strings.string(strx, sl - strx)?,
                kind,
                section,
                desc,
                value,
            });
        }
        let debug_bytes = (retain_debug && sections.iter().any(|s| s.segment == "__DWARF"))
            .then(|| bytes.to_vec());
        Ok(Self {
            name: name.into(),
            sections,
            symbols,
            minimum,
            debug_bytes,
            unwind: Vec::new(),
            subsections: r.u32(24)? & 0x2000 != 0,
        })
    }
}

/// Validate every member in input order, retaining symbol/section summaries but
/// borrowing payloads until extraction. Archive indexes are never trusted.
pub(crate) fn archive<'a>(
    name: &str,
    bytes: &'a [u8],
) -> Result<(Vec<Object<'static>>, Vec<&'a [u8]>)> {
    let r = Reader(bytes);
    if r.bytes(0, 8)? != b"!<arch>\n" {
        return fail("expected BSD ar archive");
    }
    let mut at = 8;
    let mut members = Vec::new();
    let mut bodies = Vec::new();
    while at < bytes.len() {
        let header = r.bytes(at, 60)?;
        if &header[58..] != b"`\n" {
            return fail(format!("{name}: invalid archive header"));
        }
        let raw = std::str::from_utf8(&header[..16])
            .map_err(|_| "invalid archive name")?
            .trim();
        let size: usize = std::str::from_utf8(&header[48..58])
            .map_err(|_| "invalid ar size")?
            .trim()
            .parse()
            .map_err(|_| "invalid ar size")?;
        let data = r.bytes(at + 60, size)?;
        let (member, body) = if let Some(len) = raw.strip_prefix("#1/") {
            let len: usize = len.parse().map_err(|_| "invalid BSD ar name length")?;
            if len > data.len() {
                return fail("truncated ar extended name");
            }
            (Reader(data).string(0, len)?, &data[len..])
        } else {
            (raw.trim_end_matches('/').into(), data)
        };
        if !member.starts_with("__.SYMDEF") && !matches!(member.as_str(), "" | "/" | "/SYM64") {
            let name = format!("{name}({member})");
            members
                .push(Object::read(&name, body, false, false).map_err(|e| format!("{name}: {e}"))?);
            bodies.push(body);
        }
        at = (at + 60 + size + 1) & !1;
    }
    Ok((members, bodies))
}

/// Owned/cache inputs receive the same structural checks as decoded Mach-O.
fn validate_object(object: &Object<'_>) -> Result<()> {
    for section in &object.sections {
        let kind = section.flags & 255;
        let zero = matches!(kind, 1 | 0x12);
        if section.alignment > 16
            || section.size > 512 * 1024 * 1024
            || section.address.checked_add(section.size as u64).is_none()
            || !matches!(
                kind,
                0 | 1 | 2 | 3 | 4 | 5 | 9 | 0xb | 0xe | 0x11 | 0x12 | 0x13
            )
            || (if zero {
                !section.data.is_empty()
            } else {
                section.data.len() != section.size
            })
            || section.name.is_empty()
            || section.segment.is_empty()
            || section.name.contains('\0')
            || section.segment.contains('\0')
        {
            return fail("invalid owned section");
        }
        let mut previous_end = 0usize;
        for r in &section.relocs {
            if (r.operation == Operation::Difference
                && (r.aux as usize >= object.symbols.len()
                    || r.kind != 0
                    || !matches!(r.width as usize, 4 | 8)))
                || (r.operation == Operation::PagePair
                    && (!matches!(r.kind, 3 | 5 | 8)
                        || r.aux as usize + 4 > section.size
                        || r.offset.checked_add(4) != Some(r.aux)))
                || zero
                || !matches!(r.width as usize, 1 | 2 | 4 | 8)
                || (r.offset as usize)
                    .checked_add(r.width as usize)
                    .is_none_or(|end| end > section.size)
                || r.kind > 10
                || (r.kind != 10
                    && if r.external {
                        (r.symbol as usize) >= object.symbols.len()
                    } else {
                        (r.symbol as usize) == 0 || (r.symbol as usize) > object.sections.len()
                    })
                || (r.kind == 10 && (r.symbol as usize) > 0xffffff)
            {
                return fail("invalid owned relocation");
            }
            if r.operation != Operation::Foreign {
                if (r.offset as usize) < previous_end {
                    return fail("native fixups must be ordered and nonoverlapping");
                }
                previous_end = if r.operation == Operation::PagePair {
                    r.aux as usize + 4
                } else {
                    r.offset as usize + r.width as usize
                };
            }
        }
    }
    for s in &object.symbols {
        let kind = s.kind & 0xe;
        if s.name.contains('\0')
            || s.kind & 0xe0 != 0
            || !matches!(kind, 0 | 2 | 0xe)
            || (kind != 0xe && s.section != 0)
            || (kind == 0 && s.kind & 1 == 0)
            || (kind != 0 && s.desc & 0x100 != 0)
            || (s.desc & 0x80 != 0 && kind != 0 && (kind != 0xe || s.kind & 1 == 0))
        {
            return fail("invalid owned symbol");
        }
        if kind == 0xe {
            let section = object
                .sections
                .get(s.section.wrapping_sub(1))
                .ok_or("invalid owned symbol section")?;
            if s.value < section.address || s.value - section.address > section.size as u64 {
                return fail("owned symbol outside its section");
            }
        }
    }
    Ok(())
}

struct NativeMap {
    atoms: Vec<usize>,
    symbols: usize,
}
fn native_symbols(
    atoms: &[keel_object::Atom],
    named: Vec<keel_object::Symbol>,
) -> Result<(Vec<Symbol>, NativeMap)> {
    use keel_object::Definition;
    for a in atoms {
        a.section
            .checked_add(1)
            .ok_or("invalid native atom section")?;
    }
    let count = named.len();
    let mut symbols = Vec::with_capacity(count + atoms.len());
    for s in named {
        let defined = matches!(s.definition, Definition::Atom(_));
        if s.name.is_empty()
            || s.name.contains('\0')
            || (s.weak_definition && !defined)
            || (s.weak_reference && !matches!(s.definition, Definition::Undefined))
            || (s.weak_can_be_hidden && !s.weak_definition)
            || matches!(s.definition,Definition::Common{size,alignment} if size==0 || alignment>15)
        {
            return fail("invalid native linkage symbol");
        }
        let (kind, section, value, alignment) = match s.definition {
            Definition::Undefined => (1, 0, 0, 0),
            Definition::Common { size, alignment } => (1, 0, size, alignment),
            Definition::Atom(id) => {
                let a = atoms.get(id).ok_or("invalid native symbol atom")?;
                (0xf, a.section as usize + 1, a.offset as u64, 0)
            }
        };
        symbols.push(Symbol {
            name: s.name,
            kind: kind | if s.private { 0x10 } else { 0 },
            section,
            value,
            desc: (u16::from(alignment) << 8)
                | if s.no_dead_strip { 0x20 } else { 0 }
                | if s.weak_definition { 0x80 } else { 0 }
                | if s.weak_reference || s.weak_can_be_hidden {
                    0x40
                } else {
                    0
                },
        });
    }
    let map: Vec<_> = atoms
        .iter()
        .map(|a| {
            let id = symbols.len();
            symbols.push(Symbol {
                name: String::new(),
                kind: 0xe,
                section: a.section as usize + 1,
                value: a.offset as u64,
                desc: if a.boundary { 0 } else { 0x200 } | if a.no_dead_strip { 0x20 } else { 0 },
            });
            id
        })
        .collect();
    Ok((
        symbols,
        NativeMap {
            atoms: map,
            symbols: count,
        },
    ))
}
fn native_fixup(f: keel_object::Fixup, map: &NativeMap) -> Result<Reloc> {
    use Operation::{Difference, PagePair, Single};
    use keel_object::FixupKind as K;
    let index = |t| map.index(t);
    let (kind, operation, aux) = match f.kind {
        K::Pointer64 => (0, Single, 0),
        K::Branch26 => (2, Single, 0),
        K::Page21 => (3, Single, 0),
        K::PageOff12 => (4, Single, 0),
        K::GotPage21 => (5, Single, 0),
        K::GotPageOff12 => (6, Single, 0),
        K::PointerToGot => (7, Single, 0),
        K::TlvpPage21 => (8, Single, 0),
        K::TlvpPageOff12 => (9, Single, 0),
        K::Difference32 { minus } | K::Difference64 { minus } => (
            0,
            Difference,
            u32::try_from(index(minus)?).map_err(|_| "native target exceeds 32 bits")?,
        ),
        K::PagePair { low_offset } => (3, PagePair, low_offset),
        K::GotPagePair { low_offset } => (5, PagePair, low_offset),
        K::TlvpPagePair { low_offset } => (8, PagePair, low_offset),
    };
    Ok(Reloc {
        offset: f.offset,
        symbol: u32::try_from(index(f.target)?).map_err(|_| "native target exceeds 32 bits")?,
        external: true,
        pc: matches!(kind, 2 | 3 | 5 | 7 | 8),
        width: f.kind.width() as u8,
        kind,
        operation,
        addend: f.addend,
        aux,
    })
}

fn native_unwind(u: keel_object::Unwind, map: &NativeMap) -> Result<NativeUnwind> {
    Ok(NativeUnwind {
        owner: map
            .atoms
            .get(u.owner as usize)
            .copied()
            .ok_or("invalid native unwind owner")?,
        length: u.length,
        encoding: u.encoding,
        personality: u.personality.map(|t| map.index(t)).transpose()?,
        lsda: u.lsda.map(|t| map.index(t)).transpose()?,
    })
}
impl NativeMap {
    fn index(&self, t: keel_object::Target) -> Result<usize> {
        match t {
            keel_object::Target::Atom(id) => self
                .atoms
                .get(id as usize)
                .copied()
                .ok_or_else(|| Error::from("invalid native atom target")),
            keel_object::Target::Symbol(id) if (id as usize) < self.symbols => Ok(id as usize),
            _ => fail("invalid native symbol target"),
        }
    }
}
