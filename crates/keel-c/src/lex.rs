//! Tokens of preprocessed C.
//!
//! The input is the output of a preprocessor (`clang -E`), so there are no
//! directives left except line markers, `# 12 "file.c"`, which say where
//! the following lines came from; they give tokens their locations, and
//! `#pragma pack`, which limits struct member alignment from there on (as
//! Apple's Mach headers do); each token records the limit. Other `#` lines
//! are skipped.

use crate::strings::Strings;
use crate::{Error, Loc};
use keel_ir::fxhash::FxHashMap as HashMap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tok<'a> {
    /// Identifiers and keywords alike; the parser tells them apart.
    Ident(&'a str),
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
        bytes: &'a [u8],
        wide: bool,
    },
    Punct(&'static str),
    /// A preprocessing number that is not a valid constant, like the
    /// `10.12.1` in `availability(macos, introduced=10.12.1)`: only an
    /// error if an expression uses it.
    BadNumber(&'a str),
    Eof,
}

#[derive(Clone, Copy, Debug)]
pub struct Token<'a> {
    pub tok: Tok<'a>,
    pub loc: Loc,
    /// The `#pragma pack` alignment limit in force, if any.
    pub pack: Option<u64>,
}

#[cfg(test)]
const PUNCTS: [&str; 48] = [
    "...", "<<=", ">>=", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "*=",
    "/=", "%=", "+=", "-=", "&=", "^=", "|=", "##", "[", "]", "(", ")", "{", "}", ".", "&", "*",
    "+", "-", "~", "!", "/", "%", "<", ">", "^", "|", "?", ":", ";", "=", ",", "#",
];

// Longest-match dispatch reads at most three bounded bytes. A punctuation
// token never walks unrelated spellings or calls a general string comparator.
#[inline]
fn punctuation(bytes: &[u8]) -> Option<&'static str> {
    let a = *bytes.first()?;
    let b = bytes.get(1).copied().unwrap_or(0);
    let c = bytes.get(2).copied().unwrap_or(0);
    Some(match a {
        b'.' if b == b'.' && c == b'.' => "...",
        b'<' if b == b'<' => {
            if c == b'=' {
                "<<="
            } else {
                "<<"
            }
        }
        b'>' if b == b'>' => {
            if c == b'=' {
                ">>="
            } else {
                ">>"
            }
        }
        b'-' if b == b'>' => "->",
        b'+' if b == b'+' => "++",
        b'-' if b == b'-' => "--",
        b'<' if b == b'=' => "<=",
        b'>' if b == b'=' => ">=",
        b'=' if b == b'=' => "==",
        b'!' if b == b'=' => "!=",
        b'&' if b == b'&' => "&&",
        b'|' if b == b'|' => "||",
        b'*' if b == b'=' => "*=",
        b'/' if b == b'=' => "/=",
        b'%' if b == b'=' => "%=",
        b'+' if b == b'=' => "+=",
        b'-' if b == b'=' => "-=",
        b'&' if b == b'=' => "&=",
        b'^' if b == b'=' => "^=",
        b'|' if b == b'=' => "|=",
        b'#' if b == b'#' => "##",
        b'[' => "[",
        b']' => "]",
        b'(' => "(",
        b')' => ")",
        b'{' => "{",
        b'}' => "}",
        b'.' => ".",
        b'&' => "&",
        b'*' => "*",
        b'+' => "+",
        b'-' => "-",
        b'~' => "~",
        b'!' => "!",
        b'/' => "/",
        b'%' => "%",
        b'<' => "<",
        b'>' => ">",
        b'^' => "^",
        b'|' => "|",
        b'?' => "?",
        b':' => ":",
        b';' => ";",
        b'=' => "=",
        b',' => ",",
        b'#' => "#",
        _ => return None,
    })
}

const IDENT: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = (i as u8).is_ascii_alphanumeric() || i == b'_' as usize || i == b'$' as usize;
        i += 1;
    }
    table
};

