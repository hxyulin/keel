//! DWARF line programs and call-frame records. Addresses are section-relative
//! until the Mach-O linker applies the ordinary pointer/difference relocations.
use super::compact::{self, Rule};
use super::data::{self, Location};
use crate::{Assembly, Error, Relocation, RelocationKind, SectionKind};
use bumpalo::{Bump, collections::Vec as BumpVec};
use std::collections::BTreeMap;

pub(super) struct Debug<'a> {
    arena: &'a Bump,
    files: BTreeMap<u32, Vec<u8>>,
    rows: BumpVec<'a, Row>,
    frames: BumpVec<'a, Frame<'a>>,
    active: Option<Frame<'a>>,
}
struct Row {
    address: u64,
    file: u32,
    line: u32,
    column: u32,
    prologue: bool,
}
struct Frame<'a> {
    start: u64,
    end: u64,
    line: usize,
    events: BumpVec<'a, (u64, BumpVec<'a, u8>)>,
    rules: BumpVec<'a, Rule>,
    saved: usize,
    personality: Option<&'a str>,
    lsda: Option<&'a str>,
}
pub(super) fn uleb(mut n: u64, out: &mut impl Extend<u8>) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        out.extend([b | if n != 0 { 128 } else { 0 }]);
        if n == 0 {
            break;
        }
    }
}
pub(super) fn sleb(mut n: i64, out: &mut impl Extend<u8>) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        let end = (n == 0 && b & 64 == 0) || (n == -1 && b & 64 != 0);
        out.extend([b | if end { 0 } else { 128 }]);
        if end {
            break;
        }
    }
}
pub(super) fn string(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err("expected quoted string".into());
    }
    let mut out = Vec::new();
    let mut chars = s.as_bytes()[1..s.len() - 1].iter().copied().peekable();
    while let Some(b) = chars.next() {
        if b == b'"' {
            return Err("unexpected quote in string".into());
        }
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let b = chars.next().ok_or("unterminated string escape")?;
        out.push(match b {
            b'n' => b'\n',
            b't' => b'\t',
            b'r' => b'\r',
            b'"' => b'"',
            b'\\' => b'\\',
            b'0'..=b'7' => {
                let mut n = u16::from(b - b'0');
                for _ in 0..2 {
                    if let Some(b'0'..=b'7') = chars.peek() {
                        n = n * 8 + u16::from(chars.next().unwrap() - b'0');
                    } else {
                        break;
                    }
                }
                u8::try_from(n).map_err(|_| "octal escape exceeds a byte")?
            }
            _ => return Err("unsupported string escape".into()),
        });
    }
    Ok(out)
}
fn reg(s: &str) -> Result<u64, String> {
    if s == "sp" {
        return Ok(31);
    }
    if s == "fp" {
        return Ok(29);
    }
    if s == "lr" {
        return Ok(30);
    }
    if let Some(n) = s.strip_prefix('x') {
        let n: u64 = n.parse().map_err(|_| "invalid CFI register")?;
        if n <= 30 {
            return Ok(n);
        }
    }
    if let Some(n) = s.strip_prefix('d').or_else(|| s.strip_prefix('v')) {
        let n: u64 = n.parse().map_err(|_| "invalid CFI register")?;
        if n <= 31 {
            return Ok(n + 64);
        }
    }
    let n: u64 = s.parse().map_err(|_| "invalid CFI register")?;
    if n <= 95 {
        Ok(n)
    } else {
        Err("invalid CFI register".into())
    }
}
impl<'a> Debug<'a> {
    pub(super) fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            files: BTreeMap::new(),
            rows: BumpVec::new_in(arena),
            frames: BumpVec::new_in(arena),
            active: None,
        }
    }
    pub(super) fn directive(
        &mut self,
        op: &str,
        operands: &'a str,
        section: usize,
        address: u64,
        line: usize,
    ) -> Result<bool, String> {
        if op == ".file" {
            let (id, path) = operands
                .split_once(char::is_whitespace)
                .ok_or("expected .file number and path")?;
            let id: u32 = id.parse().map_err(|_| "invalid file number")?;
            if id == 0 || self.files.insert(id, string(path)?).is_some() {
                return Err("duplicate or zero file number".into());
            }
            if self.files[&id].contains(&0) {
                return Err("file path contains NUL".into());
            }
            return Ok(true);
        }
        if op == ".loc" {
            if section != 0 {
                return Err(".loc requires __text".into());
            }
            let args: Vec<_> = operands.split_whitespace().collect();
            if !(3..=4).contains(&args.len()) || (args.len() == 4 && args[3] != "prologue_end") {
                return Err("expected .loc file line column [prologue_end]".into());
            }
            let number = |s: &str| s.parse::<u32>().map_err(|_| "invalid .loc number");
            let file = number(args[0])?;
            if !self.files.contains_key(&file) {
                return Err(".loc references undeclared file".into());
            }
            self.rows.push(Row {
                address,
                file,
                line: number(args[1])?,
                column: number(args[2])?,
                prologue: args.len() == 4,
            });
            return Ok(true);
        }
        if !op.starts_with(".cfi_") {
            return Ok(false);
        }
        if section != 0 {
            return Err("CFI directives require __text".into());
        }
        if op == ".cfi_startproc" {
            if !operands.is_empty() || self.active.is_some() {
                return Err("invalid or nested .cfi_startproc".into());
            }
            self.active = Some(Frame {
                start: address,
                end: address,
                line,
                events: BumpVec::new_in(self.arena),
                rules: BumpVec::new_in(self.arena),
                saved: 0,
                personality: None,
                lsda: None,
            });
            return Ok(true);
        }
        if op == ".cfi_endproc" {
            if !operands.is_empty() {
                return Err(".cfi_endproc takes no operands".into());
            }
            let mut frame = self
                .active
                .take()
                .ok_or(".cfi_endproc without .cfi_startproc")?;
            if frame.saved != 0 {
                return Err("unbalanced CFI state stack".into());
            }
            frame.end = address;
            self.frames.push(frame);
            return Ok(true);
        }
        let frame = self
            .active
            .as_mut()
            .ok_or("CFI directive outside a procedure")?;
        let args: Vec<_> = operands
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let mut bytes = BumpVec::new_in(self.arena);
        let mut rule = Rule::Other;
        match (op, args.as_slice()) {
            (".cfi_personality", [encoding, name]) | (".cfi_lsda", [encoding, name]) => {
                let personality = op == ".cfi_personality";
                let expected = if personality { 155 } else { 16 };
                let encoding = super::unsigned(encoding, 255)?;
                if encoding != expected {
                    return Err(format!("{op} currently requires encoding {expected}"));
                }
                super::symbol(name)?;
                let slot = if personality {
                    &mut frame.personality
                } else {
                    &mut frame.lsda
                };
                if slot.replace(*name).is_some() {
                    return Err(format!("duplicate {op}"));
                }
                return Ok(true);
            }
            (".cfi_def_cfa", [r, n]) => {
                let r = reg(r)?;
                let n = n.parse().map_err(|_| "invalid CFA offset")?;
                bytes.push(0x0c);
                uleb(r, &mut bytes);
                uleb(n, &mut bytes);
                rule = Rule::Cfa(r, n);
            }
            (".cfi_def_cfa_register", [r]) => {
                bytes.push(0x0d);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_def_cfa_offset", [n]) => {
                let n = n.parse().map_err(|_| "invalid CFA offset")?;
                bytes.push(0x0e);
                uleb(n, &mut bytes);
                rule = Rule::CfaOffset(n);
            }
            (".cfi_offset", [r, n]) => {
                let n: i64 = n.parse().map_err(|_| "invalid saved-register offset")?;
                if n > 0 || n % 8 != 0 {
                    return Err("CFI offset must be a nonpositive multiple of 8".into());
                }
                let r = reg(r)?;
                bytes.push(0x05);
                uleb(r, &mut bytes);
                uleb(n.unsigned_abs() / 8, &mut bytes);
                rule = Rule::Offset(r, n);
            }
            (".cfi_restore", [r]) => {
                bytes.push(0x06);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_same_value", [r]) => {
                bytes.push(0x08);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_undefined", [r]) => {
                bytes.push(0x07);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_remember_state", []) => {
                bytes.push(0x0a);
                frame.saved += 1;
            }
            (".cfi_restore_state", []) => {
                if frame.saved == 0 {
                    return Err("CFI state stack underflow".into());
                }
                frame.saved -= 1;
                bytes.push(0x0b);
            }
            _ => return Err(format!("unsupported CFI directive or operands `{op}`")),
        }
        frame.rules.push(rule);
        frame.events.push((address, bytes));
        Ok(true)
    }
    pub(super) fn finish(
        self,
        a: &mut Assembly,
        labels: &mut BTreeMap<&'a str, Location>,
        prefix: &str,
    ) -> Result<(), Error> {
        if let Some(frame) = self.active {
            return Err(Error::new(frame.line, "unterminated .cfi_startproc"));
        }
        let mut references = References {
            arena: self.arena,
            labels,
            prefix,
            counter: 0,
        };
        if !self.frames.is_empty() {
            if a.sections
                .iter()
                .any(|s| s.segment == "__TEXT" && s.name == "__eh_frame" && s.size() != 0)
            {
                return Err(Error::new(
                    0,
                    "cannot mix CFI with explicit __eh_frame contents",
                ));
            }
            // Darwin's linker builds its runtime unwind index from these
            // entries. DWARF mode asks it to find the corresponding FDE.
            let compact = data::select(a, "__LD", "__compact_unwind", SectionKind::Debug)
                .map_err(|s| Error::new(0, s))?;
            if a.sections[compact - 1].size() != 0 {
                return Err(Error::new(
                    0,
                    "cannot mix CFI with explicit compact unwind contents",
                ));
            }
            a.sections[compact - 1].alignment = 3;
            let mut index = Vec::new();
            let encodings: Vec<_> = self
                .frames
                .iter()
                .map(|frame| compact::encoding(&frame.rules, frame.personality))
                .collect();
            for (frame, encoding) in self.frames.iter().zip(&encodings) {
                references.address(&mut index, compact, frame.start, false, a);
                let length = u32::try_from(frame.end - frame.start).map_err(|_| {
                    Error::new(frame.line, "unwind function length exceeds 32 bits")
                })?;
                index.extend_from_slice(&length.to_le_bytes());
                let encoding = encoding
                    .map(|e| e | if frame.lsda.is_some() { 0x4000_0000 } else { 0 })
                    .unwrap_or(0x0300_0000);
                index.extend_from_slice(&encoding.to_le_bytes());
                if encoding & 0x0f00_0000 != 0x0300_0000 {
                    if let Some(personality) = &frame.personality {
                        references.symbol(&mut index, compact, personality.to_string(), false, a);
                    } else {
                        index.extend_from_slice(&[0; 8]);
                    }
                    if let Some(lsda) = &frame.lsda {
                        references.symbol(&mut index, compact, lsda.to_string(), false, a);
                    } else {
                        index.extend_from_slice(&[0; 8]);
                    }
                } else {
                    index.extend_from_slice(&[0; 16]);
                }
            }
            a.sections[compact - 1].contents = crate::SectionContents::Bytes(index);
            if encodings.iter().any(Option::is_none) {
                let section = data::select(a, "__TEXT", "__eh_frame", SectionKind::EhFrame)
                    .map_err(|s| Error::new(0, s))?;
                a.sections[section - 1].alignment = 3;
                let mut bytes = Vec::new();
                let mut cies = BTreeMap::new();
                for (frame, encoding) in self.frames.into_iter().zip(encodings) {
                    if encoding.is_some() {
                        continue;
                    }
                    let key = (frame.personality, frame.lsda.is_some());
                    let cie = if let Some(cie) = cies.get(&key) {
                        *cie
                    } else {
                        let cie = bytes.len();
                        bytes.extend_from_slice(&[0; 8]);
                        bytes.push(1);
                        bytes.push(b'z');
                        if frame.personality.is_some() {
                            bytes.push(b'P');
                        }
                        if frame.lsda.is_some() {
                            bytes.push(b'L');
                        }
                        bytes.extend_from_slice(b"R\0");
                        bytes.extend_from_slice(&[1, 0x78, 30]);
                        let augmentation = 1
                            + usize::from(frame.lsda.is_some())
                            + if frame.personality.is_some() { 5 } else { 0 };
                        uleb(augmentation as u64, &mut bytes);
                        if let Some(personality) = &frame.personality {
                            bytes.push(0x9b); // indirect, PC-relative signed 32-bit GOT pointer
                            a.relocations.push(Relocation {
                                section,
                                size: 4,
                                subtract_symbol: None,
                                offset: bytes.len() as u32,
                                symbol: personality.to_string(),
                                kind: RelocationKind::PointerToGot,
                                addend: 0,
                            });
                            bytes.extend_from_slice(&[0; 4]);
                        }
                        if frame.lsda.is_some() {
                            bytes.push(0x10);
                        }
                        bytes.push(0x10); // PC-relative native-width FDE address
                        bytes.extend_from_slice(&[0x0c, 31, 0]);
                        while !bytes.len().is_multiple_of(4) {
                            bytes.push(0);
                        }
                        let len = (bytes.len() - cie - 4) as u32;
                        bytes[cie..cie + 4].copy_from_slice(&len.to_le_bytes());
                        cies.insert(key, cie);
                        cie
                    };
                    let start = bytes.len();
                    bytes.extend_from_slice(&[0; 4]);
                    bytes.extend_from_slice(&((start + 4 - cie) as u32).to_le_bytes());
                    references.address(&mut bytes, section, frame.start, true, a);
                    bytes.extend_from_slice(&(frame.end - frame.start).to_le_bytes());
                    bytes.push(if frame.lsda.is_some() { 8 } else { 0 });
                    if let Some(lsda) = &frame.lsda {
                        references.symbol(&mut bytes, section, lsda.to_string(), true, a);
                    }
                    let mut position = frame.start;
                    for (address, event) in frame.events {
                        let delta = address - position;
                        if delta != 0 {
                            bytes.push(0x04);
                            bytes.extend_from_slice(
                                &u32::try_from(delta)
                                    .map_err(|_| {
                                        Error::new(
                                            frame.line,
                                            "CFI address advance exceeds 32 bits",
                                        )
                                    })?
                                    .to_le_bytes(),
                            );
                        }
                        bytes.extend(event);
                        position = address;
                    }
                    while bytes.len() % 8 != 0 {
                        bytes.push(0);
                    }
                    let len = (bytes.len() - start - 4) as u32;
                    bytes[start..start + 4].copy_from_slice(&len.to_le_bytes());
                }
                a.sections[section - 1].contents = crate::SectionContents::Bytes(bytes);
            }
        }
        if !self.rows.is_empty() {
            if self.files.keys().copied().ne(1..=self.files.len() as u32) {
                return Err(Error::new(
                    0,
                    "DWARF file numbers must be contiguous starting at 1",
                ));
            }
            let section = data::select(a, "__DWARF", "__debug_line", SectionKind::Debug)
                .map_err(|s| Error::new(0, s))?;
            if a.sections[section - 1].size() != 0 {
                return Err(Error::new(
                    0,
                    "cannot mix .loc with explicit __debug_line contents",
                ));
            }
            let mut bytes = vec![0; 4];
            bytes.extend_from_slice(&4u16.to_le_bytes());
            bytes.extend_from_slice(&[0; 4]);
            let header = bytes.len();
            bytes.extend_from_slice(&[
                1,
                1,
                1,
                (-5i8) as u8,
                14,
                13,
                0,
                1,
                1,
                1,
                1,
                0,
                0,
                0,
                1,
                0,
                0,
                1,
                0,
            ]);
            for path in self.files.values() {
                bytes.extend(path);
                bytes.extend_from_slice(&[0, 0, 0, 0]);
            }
            bytes.push(0);
            let len = (bytes.len() - header) as u32;
            bytes[6..10].copy_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(&[0, 9, 2]);
            references.address(&mut bytes, section, 0, false, a);
            let (mut address, mut file, mut line, mut column) = (0, 1, 1i64, 0);
            for row in self.rows {
                if row.address != address {
                    bytes.push(2);
                    uleb(row.address - address, &mut bytes);
                    address = row.address;
                }
                if row.file != file {
                    bytes.push(4);
                    uleb(row.file.into(), &mut bytes);
                    file = row.file;
                }
                if i64::from(row.line) != line {
                    bytes.push(3);
                    sleb(i64::from(row.line) - line, &mut bytes);
                    line = row.line.into();
                }
                if row.column != column {
                    bytes.push(5);
                    uleb(row.column.into(), &mut bytes);
                    column = row.column;
                }
                if row.prologue {
                    bytes.push(10);
                }
                bytes.push(1);
            }
            bytes.push(2);
            uleb(a.text.len() as u64 - address, &mut bytes);
            bytes.extend_from_slice(&[0, 1, 1]);
            let len = (bytes.len() - 4) as u32;
            bytes[..4].copy_from_slice(&len.to_le_bytes());
            a.sections[section - 1].contents = crate::SectionContents::Bytes(bytes);
        }
        Ok(())
    }
}

