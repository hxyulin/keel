//! ELF line tables. File paths, checksums, source text and every supported
//! location flag survive assembly; unsupported operands are diagnosed.
use super::metadata::leb;
use super::{Assembler, Expr, Fix, Sym};
use keel_asm_ir::x86::{self, Modifier};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Debug {
    files: BTreeMap<u32, File>,
    directories: Vec<Vec<u8>>,
    root: Option<File>,
    rows: Vec<Row>,
    pending: Option<Row>,
    is_stmt: bool,
    started: bool,
}
struct File {
    name: Vec<u8>,
    directory: u32,
    md5: Option<[u8; 16]>,
    source: Option<Vec<u8>>,
}
struct Row {
    section: usize,
    offset: u64,
    file: u32,
    line: u32,
    column: u32,
    flags: u8,
    isa: u32,
    discriminator: u32,
}
fn quoted(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim();
    if !s.starts_with('"') {
        return Err("expected quoted string".into());
    }
    let mut i = 1;
    while i < s.len() {
        match s.as_bytes()[i] {
            b'\\' => i += 2,
            b'"' => return Ok((&s[..=i], &s[i + 1..])),
            _ => i += 1,
        }
    }
    Err("unterminated string".into())
}
impl Debug {
    pub(super) fn parse_file(&mut self, args: &str) -> Result<(), String> {
        let (index, rest) = args
            .split_once(char::is_whitespace)
            .ok_or("expected file index and path")?;
        let index = index.parse().map_err(|_| "invalid file index")?;
        let (first, rest) = quoted(rest)?;
        let (directory, name, mut rest) = if rest.trim().starts_with('"') {
            let (name, rest) = quoted(rest)?;
            (Some(x86::unquote(first)?), x86::unquote(name)?, rest)
        } else {
            (None, x86::unquote(first)?, rest)
        };
        let (mut checksum, mut source) = (None, None);
        while !rest.trim().is_empty() {
            let (keyword, tail) = rest
                .trim()
                .split_once(char::is_whitespace)
                .ok_or("missing .file operand")?;
            match keyword {
                "md5" if checksum.is_none() => {
                    let (token, tail) = tail
                        .trim()
                        .split_once(char::is_whitespace)
                        .unwrap_or((tail.trim(), ""));
                    let (digits, radix) = if let Some(d) = token.strip_prefix("0x") {
                        (d, 16)
                    } else if token.starts_with('0') && token.len() > 1 {
                        (&token[1..], 8)
                    } else {
                        (token, 10)
                    };
                    checksum = Some(
                        u128::from_str_radix(digits, radix)
                            .map_err(|_| "invalid 128-bit MD5 checksum")?
                            .to_be_bytes(),
                    );
                    rest = tail;
                }
                "source" if source.is_none() => {
                    let (text, tail) = quoted(tail)?;
                    source = Some(x86::unquote(text)?);
                    rest = tail;
                }
                _ => return Err(format!("unsupported or duplicate .file operand {keyword}")),
            }
        }
        self.file(index, directory, name, checksum, source)
    }
    pub(super) fn file(
        &mut self,
        index: u32,
        directory: Option<Vec<u8>>,
        mut name: Vec<u8>,
        md5: Option<[u8; 16]>,
        source: Option<Vec<u8>>,
    ) -> Result<(), String> {
        if name.contains(&0)
            || directory.as_ref().is_some_and(|s| s.contains(&0))
            || source.as_ref().is_some_and(|s| s.contains(&0))
        {
            return Err(".file strings must not contain NUL".into());
        }
        if index == 0 {
            if self.root.is_some() || !self.files.is_empty() || !self.rows.is_empty() {
                return Err(".file 0 must precede numbered files and locations".into());
            }
            let dir = directory
                .filter(|d| !d.is_empty())
                .ok_or(".file 0 requires an explicit nonempty compilation directory")?;
            if name.is_empty() {
                return Err(".file 0 requires a nonempty name".into());
            }
            self.directories.push(dir);
            self.root = Some(File {
                name,
                directory: 0,
                md5,
                source,
            });
            return Ok(());
        }
        if self.files.contains_key(&index) {
            return Err("duplicate file index".into());
        }
        if self.root.is_none() && name.is_empty() {
            return Err("empty DWARF4 file name".into());
        }
        if self.root.is_none() && (md5.is_some() || source.is_some()) {
            return Err(".file md5/source requires .file 0 (DWARF5)".into());
        }
        let mut dir = directory.unwrap_or_default();
        if let Some(root) = &self.root {
            if name == root.name && md5 == root.md5 {
                if source.is_some() && source != root.source {
                    return Err("source differs from matching root file".into());
                }
                // LLVM does not allocate a numbered duplicate of the root file.
                return Ok(());
            }
            if dir == self.directories[0] {
                dir.clear();
            }
            if name.is_empty() {
                name = b"<stdin>".to_vec();
                dir.clear();
            }
            if dir.is_empty() {
                if name.ends_with(b"/") || name.starts_with(b"//") {
                    return Err("unsupported ambiguous .file path".into());
                }
                if let Some(at) = name.iter().rposition(|&c| c == b'/') {
                    dir = name[..at.max(1)].to_vec();
                    name = name[at + 1..].to_vec();
                }
            }
        }
        let directory = if dir.is_empty() {
            0
        } else if let Some(at) = self.directories.iter().position(|d| *d == dir) {
            at as u32 + u32::from(self.root.is_none())
        } else {
            self.directories.push(dir);
            self.directories.len() as u32 - u32::from(self.root.is_some())
        };
        self.files.insert(
            index,
            File {
                name,
                directory,
                md5,
                source,
            },
        );
        Ok(())
    }
    pub(super) fn loc(&mut self, section: usize, offset: u64, args: &str) -> Result<(), String> {
        let mut p = args.split_whitespace().peekable();
        let number = |s: Option<&str>| -> Result<u32, String> {
            s.ok_or("missing .loc operand")?
                .parse()
                .map_err(|_| "invalid .loc number".into())
        };
        let file = number(p.next())?;
        if !(file == 0 && self.root.is_some() || self.files.contains_key(&file)) {
            return Err("location references undeclared file".into());
        }
        let mut position = [0, 0];
        for slot in &mut position {
            if p.peek()
                .is_some_and(|s| s.as_bytes().first().is_some_and(u8::is_ascii_digit))
            {
                *slot = number(p.next())?;
            }
        }
        if !self.started {
            self.is_stmt = true;
            self.started = true;
        }
        let (mut flags, mut isa, mut discriminator) = (0, 0, 0);
        while let Some(op) = p.next() {
            match op {
                "basic_block" => flags |= 2,
                "prologue_end" => flags |= 4,
                "epilogue_begin" => flags |= 8,
                "is_stmt" => {
                    self.is_stmt = match p.next() {
                        Some("0") => false,
                        Some("1") => true,
                        _ => return Err("is_stmt requires 0 or 1".into()),
                    }
                }
                "isa" => isa = number(p.next())?,
                "discriminator" => discriminator = number(p.next())?,
                _ => return Err(format!("unsupported .loc operand {op}")),
            }
        }
        flags |= u8::from(self.is_stmt);
        let row = Row {
            section,
            offset,
            file,
            line: position[0],
            column: position[1],
            flags,
            isa,
            discriminator,
        };
        // MC commits a preceding .loc at the next .loc, or at the next
        // instruction. In particular alignment padding does not consume it.
        self.emit(section, offset);
        self.pending = Some(row);
        Ok(())
    }
    pub(super) fn emit(&mut self, section: usize, offset: u64) {
        if let Some(mut row) = self.pending.take() {
            row.section = section;
            row.offset = offset;
            self.rows.push(row);
        }
    }
}
fn address(a: &mut Assembler, b: &mut Vec<u8>, section: usize, offset: u64, width: u8, kind: u32) {
    let id = a
        .symbols
        .intern(&format!(".Lkeel.line.{}", a.symbols.len()));
    a.symbols.insert(
        id,
        Sym {
            section: Some(section),
            value: offset,
            ..Sym::default()
        },
    );
    a.fixes.push(Fix {
        section: a.current,
        offset: b.len() as u64,
        width,
        modifier: Modifier::None,
        expr: Expr::symbol(id),
        kind,
        line: 0,
    });
    b.resize(b.len() + usize::from(width), 0);
}
fn patch32(b: &mut [u8], at: usize, n: usize) -> Result<(), String> {
    b[at..at + 4].copy_from_slice(
        &u32::try_from(n)
            .map_err(|_| "line table too large")?
            .to_le_bytes(),
    );
    Ok(())
}
pub(super) fn finish(a: &mut Assembler) -> Result<(), String> {
    let d = std::mem::take(&mut a.debug);
    if d.root.is_none() && d.rows.is_empty() {
        return Ok(());
    }
    if d.files.keys().copied().ne(1..=d.files.len() as u32) {
        return Err("file indices must be contiguous starting at 1".into());
    }
    a.section(".debug_line", Some(0), false)?;
    if a.offset() != 0 {
        return Err("generated line table conflicts with explicit .debug_line".into());
    }
    let line_section = a.current;
    let version: u16 = if d.root.is_some() { 5 } else { 4 };
    let mut b = vec![0; 4];
    b.extend(version.to_le_bytes());
    if version == 5 {
        b.extend([8, 0]);
    }
    let length_at = b.len();
    b.extend([0; 4]);
    let header_at = b.len();
    b.extend([
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
    if let Some(root) = &d.root {
        a.section_metadata(".debug_line_str", Some(0x30), false, 1, None)?;
        let string_section = a.current;
        if a.offset() != 0 {
            return Err("generated strings conflict with explicit .debug_line_str".into());
        }
        let mut strings = Vec::new();
        let mut offsets = BTreeMap::<Vec<u8>, u64>::new();
        a.current = line_section;
        let mut strp = |text: &[u8], b: &mut Vec<u8>| {
            let offset = *offsets.entry(text.to_vec()).or_insert_with(|| {
                let at = strings.len() as u64;
                strings.extend(text);
                strings.push(0);
                at
            });
            address(a, b, string_section, offset, 4, 10);
        };
        b.extend([1, 1, 0x1f]); // directory table: path, line_strp
        leb(&mut b, d.directories.len() as i128, false)?;
        for dir in &d.directories {
            strp(dir, &mut b);
        }
        // LLVM drops the checksum column when any entry lacks a checksum.
        let md5 = root.md5.is_some() && d.files.values().all(|f| f.md5.is_some());
        let source = root.source.is_some() || d.files.values().any(|f| f.source.is_some());
        b.push(2 + u8::from(md5) + u8::from(source));
        b.extend([1, 0x1f, 2, 0x0f]);
        if md5 {
            b.extend([5, 0x1e]);
        }
        if source {
            leb(&mut b, 0x2001, false)?;
            b.push(0x1f);
        }
        leb(&mut b, d.files.len() as i128 + 1, false)?;
        for f in std::iter::once(root).chain(d.files.values()) {
            strp(&f.name, &mut b);
            leb(&mut b, f.directory.into(), false)?;
            if md5 {
                b.extend(f.md5.unwrap());
            }
            if source {
                strp(
                    match f.source.as_deref() {
                        None => b"",
                        Some([]) => b"\n",
                        Some(s) => s,
                    },
                    &mut b,
                );
            }
        }
        a.current = string_section;
        a.bytes(&strings)?;
        a.current = line_section;
    } else {
        for dir in &d.directories {
            b.extend(dir);
            b.push(0);
        }
        b.push(0);
        for f in d.files.values() {
            b.extend(&f.name);
            b.push(0);
            leb(&mut b, f.directory.into(), false)?;
            b.extend([0, 0]);
        }
        b.push(0);
    }
    let length = b.len() - header_at;
    patch32(&mut b, length_at, length)?;
    // Each executable section is a distinct line-program sequence.
    let mut rows = BTreeMap::<usize, Vec<Row>>::new();
    let mut order = Vec::new();
    for row in d.rows {
        if !rows.contains_key(&row.section) {
            order.push(row.section);
        }
        rows.entry(row.section).or_default().push(row);
    }
    for section in order {
        let rows = rows.remove(&section).unwrap();
        b.extend([0, 9, 2]);
        address(a, &mut b, section, 0, 8, 1);
        let (mut offset, mut file, mut line, mut column, mut isa) = (0, 1, 1i128, 0, 0);
        let mut is_stmt = true;
        for r in rows {
            if r.offset < offset {
                return Err("line addresses must be monotonic within a section".into());
            }
            if r.offset != offset {
                b.push(2);
                leb(&mut b, (r.offset - offset).into(), false)?;
                offset = r.offset;
            }
            if file != r.file {
                b.push(4);
                leb(&mut b, r.file.into(), false)?;
                file = r.file;
            }
            if column != r.column {
                b.push(5);
                leb(&mut b, r.column.into(), false)?;
                column = r.column;
            }
            if line != i128::from(r.line) {
                b.push(3);
                leb(&mut b, i128::from(r.line) - line, true)?;
                line = r.line.into();
            }
            if is_stmt != (r.flags & 1 != 0) {
                b.push(6);
                is_stmt = !is_stmt;
            }
            if r.flags & 2 != 0 {
                b.push(7);
            }
            if r.flags & 4 != 0 {
                b.push(10);
            }
            if r.flags & 8 != 0 {
                b.push(11);
            }
            if isa != r.isa {
                b.push(12);
                leb(&mut b, r.isa.into(), false)?;
                isa = r.isa;
            }
            if r.discriminator != 0 {
                let mut value = vec![4];
                leb(&mut value, r.discriminator.into(), false)?;
                b.push(0);
                leb(&mut b, value.len() as i128, false)?;
                b.extend(value);
            }
            b.push(1);
        }
        b.push(2);
        leb(&mut b, (a.sections[section].size - offset).into(), false)?;
        b.extend([0, 1, 1]);
    }
    let length = b.len() - 4;
    patch32(&mut b, 0, length)?;
    a.bytes(&b)
}
