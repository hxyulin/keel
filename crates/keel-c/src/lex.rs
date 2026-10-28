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
    /// GNU floating imaginary constant, with a positive-zero real component.
    Imaginary {
        value: f64,
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
    /// The `#pragma pack` alignment limit in force, by index for
    /// [`Lexer::pack_limit`].
    pub pack: u32,
}

#[cfg(test)]
const PUNCTS: [&str; 48] = [
    "...", "<<=", ">>=", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "*=",
    "/=", "%=", "+=", "-=", "&=", "^=", "|=", "##", "[", "]", "(", ")", "{", "}", ".", "&", "*",
    "+", "-", "~", "!", "/", "%", "<", ">", "^", "|", "?", ":", ";", "=", ",", "#",
];

// Longest-match dispatch reads at most three bounded bytes. A punctuation
// token never walks unrelated spellings or calls a general string comparator.
#[inline(always)]
fn punctuation(bytes: &[u8]) -> Option<&'static str> {
    let (a, b, c) = match bytes {
        [a, b, c, ..] => (*a, *b, *c),
        [a, b] => (*a, *b, 0),
        [a] => (*a, 0, 0),
        [] => return None,
    };
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

/// What a token's first byte starts.
#[derive(Clone, Copy)]
enum Start {
    Ident,
    /// An identifier, or the prefix of a wide or UTF-8 literal.
    Prefix,
    Digit,
    /// A number if a digit follows, else punctuation.
    Dot,
    Quote,
    /// The end of the input.
    Nul,
    Other,
    Single,
}

const START: [Start; 256] = {
    let mut table = [Start::Other; 256];
    let mut i = 0;
    while i < 256 {
        let c = i as u8;
        table[i] = if matches!(c, b'L' | b'u' | b'U') {
            Start::Prefix
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            Start::Ident
        } else if c.is_ascii_digit() {
            Start::Digit
        } else {
            match c {
                b'.' => Start::Dot,
                b'"' | b'\'' => Start::Quote,
                0 => Start::Nul,
                _ if !SINGLE[i].is_empty() => Start::Single,
                _ => Start::Other,
            }
        };
        i += 1;
    }
    table
};

