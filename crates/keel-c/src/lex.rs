//! Tokens of preprocessed C.
//!
//! The input is the output of a preprocessor (`clang -E`), so there are no
//! directives left except line markers, `# 12 "file.c"`, which say where
//! the following lines came from; they give tokens their locations, and
//! `#pragma pack`, which limits struct member alignment from there on (as
//! Apple's Mach headers do); each token records the limit. Other `#` lines
//! are skipped.

use crate::{Error, Loc};

#[derive(Clone, PartialEq, Debug)]
pub enum Tok {
    /// Identifiers and keywords alike; the parser tells them apart.
    Ident(String),
    /// An integer constant: its value, and whether it was decimal, which
    /// changes the types it may get.
    Int {
        value: u64,
        decimal: bool,
        unsigned: bool,
        /// 0, 1 (`l`) or 2 (`ll`).
        longs: u8,
    },
    Float {
        value: f64,
        /// `f` suffix: the value is already rounded to `f32`.
        single: bool,
    },
    /// A character constant, already the value of its `int` (or `wchar_t`).
    Char(i64),
    /// A string literal's bytes, without the terminating zero. Wide
    /// literals (`L"..."`) hold their code points as `u32`s.
    Str {
        bytes: Vec<u8>,
        wide: bool,
    },
    Punct(&'static str),
    /// A preprocessing number that is not a valid constant, like the
    /// `10.12.1` in `availability(macos, introduced=10.12.1)`: only an
    /// error if an expression uses it.
    BadNumber(String),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub loc: Loc,
    /// The `#pragma pack` alignment limit in force, if any.
    pub pack: Option<u64>,
}

const PUNCTS: [&str; 48] = [
    "...", "<<=", ">>=", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "*=",
    "/=", "%=", "+=", "-=", "&=", "^=", "|=", "##", "[", "]", "(", ")", "{", "}", ".", "&", "*",
    "+", "-", "~", "!", "/", "%", "<", ">", "^", "|", "?", ":", ";", "=", ",", "#",
];

/// Splits `src` into tokens. `files` collects the file names line markers
/// mention; a token's `Loc::file` indexes it.
pub fn tokenize(src: &str, path: &str, files: &mut Vec<String>) -> Result<Vec<Token>, Error> {
    let mut lx = Lexer {
        src: src.as_bytes(),
        pos: 0,
        line: 1,
        line_start: 0,
        file: file_index(files, path),
        files,
        pack: Vec::new(),
    };
    let mut out = Vec::new();
    loop {
        let tok = lx.next()?;
        // GNU's `__extension__` only silences pedantic warnings, and may
        // come before a declaration or any expression.
        if matches!(&tok.tok, Tok::Ident(s) if s == "__extension__") {
            continue;
        }
        let end = tok.tok == Tok::Eof;
        out.push(tok);
        if end {
            return Ok(out);
        }
    }
}

fn file_index(files: &mut Vec<String>, name: &str) -> u32 {
    match files.iter().position(|f| f == name) {
        Some(i) => i as u32,
        None => {
            files.push(name.to_string());
            files.len() as u32 - 1
        }
    }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    line_start: usize,
    file: u32,
    files: &'a mut Vec<String>,
    /// `#pragma pack` limits: the last is in force, `None` for none.
    pack: Vec<Option<u64>>,
}

impl Lexer<'_> {
    fn peek(&self, n: usize) -> u8 {
        self.src.get(self.pos + n).copied().unwrap_or(0)
    }

