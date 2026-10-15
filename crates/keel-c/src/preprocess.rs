//! Token-based C preprocessing using Keel's target environment and headers.

use crate::Diagnostic;
use bumpalo::{Bump, collections::Vec as BumpVec};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

// The two constants are also used for controlled profiling ablations.
const CACHE_HEADERS: bool = true;

#[derive(Clone, Default)]
pub struct PreprocessOptions {
    /// Searched after the including file's directory, in order.
    pub include_dirs: Vec<PathBuf>,
    /// Explicit system search directories, after -I and before compiler headers.
    pub system_include_dirs: Vec<PathBuf>,
    /// Root of the target's SDK/filesystem. User -I/-isystem paths are not rebased.
    pub sysroot: Option<PathBuf>,
    /// macOS minimum deployment version, independent of SDK version (native default 11.0).
    pub darwin_deployment: Option<String>,
    /// Definitions/undefinitions applied in order, after predefined macros.
    pub defines: Vec<(String, Option<String>)>,
    pub target: keel_ir::Target,
    /// Return the final macro definitions instead of preprocessed source (-dM).
    pub dump_macros: bool,
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    space: bool,
    raw_spelling: Option<&'a str>,
    file: &'a str,
    line: u32,
    col: u32,
    hide: &'a [&'a str],
}
impl<'a> Token<'a> {
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            file: self.file.to_string(),
            line: self.line,
            col: self.col,
            message: message.into(),
        }
    }
    fn with_text(&self, arena: &'a Bump, text: impl AsRef<str>) -> Self {
        let mut t = *self;
        t.text = arena.alloc_str(text.as_ref());
        t.raw_spelling = None;
        t
    }
}

#[derive(Clone)]
struct Macro<'a> {
    params: Option<&'a [&'a str]>,
    variadic: bool,
    body: &'a [Token<'a>],
    has_paste: bool,
}

/// Preprocess raw C to text accepted by [`crate::compile`], including line
/// markers and pragmas. With default options this needs no external compiler.
/// Included files and include search directories must remain unchanged until
/// this call completes. Sources and
/// lexical tokens are cached only within this invocation; large stable files
/// can be memory-mapped. The returned text owns its storage independently.
pub fn preprocess(
    src: &str,
    path: &str,
    options: &PreprocessOptions,
) -> Result<String, Diagnostic> {
    preprocess_inner::<false>(src, path, options).map(|(text, _)| text)
}

/// Counters are collected only by the profiling entry point; the ordinary
/// entry point instantiates TRACK=false so instrumentation compiles away.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreprocessStats {
    pub include_requests: usize,
    pub file_opens: usize,
    pub mapped_files: usize,
    pub source_cache_hits: usize,
    pub token_cache_hits: usize,
    pub scanned_files: usize,
    pub guard_skips: usize,
    pub once_skips: usize,
    pub source_bytes: usize,
    pub arena_bytes: usize,
    pub token_bytes: usize,
}
#[cfg(feature = "profile")]
pub fn preprocess_with_stats(
    src: &str,
    path: &str,
    options: &PreprocessOptions,
) -> Result<(String, PreprocessStats), Diagnostic> {
    preprocess_inner::<true>(src, path, options)
}
fn preprocess_inner<const TRACK: bool>(
    src: &str,
    path: &str,
    options: &PreprocessOptions,
) -> Result<(String, PreprocessStats), Diagnostic> {
    let arena = Bump::new();
    let sources = typed_arena::Arena::new();
    let token_buffers = typed_arena::Arena::new();
    let mut pp = Preprocessor::<TRACK> {
        arena: &arena,
        sources: &sources,
        token_buffers: &token_buffers,
        stats: PreprocessStats::default(),
        options,
        in_condition: false,
        macros: HashMap::new(),
        expansion_scratch: Vec::new(),
        undefined: HashSet::new(),
        search_dirs: Vec::new(),
        framework_dirs: Vec::new(),
        include_stack: Vec::new(),
        once: HashSet::new(),
        canonical_paths: HashMap::new(),
        canonical_dirs: HashMap::new(),
        header_cache: HashMap::new(),
        file_sources: HashMap::new(),
        scanned: HashMap::new(),
        counter: 0,
        output: String::new(),
    };
    let predefined = arena.alloc_str(
        &crate::predefined_macros(options.target, options.darwin_deployment.as_deref())
            .map_err(|e| pp.error(e))?,
    );
    for line in predefined.lines() {
        pp.define(&scan(
            line.strip_prefix("#define ").unwrap(),
            "<built-in>",
            &arena,
        )?)?;
    }
    pp.native_paths();
    for (name, value) in &options.defines {
        if let Some(value) = value {
            let definition = arena.alloc_str(&format!("{name} {value}"));
            pp.define(&scan(definition, "<command line>", &arena)?)?;
        } else {
            if !ident(name) {
                return Err(pp.error("expected macro name after -U"));
            }
            pp.macros.remove(name.as_str());
            pp.undefined.insert(arena.alloc_str(name));
        }
    }
    pp.file(src, path, 0, None, false)?;
    let output = if options.dump_macros {
        pp.macro_dump()
    } else {
        std::mem::take(&mut pp.output)
    };
    if TRACK {
        pp.stats.arena_bytes = arena.allocated_bytes();
    }
    Ok((output, pp.stats))
}

struct Preprocessor<'a, const TRACK: bool> {
    stats: PreprocessStats,
    arena: &'a Bump,
    sources: &'a typed_arena::Arena<FileSource>,
    token_buffers: &'a typed_arena::Arena<Box<[Token<'a>]>>,
    options: &'a PreprocessOptions,
    in_condition: bool,
    macros: HashMap<&'a str, &'a Macro<'a>>,
    expansion_scratch: Vec<Token<'a>>,
    undefined: HashSet<&'a str>,
    search_dirs: Vec<PathBuf>,
    framework_dirs: Vec<PathBuf>,
    include_stack: Vec<(&'a str, Option<usize>)>,
    once: HashSet<PathBuf>,
    canonical_paths: HashMap<PathBuf, PathBuf>,
    canonical_dirs: HashMap<PathBuf, PathBuf>,
    header_cache: HashMap<HeaderContext<'a>, HashMap<String, Option<Include>>>,
    file_sources: HashMap<(PathBuf, bool), &'a str>,
    scanned: HashMap<(PathBuf, bool), CachedTokens<'a>>,
    counter: u64,
    output: String,
}

enum FileSource {
    Read(String),
    Embedded(&'static str),
    Mapped(memmap2::Mmap),
}
#[derive(Clone, Copy)]
struct CachedTokens<'a> {
    tokens: &'a [Token<'a>],
    guard: Option<&'a str>,
}

// Recognize only a complete outer #ifndef / #define / #endif guard.
// An outer #else/#elif or any tokens after #endif disable this shortcut.
// The guard's current macro state is checked again on every inclusion.
fn header_guard<'a>(tokens: &[Token<'a>]) -> Option<&'a str> {
    let mut lines = tokens
        .split(|t| t.text == "\n")
        .filter(|line| !line.is_empty());
    let first = lines.next()?;
    if first.len() != 3
        || first[0].text != "#"
        || first[1].text != "ifndef"
        || !ident(first[2].text)
    {
        return None;
    }
    let guard = first[2].text;
    let define = lines.next()?;
    if define.len() < 3
        || define[0].text != "#"
        || define[1].text != "define"
        || define[2].text != guard
    {
        return None;
    }
    // Keep diagnostics for malformed nested conditionals even if the guard
    // is already defined on the first include. Deep headers use the regular
    // conditional parser rather than growing a separate optimization stack.
    let mut otherwise = [false; 32];
    let mut depth = 1usize;
    while let Some(line) = lines.next() {
        if line.first()?.text != "#" {
            continue;
        }
        match line.get(1).map(|t| t.text) {
            Some("if" | "ifdef" | "ifndef") => {
                if depth == otherwise.len() {
                    return None;
                }
                otherwise[depth] = false;
                depth += 1;
            }
            Some(directive @ ("else" | "elif")) => {
                if depth == 1 || otherwise[depth - 1] {
                    return None;
                }
                otherwise[depth - 1] = directive == "else";
            }
            Some("endif") => {
                depth -= 1;
                if depth == 0 {
                    return (lines.next().is_none()).then_some(guard);
                }
            }
            _ => {}
        }
    }
    None
}
impl FileSource {
    fn open(path: &Path) -> std::io::Result<Self> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        const MAP_HEADERS: bool = true;
        if MAP_HEADERS && metadata.is_file() && metadata.len() >= 64 * 1024 {
            // SAFETY: Included files remain stable during preprocessing, as
            // documented on preprocess. The owning source arena retains the
            // mapping until all borrowed tokens and cached &strs are dropped.
            // COW does not protect against another process truncating a file.
            if let Ok(map) = unsafe { memmap2::MmapOptions::new().map_copy_read_only(&file) } {
                return Ok(Self::Mapped(map));
            }
        }
        let mut text = String::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        file.read_to_string(&mut text)?;
        Ok(Self::Read(text))
    }
    fn text(&self) -> std::io::Result<&str> {
        match self {
            Self::Read(text) => Ok(text),
            Self::Embedded(text) => Ok(text),
            Self::Mapped(map) => simdutf8::basic::from_utf8(map).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                )
            }),
        }
    }
}