/// Synthetic labels let the existing Mach-O difference relocation machinery
/// represent PC-relative native-width pointers without inventing a new kind.
struct References<'a, 'b> {
    arena: &'a Bump,
    labels: &'b mut BTreeMap<&'a str, Location>,
    prefix: &'b str,
    counter: usize,
}
impl References<'_, '_> {
    fn name(&mut self) -> String {
        let name = format!("{}ref{}", self.prefix, self.counter);
        self.counter += 1;
        name
    }
    fn address(
        &mut self,
        bytes: &mut Vec<u8>,
        section: usize,
        address: u64,
        relative: bool,
        a: &mut Assembly,
    ) {
        let name = self.name();
        self.labels.insert(
            self.arena.alloc_str(&name),
            Location {
                section: 0,
                offset: address,
            },
        );
        self.symbol(bytes, section, name, relative, a);
    }
    fn symbol(
        &mut self,
        bytes: &mut Vec<u8>,
        section: usize,
        symbol: String,
        relative: bool,
        a: &mut Assembly,
    ) {
        let subtract_symbol = if relative {
            let name = self.name();
            self.labels.insert(
                self.arena.alloc_str(&name),
                Location {
                    section,
                    offset: bytes.len() as u64,
                },
            );
            Some(name)
        } else {
            None
        };
        a.relocations.push(Relocation {
            section,
            size: 8,
            subtract_symbol,
            offset: bytes.len() as u32,
            symbol,
            kind: RelocationKind::Unsigned,
            addend: 0,
        });
        bytes.extend_from_slice(&[0; 8]);
    }
}
