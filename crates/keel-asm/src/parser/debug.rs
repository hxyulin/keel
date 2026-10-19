//! DWARF line programs and call-frame records. Addresses are section-relative
//! until the Mach-O linker applies the ordinary pointer/difference relocations.
use super::compact::{self, Rule};
use super::data::{self, Location};
use super::text::Trim;
use crate::{Assembly, Error, Relocation, RelocationKind, SectionKind};
use bumpalo::{Bump, collections::Vec as BumpVec};
use std::collections::BTreeMap;

pub(super) struct Debug<'a> {
    arena: &'a Bump,
    files: BTreeMap<u32, File>,
    /// DWARF 4 include_directories, in first-use order; entry `n` is index `n + 1`.
    directories: Vec<Vec<u8>>,
    /// LLVM carries only is_stmt from one `.loc` to the next; it starts set.
    is_stmt: bool,
    rows: BumpVec<'a, Row>,
    frames: BumpVec<'a, Frame<'a>>,
    active: Option<Frame<'a>>,
}
struct File {
    directory: u32,
    name: Vec<u8>,
}
struct Row {
    address: u64,
    file: u32,
    line: u32,
    column: u32,
    flags: u8,
    isa: u32,
    discriminator: u32,
}
const IS_STMT: u8 = 1;
const BASIC_BLOCK: u8 = 2;
const PROLOGUE_END: u8 = 4;
const EPILOGUE_BEGIN: u8 = 8;
/// Splits one leading quoted string, honoring backslash escapes.
fn quoted(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim_fast();
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return Err("expected quoted string".into());
    }
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Ok((&s[..=i], &s[i + 1..])),
            _ => i += 1,
        }
    }
    Err("unterminated string".into())
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
/// The CIE uses a code-alignment factor of one, so deltas are byte offsets.
/// Prefer the primary opcode, then the narrowest extended advance operand.
fn advance(delta: u64, out: &mut Vec<u8>) -> Result<(), &'static str> {
    let delta = u32::try_from(delta).map_err(|_| "CFI address advance exceeds 32 bits")?;
    match delta {
        0 => {}
        1..=63 => out.push(0x40 | delta as u8),
        64..=255 => out.extend_from_slice(&[0x02, delta as u8]),
        256..=65535 => {
            out.push(0x03);
            out.extend_from_slice(&(delta as u16).to_le_bytes());
        }
        _ => {
            out.push(0x04);
            out.extend_from_slice(&delta.to_le_bytes());
        }
    }
    Ok(())
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
    let s = s.trim_fast();
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
            b'b' => 8,
            b'f' => 12,
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
    if let Some(n) = s.strip_prefix('x').or_else(|| s.strip_prefix('w')) {
        let n: u64 = n.parse().map_err(|_| "invalid CFI register")?;
        if n <= 30 {
            return Ok(n);
        }
    }
    if let Some(n) = s.strip_prefix(['b', 'h', 's', 'd', 'q', 'v']) {
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
            directories: Vec::new(),
            is_stmt: true,
            rows: BumpVec::new_in(arena),
            frames: BumpVec::new_in(arena),
            active: None,
        }
    }
    pub(super) fn file(&mut self, index: u32, path: Vec<u8>) -> Result<(), String> {
        self.file_in(index, None, path)
    }
    /// Like Clang's DWARF 4 tables, an explicit directory is kept verbatim as
    /// an include directory and the name is stored unsplit. An empty directory
    /// string means none, so the name keeps directory index 0.
    fn file_in(
        &mut self,
        index: u32,
        directory: Option<Vec<u8>>,
        name: Vec<u8>,
    ) -> Result<(), String> {
        if index == 0 || self.files.contains_key(&index) {
            return Err("duplicate or zero file number".into());
        }
        if name.contains(&0) || directory.as_ref().is_some_and(|d| d.contains(&0)) {
            return Err("file path contains NUL".into());
        }
        let directory = match directory.filter(|d| !d.is_empty()) {
            None => 0,
            Some(d) => match self.directories.iter().position(|e| *e == d) {
                Some(n) => n as u32 + 1,
                None => {
                    self.directories.push(d);
                    self.directories.len() as u32
                }
            },
        };
        self.files.insert(index, File { directory, name });
        Ok(())
    }
    pub(super) fn loc(
        &mut self,
        file: u32,
        row: u32,
        column: u32,
        prologue: bool,
        section: usize,
        address: u64,
    ) -> Result<(), String> {
        if section != 0 {
            return Err(".loc requires __text".into());
        }
        self.row(Row {
            address,
            file,
            line: row,
            column,
            flags: if prologue { PROLOGUE_END } else { 0 },
            isa: 0,
            discriminator: 0,
        })
    }
    /// `row.flags` holds the per-directive bits; is_stmt comes from `self.is_stmt`.
    fn row(&mut self, mut row: Row) -> Result<(), String> {
        if !self.files.contains_key(&row.file) {
            return Err(".loc references undeclared file".into());
        }
        if self.is_stmt {
            row.flags |= IS_STMT;
        }
        self.rows.push(row);
        Ok(())
    }
    pub(super) fn cfi(
        &mut self,
        cfi: &keel_asm_ir::Cfi,
        section: usize,
        address: u64,
        line: usize,
    ) -> Result<(), String> {
        use keel_asm_ir::Cfi as C;
        if section != 0 {
            return Err("CFI directives require __text".into());
        }
        match cfi {
            C::Start => {
                if self.active.is_some() {
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
                return Ok(());
            }
            C::End => {
                let mut frame = self
                    .active
                    .take()
                    .ok_or(".cfi_endproc without .cfi_startproc")?;
                if frame.saved != 0 {
                    return Err("unbalanced CFI state stack".into());
                }
                frame.end = address;
                self.frames.push(frame);
                return Ok(());
            }
            _ => {}
        }
        let frame = self
            .active
            .as_mut()
            .ok_or("CFI directive outside a procedure")?;
        let mut bytes = BumpVec::new_in(self.arena);
        let rule = match cfi {
            C::Personality(name) | C::Lsda(name) => {
                super::symbol(name)?;
                let slot = if matches!(cfi, C::Personality(_)) {
                    &mut frame.personality
                } else {
                    &mut frame.lsda
                };
                if slot.replace(self.arena.alloc_str(name)).is_some() {
                    return Err(format!(
                        "duplicate {}",
                        if matches!(cfi, C::Personality(_)) {
                            ".cfi_personality"
                        } else {
                            ".cfi_lsda"
                        }
                    ));
                }
                return Ok(());
            }
            C::DefCfaOffset(offset) => {
                bytes.push(0x0e);
                uleb(*offset, &mut bytes);
                Rule::CfaOffset(*offset)
            }
            C::DefCfaRegister(register) => {
                if *register > 95 {
                    return Err("invalid CFI register".into());
                }
                bytes.push(0x0d);
                uleb((*register).into(), &mut bytes);
                Rule::Other
            }
            C::DefCfa { register, offset } => {
                if *register > 95 {
                    return Err("invalid CFI register".into());
                }
                bytes.push(0x0c);
                uleb(u64::from(*register), &mut bytes);
                uleb(*offset, &mut bytes);
                Rule::Cfa(u64::from(*register), *offset)
            }
            C::Offset { register, offset } => {
                if *register > 95 {
                    return Err("invalid CFI register".into());
                }
                if *offset > 0 || offset % 8 != 0 {
                    return Err("CFI offset must be a nonpositive multiple of 8".into());
                }
                if *register < 64 {
                    bytes.push(0x80 | *register)
                } else {
                    bytes.push(0x05);
                    uleb(u64::from(*register), &mut bytes)
                }
                uleb(offset.unsigned_abs() / 8, &mut bytes);
                Rule::Offset(u64::from(*register), *offset)
            }
            _ => unreachable!(),
        };
        frame.rules.push(rule);
        frame.events.push((address, bytes));
        Ok(())
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
            // DWARF 4 forms only: `.file N "path"` and `.file N "dir" "name"`.
            let operands = operands.trim_fast();
            if operands.starts_with('"') {
                return Err(
                    "unnumbered .file is unsupported; expected .file number and path".into(),
                );
            }
            let (id, rest) = operands
                .split_once(char::is_whitespace)
                .ok_or("expected .file number and path")?;
            let id: u32 = id.parse().map_err(|_| "invalid file number")?;
            if id == 0 {
                return Err(".file 0 requires DWARF 5 line tables, which are unsupported".into());
            }
            let (first, rest) = quoted(rest)?;
            let (directory, name, rest) = if rest.trim_fast().starts_with('"') {
                let (name, rest) = quoted(rest)?;
                (Some(string(first)?), string(name)?, rest)
            } else {
                (None, string(first)?, rest)
            };
            match rest.split_whitespace().next() {
                None => {}
                Some("md5" | "source") => {
                    return Err(".file md5/source operands require DWARF 5 line tables, which are unsupported".into());
                }
                Some(_) => return Err("unexpected token in .file directive".into()),
            }
            self.file_in(id, directory, name)?;
            return Ok(true);
        }
        if op == ".loc" {
            if section != 0 {
                return Err(".loc requires __text".into());
            }
            // LLVM's grammar: file [line [column]] followed by sub-directives.
            let mut args = operands.split_whitespace().peekable();
            let number = |s: Option<&str>, what: &str| {
                s.and_then(|s| s.parse::<u32>().ok())
                    .ok_or_else(|| format!("invalid .loc {what}"))
            };
            let file = number(args.next(), "file number")?;
            if file == 0 {
                return Err(".loc file number must be at least one".into());
            }
            let mut position = [0; 2];
            for slot in &mut position {
                if args
                    .peek()
                    .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit() || c == '-'))
                {
                    *slot = number(args.next(), "line or column")?;
                }
            }
            let (mut flags, mut isa, mut discriminator) = (0, 0, 0);
            while let Some(name) = args.next() {
                match name {
                    "basic_block" => flags |= BASIC_BLOCK,
                    "prologue_end" => flags |= PROLOGUE_END,
                    "epilogue_begin" => flags |= EPILOGUE_BEGIN,
                    "is_stmt" => match args.next() {
                        Some("0") => self.is_stmt = false,
                        Some("1") => self.is_stmt = true,
                        _ => return Err(".loc is_stmt value must be 0 or 1".into()),
                    },
                    "isa" => isa = number(args.next(), "isa value")?,
                    "discriminator" => discriminator = number(args.next(), "discriminator")?,
                    _ => return Err(format!("unknown .loc sub-directive `{name}`")),
                }
            }
            self.row(Row {
                address,
                file,
                line: position[0],
                column: position[1],
                flags,
                isa,
                discriminator,
            })?;
            return Ok(true);
        }
        if !op.starts_with(".cfi_") {
            return Ok(false);
        }
        if section != 0 {
            return Err("CFI directives require __text".into());
        }
        use keel_asm_ir::Cfi as C;
        if op == ".cfi_startproc" || op == ".cfi_endproc" {
            if !operands.is_empty() {
                return Err(format!("{op} takes no operands"));
            }
            self.cfi(
                &if op == ".cfi_startproc" {
                    C::Start
                } else {
                    C::End
                },
                section,
                address,
                line,
            )?;
            return Ok(true);
        }
        let args: Vec<_> = operands
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let typed = match (op, args.as_slice()) {
            (".cfi_personality", [encoding, name]) | (".cfi_lsda", [encoding, name]) => {
                let personality = op == ".cfi_personality";
                let expected = if personality { 155 } else { 16 };
                if super::unsigned(encoding, 255)? != expected {
                    return Err(format!("{op} currently requires encoding {expected}"));
                }
                Some(if personality {
                    C::Personality((*name).into())
                } else {
                    C::Lsda((*name).into())
                })
            }
            (".cfi_def_cfa", [r, n]) => Some(C::DefCfa {
                register: reg(r)? as u8,
                offset: n.parse().map_err(|_| "invalid CFA offset")?,
            }),
            (".cfi_offset", [r, n]) => Some(C::Offset {
                register: reg(r)? as u8,
                offset: n.parse().map_err(|_| "invalid saved-register offset")?,
            }),
            _ => None,
        };
        if let Some(cfi) = typed {
            self.cfi(&cfi, section, address, line)?;
            return Ok(true);
        }
        let frame = self
            .active
            .as_mut()
            .ok_or("CFI directive outside a procedure")?;
        let mut bytes = BumpVec::new_in(self.arena);
        let mut rule = Rule::Other;
        match (op, args.as_slice()) {
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
            (".cfi_restore", [r]) => {
                let r = reg(r)?;
                if r < 64 {
                    bytes.push(0xc0 | r as u8);
                } else {
                    bytes.push(0x06);
                    uleb(r, &mut bytes);
                }
            }
            (".cfi_same_value", [r]) => {
                bytes.push(0x08);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_undefined", [r]) => {
                bytes.push(0x07);
                uleb(reg(r)?, &mut bytes);
            }
            (".cfi_negate_ra_state", []) => {
                // DW_CFA_AARCH64_negate_ra_state. Rule::Other keeps the
                // signed return-address state in DWARF instead of discarding
                // it through compact-unwind selection.
                bytes.push(0x2d);
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
                        advance(delta, &mut bytes)
                            .map_err(|message| Error::new(frame.line, message))?;
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
            ]);
            for directory in &self.directories {
                bytes.extend(directory);
                bytes.push(0);
            }
            bytes.push(0);
            for file in self.files.values() {
                bytes.extend(&file.name);
                bytes.push(0);
                uleb(file.directory.into(), &mut bytes);
                bytes.extend_from_slice(&[0, 0]);
            }
            bytes.push(0);
            let len = (bytes.len() - header) as u32;
            bytes[6..10].copy_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(&[0, 9, 2]);
            references.address(&mut bytes, section, 0, false, a);
            let (mut address, mut file, mut line, mut column) = (0, 1, 1i64, 0);
            let (mut is_stmt, mut isa) = (true, 0);
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
                if row.discriminator != 0 {
                    let mut operand = Vec::new();
                    uleb(row.discriminator.into(), &mut operand);
                    bytes.push(0);
                    uleb(operand.len() as u64 + 1, &mut bytes);
                    bytes.push(4);
                    bytes.extend(operand);
                }
                if row.isa != isa {
                    bytes.push(12);
                    uleb(row.isa.into(), &mut bytes);
                    isa = row.isa;
                }
                if (row.flags & IS_STMT != 0) != is_stmt {
                    bytes.push(6);
                    is_stmt = !is_stmt;
                }
                for (flag, opcode) in [(BASIC_BLOCK, 7), (PROLOGUE_END, 10), (EPILOGUE_BEGIN, 11)] {
                    if row.flags & flag != 0 {
                        bytes.push(opcode);
                    }
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

#[cfg(test)]
mod tests {
    use super::advance;

    #[test]
    fn cfi_advances_use_the_narrowest_operand_and_reject_overflow() {
        for (delta, expected) in [
            (0, vec![]),
            (1, vec![0x41]),
            (63, vec![0x7f]),
            (64, vec![0x02, 64]),
            (255, vec![0x02, 255]),
            (256, vec![0x03, 0, 1]),
            (65535, vec![0x03, 255, 255]),
            (65536, vec![0x04, 0, 0, 1, 0]),
            (u32::MAX as u64, vec![0x04, 255, 255, 255, 255]),
        ] {
            let mut bytes = Vec::new();
            advance(delta, &mut bytes).unwrap();
            assert_eq!(bytes, expected, "delta {delta}");
        }
        let mut bytes = Vec::new();
        assert_eq!(
            advance(u32::MAX as u64 + 1, &mut bytes),
            Err("CFI address advance exceeds 32 bits")
        );
        assert!(bytes.is_empty());
    }
}