fn hide_insert<'a>(arena: &'a Bump, hide: &'a [&'a str], name: &'a str) -> &'a [&'a str] {
    if hide.contains(&name) {
        return hide;
    }
    let mut out = BumpVec::with_capacity_in(hide.len() + 1, arena);
    out.extend_from_slice(hide);
    out.push(name);
    out.into_bump_slice()
}
fn hide_union<'a>(arena: &'a Bump, left: &'a [&'a str], right: &'a [&'a str]) -> &'a [&'a str] {
    if left.is_empty() {
        return right;
    }
    if right.iter().all(|s| left.contains(s)) {
        return left;
    }
    if left.iter().all(|s| right.contains(s)) {
        return right;
    }
    let mut out = BumpVec::with_capacity_in(left.len() + right.len(), arena);
    out.extend_from_slice(left);
    for &name in right {
        if !left.contains(&name) {
            out.push(name);
        }
    }
    out.into_bump_slice()
}
fn hide_intersection<'a>(
    arena: &'a Bump,
    left: &'a [&'a str],
    right: &'a [&'a str],
) -> &'a [&'a str] {
    if left.is_empty() || right.is_empty() {
        return &[];
    }
    if left.iter().all(|s| right.contains(s)) {
        return left;
    }
    let mut out = BumpVec::with_capacity_in(left.len().min(right.len()), arena);
    for &name in left {
        if right.contains(&name) {
            out.push(name);
        }
    }
    out.into_bump_slice()
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    quoted_to(&mut out, s);
    out
}
fn quoted_to(out: &mut String, s: &str) {
    out.push('"');
    let mut from = 0;
    for offset in memchr::memchr2_iter(b'\\', b'"', s.as_bytes()) {
        out.push_str(&s[from..offset]);
        out.push('\\');
        out.push(s.as_bytes()[offset] as char);
        from = offset + 1;
    }
    out.push_str(&s[from..]);
    out.push('"');
}

fn ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && c.all(|c| c == '_' || c.is_alphanumeric())
}

// Keep non-ASCII bytes in the same token, as before; semantic identifier
// validation remains Unicode-aware. The table combines the common byte tests
// in the identifier and preprocessing-number loops into one indexed load.
const IDENT_START: u8 = 1;
const IDENT_CONTINUE: u8 = 2;
const NUMBER_CONTINUE: u8 = 4;
const BYTE_CLASS: [u8; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < table.len() {
        let b = i as u8;
        if b.is_ascii_alphabetic() || b == b'_' || b >= 128 {
            table[i] = IDENT_START | IDENT_CONTINUE | NUMBER_CONTINUE;
        } else if b.is_ascii_digit() {
            table[i] = IDENT_CONTINUE | NUMBER_CONTINUE;
        } else if b == b'.' {
            table[i] = NUMBER_CONTINUE;
        }
        i += 1;
    }
    table
};

/// Logical offsets where phase 2 removed bytes, and the cumulative removal.
/// Most input can be scanned directly, without a byte-sized location table.
struct Spliced<'a> {
    bytes: Cow<'a, [u8]>,
    removed: Vec<(usize, usize)>,
}

fn splice(src: &[u8]) -> Spliced<'_> {
    let mut removed = Vec::new();
    let mut output = Vec::new();
    let mut copied = 0;
    let mut search = 0;
    while let Some(offset) = find_byte(b'\\', &src[search..]) {
        let at = search + offset;
        let len = if src.get(at + 1) == Some(&b'\n') {
            2
        } else if src.get(at + 1..at + 3) == Some(b"\r\n") {
            3
        } else {
            search = at + 1;
            continue;
        };
        if removed.is_empty() {
            output.reserve(src.len());
        }
        output.extend_from_slice(&src[copied..at]);
        copied = at + len;
        removed.push((output.len(), copied - output.len()));
        search = copied;
    }
    if removed.is_empty() {
        Spliced {
            bytes: Cow::Borrowed(src),
            removed,
        }
    } else {
        output.extend_from_slice(&src[copied..]);
        Spliced {
            bytes: Cow::Owned(output),
            removed,
        }
    }
}

/// Advances only over consumed source spans. Sparse splice events translate
/// logical token offsets back to physical offsets, including adjacent splices.
struct Locations<'a> {
    source: &'a [u8],
    removed: &'a [(usize, usize)],
    next: usize,
    skipped: usize,
    next_newline: Option<usize>,
    line: u32,
    line_start: usize,
}
impl Locations<'_> {
    fn at(&mut self, logical: usize) -> (u32, u32) {
        while let Some(&(offset, skipped)) = self.removed.get(self.next) {
            if offset > logical {
                break;
            }
            self.skipped = skipped;
            self.next += 1;
        }
        let physical = logical + self.skipped;
        while let Some(newline) = self.next_newline {
            if newline >= physical {
                break;
            }
            self.line += 1;
            self.line_start = newline + 1;
            self.next_newline =
                find_byte(b'\n', &self.source[newline + 1..]).map(|offset| newline + 1 + offset);
        }
        (self.line, (physical - self.line_start + 1) as u32)
    }
}

// SIMD-backed searches use the portable memchr dispatch and bounded slices.
// Tiny token spans also use its word/scalar paths; no source padding is needed.
fn find_byte(needle: u8, bytes: &[u8]) -> Option<usize> {
    memchr::memchr(needle, bytes)
}
fn find_literal_end(quote: u8, bytes: &[u8]) -> Option<usize> {
    memchr::memchr3(quote, b'\\', b'\n', bytes)
}
fn find_comment_end(bytes: &[u8]) -> Option<usize> {
    memchr::memmem::find(bytes, b"*/")
}

fn punctuation_len(bytes: &[u8]) -> usize {
    let a = bytes[0];
    let b = bytes.get(1).copied().unwrap_or(0);
    let c = bytes.get(2).copied().unwrap_or(0);
    match a {
        b'%' if bytes.starts_with(b"%:%:") => 4,
        b'.' if b == b'.' && c == b'.' => 3,
        b'<' | b'>' if b == a => 2 + usize::from(c == b'='),
        b'#' if b == b'#' => 2,
        b'-' if b == b'>' || b == b'-' || b == b'=' => 2,
        b'+' | b'&' | b'|' if b == a || b == b'=' => 2,
        b'<' if b == b':' || b == b'%' || b == b'=' => 2,
        b'>' | b'=' | b'!' | b'*' | b'/' | b'^' if b == b'=' => 2,
        b'%' if b == b'>' || b == b':' || b == b'=' => 2,
        b':' if b == b'>' => 2,
        _ => 1,
    }
}