    fn loc(&self) -> Loc {
        Loc {
            file: self.file,
            line: self.line,
            col: (self.pos - self.line_start + 1) as u32,
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::new(self.loc(), message)
    }

    fn newline(&mut self) {
        self.pos += 1;
        self.line += 1;
        self.line_start = self.pos;
    }

    /// Skips whitespace, comments (in case the preprocessor kept them) and
    /// `#` lines, applying line markers.
    fn skip_space(&mut self) -> Result<(), Error> {
        loop {
            match self.peek(0) {
                b'\n' => self.newline(),
                b' ' | b'\t' | b'\r' | b'\x0c' | b'\x0b' => self.pos += 1,
                b'\\' if self.peek(1) == b'\n' => {
                    self.pos += 1;
                    self.newline();
                }
                b'/' if self.peek(1) == b'/' => {
                    while !matches!(self.peek(0), b'\n' | 0) {
                        self.pos += 1;
                    }
                }
                b'/' if self.peek(1) == b'*' => {
                    self.pos += 2;
                    loop {
                        match self.peek(0) {
                            0 => return Err(self.error("unterminated comment")),
                            b'*' if self.peek(1) == b'/' => break,
                            b'\n' => self.newline(),
                            _ => self.pos += 1,
                        }
                    }
                    self.pos += 2;
                }
                b'#' if self.src[self.line_start..self.pos]
                    .iter()
                    .all(|c| c.is_ascii_whitespace()) =>
                {
                    self.directive();
                }
                _ => return Ok(()),
            }
        }
    }

    /// A `#` line: a line marker (`# 12 "file.c" 1 3`, or `#line 12
    /// "file.c"`) sets where the next line came from; anything else is
    /// ignored.
    fn directive(&mut self) {
        let start = self.pos + 1;
        while !matches!(self.peek(0), b'\n' | 0) {
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
        let text = text.trim_start();
        if let Some(rest) = text.strip_prefix("pragma") {
            self.pragma(rest);
        }
        let text = text.strip_prefix("line").unwrap_or(text).trim_start();
        let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
        let Ok(line) = digits.parse::<u32>() else {
            return;
        };
        let rest = text[digits.len()..].trim_start();
        if let Some(name) = rest.strip_prefix('"')
            && let Some(end) = name.find('"')
        {
            self.file = file_index(self.files, &name[..end]);
        }
        if self.peek(0) == b'\n' {
            self.newline();
        }
        // The marker names the line after it.
        self.line = line;
    }

    /// `#pragma pack(push, 4)`, `pack(4)`, `pack(pop)`, `pack()`.
    fn pragma(&mut self, text: &str) {
        let Some(args) = text.trim().strip_prefix("pack") else {
            return;
        };
        let args = args.trim().trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        let current = self.pack.last().copied().flatten();
        match parts.as_slice() {
            ["pop", ..] => {
                self.pack.pop();
            }
            ["push"] => self.pack.push(current),
            ["push", n] => self.pack.push(n.parse().ok()),
            [""] => self.pack.push(None),
            [n] => {
                self.pack.pop();
                self.pack.push(n.parse().ok());
            }
            _ => {}
        }
    }

    fn next(&mut self) -> Result<Token, Error> {
        self.skip_space()?;
        let loc = self.loc();
        let c = self.peek(0);
        let tok = if c == 0 {
            Tok::Eof
        } else if (c == b'L' || c == b'u' || c == b'U') && matches!(self.peek(1), b'"' | b'\'') {
            // `u` and `U` literals are treated like `L` (32-bit units).
            self.pos += 1;
            self.quoted(true)?
        } else if c == b'u' && self.peek(1) == b'8' && self.peek(2) == b'"' {
            self.pos += 2;
            self.quoted(false)?
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let start = self.pos;
            while self.peek(0).is_ascii_alphanumeric() || matches!(self.peek(0), b'_' | b'$') {
                self.pos += 1;
            }
            Tok::Ident(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
        } else if c.is_ascii_digit() || (c == b'.' && self.peek(1).is_ascii_digit()) {
            self.number()?
        } else if c == b'"' || c == b'\'' {
            self.quoted(false)?
        } else {
            let rest = &self.src[self.pos..];
            let Some(p) = PUNCTS.iter().find(|p| rest.starts_with(p.as_bytes())) else {
                return Err(self.error(format!("unexpected character `{}`", c as char)));
            };
            self.pos += p.len();
            Tok::Punct(p)
        };
        let pack = self.pack.last().copied().flatten();
        Ok(Token { tok, loc, pack })
    }

    /// A preprocessing number, then its value.
    fn number(&mut self) -> Result<Tok, Error> {
        let start = self.pos;
        loop {
            let c = self.peek(0);
            if matches!(c, b'e' | b'E' | b'p' | b'P') && matches!(self.peek(1), b'+' | b'-') {
                self.pos += 2;
            } else if c.is_ascii_alphanumeric() || c == b'.' || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        Ok(parse_number(text).unwrap_or_else(|| Tok::BadNumber(text.to_string())))
    }

    /// A character constant or string literal, the opening quote next.
    fn quoted(&mut self, wide: bool) -> Result<Tok, Error> {
        let quote = self.peek(0);
        self.pos += 1;
        let mut units: Vec<u32> = Vec::new();
        loop {
            match self.peek(0) {
                0 | b'\n' => return Err(self.error("unterminated literal")),
                c if c == quote => {
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    self.pos += 1;
                    let unit = self.escape()?;
                    units.push(unit);
                }
                _ if wide => {
                    // Decode one UTF-8 character into a code point.
                    let rest = std::str::from_utf8(&self.src[self.pos..]).unwrap_or("\u{fffd}");
                    let ch = rest.chars().next().unwrap_or('\u{fffd}');
                    self.pos += ch.len_utf8().max(1);
                    units.push(ch as u32);
                }
                c => {
                    self.pos += 1;
                    units.push(u32::from(c));
                }
            }
        }
        if quote == b'"' {
            let bytes = if wide {
                units.iter().flat_map(|u| u.to_le_bytes()).collect()
            } else {
                units.iter().map(|&u| u as u8).collect()
            };
            return Ok(Tok::Str { bytes, wide });
        }
        match units.as_slice() {
            [] => Err(self.error("empty character constant")),
            // A plain `char` is signed: '\xff' is -1.
            [u] if !wide => Ok(Tok::Char(i64::from(*u as u8 as i8))),
            [u] => Ok(Tok::Char(i64::from(*u))),
            // Multi-character constants, as GCC and clang give them.
            _ => Ok(Tok::Char(i64::from(
                units.iter().fold(0u32, |acc, &u| (acc << 8) | (u & 0xff)) as i32,
            ))),
        }
    }

    /// The escape after a backslash.
    fn escape(&mut self) -> Result<u32, Error> {
        let c = self.peek(0);
        self.pos += 1;
        Ok(match c {
            b'n' => 10,
            b't' => 9,
            b'r' => 13,
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'v' => 11,
            b'e' => 27,
            b'0'..=b'7' => {
                let mut v = u32::from(c - b'0');
                for _ in 0..2 {
                    match self.peek(0) {
                        d @ b'0'..=b'7' => {
                            v = v * 8 + u32::from(d - b'0');
                            self.pos += 1;
                        }
                        _ => break,
                    }
                }
                v
            }
            b'x' => {
                let mut v: u32 = 0;
                let mut any = false;
                while let Some(d) = (self.peek(0) as char).to_digit(16) {
                    v = v.wrapping_mul(16).wrapping_add(d);
                    self.pos += 1;
                    any = true;
                }
                if !any {
                    return Err(self.error("`\\x` needs hex digits"));
                }
                v
            }
            b'u' | b'U' => {
                let n = if c == b'u' { 4 } else { 8 };
                let mut v: u32 = 0;
                for _ in 0..n {
                    let d = (self.peek(0) as char)
                        .to_digit(16)
                        .ok_or_else(|| self.error("bad universal character name"))?;
                    v = v * 16 + d;
                    self.pos += 1;
                }
                v
            }
            0 => return Err(self.error("unterminated literal")),
            c => u32::from(c),
        })
    }
}

/// The value of an integer or floating constant.
fn parse_number(text: &str) -> Option<Tok> {
    let lower = text.to_ascii_lowercase();
    let hex = lower.starts_with("0x");
    let is_float = if hex {
        lower.contains('.') || lower.contains('p')
    } else {
        lower.contains('.') || lower.contains('e')
    };
    if is_float {
        let (body, single) = if let Some(b) = lower.strip_suffix('f') {
            (b, true)
        } else {
            (lower.strip_suffix('l').unwrap_or(&lower), false)
        };
        let value = if hex {
            parse_hex_float(&body[2..])?
        } else if single {
            f64::from(body.parse::<f32>().ok()?)
        } else {
            body.parse::<f64>().ok()?
        };
        let value = if single && hex {
            f64::from(value as f32)
        } else {
            value
        };
        return Some(Tok::Float { value, single });
    }
    let digits_end = lower.find(['u', 'l']).unwrap_or(lower.len());
    let (digits, suffix) = lower.split_at(digits_end);
    let (unsigned, longs) = match suffix {
        "" => (false, 0),
        "u" => (true, 0),
        "l" => (false, 1),
        "ul" | "lu" => (true, 1),
        "ll" => (false, 2),
        "ull" | "llu" => (true, 2),
        _ => return None,
    };
    let (radix, body) = if let Some(h) = digits.strip_prefix("0x") {
        (16, h)
    } else if let Some(b) = digits.strip_prefix("0b") {
        (2, b)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let value = u64::from_str_radix(body, radix).ok()?;
    Some(Tok::Int {
        value,
        decimal: radix == 10,
        unsigned,
        longs,
    })
}

/// `1.8p3` (the part after `0x`): hex digits with an optional point, and
/// a binary exponent. Exact while the digits fit in 64 bits.
fn parse_hex_float(text: &str) -> Option<f64> {
    let (mantissa, exp) = match text.split_once('p') {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (text, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut bits: u64 = 0;
    let mut shift = exp;
    let mut lost = false;
    for (i, c) in int.chars().chain(frac.chars()).enumerate() {
        let d = u64::from(c.to_digit(16)?);
        if bits >> 60 == 0 {
            bits = bits * 16 + d;
            if i >= int.len() {
                shift -= 4;
            }
        } else {
            // Further digits only scale (integer part) or are dropped
            // (fraction); a non-zero one is folded into the sticky bit.
            lost |= d != 0;
            if i < int.len() {
                shift += 4;
            }
        }
    }
    if lost {
        bits |= 1;
    }
    // Rounds once, to 53 bits, then scales exactly (unless the result is
    // subnormal). The scaling is done in steps, since 2^-1074 itself is
    // representable but 2^1074 is not.
    let mut value = bits as f64;
    while shift < -1000 {
        value *= 2f64.powi(-1000);
        shift += 1000;
    }
    while shift > 1000 {
        value *= 2f64.powi(1000);
        shift -= 1000;
    }
    Some(value * 2f64.powi(shift))
}