const SINGLE: [&str; 256] = {
    let mut t = [""; 256];
    t[b'[' as usize] = "[";
    t[b']' as usize] = "]";
    t[b'(' as usize] = "(";
    t[b')' as usize] = ")";
    t[b'{' as usize] = "{";
    t[b'}' as usize] = "}";
    t[b'~' as usize] = "~";
    t[b'?' as usize] = "?";
    t[b':' as usize] = ":";
    t[b';' as usize] = ";";
    t[b',' as usize] = ",";
    t
};
/// Bytes that may start whitespace, a comment or a directive.
const SPACE: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = matches!(
            i as u8,
            b' ' | b'\t' | b'\r' | b'\x0c' | b'\x0b' | b'\n' | b'\\' | b'/' | b'#'
        );
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
    /// Each limit put in force, the first none; tokens index it.
    pack_limits: Vec<Option<u64>>,
    /// A string literal's bytes, as they are decoded.
    scratch: Vec<u8>,
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
            pack_limits: vec![None],
            scratch: Vec::new(),
        }
    }

    /// The `#pragma pack` limit of a token's `pack`.
    pub fn pack_limit(&self, pack: u32) -> Option<u64> {
        self.pack_limits[pack as usize]
    }

    /// The next token. After `Eof`, `Eof` again.
    pub fn token(&mut self, text: &mut Strings<'a>) -> Result<Token<'a>, Error> {
        self.next(text)
    }

    /// Appends at least `n` more tokens to `out`, or fewer ending with
    /// `Eof`; returns whether `Eof` was reached. A batch keeps the lexer's
    /// position in registers from one token to the next.
    pub fn tokens(
        &mut self,
        text: &mut Strings<'a>,
        out: &mut Vec<Token<'a>>,
        n: usize,
    ) -> Result<bool, Error> {
        out.reserve(n);
        let mut left = n;
        while left > 0 {
            let tok = self.next(text)?;
            out.push(tok);
            if tok.tok == Tok::Eof {
                return Ok(true);
            }
            left -= 1;
        }
        Ok(false)
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
    #[inline]
    fn skip_space(&mut self, text: &mut Strings<'a>) -> Result<u8, Error> {
        // Most tokens follow the last directly, after one blank, or at the
        // start of the next line after its indentation.
        let mut pos = self.pos;
        if self.src.get(pos) == Some(&b' ') {
            pos += 1;
        }
        if self.src.get(pos) == Some(&b'\n') {
            pos += 1;
            self.line += 1;
            self.line_start = pos;
            while self.src.get(pos) == Some(&b' ') {
                pos += 1;
            }
        }
        self.pos = pos;
        let c = self.src.get(pos).copied().unwrap_or(0);
        if !SPACE[c as usize] {
            return Ok(c);
        }
        self.skip_space_slow(text)?;
        Ok(self.peek(0))
    }

    #[inline(never)]
    fn skip_space_slow(&mut self, text: &mut Strings<'a>) -> Result<(), Error> {
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
        let limit = self.pack.last().copied().flatten();
        if self.pack_limits.last() != Some(&limit) {
            self.pack_limits.push(limit);
        }
    }

    #[inline(always)]
    fn next(&mut self, text: &mut Strings<'a>) -> Result<Token<'a>, Error> {
        loop {
            let c = self.skip_space(text)?;
            let loc = self.loc();
            let tok = match START[c as usize] {
                Start::Prefix if matches!(self.peek(1), b'"' | b'\'') => {
                    // `u` and `U` literals are treated like `L` (32-bit units).
                    self.pos += 1;
                    self.quoted(true, text)?
                }
                Start::Prefix if c == b'u' && self.peek(1) == b'8' && self.peek(2) == b'"' => {
                    self.pos += 2;
                    self.quoted(false, text)?
                }
                Start::Ident | Start::Prefix => {
                    let start = self.pos;
                    let rest = &self.src[start..];
                    self.pos += rest
                        .iter()
                        .position(|&c| !IDENT[c as usize])
                        .unwrap_or(rest.len());
                    let spelling = &self.source[start..self.pos];
                    // GNU extension markers have no semantics; skip them before interning.
                    if spelling == "__extension__" {
                        continue;
                    }
                    Tok::Ident(text.intern_source(spelling))
                }
                Start::Digit => self.number(text)?,
                Start::Dot if self.peek(1).is_ascii_digit() => self.number(text)?,
                Start::Quote => self.quoted(false, text)?,
                Start::Nul => Tok::Eof,
                Start::Single => {
                    self.pos += 1;
                    Tok::Punct(SINGLE[c as usize])
                }
                Start::Dot | Start::Other => {
                    let rest = &self.src[self.pos..];
                    let Some(p) = punctuation(rest) else {
                        return Err(self.error(format!("unexpected character `{}`", c as char)));
                    };
                    self.pos += p.len();
                    Tok::Punct(p)
                }
            };
            let pack = self.pack_limits.len() as u32 - 1;
            return Ok(Token { tok, loc, pack });
        }
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
        // Most constants are short plain decimals, which cannot overflow.
        if text.len() < 20
            && (text.len() == 1 || !text.starts_with('0'))
            && text.bytes().all(|b| b.is_ascii_digit())
        {
            let value = text
                .bytes()
                .fold(0, |v: u64, b| v * 10 + u64::from(b - b'0'));
            return Ok(Tok::Int {
                value,
                decimal: true,
                unsigned: false,
                longs: 0,
            });
        }
        Ok(parse_number(text).unwrap_or_else(|| Tok::BadNumber(strings.intern(text))))
    }

    /// A character constant or string literal, the opening quote next.
    fn quoted(&mut self, wide: bool, text: &mut Strings<'a>) -> Result<Tok<'a>, Error> {
        let quote = self.peek(0);
        self.pos += 1;
        let string = quote == b'"';
        if string && !wide {
            let rest = &self.src[self.pos..];
            let n = memchr::memchr3(b'"', b'\\', b'\n', rest).unwrap_or(rest.len());
            if rest.get(n) == Some(&b'"') {
                if let Some(at) = memchr::memchr(0, &rest[..n]) {
                    self.pos += at;
                    return Err(self.error("unterminated literal"));
                }
                self.pos += n + 1;
                return Ok(Tok::Str {
                    bytes: text.bytes_source(&rest[..n]),
                    wide: false,
                });
            }
        }
        let mut bytes = std::mem::take(&mut self.scratch);
        bytes.clear();
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
            let bytes_ref = text.bytes(&bytes);
            self.scratch = bytes;
            return Ok(Tok::Str {
                bytes: bytes_ref,
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
    // GNU accepts i/j before or after f. Deliberately exclude integer and
    // long-double imaginary constants rather than silently narrowing them.
    if text.bytes().any(|b| matches!(b, b'i' | b'I' | b'j' | b'J')) {
        let mut body = text.to_owned();
        let position = body.find(['i', 'I', 'j', 'J'])?;
        if position + 2 < body.len() {
            return None;
        }
        body.remove(position);
        if body.ends_with(['l', 'L']) {
            return None;
        }
        return match parse_number(&body)? {
            Tok::Float { value, single } => Some(Tok::Imaginary { value, single }),
            _ => None,
        };
    }
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
            parse_hex_float_precision(&body[2..], single)?
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

/// Decode a hexadecimal significand with guard/sticky bits, then round once
/// directly to the destination IEEE precision, including the subnormal range.
fn parse_hex_float_precision(text: &str, single: bool) -> Option<f64> {
    // C hexadecimal floating constants require a binary exponent, including
    // when the token carries a GNU imaginary suffix.
    let (mantissa, exp) = text.split_once(['p', 'P'])?;
    let exp = exp.parse::<i32>().ok()? as i64;
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
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
            lost |= d != 0;
            if i < int.len() {
                shift += 4;
            }
        }
    }
    if lost {
        bits |= 1;
    }
    if bits == 0 {
        return Some(0.0);
    }
    let (fraction, bias) = if single { (23, 127) } else { (52, 1023) };
    let high = 63 - bits.leading_zeros() as i64;
    let exponent = high + shift;
    if exponent > bias {
        return Some(f64::INFINITY);
    }
    let quantum = (exponent - fraction).max(1 - bias - fraction);
    let discard = quantum - shift;
    let rounded = if discard > 64 {
        0
    } else if discard == 64 {
        u64::from(bits > (1u64 << 63))
    } else if discard > 0 {
        let whole = bits >> discard;
        let remainder = bits & ((1u64 << discard) - 1);
        let half = 1u64 << (discard - 1);
        whole + u64::from(remainder > half || (remainder == half && whole & 1 != 0))
    } else {
        bits << -discard
    };
    if rounded == 0 {
        return Some(0.0);
    }
    let high = 63 - rounded.leading_zeros() as i64;
    let exponent = high + quantum;
    let payload = if exponent > bias {
        ((2 * bias + 1) as u64) << fraction
    } else if exponent < 1 - bias {
        rounded
    } else {
        let significand = if high > fraction {
            rounded >> (high - fraction)
        } else {
            rounded << (fraction - high)
        };
        (((exponent + bias) as u64) << fraction) | (significand & ((1u64 << fraction) - 1))
    };
    Some(if single {
        f64::from(f32::from_bits(payload as u32))
    } else {
        f64::from_bits(payload)
    })
}

#[cfg(test)]
fn parse_hex_float(text: &str) -> Option<f64> {
    parse_hex_float_precision(text, false)
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
    fn plain_decimal_tokens_match_the_general_decoder() {
        let text = "0 7 123 1234567890123456789 18446744073709551615 99999999999999999999 0123 00";
        let arena = bumpalo::Bump::new();
        let mut pool = Strings::new(&arena);
        let mut files = Vec::new();
        let tokens = tokenize(text, "numbers.c", &mut files, &mut pool).unwrap();
        for (spelling, token) in text.split(' ').zip(&tokens) {
            let expected = parse_number(spelling).unwrap_or(Tok::BadNumber(spelling));
            assert_eq!(token.tok, expected, "{spelling}");
        }
    }

    #[test]
    fn extension_markers_preserve_batch_counts_locations_and_partial_errors() {
        let source = "# 40 \"markers.c\"\n__extension__ int __extension__ x; __extension__suffix \"__extension__\" __extension__";
        for size in [1, 2, 7, 256] {
            let arena = bumpalo::Bump::new();
            let mut strings = Strings::new(&arena);
            let mut lexer = Lexer::new(source, "test.i", &mut strings);
            let mut tokens = Vec::new();
            assert!(!lexer.tokens(&mut strings, &mut tokens, 0).unwrap());
            assert!(tokens.is_empty());
            while !lexer.tokens(&mut strings, &mut tokens, size).unwrap() {}
            let kinds: Vec<_> = tokens.iter().map(|t| t.tok).collect();
            assert_eq!(
                kinds,
                [
                    Tok::Ident("int"),
                    Tok::Ident("x"),
                    Tok::Punct(";"),
                    Tok::Ident("__extension__suffix"),
                    Tok::Str {
                        bytes: b"__extension__",
                        wide: false
                    },
                    Tok::Eof,
                ]
            );
            assert_eq!(
                tokens[0].loc,
                Loc {
                    file: 1,
                    line: 40,
                    col: 15
                }
            );
            assert_eq!(lexer.files, ["test.i", "markers.c"]);
        }
        let arena = bumpalo::Bump::new();
        let mut strings = Strings::new(&arena);
        let mut lexer = Lexer::new("__extension__ x @", "bad.i", &mut strings);
        let mut tokens = Vec::new();
        let error = lexer.tokens(&mut strings, &mut tokens, 256).unwrap_err();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].tok, Tok::Ident("x"));
        assert_eq!(error.loc.col, 17);
    }

    #[test]
    fn raw_and_escaped_literal_spellings_share_bytes_in_either_order() {
        for source in [r#""ab" "a\x62" "ab""#, r#""a\x62" "ab" "a\x62""#] {
            let arena = bumpalo::Bump::new();
            let mut strings = Strings::new(&arena);
            let mut files = Vec::new();
            let tokens = tokenize(source, "strings.i", &mut files, &mut strings).unwrap();
            let bytes: Vec<_> = tokens
                .iter()
                .filter_map(|t| match t.tok {
                    Tok::Str { bytes, wide: false } => Some(bytes),
                    _ => None,
                })
                .collect();
            assert_eq!(bytes.len(), 3);
            assert_eq!(bytes[0], b"ab");
            assert!(bytes.iter().all(|b| std::ptr::eq(*b, bytes[0])));
        }
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