/// Phase 2 splicing precedes phase 3 comments and preprocessing tokens.
fn scan<'a>(src: &'a str, file: &'a str, arena: &'a Bump) -> Result<Vec<Token<'a>>, Diagnostic> {
    let input = splice(src.as_bytes());
    // Phase 2 removes only complete ASCII sequences, preserving UTF-8.
    // Validate the spliced buffer once rather than validating every token.
    let text: &'a str = match &input.bytes {
        Cow::Borrowed(_) => src,
        Cow::Owned(bytes) => arena.alloc_str(std::str::from_utf8(bytes).unwrap()),
    };
    let b = text.as_bytes();
    let mut locations = Locations {
        source: src.as_bytes(),
        removed: &input.removed,
        next: 0,
        skipped: 0,
        next_newline: find_byte(b'\n', src.as_bytes()),
        line: 1,
        line_start: 0,
    };
    let mut out = Vec::new();
    let (mut i, mut space) = (0, false);
    while i < b.len() {
        // Whitespace needs no token location. Advance the cursor once for the
        // entire span; physical newlines remain explicit preprocessing tokens.
        if b[i] != b'\n' && b[i].is_ascii_whitespace() {
            i += 1;
            while i < b.len() && b[i] != b'\n' && b[i].is_ascii_whitespace() {
                i += 1;
            }
            space = true;
            continue;
        }
        let start = i;
        let (line, col) = locations.at(i);
        let token = |text: &'a str, space| Token {
            text,
            space,
            raw_spelling: None,
            file,
            line,
            col,
            hide: &[],
        };
        if b[i] == b'\n' {
            out.push(token("\n", true));
            i += 1;
            space = true;
            continue;
        }
        if b.get(i..i + 2) == Some(b"//") {
            i += find_byte(b'\n', &b[i..]).unwrap_or(b.len() - i);
            space = true;
            continue;
        }
        if b.get(i..i + 2) == Some(b"/*") {
            let end = find_comment_end(&b[i + 2..])
                .map(|offset| i + 2 + offset)
                .ok_or_else(|| token("", false).error("unterminated comment"))?;
            i += 2;
            while let Some(offset) = find_byte(b'\n', &b[i..end]) {
                i += offset;
                let (line, col) = locations.at(i);
                out.push(Token {
                    line,
                    col,
                    ..token("\n", true)
                });
                i += 1;
            }
            i = end + 2;
            space = true;
            continue;
        }
        let prefix = if b.get(i..i + 2) == Some(b"u8") && matches!(b.get(i + 2), Some(b'"' | b'\''))
        {
            2
        } else if matches!(b[i], b'L' | b'u' | b'U') && matches!(b.get(i + 1), Some(b'"' | b'\'')) {
            1
        } else {
            0
        };
        if matches!(b.get(i + prefix), Some(b'"' | b'\'')) {
            i += prefix;
            let q = b[i];
            i += 1;
            loop {
                i += find_literal_end(q, &b[i.min(b.len())..]).unwrap_or(b.len() - i.min(b.len()));
                match b.get(i) {
                    Some(c) if *c == q => break,
                    Some(b'\\') => i += 2,
                    _ => return Err(token("", false).error("unterminated literal")),
                }
                if i > b.len() {
                    return Err(token("", false).error("unterminated literal"));
                }
            }
            i += 1;
        } else if b[i].is_ascii_digit()
            || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i += 1;
            while i < b.len() {
                if BYTE_CLASS[b[i] as usize] & NUMBER_CONTINUE != 0
                    || (matches!(b[i], b'+' | b'-')
                        && matches!(b[i - 1], b'e' | b'E' | b'p' | b'P'))
                {
                    i += 1;
                } else {
                    break;
                }
            }
        } else if BYTE_CLASS[b[i] as usize] & IDENT_START != 0 {
            i += 1;
            while i < b.len() && BYTE_CLASS[b[i] as usize] & IDENT_CONTINUE != 0 {
                i += 1;
            }
        } else {
            i += punctuation_len(&b[i..]);
        }
        let raw = text
            .get(start..i)
            .ok_or_else(|| token("", false).error("invalid UTF-8 token"))?;
        let normalized = match raw {
            "%:%:" => Some("##"),
            "%:" => Some("#"),
            "<:" => Some("["),
            ":>" => Some("]"),
            "<%" => Some("{"),
            "%>" => Some("}"),
            _ => None,
        };
        let mut t = token(normalized.unwrap_or(raw), space);
        t.raw_spelling = normalized.map(|_| raw);
        out.push(t);
        space = false;
    }
    Ok(out)
}

