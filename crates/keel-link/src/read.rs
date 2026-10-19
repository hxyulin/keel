use crate::{Result, fail};

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
#[derive(Clone, Debug)]
pub(crate) struct Reloc {
    pub offset: usize,
    pub symbol: usize,
    pub external: bool,
    pub pc: bool,
    pub width: usize,
    pub kind: u32,
}
#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub segment: String,
    pub name: String,
    pub address: u64,
    pub size: usize,
    pub alignment: u32,
    pub flags: u32,
    pub data: Vec<u8>,
    pub relocs: Vec<Reloc>,
    pub output: Option<usize>,
    pub offset: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: u8,
    pub section: usize,
    pub desc: u16,
    pub value: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub name: String,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub minimum: u32,
    pub debug_bytes: Option<Vec<u8>>,
}
impl Object {
    pub fn parse(name: &str, bytes: &[u8], retain_debug: bool) -> Result<Self> {
        Self::read(name, bytes, retain_debug).map_err(|e| format!("{name}: {e}"))
    }
    fn read(name: &str, bytes: &[u8], retain_debug: bool) -> Result<Self> {
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
                            if segment == "__DWARF" {
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
                            let off = r.u32(ro + j * 8)?;
                            if off & 0x80000000 != 0 {
                                return fail("scattered/negative relocations are unsupported");
                            }
                            let bits = r.u32(ro + j * 8 + 4)?;
                            let width = 1 << ((bits >> 25) & 3);
                            if off as usize + width > len || zero {
                                return fail("relocation outside file-backed section");
                            }
                            relocs.push(Reloc {
                                offset: off as usize,
                                symbol: (bits & 0xffffff) as usize,
                                external: bits & (1 << 27) != 0,
                                pc: bits & (1 << 24) != 0,
                                width,
                                kind: bits >> 28,
                            });
                        }
                        sections.push(Section {
                            name: r.string(s, 16)?,
                            segment,
                            address: r.u64(s + 32)?,
                            size: len,
                            alignment,
                            flags,
                            data,
                            relocs,
                            output: None,
                            offset: 0,
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
        })
    }
}

/// BSD/Darwin ar members; archive indexes are hints, never trusted for resolution.
pub(crate) fn archive(name: &str, bytes: &[u8], retain_debug: bool) -> Result<Vec<Object>> {
    let r = Reader(bytes);
    if r.bytes(0, 8)? != b"!<arch>\n" {
        return fail("expected BSD ar archive");
    }
    let mut at = 8;
    let mut members = Vec::new();
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
            members.push(Object::parse(
                &format!("{name}({member})"),
                body,
                retain_debug,
            )?);
        }
        at = (at + 60 + size + 1) & !1;
    }
    Ok(members)
}