/// Splits all of `src` into tokens. `files` receives the file names line
/// markers mention; a token's `Loc::file` indexes it. The parser instead
/// pulls tokens from a [`Lexer`] as it goes.
pub fn tokenize<'a>(
    src: &'a str,
    path: &str,
    files: &mut Vec<&'a str>,
    text: &mut Strings<'a>,
) -> Result<Vec<Token<'a>>, Error> {
    let mut lx = Lexer::new(src, path, text);
    let mut out = Vec::new();
    loop {
        let tok = lx.token(text)?;
        let end = tok.tok == Tok::Eof;
        out.push(tok);
        if end {
            *files = lx.files;
            return Ok(out);
        }
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    src: &'a [u8],
    pos: usize,
    line: u32,
    line_start: usize,
    file: u32,
    /// The file names line markers mention; a token's `Loc::file` indexes
    /// it.
    pub files: Vec<&'a str>,
    file_ids: HashMap<&'a str, u32>,
    /// `#pragma pack` limits: the last is in force, `None` for none.
    pack: Vec<Option<u64>>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str, path: &str, text: &mut Strings<'a>) -> Self {
        let path = text.intern(path);
        let mut file_ids = HashMap::default();
        file_ids.insert(path, 0);
        Lexer {
            source: src,
            src: src.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
            file: 0,
            files: vec![path],
            file_ids,
            pack: Vec::new(),
        }
    }

    /// The next token. After `Eof`, `Eof` again.
    pub fn token(&mut self, text: &mut Strings<'a>) -> Result<Token<'a>, Error> {
        loop {
            let tok = self.next(text)?;
            // GNU's `__extension__` only silences pedantic warnings, and may
            // come before a declaration or any expression.
            if !matches!(&tok.tok, Tok::Ident(s) if *s == "__extension__") {
                return Ok(tok);
            }
        }
    }

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
    fn skip_space(&mut self, text: &mut Strings<'a>) -> Result<(), Error> {
        loop {
            match self.peek(0) {
                b'\n' => self.newline(),
                b' ' | b'\t' | b'\r' | b'\x0c' | b'\x0b' => {
                    self.pos += 1;
                    while matches!(self.peek(0), b' ' | b'\t' | b'\r' | b'\x0c' | b'\x0b') {
                        self.pos += 1;
                    }
                }
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
                    self.directive(text);
                }
                _ => return Ok(()),
            }
        }
    }

    /// A `#` line: a line marker (`# 12 "file.c" 1 3`, or `#line 12
    /// "file.c"`) sets where the next line came from; anything else is
    /// ignored.
    fn directive(&mut self, strings: &mut Strings<'a>) {
        let start = self.pos + 1;
        while !matches!(self.peek(0), b'\n' | 0) {
            self.pos += 1;
        }
        let text = &self.source[start..self.pos];
        let text = text.trim_start();
        if let Some(rest) = text.strip_prefix("pragma") {
            self.pragma(rest);
        }
        let text = text.strip_prefix("line").unwrap_or(text).trim_start();
        let end = text.bytes().take_while(u8::is_ascii_digit).count();
        let Ok(line) = text[..end].parse::<u32>() else {
            return;
        };
        let rest = text[end..].trim_start();
        if let Some(name) = rest.strip_prefix('"')
            && let Some(end) = name.find('"')
        {
            let name = &name[..end];
            self.file = if let Some(&id) = self.file_ids.get(name) {
                id
            } else {
                let name = strings.intern(name);
                let id = self.files.len() as u32;
                self.files.push(name);
                self.file_ids.insert(name, id);
                id
            };
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

    fn next(&mut self, text: &mut Strings<'a>) -> Result<Token<'a>, Error> {
        self.skip_space(text)?;
        let loc = self.loc();
        let c = self.peek(0);
        let tok = if c == 0 {
            Tok::Eof
        } else if (c == b'L' || c == b'u' || c == b'U') && matches!(self.peek(1), b'"' | b'\'') {
            // `u` and `U` literals are treated like `L` (32-bit units).
            self.pos += 1;
            self.quoted(true, text)?
        } else if c == b'u' && self.peek(1) == b'8' && self.peek(2) == b'"' {
            self.pos += 2;
            self.quoted(false, text)?
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let start = self.pos;
            while IDENT[self.peek(0) as usize] {
                self.pos += 1;
            }
            // ASCII bounds: slicing the source needs no UTF-8 validation.
            Tok::Ident(text.intern(&self.source[start..self.pos]))
        } else if c.is_ascii_digit() || (c == b'.' && self.peek(1).is_ascii_digit()) {
            self.number(text)?
        } else if c == b'"' || c == b'\'' {
            self.quoted(false, text)?
        } else {
            let rest = &self.src[self.pos..];
            let Some(p) = punctuation(rest) else {
                return Err(self.error(format!("unexpected character `{}`", c as char)));
            };
            self.pos += p.len();
            Tok::Punct(p)
        };
        let pack = self.pack.last().copied().flatten();
        Ok(Token { tok, loc, pack })
    }

    /// A preprocessing number, then its value.
    fn number(&mut self, strings: &mut Strings<'a>) -> Result<Tok<'a>, Error> {
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
        let text = &self.source[start..self.pos];
        Ok(parse_number(text).unwrap_or_else(|| Tok::BadNumber(strings.intern(text))))
    }

    /// A character constant or string literal, the opening quote next.
    fn quoted(&mut self, wide: bool, text: &mut Strings<'a>) -> Result<Tok<'a>, Error> {
        let quote = self.peek(0);
        self.pos += 1;
        let string = quote == b'"';
        let mut bytes = Vec::new();
        let (mut count, mut first, mut packed) = (0usize, 0u32, 0u32);
        loop {
            let c = self.peek(0);
            let unit = match c {
                0 | b'\n' => return Err(self.error("unterminated literal")),
                c if c == quote => {
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    self.pos += 1;
                    self.escape()?
                }
                _ if wide => {
                    // Decode only this scalar, never revalidate the remainder
                    // of a potentially megabyte-sized translation unit.
                    let width = match c {
                        0..=0x7f => 1,
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    let end = (self.pos + width).min(self.src.len());
                    let text = std::str::from_utf8(&self.src[self.pos..end]).unwrap_or("\u{fffd}");
                    let ch = text.chars().next().unwrap_or('\u{fffd}');
                    self.pos += ch.len_utf8();
                    ch as u32
                }
                _ if string => {
                    let rest = &self.src[self.pos..];
                    let end = memchr::memchr3(quote, b'\\', b'\n', rest).unwrap_or(rest.len());
                    let span = &rest[..end];
                    if let Some(at) = memchr::memchr(0, span) {
                        self.pos += at;
                        return Err(self.error("unterminated literal"));
                    }
                    bytes.extend_from_slice(span);
                    self.pos += end;
                    continue;
                }
                c => {
                    self.pos += 1;
                    u32::from(c)
                }
            };
            if string {
                if wide {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                } else {
                    bytes.push(unit as u8);
                }
            } else {
                first = if count == 0 { unit } else { first };
                count += 1;
                packed = (packed << 8) | (unit & 0xff);
            }
        }
        if string {
            return Ok(Tok::Str {
                bytes: text.bytes(&bytes),
                wide,
            });
        }
        match count {
            0 => Err(self.error("empty character constant")),
            1 if !wide => Ok(Tok::Char(i64::from(first as u8 as i8))),
            1 => Ok(Tok::Char(i64::from(first))),
            _ => Ok(Tok::Char(i64::from(packed as i32))),
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
fn parse_number<'a>(text: &str) -> Option<Tok<'a>> {
    let hex = text.starts_with("0x") || text.starts_with("0X");
    let is_float = text.bytes().any(|b| {
        b == b'.'
            || if hex {
                matches!(b, b'p' | b'P')
            } else {
                matches!(b, b'e' | b'E')
            }
    });
    if is_float {
        let (body, single) = match text.as_bytes().last()? {
            b'f' | b'F' => (&text[..text.len() - 1], true),
            b'l' | b'L' => (&text[..text.len() - 1], false),
            _ => (text, false),
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
    let end = text.find(['u', 'U', 'l', 'L']).unwrap_or(text.len());
    let (digits, suffix) = text.split_at(end);
    // Only the at-most-three suffix bytes need folding; numeric tokens never
    // allocate a lowercase copy of the full spelling.
    if suffix.len() > 3 {
        return None;
    }
    let mut folded = [0; 3];
    for (to, from) in folded.iter_mut().zip(suffix.bytes()) {
        *to = from.to_ascii_lowercase();
    }
    let (unsigned, longs) = match &folded[..suffix.len()] {
        b"" => (false, 0),
        b"u" => (true, 0),
        b"l" => (false, 1),
        b"ul" | b"lu" => (true, 1),
        b"ll" => (false, 2),
        b"ull" | b"llu" => (true, 2),
        _ => return None,
    };
    let (radix, body) = if hex {
        (16, &digits[2..])
    } else if digits.starts_with("0b") || digits.starts_with("0B") {
        (2, &digits[2..])
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
    let (mantissa, exp) = match text.split_once(['p', 'P']) {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_spellings_match_the_previous_decoder() {
        for value in [0, 1, 7, 255, 65536, u32::MAX as u64, u64::MAX] {
            for body in [
                format!("{value}"),
                format!("0{value:o}"),
                format!("0x{value:x}"),
                format!("0X{value:X}"),
                format!("0b{value:b}"),
                format!("0B{value:b}"),
            ] {
                for suffix in [
                    "", "U", "l", "LL", "ul", "Lu", "uLL", "LLu", "lL", "UU", "f", "invalid",
                ] {
                    let text = format!("{body}{suffix}");
                    assert_eq!(parse_number(&text), parse_number_reference(&text), "{text}");
                }
            }
        }
        for text in [
            "1.0",
            ".5f",
            "1E20F",
            "1e999L",
            "0x1.fp3",
            "0X1.FP-10L",
            "0x1.1",
            "12_34",
            "0x",
            "1.2.3",
            "0XFF",
            "0x1p0F",
        ] {
            assert_eq!(parse_number(text), parse_number_reference(text), "{text}");
        }
    }

    fn parse_number_reference<'a>(text: &str) -> Option<Tok<'a>> {
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

    #[test]
    fn literals_keep_width_escape_bytes_and_multibyte_scalars() {
        let arena = bumpalo::Bump::new();
        let mut pool = Strings::new(&arena);
        let mut files = Vec::new();
        let tokens = tokenize(
            r#""ab\0cd" "hé" L"λ中𐐀" '\xff' 'ab' L'λ'"#,
            "literals.c",
            &mut files,
            &mut pool,
        )
        .unwrap();
        assert_eq!(
            tokens[0].tok,
            Tok::Str {
                bytes: b"ab\0cd",
                wide: false
            }
        );
        assert_eq!(
            tokens[1].tok,
            Tok::Str {
                bytes: "hé".as_bytes(),
                wide: false
            }
        );
        let wide: Vec<u8> = "λ中𐐀"
            .chars()
            .flat_map(|c| (c as u32).to_le_bytes())
            .collect();
        assert_eq!(
            tokens[2].tok,
            Tok::Str {
                bytes: &wide,
                wide: true
            }
        );
        assert_eq!(tokens[3].tok, Tok::Char(-1));
        assert_eq!(tokens[4].tok, Tok::Char(0x6162));
        assert_eq!(tokens[5].tok, Tok::Char('λ' as i64));
    }

    #[test]
    fn punctuation_longest_match_on_all_spellings_and_boundaries() {
        for a in PUNCTS {
            for b in PUNCTS {
                let text = format!("{a}{b}");
                for end in 0..=text.len() {
                    let input = &text.as_bytes()[..end];
                    let expected = PUNCTS
                        .iter()
                        .find(|p| input.starts_with(p.as_bytes()))
                        .copied();
                    assert_eq!(punctuation(input), expected, "{text:?} at {end}");
                }
            }
        }
        for byte in 0..=255u8 {
            let input = [byte];
            let expected = PUNCTS
                .iter()
                .find(|p| input.starts_with(p.as_bytes()))
                .copied();
            assert_eq!(punctuation(&input), expected);
        }
    }
}