impl<'a, const TRACK: bool> Preprocessor<'a, TRACK> {
    fn macro_dump(&self) -> String {
        let mut names: Vec<_> = self.macros.keys().collect();
        names.sort();
        let mut out = String::new();
        for name in names {
            let m = &self.macros[name];
            out.push_str("#define ");
            out.push_str(name);
            if let Some(params) = &m.params {
                let mut params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                if m.variadic {
                    let last = params.last_mut().unwrap();
                    if last == "__VA_ARGS__" {
                        *last = "...".into();
                    } else {
                        last.push_str("...");
                    }
                }
                out.push('(');
                out.push_str(&params.join(","));
                out.push(')');
            }
            out.push(' ');
            out.push_str(&spelling(&m.body));
            out.push('\n');
        }
        out
    }
    fn native_paths(&mut self) {
        self.search_dirs
            .extend(self.options.include_dirs.iter().cloned());
        self.search_dirs
            .extend(self.options.system_include_dirs.iter().cloned());
        self.search_dirs.push(crate::headers::ROOT.into());
        // With an explicit root, never consult host system include directories.
        let root = self.options.sysroot.clone().or_else(|| {
            (self.options.target == keel_ir::Target::X86_64LinuxGnu
                && keel_ir::Target::host() == Some(self.options.target))
            .then(|| PathBuf::from("/"))
        });
        let Some(root) = root else {
            return;
        };
        if self.options.target == keel_ir::Target::X86_64LinuxGnu {
            self.search_dirs
                .push(root.join("usr/include/x86_64-linux-gnu"));
        }
        self.search_dirs.push(root.join("usr/include"));
        if self.options.target == keel_ir::Target::Aarch64AppleDarwin {
            self.framework_dirs
                .push(root.join("System/Library/Frameworks"));
        }
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            file: "<preprocessor>".into(),
            line: 1,
            col: 1,
            message: message.into(),
        }
    }
    fn define(&mut self, ts: &[Token<'a>]) -> Result<(), Diagnostic> {
        let Some(name) = ts.first() else {
            return Err(self.error("expected macro name"));
        };
        if !ident(&name.text) {
            return Err(name.error("expected macro name"));
        }
        let mut i = 1;
        let mut params: Option<&'a [&'a str]> = None;
        let mut variadic = false;
        if ts.get(i).is_some_and(|t| t.text == "(" && !t.space) {
            i += 1;
            let mut names = BumpVec::new_in(self.arena);
            while ts.get(i).is_some_and(|t| t.text != ")") {
                let p = &ts[i];
                if p.text == "..." {
                    variadic = true;
                    names.push("__VA_ARGS__".into());
                    i += 1;
                } else if ident(&p.text) && !names.contains(&p.text) {
                    names.push(p.text);
                    i += 1;
                    if ts.get(i).is_some_and(|t| t.text == "...") {
                        variadic = true;
                        i += 1;
                    }
                } else {
                    return Err(p.error("invalid or duplicate macro parameter"));
                }
                if variadic || ts.get(i).is_some_and(|t| t.text == ")") {
                    break;
                }
                if !ts.get(i).is_some_and(|t| t.text == ",") {
                    return Err(p.error("expected ',' in macro parameters"));
                }
                i += 1;
                if ts.get(i).is_some_and(|t| t.text == ")") {
                    return Err(p.error("expected macro parameter after ','"));
                }
            }
            if !ts.get(i).is_some_and(|t| t.text == ")") {
                return Err(name.error("unterminated macro parameters"));
            }
            i += 1;
            params = Some(names.into_bump_slice());
        }
        let body: &'a [Token<'a>] = self.arena.alloc_slice_copy(&ts[i..]);
        if body.first().is_some_and(|t| t.text == "##")
            || body.last().is_some_and(|t| t.text == "##")
        {
            return Err(name.error("'##' cannot appear at either end of a macro"));
        }
        if let Some(params) = &params {
            for (i, t) in body.iter().enumerate() {
                if t.text == "#" && !body.get(i + 1).is_some_and(|t| params.contains(&t.text)) {
                    return Err(t.error("'#' must precede a macro parameter"));
                }
            }
        }
        self.macros.insert(
            name.text,
            self.arena.alloc(Macro {
                params,
                variadic,
                has_paste: body.iter().any(|t| t.text == "##"),
                body,
            }),
        );
        Ok(())
    }

    fn emit(&mut self, ts: &[Token<'a>]) {
        let mut prev: Option<(&str, u32)> = None;
        for t in ts {
            if t.text.is_empty() {
                continue;
            }
            if prev != Some((&t.file, t.line)) {
                if prev.is_some() {
                    self.output.push('\n');
                }
                write!(self.output, "# {} ", t.line).unwrap();
                quoted_to(&mut self.output, t.file);
                self.output.push('\n');
                prev = Some((&t.file, t.line));
            }
            self.output.push_str(&t.text);
            self.output.push(' ');
            if t.text.contains('\n') {
                prev = None;
            }
        }
        if prev.is_some() {
            self.output.push('\n');
        }
    }
    fn flush(&mut self, pending: &mut Vec<Token<'a>>) -> Result<(), Diagnostic> {
        if pending.is_empty() {
            return Ok(());
        }
        let mut expanded = self.expand(std::mem::take(pending), 0)?;
        self.emit(&expanded);
        expanded.clear();
        *pending = expanded;
        Ok(())
    }
    fn file(
        &mut self,
        src: &'a str,
        path: &'a str,
        depth: usize,
        origin: Option<usize>,
        embedded: bool,
    ) -> Result<(), Diagnostic> {
        if depth > 200 {
            return Err(self.error("include nesting exceeds 200"));
        }
        let canonical = self.canonical_path(Path::new(path));
        if self.once.contains(&canonical) {
            if TRACK {
                self.stats.once_skips += 1;
            }
            return Ok(());
        }
        // The caller-provided top-level text may differ from the on-disk file.
        // Only disk/embedded includes participate in the lexical file cache.
        let key = (canonical.clone(), embedded);
        let cached = if depth == 0 || !CACHE_HEADERS {
            if TRACK {
                self.stats.scanned_files += 1;
            }
            CachedTokens {
                tokens: self.scan_file(src, path)?,
                guard: None,
            }
        } else if let Some(&cached) = self.scanned.get(&key) {
            if TRACK {
                self.stats.token_cache_hits += 1;
            }
            cached
        } else {
            if TRACK {
                self.stats.scanned_files += 1;
            }
            let tokens = self.scan_file(src, path)?;
            let cached = CachedTokens {
                tokens,
                guard: header_guard(tokens),
            };
            self.scanned.insert(key, cached);
            cached
        };
        if cached
            .guard
            .is_some_and(|name| self.macros.contains_key(name))
        {
            if TRACK {
                self.stats.guard_skips += 1;
            }
            return Ok(());
        }
        self.include_stack.push((path.into(), origin));
        let ts = cached.tokens;
        let mut pending = Vec::new();
        let mut stack: Vec<Conditional> = Vec::new();
        let mut logical_file = path;
        let mut line_delta: i64 = 0;
        for physical in ts.split(|t| t.text == "\n") {
            let active = stack.last().is_none_or(|c| c.active);
            if !physical.first().is_some_and(|t| t.text == "#") {
                if active {
                    let start = pending.len();
                    pending.extend_from_slice(physical);
                    let added = &mut pending[start..];
                    if line_delta != 0 || added.first().is_some_and(|t| t.file != logical_file) {
                        for t in added.iter_mut() {
                            t.file = logical_file;
                            t.line = (t.line as i64 + line_delta) as u32;
                        }
                    }
                    if let Some(first) = added.first_mut() {
                        first.space = true;
                    }
                }
                continue;
            }
            self.flush(&mut pending)?;
            // Borrow directive lines unless #line or a cached path alias needs
            // new locations. Inactive ordinary lines require no token copies.
            let physical_line = physical.get(1).map_or(0, |t| t.line);
            let mut remapped;
            let line =
                if line_delta != 0 || physical.first().is_some_and(|t| t.file != logical_file) {
                    remapped = physical.to_vec();
                    for t in &mut remapped {
                        t.file = logical_file;
                        t.line = (t.line as i64 + line_delta) as u32;
                    }
                    remapped.as_slice()
                } else {
                    physical
                };
            let Some(directive) = line.get(1) else {
                continue;
            };
            let args = &line[2..];
            match directive.text {
                "if" | "ifdef" | "ifndef" => {
                    let yes = if !active {
                        false
                    } else if directive.text == "if" {
                        self.condition(args)?
                    } else {
                        if args.len() != 1 || !ident(&args[0].text) {
                            return Err(directive.error("expected one macro name"));
                        }
                        self.is_defined(&args[0].text) != (directive.text == "ifndef")
                    };
                    stack.push(Conditional {
                        parent: active,
                        active: active && yes,
                        taken: yes,
                        otherwise: false,
                        start: *directive,
                    });
                }
                "elif" | "else" => {
                    let Some(c) = stack.last_mut() else {
                        return Err(directive.error("conditional without #if"));
                    };
                    if c.otherwise {
                        return Err(directive.error("conditional after #else"));
                    }
                    let yes = if directive.text == "else" {
                        c.otherwise = true;
                        true
                    } else if c.parent && !c.taken {
                        self.condition(args)?
                    } else {
                        false
                    };
                    c.active = c.parent && !c.taken && yes;
                    c.taken |= c.active;
                }
                "endif" => {
                    if stack.pop().is_none() {
                        return Err(directive.error("#endif without #if"));
                    }
                }
                _ if !active => {}
                "define" => self.define(args)?,
                "undef" => {
                    if args.len() != 1 || !ident(&args[0].text) {
                        return Err(directive.error("expected one macro name"));
                    }
                    self.macros.remove(args[0].text);
                    self.undefined.insert(args[0].text);
                }
                "include" | "include_next" => {
                    let expanded = if args
                        .first()
                        .is_some_and(|t| t.text == "<" || t.text.starts_with('"'))
                    {
                        args.to_vec()
                    } else {
                        self.expand(args.to_vec(), 0)?
                    };
                    let (name, quoted) =
                        header(&expanded).ok_or_else(|| directive.error("expected header name"))?;
                    let include = self
                        .find_header(&name, quoted, directive.text == "include_next")
                        .ok_or_else(|| directive.error(format!("header not found: {name}")))?;
                    self.include_file(include, depth + 1, directive)?;
                }
                "pragma" => {
                    if args.first().is_some_and(|t| t.text == "once") {
                        self.once.insert(canonical.clone());
                    } else {
                        self.output.push_str("#pragma ");
                        self.output.push_str(&spelling(args));
                        self.output.push('\n');
                    }
                }
                "line" => {
                    let expanded = self.expand(args.to_vec(), 0)?;
                    let n = expanded
                        .first()
                        .and_then(|t| t.text.parse::<u32>().ok())
                        .filter(|&n| n > 0)
                        .ok_or_else(|| directive.error("invalid #line number"))?;
                    // The directive's physical line, before any previous #line adjustment.
                    line_delta = n as i64 - physical_line as i64 - 1;
                    if let Some(t) = expanded.get(1) {
                        logical_file = self.arena.alloc_str(
                            &unquote(t.text).ok_or_else(|| t.error("expected #line file name"))?,
                        );
                    }
                }
                "error" => return Err(directive.error(spelling(args))),
                "warning" => {} // Warnings are currently non-fatal; no warning sink yet.
                _ => {
                    return Err(directive.error(format!(
                        "unsupported preprocessing directive #{}",
                        directive.text
                    )));
                }
            }
        }
        if let Some(c) = stack.last() {
            return Err(c.start.error("unterminated conditional directive"));
        }
        self.flush(&mut pending)?;
        self.include_stack.pop();
        Ok(())
    }
    fn scan_file(&mut self, src: &'a str, path: &'a str) -> Result<&'a [Token<'a>], Diagnostic> {
        // Large lexical buffers have exact-size owners. Growing them inside
        // the bump arena would retain discarded capacities until TU teardown.
        let tokens = scan(src, path, self.arena)?.into_boxed_slice();
        if TRACK {
            self.stats.token_bytes += std::mem::size_of_val(tokens.as_ref());
        }
        let owned: &'a Box<[Token<'a>]> = self.token_buffers.alloc(tokens);
        Ok(owned.as_ref())
    }
    fn canonical_path(&mut self, path: &Path) -> PathBuf {
        if let Some(canonical) = self.canonical_paths.get(path) {
            return canonical.clone();
        }
        // realpath walks every component. Most headers are ordinary files in
        // a few shared directories: resolve each directory once, then append
        // the final component only after verifying that it is not a symlink.
        // Symlinks (including dangling ones), missing paths and special path
        // components retain the full canonicalize/fallback behavior.
        let canonical = path
            .parent()
            .zip(path.file_name())
            .and_then(|(dir, name)| {
                let metadata = std::fs::symlink_metadata(path).ok()?;
                if !metadata.is_file() {
                    return None;
                }
                let canonical_dir = if let Some(dir) = self.canonical_dirs.get(dir) {
                    dir.clone()
                } else {
                    let canonical = std::fs::canonicalize(dir).ok()?;
                    self.canonical_dirs
                        .insert(dir.to_path_buf(), canonical.clone());
                    canonical
                };
                Some(canonical_dir.join(name))
            })
            .unwrap_or_else(|| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
        self.canonical_paths
            .insert(path.to_path_buf(), canonical.clone());
        canonical
    }
    fn include_file(
        &mut self,
        include: Include,
        depth: usize,
        at: &Token<'a>,
    ) -> Result<(), Diagnostic> {
        if TRACK {
            self.stats.include_requests += 1;
        }
        let canonical = self.canonical_path(&include.path);
        if self.once.contains(&canonical) {
            if TRACK {
                self.stats.once_skips += 1;
            }
            return Ok(());
        }
        let embedded = include.contents.is_some();
        let key = (canonical, embedded);
        let source = if CACHE_HEADERS && let Some(&source) = self.file_sources.get(&key) {
            if TRACK {
                self.stats.source_cache_hits += 1;
            }
            source
        } else {
            let loaded = if let Some(text) = include.contents {
                FileSource::Embedded(text)
            } else {
                if TRACK {
                    self.stats.file_opens += 1;
                }
                FileSource::open(&include.path)
                    .map_err(|e| at.error(format!("{}: {e}", include.path.display())))?
            };
            if TRACK && matches!(loaded, FileSource::Mapped(_)) {
                self.stats.mapped_files += 1;
            }
            let source = self
                .sources
                .alloc(loaded)
                .text()
                .map_err(|e| at.error(format!("{}: {e}", include.path.display())))?;
            if TRACK {
                self.stats.source_bytes += source.len();
            }
            if CACHE_HEADERS {
                self.file_sources.insert(key, source);
            }
            source
        };
        let path = self.arena.alloc_str(&include.path.to_string_lossy());
        self.file(source, path, depth, include.origin, embedded)
    }
    fn find_header(&mut self, name: &str, quoted: bool, next: bool) -> Option<Include> {
        let &(path, origin) = self.include_stack.last()?;
        let context = HeaderContext {
            including_file: (quoted && !next).then_some(path),
            origin: if next { origin } else { None },
            next,
        };
        if let Some(result) = self
            .header_cache
            .get(&context)
            .and_then(|cache| cache.get(name))
        {
            return result.clone();
        }
        let result = self.search_header(name, quoted, next);
        self.header_cache
            .entry(context)
            .or_default()
            .insert(name.to_owned(), result.clone());
        result
    }
    fn search_header(&self, name: &str, quoted: bool, next: bool) -> Option<Include> {
        let (path, origin) = self.include_stack.last()?;
        if quoted && !next {
            let path = Path::new(path)
                .parent()
                .unwrap_or(Path::new("."))
                .join(name);
            if path.is_file() {
                return Some(Include {
                    path,
                    origin: None,
                    contents: None,
                });
            }
        }
        let builtin_index =
            self.options.include_dirs.len() + self.options.system_include_dirs.len();
        for (i, dir) in self.search_dirs.iter().enumerate() {
            if next && origin.is_some_and(|origin| i <= origin) {
                continue;
            }
            let contents = if i == builtin_index {
                crate::headers::get(Path::new(name))
            } else {
                None
            };
            let path = dir.join(name);
            if contents.is_some() || path.is_file() {
                return Some(Include {
                    path,
                    origin: Some(i),
                    contents,
                });
            }
        }
        if let Some((framework, rest)) = name.split_once('/') {
            for (i, dir) in self.framework_dirs.iter().enumerate() {
                let i = i + self.search_dirs.len();
                if !next || origin.is_none_or(|origin| i > origin) {
                    let path = dir.join(format!("{framework}.framework/Headers/{rest}"));
                    if path.is_file() {
                        return Some(Include {
                            path,
                            origin: Some(i),
                            contents: None,
                        });
                    }
                }
            }
        }
        None
    }

    fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
            || (!self.undefined.contains(name)
                && (matches!(
                    name,
                    "__LINE__"
                        | "__FILE__"
                        | "__BASE_FILE__"
                        | "__FILE_NAME__"
                        | "__INCLUDE_LEVEL__"
                        | "__COUNTER__"
                        | "_Pragma"
                        | "__has_include"
                        | "__has_include_next"
                ) || query_builtin(name)))
    }
}
#[derive(Hash, PartialEq, Eq)]
struct HeaderContext<'a> {
    including_file: Option<&'a str>,
    origin: Option<usize>,
    next: bool,
}
#[derive(Clone)]
struct Include {
    path: PathBuf,
    origin: Option<usize>,
    contents: Option<&'static str>,
}
struct Conditional<'a> {
    parent: bool,
    active: bool,
    taken: bool,
    otherwise: bool,
    start: Token<'a>,
}
fn spelling<'a>(ts: &[Token<'a>]) -> String {
    let mut s = String::new();
    for t in ts {
        if t.space && !s.is_empty() {
            s.push(' ');
        }
        s.push_str(t.raw_spelling.unwrap_or(&t.text));
    }
    s
}
fn unquote(s: &str) -> Option<String> {
    let s = s.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' { chars.next()? } else { c });
    }
    Some(out)
}
fn header<'a>(ts: &[Token<'a>]) -> Option<(String, bool)> {
    if ts.len() == 1 {
        return unquote(&ts[0].text).map(|s| (s, true));
    }
    if ts.first()?.text == "<" && ts.last()?.text == ">" {
        return Some((ts[1..ts.len() - 1].iter().map(|t| t.text).collect(), false));
    }
    None
}
fn query_builtin(name: &str) -> bool {
    matches!(
        name,
        "__building_module"
            | "__has_builtin"
            | "__has_feature"
            | "__has_extension"
            | "__has_attribute"
            | "__has_c_attribute"
            | "__has_declspec_attribute"
            | "__is_identifier"
            | "__is_target_arch"
            | "__is_target_vendor"
            | "__is_target_os"
            | "__is_target_environment"
            | "__is_target_variant_os"
            | "__is_target_variant_environment"
    )
}

// Unconsumed tokens are stored backwards: replacing the prefix pops the
// invocation and pushes its replacement without shifting the remaining file.
// Rescanning still sees replacement tokens adjacent to the original suffix.
struct ExpansionInput<'a>(Vec<Token<'a>>);
impl<'a> ExpansionInput<'a> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> Option<&Token<'a>> {
        self.0
            .len()
            .checked_sub(index + 1)
            .and_then(|i| self.0.get(i))
    }
    fn iter(&self) -> impl Iterator<Item = &Token<'a>> {
        self.0.iter().rev()
    }
    fn pop(&mut self) -> Option<Token<'a>> {
        self.0.pop()
    }
    fn replace<R>(&mut self, end: usize, replacement: R)
    where
        R: IntoIterator<Item = Token<'a>>,
        R::IntoIter: DoubleEndedIterator,
    {
        self.0.truncate(self.0.len() - end);
        self.0.extend(replacement.into_iter().rev());
    }
}
impl<'a> std::ops::Index<usize> for ExpansionInput<'a> {
    type Output = Token<'a>;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[self.0.len() - 1 - index]
    }
}
impl std::ops::IndexMut<usize> for ExpansionInput<'_> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let position = self.0.len() - 1 - index;
        &mut self.0[position]
    }
}

/// Returns arguments and the index immediately after the closing parenthesis.
fn arguments<'a>(
    ts: &ExpansionInput<'a>,
    open: usize,
) -> Result<(Vec<Vec<Token<'a>>>, usize), Diagnostic> {
    let mut args = vec![Vec::new()];
    let mut depth = 0;
    for (i, t) in ts.iter().enumerate().skip(open + 1) {
        match t.text {
            "(" => depth += 1,
            ")" if depth == 0 => return Ok((args, i + 1)),
            ")" => depth -= 1,
            "," if depth == 0 => {
                args.push(Vec::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().unwrap().push(*t);
    }
    Err(ts[open].error("unterminated macro invocation"))
}

impl<'a, const TRACK: bool> Preprocessor<'a, TRACK> {
    fn expand(
        &mut self,
        mut ts: Vec<Token<'a>>,
        depth: usize,
    ) -> Result<Vec<Token<'a>>, Diagnostic> {
        if depth > 200 {
            return Err(self.error("macro argument nesting exceeds 200"));
        }
        ts.reverse();
        let mut input = ExpansionInput(ts);
        let mut out = std::mem::take(&mut self.expansion_scratch);
        out.reserve(input.len());
        let mut steps = 0;
        while !input.0.is_empty() {
            steps += 1;
            if steps > 1_000_000 || input.len().saturating_add(out.len()) > 1_000_000 {
                return Err(input[0].error("macro expansion limit exceeded"));
            }
            let current = &input[0];
            let name = current.text;
            let first = name.as_bytes().first().copied().unwrap_or(0);
            if !(first.is_ascii_alphabetic() || first == b'_' || first >= 128)
                || (current.hide.contains(&name) && !(self.in_condition && name == "defined"))
            {
                out.push(input.pop().unwrap());
                continue;
            }
            let macro_definition = self.macros.get(name).copied();
            let special = (self.in_condition && name == "defined")
                || matches!(
                    name,
                    "__LINE__"
                        | "__FILE__"
                        | "__BASE_FILE__"
                        | "__FILE_NAME__"
                        | "__INCLUDE_LEVEL__"
                        | "__COUNTER__"
                        | "__has_include"
                        | "__has_include_next"
                        | "_Pragma"
                )
                || query_builtin(name);
            if !special && macro_definition.is_none() {
                out.push(input.pop().unwrap());
                continue;
            }
            let t = *current;
            let name = t.text;
            if name == "defined" && self.in_condition {
                let mut end = 1;
                let paren = input.get(end).is_some_and(|t| t.text == "(");
                if paren {
                    end += 1;
                }
                let operand = input
                    .get(end)
                    .filter(|t| ident(&t.text))
                    .ok_or_else(|| t.error("expected identifier after defined"))?;
                let value = self.is_defined(&operand.text);
                end += 1;
                if paren {
                    if !input.get(end).is_some_and(|t| t.text == ")") {
                        return Err(t.error("expected ')' after defined"));
                    }
                    end += 1;
                }
                input.replace(end, [t.with_text(self.arena, (value as u8).to_string())]);
                out.push(input.pop().unwrap());
                continue;
            }
            if t.hide.contains(&name) {
                out.push(input.pop().unwrap());
                continue;
            }
            let builtin_enabled = macro_definition.is_none() && !self.undefined.contains(name);
            let builtin = if builtin_enabled {
                match name {
                    "__LINE__" => Some(t.line.to_string()),
                    "__FILE__" => Some(quote(&t.file)),
                    "__BASE_FILE__" => Some(quote(&self.include_stack[0].0)),
                    "__FILE_NAME__" => Some(quote(
                        Path::new(t.file)
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or(""),
                    )),
                    "__INCLUDE_LEVEL__" => Some((self.include_stack.len() - 1).to_string()),
                    "__COUNTER__" => {
                        let n = self.counter;
                        self.counter += 1;
                        Some(n.to_string())
                    }
                    _ => None,
                }
            } else {
                None
            };
            if let Some(value) = builtin {
                input[0].text = self.arena.alloc_str(&value);
                out.push(input.pop().unwrap());
                continue;
            }
            if (matches!(name, "__has_include" | "__has_include_next") || query_builtin(name))
                && builtin_enabled
            {
                if !input.get(1).is_some_and(|t| t.text == "(") {
                    return Err(t.error("expected '(' after preprocessing builtin"));
                }
                let (args, end) = arguments(&input, 1)?;
                if args.len() != 1 {
                    return Err(t.error("expected one argument to preprocessing builtin"));
                }
                let value = if matches!(name, "__has_include" | "__has_include_next") {
                    let expanded = self.expand(args[0].clone(), depth + 1)?;
                    let (h, q) = header(&expanded)
                        .ok_or_else(|| t.error("expected header name in __has_include"))?;
                    (self
                        .find_header(&h, q, name == "__has_include_next")
                        .is_some() as u8)
                        .to_string()
                } else {
                    if args[0].len() != 1 || !ident(&args[0][0].text) {
                        return Err(t.error("expected identifier in feature query"));
                    }
                    crate::features::query(name, &args[0][0].text, self.options.target).to_string()
                };
                input.replace(end, [t.with_text(self.arena, value)]);
                out.push(input.pop().unwrap());
                continue;
            }
            if name == "_Pragma" && builtin_enabled {
                if !input.get(1).is_some_and(|t| t.text == "(") {
                    return Err(t.error("expected '(' after _Pragma"));
                }
                let (args, end) = arguments(&input, 1)?;
                if args.len() != 1 {
                    return Err(t.error("expected one _Pragma argument"));
                }
                let expanded = self.expand(args[0].clone(), depth + 1)?;
                let value = if expanded.len() == 1 {
                    unquote(&expanded[0].text)
                } else {
                    None
                }
                .ok_or_else(|| t.error("expected string literal in _Pragma"))?;
                // Keep pragmas in the output stream, in token order.
                input.replace(
                    end,
                    [t.with_text(self.arena, format!("\n#pragma {value}\n"))],
                );
                out.push(input.pop().unwrap());
                continue;
            }
            let Some(m) = macro_definition else {
                out.push(input.pop().unwrap());
                continue;
            };
            let (end, mut replacement, hide) = if let Some(params) = &m.params {
                if !input.get(1).is_some_and(|t| t.text == "(") {
                    out.push(input.pop().unwrap());
                    continue;
                }
                let (mut args, end) = arguments(&input, 1)?;
                if params.is_empty() && args.len() == 1 && args[0].is_empty() {
                    args.clear();
                }
                let fixed = params.len() - usize::from(m.variadic);
                let omitted = m.variadic && args.len() <= fixed;
                if args.len() < fixed || (!m.variadic && args.len() != fixed) {
                    return Err(t.error(format!("wrong number of arguments to macro {name}")));
                }
                if m.variadic {
                    let extras = args.split_off(fixed);
                    let mut rest = Vec::new();
                    for (n, arg) in extras.into_iter().enumerate() {
                        if n > 0 {
                            rest.push(t.with_text(self.arena, ","));
                        }
                        rest.extend(arg);
                    }
                    args.push(rest);
                }
                let mut expanded = HashMap::new();
                let mut body = Vec::new();
                let mut j = 0;
                while j < m.body.len() {
                    let b = &m.body[j];
                    if b.text == "#" {
                        let p = params
                            .iter()
                            .position(|p| p == &m.body[j + 1].text)
                            .unwrap();
                        body.push((b.with_text(self.arena, quote(&spelling(&args[p]))), false));
                        j += 2;
                        continue;
                    }
                    // GNU comma elision: only a missing variadic argument
                    // removes the comma; an explicitly empty one preserves it.
                    if m.variadic
                        && b.text == ","
                        && m.body.get(j + 1).is_some_and(|t| t.text == "##")
                        && m.body
                            .get(j + 2)
                            .is_some_and(|t| Some(&t.text) == params.last())
                    {
                        if !omitted {
                            body.push((*b, false));
                            body.extend(
                                self.expand(args.last().unwrap().clone(), depth + 1)?
                                    .into_iter()
                                    .map(|t| (t, false)),
                            );
                        }
                        j += 3;
                        continue;
                    }
                    if let Some(p) = params.iter().position(|p| *p == b.text) {
                        let paste = (j > 0 && m.body[j - 1].text == "##")
                            || m.body.get(j + 1).is_some_and(|t| t.text == "##");
                        let mut value = if paste {
                            args[p].clone()
                        } else {
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                expanded.entry(p)
                            {
                                entry.insert(self.expand(args[p].clone(), depth + 1)?);
                            }
                            expanded[&p].clone()
                        };
                        if value.is_empty() && paste {
                            value.push(b.with_text(self.arena, ""));
                        }
                        if let Some(first) = value.first_mut() {
                            first.space = b.space;
                        }
                        // Argument tokens spelling ## are ordinary tokens,
                        // never operators in this replacement list.
                        body.extend(value.into_iter().map(|t| (t, false)));
                    } else {
                        body.push((*b, b.text == "##"));
                    }
                    j += 1;
                }
                let hide = hide_intersection(self.arena, t.hide, input[end - 1].hide);
                (end, self.paste(body)?, hide)
            } else {
                (
                    1,
                    if m.has_paste {
                        self.paste(m.body.iter().map(|t| (*t, t.text == "##")).collect())?
                    } else {
                        m.body.to_vec()
                    },
                    t.hide,
                )
            };
            let hide = hide_insert(self.arena, hide, name);
            for r in &mut replacement {
                r.hide = hide_union(self.arena, r.hide, hide);
                r.file = t.file;
                r.line = t.line;
                r.col = t.col;
            }
            if let Some(first) = replacement.first_mut() {
                first.space = t.space;
            }
            input.replace(end, replacement);
        }
        // Recycle small drained inputs across conditions/arguments. Retaining
        // an entire source-sized buffer here would raise per-TU peak memory.
        if input.0.capacity() <= 4096 && input.0.capacity() >= self.expansion_scratch.capacity() {
            self.expansion_scratch = input.0;
        }
        Ok(out)
    }
    fn paste(&self, body: Vec<(Token<'a>, bool)>) -> Result<Vec<Token<'a>>, Diagnostic> {
        let mut out: Vec<Token<'a>> = Vec::new();
        let mut body = body.into_iter();
        while let Some((t, operator)) = body.next() {
            if operator {
                let left = out
                    .pop()
                    .ok_or_else(|| t.error("missing left operand to ##"))?;
                let (right, _) = body
                    .next()
                    .ok_or_else(|| t.error("missing right operand to ##"))?;
                let text = format!(
                    "{}{}",
                    left.raw_spelling.unwrap_or(&left.text),
                    right.raw_spelling.unwrap_or(&right.text)
                );
                let mut result = left.with_text(self.arena, &text);
                if !text.is_empty() {
                    let check = scan(self.arena.alloc_str(&text), t.file, self.arena)?;
                    if check.len() != 1 || check[0].raw_spelling.unwrap_or(&check[0].text) != text {
                        return Err(t.error(format!(
                            "token paste does not form one preprocessing token: {text}"
                        )));
                    }
                    result.text = check[0].text;
                    result.raw_spelling = check[0].raw_spelling;
                }
                result.hide = hide_intersection(self.arena, left.hide, right.hide);
                out.push(result);
            } else {
                out.push(t);
            }
        }
        out.retain(|t| !t.text.is_empty());
        Ok(out)
    }
    fn condition(&mut self, ts: &[Token<'a>]) -> Result<bool, Diagnostic> {
        if ts.is_empty() {
            return Err(self.error("expected #if expression"));
        }
        self.in_condition = true;
        let expanded = self.expand(ts.to_vec(), 0);
        self.in_condition = false;
        let mut tokens = expanded?;
        for t in &mut tokens {
            if ident(&t.text) {
                t.text = "0";
            }
        }
        if tokens.is_empty() {
            return Err(ts[0].error("expected #if expression after expansion"));
        }
        let mut expr = Expression {
            ts: &tokens,
            pos: 0,
        };
        let value = expr.eval(0, true)?;
        if expr.pos != tokens.len() {
            return Err(tokens[expr.pos].error("unexpected token in #if expression"));
        }
        Ok(value.bits != 0)
    }
}

#[derive(Clone, Copy)]
struct Integer {
    bits: u64,
    unsigned: bool,
}
impl Integer {
    fn boolean(v: bool) -> Self {
        Self {
            bits: v as u64,
            unsigned: false,
        }
    }
    fn negative(self) -> bool {
        !self.unsigned && (self.bits as i64) < 0
    }
}
struct Expression<'t, 'a> {
    ts: &'t [Token<'a>],
    pos: usize,
}
impl Expression<'_, '_> {
    fn eval(&mut self, min: u8, live: bool) -> Result<Integer, Diagnostic> {
        let Some(t) = self.ts.get(self.pos) else {
            return Err(self
                .ts
                .last()
                .unwrap()
                .error("expected operand in #if expression"));
        };
        self.pos += 1;
        let mut lhs = match t.text {
            "(" => {
                let v = self.eval(0, live)?;
                self.expect(")", t)?;
                v
            }
            "+" | "-" | "!" | "~" => {
                let v = self.eval(13, live)?;
                match t.text {
                    "+" => v,
                    "-" => Integer {
                        bits: v.bits.wrapping_neg(),
                        ..v
                    },
                    "~" => Integer { bits: !v.bits, ..v },
                    _ => Integer::boolean(v.bits == 0),
                }
            }
            _ => integer(t)?,
        };
        while let Some(op) = self.ts.get(self.pos) {
            if op.text == "?" && min <= 1 {
                self.pos += 1;
                let yes = self.eval(0, live && lhs.bits != 0)?;
                self.expect(":", op)?;
                let no = self.eval(1, live && lhs.bits == 0)?;
                let bits = if lhs.bits != 0 { yes.bits } else { no.bits };
                lhs = Integer {
                    bits,
                    unsigned: yes.unsigned || no.unsigned,
                };
                continue;
            }
            let prec = match op.text {
                "||" => 2,
                "&&" => 3,
                "|" => 4,
                "^" => 5,
                "&" => 6,
                "==" | "!=" => 7,
                "<" | "<=" | ">" | ">=" => 8,
                "<<" | ">>" => 9,
                "+" | "-" => 10,
                "*" | "/" | "%" => 11,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.pos += 1;
            let rhs = self.eval(
                prec + 1,
                live && match op.text {
                    "&&" => lhs.bits != 0,
                    "||" => lhs.bits == 0,
                    _ => true,
                },
            )?;
            let (a, b) = (lhs.bits, rhs.bits);
            let unsigned = lhs.unsigned || rhs.unsigned;
            let less = if unsigned {
                a < b
            } else {
                (a as i64) < (b as i64)
            };
            let value = match op.text {
                "||" => Integer::boolean(a != 0 || b != 0),
                "&&" => Integer::boolean(a != 0 && b != 0),
                "==" => Integer::boolean(a == b),
                "!=" => Integer::boolean(a != b),
                "<" => Integer::boolean(less),
                ">" => Integer::boolean(!less && a != b),
                "<=" => Integer::boolean(less || a == b),
                ">=" => Integer::boolean(!less),
                _ => {
                    let bits = match op.text {
                        "|" => a | b,
                        "^" => a ^ b,
                        "&" => a & b,
                        "+" => a.wrapping_add(b),
                        "-" => a.wrapping_sub(b),
                        "*" => a.wrapping_mul(b),
                        "<<" | ">>" => {
                            if live && (rhs.negative() || b >= 64) {
                                return Err(op.error("invalid shift count in #if"));
                            }
                            if op.text == "<<" {
                                a.wrapping_shl(b as u32)
                            } else if lhs.unsigned {
                                a.wrapping_shr(b as u32)
                            } else {
                                (a as i64).wrapping_shr(b as u32) as u64
                            }
                        }
                        "/" | "%" => {
                            if b == 0 {
                                if live {
                                    return Err(op.error("division by zero in #if"));
                                }
                                0
                            } else if unsigned {
                                if op.text == "/" { a / b } else { a % b }
                            } else if op.text == "/" {
                                (a as i64).wrapping_div(b as i64) as u64
                            } else {
                                (a as i64).wrapping_rem(b as i64) as u64
                            }
                        }
                        _ => unreachable!(),
                    };
                    Integer {
                        bits,
                        unsigned: if matches!(op.text, "<<" | ">>") {
                            lhs.unsigned
                        } else {
                            unsigned
                        },
                    }
                }
            };
            lhs = value;
        }
        Ok(lhs)
    }
    fn expect(&mut self, expected: &str, at: &Token<'_>) -> Result<(), Diagnostic> {
        if self.ts.get(self.pos).is_some_and(|t| t.text == expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(at.error(format!("expected '{expected}' in #if expression")))
        }
    }
}
fn integer(t: &Token<'_>) -> Result<Integer, Diagnostic> {
    if t.text.contains('\'') {
        let mut files = Vec::new();
        let tokens =
            crate::lex::tokenize(&t.text, &t.file, &mut files).map_err(|e| t.error(e.message))?;
        if let crate::lex::Tok::Char(value) = tokens[0].tok {
            return Ok(Integer {
                bits: value as u64,
                unsigned: false,
            });
        }
        return Err(t.error("invalid character constant in #if"));
    }
    let s = &t.text;
    let (radix, start) = if s.starts_with("0x") || s.starts_with("0X") {
        (16, 2)
    } else if s.starts_with("0b") || s.starts_with("0B") {
        (2, 2)
    } else if s.starts_with('0') && s.len() > 1 {
        (8, 0)
    } else {
        (10, 0)
    };
    let end = s[start..]
        .find(|c: char| !c.is_digit(radix))
        .map_or(s.len(), |n| n + start);
    let suffix = s[end..].to_ascii_lowercase();
    if !matches!(
        suffix.as_str(),
        "" | "u" | "l" | "ul" | "lu" | "ll" | "ull" | "llu"
    ) {
        return Err(t.error("invalid integer constant in #if"));
    }
    let bits = u64::from_str_radix(&s[start..end], radix)
        .map_err(|_| t.error("invalid integer constant in #if"))?;
    Ok(Integer {
        bits,
        unsigned: suffix.contains('u') || bits > i64::MAX as u64,
    })
}

#[cfg(test)]
mod scanner_tests {
    #[test]
    fn source_loading_preserves_utf8_and_empty_file_behavior() {
        let dir = std::env::temp_dir().join(format!("keel-pp-source-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("input.h");
        for bytes in [
            Vec::new(),
            "/* λ中 */\n".repeat(16384).into_bytes(),
            vec![0xff; 65536],
        ] {
            std::fs::write(&path, &bytes).unwrap();
            let source = FileSource::open(&path).unwrap();
            match std::str::from_utf8(&bytes) {
                Ok(expected) => assert_eq!(source.text().unwrap(), expected),
                Err(_) => assert_eq!(
                    source.text().unwrap_err().kind(),
                    std::io::ErrorKind::InvalidData
                ),
            }
            drop(source); // Release mappings before rewriting the file.
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    use super::*;

    #[test]
    fn validated_token_slices_preserve_unicode_across_splices() {
        for src in ["λ中 u8\"héλ\" 42", "λ\\\n中 u8\"hé\\\r\nλ\" 42"] {
            let arena = Bump::new();
            let tokens = scan(src, "unicode.c", &arena).unwrap();
            assert_eq!(
                tokens.iter().map(|t| t.text).collect::<Vec<_>>(),
                ["λ中", "u8\"héλ\"", "42"]
            );
        }
    }

    #[test]
    fn sparse_splice_locations_match_physical_source_bytes() {
        // An independent phase-2 oracle records every surviving byte's physical
        // position. Include adjacent splices, CRLF, UTF-8, and tail boundaries.
        for source in [
            "",
            "hello\nworld",
            "a\\\nb",
            "\\\n\\\r\na\\\n",
            "abc\\\r\ndef\n\\\nxyz",
            "α\\\nβ\n\"a\\\nb\"",
            "\\",
            "\\\r",
            "\n\\\n\n",
        ] {
            let bytes = source.as_bytes();
            let mut expected = Vec::new();
            let (mut i, mut line, mut col) = (0, 1, 1);
            while i < bytes.len() {
                if bytes[i..].starts_with(b"\\\n") || bytes[i..].starts_with(b"\\\r\n") {
                    i += if bytes[i + 1] == b'\r' { 3 } else { 2 };
                    line += 1;
                    col = 1;
                    continue;
                }
                expected.push((bytes[i], line, col));
                if bytes[i] == b'\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
            let input = splice(bytes);
            assert_eq!(
                input.bytes.as_ref(),
                expected.iter().map(|x| x.0).collect::<Vec<_>>()
            );
            let mut locations = Locations {
                source: bytes,
                removed: &input.removed,
                next: 0,
                skipped: 0,
                next_newline: find_byte(b'\n', bytes),
                line: 1,
                line_start: 0,
            };
            for (offset, &(_, line, col)) in expected.iter().enumerate() {
                assert_eq!(locations.at(offset), (line, col), "{source:?} at {offset}");
            }
        }
    }

    #[test]
    fn punctuation_uses_longest_match_and_preserves_digraph_spelling() {
        let source = "%:%: <<= >>= ... ## -> ++ -- << >> <= >= == != && || *= /= %= += -= &= ^= |= <: :> <% %> %: < > . % : #";
        let arena = Bump::new();
        let tokens = scan(source, "punct.c", &arena).unwrap();
        let spelling: Vec<_> = tokens
            .iter()
            .map(|t| t.raw_spelling.unwrap_or(&t.text))
            .collect();
        assert_eq!(spelling, source.split_whitespace().collect::<Vec<_>>());
        assert_eq!(tokens[0].text, "##");
        assert_eq!(
            tokens[24..30].iter().map(|t| t.text).collect::<Vec<_>>(),
            ["[", "]", "{", "}", "#", "<"]
        );
        for source in ["%", "%:", "%:%", ".", "..", "<", "<<", "<<="] {
            let arena = Bump::new();
            let tokens = scan(source, "tail.c", &arena).unwrap();
            assert_eq!(
                tokens
                    .iter()
                    .map(|t| t.raw_spelling.unwrap_or(&t.text))
                    .collect::<String>(),
                source
            );
        }
    }

    #[test]
    fn scanner_preserves_locations_inside_spliced_tokens_and_comments() {
        let arena = Bump::new();
        let tokens = scan(
            "ab\\\ncd /* comment\nnext */ ef\\\r\ngh\n__LINE__",
            "physical.c",
            &arena,
        )
        .unwrap();
        let actual: Vec<_> = tokens.iter().map(|t| (t.text, t.line, t.col)).collect();
        assert_eq!(
            actual,
            [
                ("abcd", 1, 1),
                ("\n", 2, 14),
                ("efgh", 3, 9),
                ("\n", 4, 3),
                ("__LINE__", 5, 1)
            ]
        );
    }
}
