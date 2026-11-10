//! Token-based C preprocessing using Keel's target environment and headers.

use crate::Diagnostic;
use crate::ast::{BinOp, UnOp};
use crate::int_const::{self, IntConst, IntTy, Policy, Ub};
use bumpalo::{Bump, collections::Vec as BumpVec};
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

// The two constants are also used for controlled profiling ablations.
const CACHE_HEADERS: bool = true;
/// Files are scanned and processed in chunks of at least this many tokens,
/// each ending with a newline, so a file's tokens are never all live.
const STREAM_CHUNK_TOKENS: usize = 4096;

#[derive(Clone, Debug, Default)]
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
    /// The x86 features every function has. Only CX16 matters: it defines
    /// `__GCC_HAVE_SYNC_COMPARE_AND_SWAP_16`.
    pub x86_baseline: Option<keel_asm_ir::x86::Features>,
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

/// Caller-owned immutable header and lexical cache for sequential raw-source TUs.
/// No macros, expanded text, search results, guards or once state survive a call.
/// Disk headers are read again on first inclusion in each TU and compared byte for
/// byte, so edits (even with unchanged timestamps/length) invalidate lexical data.
/// Paths are resolved afresh per TU. Drop or clear the cache to release storage.
#[derive(Debug, Default)]
pub struct PreprocessCache {
    headers: HashMap<(PathBuf, bool), CachedHeader>,
    lexical_hits: usize,
}

#[derive(Debug)]
struct CachedHeader {
    source: Arc<str>,
    spellings: String,
    tokens: Vec<LexicalToken>,
}

#[derive(Debug)]
struct LexicalToken {
    text: Range<usize>,
    raw_spelling: Option<Range<usize>>,
    space: bool,
    line: u32,
    col: u32,
}

impl PreprocessCache {
    /// Creates an empty cache. Reuse it across calls to `preprocess`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Releases all cached headers and resets the reuse counter.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Number of header scans avoided across translation units.
    pub fn lexical_hits(&self) -> usize {
        self.lexical_hits
    }

    /// Preprocesses a raw-source TU with fresh macro and include state.
    ///
    /// # Errors
    /// Returns the same input, include and macro diagnostics as [`preprocess`].
    /// Included files must remain unchanged during each call, as for `preprocess`.
    pub fn preprocess(
        &mut self,
        src: &str,
        path: &str,
        options: &PreprocessOptions,
    ) -> Result<String, Diagnostic> {
        preprocess_inner::<false>(src, path, options, Some(self), STREAM_CHUNK_TOKENS)
            .map(|(text, _)| text)
    }
}

impl CachedHeader {
    fn new(source: &str, tokens: &[Token<'_>]) -> Self {
        let mut spellings = String::new();
        let mut save = |text: &str| {
            let start = spellings.len();
            spellings.push_str(text);
            start..spellings.len()
        };
        let tokens = tokens
            .iter()
            .map(|t| LexicalToken {
                text: save(t.text),
                raw_spelling: t.raw_spelling.map(&mut save),
                space: t.space,
                line: t.line,
                col: t.col,
            })
            .collect();
        Self {
            source: Arc::from(source),
            spellings,
            tokens,
        }
    }

    fn restore<'a>(&self, arena: &'a Bump, path: &'a str) -> Box<[Token<'a>]> {
        let text = arena.alloc_str(&self.spellings);
        self.tokens
            .iter()
            .map(|t| Token {
                text: &text[t.text.clone()],
                raw_spelling: t.raw_spelling.as_ref().map(|r| &text[r.clone()]),
                space: t.space,
                file: path,
                line: t.line,
                col: t.col,
                hide: &[],
            })
            .collect()
    }
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
    preprocess_inner::<false>(src, path, options, None, STREAM_CHUNK_TOKENS).map(|(text, _)| text)
}

/// Counters are collected only by the profiling entry point; the ordinary
/// entry point instantiates TRACK=false so instrumentation compiles away.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreprocessStats {
    pub include_requests: usize,
    pub file_opens: usize,
    pub mapped_files: usize,
    pub source_cache_hits: usize,
    /// Inclusions that processed tokens kept from an earlier inclusion.
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
    preprocess_inner::<true>(src, path, options, None, STREAM_CHUNK_TOKENS)
}
fn preprocess_inner<const TRACK: bool>(
    src: &str,
    path: &str,
    options: &PreprocessOptions,
    batch_cache: Option<&mut PreprocessCache>,
    stream_chunk: usize,
) -> Result<(String, PreprocessStats), Diagnostic> {
    if options.target == keel_ir::Target::Wasm32UnknownUnknown && options.sysroot.is_some() {
        return Err(Diagnostic {
            file: path.into(),
            line: 1,
            col: 1,
            message:
                "unsupported target wasm32-unknown-unknown: freestanding C sysroot is unavailable"
                    .into(),
        });
    }
    let arena = Bump::new();
    let sources = typed_arena::Arena::new();
    let mut pp = Preprocessor::<TRACK> {
        arena: &arena,
        sources: &sources,
        stats: PreprocessStats::default(),
        batch_cache,
        options,
        stream_chunk,
        in_condition: false,
        macros: HashMap::default(),
        expansion_scratch: Vec::new(),
        expansion_stack: Vec::new(),
        undefined: HashSet::default(),
        search_dirs: Vec::new(),
        framework_dirs: Vec::new(),
        include_stack: Vec::new(),
        once: HashSet::default(),
        canonical_paths: HashMap::default(),
        canonical_dirs: HashMap::default(),
        header_cache: HashMap::default(),
        file_sources: HashMap::default(),
        included: HashMap::default(),
        chunks: Vec::new(),
        counter: 0,
        output: String::new(),
        output_next: None,
    };
    let predefined = arena.alloc_str(
        &crate::predefined_macros(
            options.target,
            options.darwin_deployment.as_deref(),
            options.x86_baseline,
        )
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
    batch_cache: Option<&'a mut PreprocessCache>,
    stats: PreprocessStats,
    arena: &'a Bump,
    sources: &'a typed_arena::Arena<FileSource>,
    options: &'a PreprocessOptions,
    /// [`STREAM_CHUNK_TOKENS`], or less in tests.
    stream_chunk: usize,
    in_condition: bool,
    macros: HashMap<&'a str, &'a Macro<'a>>,
    /// The output buffer of the next expansion.
    expansion_scratch: Vec<Token<'a>>,
    /// The replacement stack of the next expansion.
    expansion_stack: Vec<Token<'a>>,
    undefined: HashSet<&'a str>,
    search_dirs: Vec<PathBuf>,
    framework_dirs: Vec<PathBuf>,
    include_stack: Vec<(&'a str, Option<usize>)>,
    once: HashSet<PathBuf>,
    canonical_paths: HashMap<PathBuf, PathBuf>,
    canonical_dirs: HashMap<PathBuf, PathBuf>,
    header_cache: HashMap<HeaderContext<'a>, HashMap<String, Option<Include>>>,
    file_sources: HashMap<(PathBuf, bool), &'a str>,
    /// Each header processed to its end.
    included: HashMap<(PathBuf, bool), Included<'a>>,
    /// Token buffers for the chunks of files being processed.
    chunks: Vec<Vec<Token<'a>>>,
    counter: u64,
    output: String,
    /// The source file and line that the next `output` line continues, if
    /// known, so short forward gaps need newlines rather than a line marker.
    output_next: Option<(&'a str, u32)>,
}

enum FileSource {
    Read(String),
    Embedded(&'static str),
    Shared(Arc<str>),
    Mapped(memmap2::Mmap),
}
/// What later inclusions of a header reuse from its first inclusion.
#[derive(Clone)]
struct Included<'a> {
    /// The macro of the #ifndef / #define / #endif group that starts the
    /// header, if any. Its current state is checked on every inclusion: once
    /// defined, the group's lines are all inactive and need no processing.
    guard: Option<&'a str>,
    /// Whether the guard group is the whole header.
    whole: bool,
    /// The tokens after the guard group, or all of them without a guard;
    /// `None` for a whole guard group, or if they were not kept.
    rest: Option<Rc<Vec<Token<'a>>>>,
}

/// Recognizes the guard group that starts a header from the header's
/// non-empty physical lines. An outer #else/#elif rules it out.
struct GuardScan<'a> {
    state: GuardState<'a>,
    depth: usize,
    // Malformed nested conditionals rule out the guard, so skipping a group
    // never hides a diagnostic. Nesting deeper than 32 also rules it out.
    otherwise: [bool; 32],
}
#[derive(Clone, Copy)]
enum GuardState<'a> {
    /// Before the `#ifndef NAME` line.
    Start,
    /// After `#ifndef NAME`, before `#define NAME`.
    Ifndef(&'a str),
    /// Inside the guard group.
    Open(&'a str),
    /// After the group's `#endif`.
    Closed(&'a str),
    /// After a non-empty line following the group.
    Leading(&'a str),
    /// No guard group starts the header.
    Unguarded,
    /// The guard group's conditionals are malformed.
    Malformed,
}
impl<'a> GuardScan<'a> {
    fn new() -> Self {
        Self {
            state: GuardState::Start,
            depth: 0,
            otherwise: [false; 32],
        }
    }
    /// Takes the next non-empty physical line. Returns whether it closes the
    /// guard group.
    fn line(&mut self, line: &[Token<'a>]) -> bool {
        self.state = match self.state {
            GuardState::Start
                if line.len() == 3
                    && line[0].text == "#"
                    && line[1].text == "ifndef"
                    && ident(line[2].text) =>
            {
                GuardState::Ifndef(line[2].text)
            }
            GuardState::Ifndef(guard)
                if line.len() >= 3
                    && line[0].text == "#"
                    && line[1].text == "define"
                    && line[2].text == guard =>
            {
                self.depth = 1;
                GuardState::Open(guard)
            }
            GuardState::Start | GuardState::Ifndef(_) => GuardState::Unguarded,
            GuardState::Open(guard) => self.nested(line, guard),
            GuardState::Closed(guard) => GuardState::Leading(guard),
            state => state,
        };
        matches!(self.state, GuardState::Closed(_))
    }
    fn nested(&mut self, line: &[Token<'a>], guard: &'a str) -> GuardState<'a> {
        if line[0].text != "#" {
            return GuardState::Open(guard);
        }
        match line.get(1).map(|t| t.text) {
            Some("if" | "ifdef" | "ifndef") => {
                if self.depth == self.otherwise.len() {
                    return GuardState::Malformed;
                }
                self.otherwise[self.depth] = false;
                self.depth += 1;
            }
            Some(directive @ ("else" | "elif")) => {
                if self.depth == 1 || self.otherwise[self.depth - 1] {
                    return GuardState::Malformed;
                }
                self.otherwise[self.depth - 1] = directive == "else";
            }
            Some("endif") => {
                self.depth -= 1;
                if self.depth == 0 {
                    return GuardState::Closed(guard);
                }
            }
            _ => {}
        }
        GuardState::Open(guard)
    }
    /// Updates `kept`, the tokens a later inclusion processes, with the
    /// lines of `chunk`. The guard group closed at `closed_at` in `chunk`, if
    /// it did there, and the lines are `was` before it.
    fn keep(
        &self,
        was: GuardState<'a>,
        chunk: &[Token<'a>],
        closed_at: Option<usize>,
        kept: &mut Option<Vec<Token<'a>>>,
    ) {
        let Some(tokens) = kept else {
            return;
        };
        match self.state {
            GuardState::Start | GuardState::Ifndef(_) | GuardState::Unguarded => {
                tokens.extend_from_slice(chunk);
            }
            GuardState::Open(_) | GuardState::Closed(_) => tokens.clear(),
            GuardState::Leading(_) if matches!(was, GuardState::Leading(_)) => {
                tokens.extend_from_slice(chunk);
            }
            GuardState::Leading(_) => {
                tokens.clear();
                tokens.extend_from_slice(&chunk[closed_at.unwrap_or(0)..]);
            }
            GuardState::Malformed => *kept = None,
        }
    }
    /// What later inclusions reuse, given the tokens kept by [`Self::keep`].
    fn included(&self, kept: Option<Vec<Token<'a>>>) -> Included<'a> {
        let (guard, whole) = match self.state {
            GuardState::Closed(guard) => (Some(guard), true),
            GuardState::Leading(guard) => (Some(guard), false),
            _ => (None, false),
        };
        Included {
            guard,
            whole,
            rest: kept.filter(|_| !whole).map(Rc::new),
        }
    }
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
            Self::Shared(text) => Ok(text),
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
#[derive(Default)]
struct Locations<'a> {
    source: &'a [u8],
    removed: Vec<(usize, usize)>,
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
    // `memmem::find` builds a searcher on every call; build it once.
    static FINDER: std::sync::LazyLock<memchr::memmem::Finder<'static>> =
        std::sync::LazyLock::new(|| memchr::memmem::Finder::new(b"*/"));
    FINDER.find(bytes)
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
    let mut scanner = Scanner::new(src, file, arena);
    let mut out = Vec::new();
    scanner.fill(&mut out, usize::MAX, false)?;
    Ok(out)
}

/// Scans one file incrementally. Each `fill` resumes where the previous one
/// stopped, so a file's tokens can be processed in line-aligned chunks.
struct Scanner<'a> {
    text: &'a str,
    file: &'a str,
    locations: Locations<'a>,
    i: usize,
    space: bool,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str, file: &'a str, arena: &'a Bump) -> Self {
        let input = splice(src.as_bytes());
        // Phase 2 removes only complete ASCII sequences, preserving UTF-8.
        // Validate the spliced buffer once rather than validating every token.
        let text: &'a str = match &input.bytes {
            Cow::Borrowed(_) => src,
            Cow::Owned(bytes) => arena.alloc_str(std::str::from_utf8(bytes).unwrap()),
        };
        Self {
            text,
            file,
            locations: Locations {
                source: src.as_bytes(),
                removed: input.removed,
                next: 0,
                skipped: 0,
                next_newline: find_byte(b'\n', src.as_bytes()),
                line: 1,
                line_start: 0,
            },
            i: 0,
            space: false,
        }
    }

    /// Appends tokens to `out`, stopping after a physical newline once `out`
    /// holds at least `min` tokens, or with `split_conditionals`, after a
    /// line that starts with a directive that can begin an inactive group.
    /// Returns whether the input has ended.
    fn fill(
        &mut self,
        out: &mut Vec<Token<'a>>,
        min: usize,
        split_conditionals: bool,
    ) -> Result<bool, Diagnostic> {
        let (text, file) = (self.text, self.file);
        let b = text.as_bytes();
        // Locals keep the cursor in registers; an error ends the scan.
        let mut locations = std::mem::take(&mut self.locations);
        let (mut i, mut space) = (self.i, self.space);
        let mut line_start = out.len();
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
                if out.len() >= min || (split_conditionals && conditional(&out[line_start..])) {
                    self.locations = locations;
                    self.i = i;
                    self.space = space;
                    return Ok(false);
                }
                line_start = out.len();
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
            let prefix =
                if b.get(i..i + 2) == Some(b"u8") && matches!(b.get(i + 2), Some(b'"' | b'\'')) {
                    2
                } else if matches!(b[i], b'L' | b'u' | b'U')
                    && matches!(b.get(i + 1), Some(b'"' | b'\''))
                {
                    1
                } else {
                    0
                };
            if matches!(b.get(i + prefix), Some(b'"' | b'\'')) {
                i += prefix;
                let q = b[i];
                i += 1;
                loop {
                    i += find_literal_end(q, &b[i.min(b.len())..])
                        .unwrap_or(b.len() - i.min(b.len()));
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
        self.locations = locations;
        self.i = i;
        self.space = space;
        Ok(true)
    }

    /// Skips the lines of an inactive group up to the `#elif`, `#else` or
    /// `#endif` line that may end it, which it scans with `fill` and stops
    /// after. Processing ignores the skipped lines, but their literals and
    /// comments are scanned as `fill` scans them, and the conditional
    /// directive lines of nested groups are scanned and kept. A line that
    /// might hold a lexical error is scanned with `fill` instead, which
    /// reports it, and so is the line after a directive with a comment that
    /// spans lines.
    fn skip(&mut self, out: &mut Vec<Token<'a>>) -> Result<bool, Diagnostic> {
        let b = self.text.as_bytes();
        let mut i = self.i;
        let mut depth = 0usize;
        'lines: while i < b.len() {
            let line = i;
            // Find the line's first token. A comment that spans lines ends
            // this line, and its end starts the next one.
            loop {
                let directive = match b.get(i) {
                    None => break 'lines,
                    Some(b'\n') => {
                        i += 1;
                        continue 'lines;
                    }
                    Some(b'#') => b.get(i + 1) != Some(&b'#'),
                    Some(b'%') => b[i..].starts_with(b"%:") && !b[i..].starts_with(b"%:%:"),
                    _ => false,
                };
                if directive {
                    let mut j = i + if b[i] == b'#' { 1 } else { 2 };
                    while j < b.len() && b[j] != b'\n' && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let name = j;
                    while j < b.len() && BYTE_CLASS[b[j] as usize] & IDENT_CONTINUE != 0 {
                        j += 1;
                    }
                    // Other directives have no effect in an inactive group.
                    if name < j
                        && BYTE_CLASS[b[name] as usize] & IDENT_START != 0
                        && !matches!(
                            &b[name..j],
                            b"if" | b"ifdef" | b"ifndef" | b"elif" | b"else" | b"endif"
                        )
                    {
                        i = j;
                        break;
                    }
                    let start = out.len();
                    let done = self.fill_line(line, out)?;
                    let tokens = &out[start..];
                    let ends = tokens.last().is_some_and(|t| t.text == "\n");
                    let lines = tokens.iter().filter(|t| t.text == "\n").count();
                    if lines > usize::from(ends) {
                        return Ok(done);
                    }
                    match tokens.get(1).map(|t| t.text) {
                        Some("if" | "ifdef" | "ifndef") => depth += 1,
                        Some("elif" | "else" | "endif") if depth == 0 => return Ok(done),
                        Some("endif") => depth -= 1,
                        Some("elif" | "else") => {}
                        _ => out.truncate(start),
                    }
                    if done {
                        return Ok(true);
                    }
                    i = self.i;
                    continue 'lines;
                }
                match b.get(i) {
                    None => break 'lines,
                    Some(b'/') if b.get(i + 1) == Some(&b'/') => {
                        i += find_byte(b'\n', &b[i..]).unwrap_or(b.len() - i);
                    }
                    Some(b'/') if b.get(i + 1) == Some(&b'*') => {
                        let Some(end) = find_comment_end(&b[i + 2..]) else {
                            return self.fill_line(line, out);
                        };
                        let spans = find_byte(b'\n', &b[i + 2..i + 2 + end]).is_some();
                        i += end + 4;
                        if spans {
                            continue 'lines;
                        }
                    }
                    Some(c) if c.is_ascii_whitespace() => i += 1,
                    Some(_) => break,
                }
            }
            // Skip the rest of the line.
            loop {
                let rest = &b[i..];
                let stop = memchr::memchr3(b'\n', b'"', b'\'', rest).unwrap_or(rest.len());
                i += find_byte(b'/', &rest[..stop]).unwrap_or(stop);
                match b.get(i) {
                    None => break 'lines,
                    Some(b'\n') => {
                        i += 1;
                        continue 'lines;
                    }
                    Some(b'/') if b.get(i + 1) == Some(&b'/') => {
                        i += find_byte(b'\n', &b[i..]).unwrap_or(b.len() - i);
                    }
                    Some(b'/') if b.get(i + 1) == Some(&b'*') => {
                        let Some(end) = find_comment_end(&b[i + 2..]) else {
                            return self.fill_line(line, out);
                        };
                        let spans = find_byte(b'\n', &b[i + 2..i + 2 + end]).is_some();
                        i += end + 4;
                        if spans {
                            continue 'lines;
                        }
                    }
                    Some(b'/') => i += 1,
                    Some(&q) => {
                        // The literal loop of `fill`.
                        i += 1;
                        loop {
                            i += find_literal_end(q, &b[i.min(b.len())..])
                                .unwrap_or(b.len() - i.min(b.len()));
                            match b.get(i) {
                                Some(c) if *c == q => break,
                                Some(b'\\') => i += 2,
                                _ => return self.fill_line(line, out),
                            }
                            if i > b.len() {
                                return self.fill_line(line, out);
                            }
                        }
                        i += 1;
                    }
                }
            }
        }
        self.i = b.len();
        self.space = true;
        Ok(true)
    }

    /// Scans from `line`, the start of a line, with `fill` until a newline.
    fn fill_line(&mut self, line: usize, out: &mut Vec<Token<'a>>) -> Result<bool, Diagnostic> {
        self.i = line;
        self.space = true;
        self.fill(out, out.len() + 1, false)
    }
}

/// Whether `line` starts with a directive that can begin an inactive group.
fn conditional(line: &[Token<'_>]) -> bool {
    line.first().is_some_and(|t| t.text == "#")
        && line
            .get(1)
            .is_some_and(|t| matches!(t.text, "if" | "ifdef" | "ifndef" | "elif" | "else"))
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
            out.push_str(&spelling(m.body));
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
        if self.options.target == keel_ir::Target::Wasm32UnknownUnknown {
            return;
        }
        // With an explicit root, never consult host system include directories.
        let root = self.options.sysroot.clone().or_else(|| {
            (self.options.target != keel_ir::Target::Aarch64AppleDarwin
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
        if self.options.target == keel_ir::Target::Aarch64LinuxGnu {
            self.search_dirs
                .push(root.join("usr/include/aarch64-linux-gnu"));
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
        if !ident(name.text) {
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
                    names.push("__VA_ARGS__");
                    i += 1;
                } else if ident(p.text) && !names.contains(&p.text) {
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
        let mut prev: Option<(&'a str, u32)> = None;
        for t in ts {
            if t.text.is_empty() {
                continue;
            }
            if prev != Some((t.file, t.line)) {
                if let Some((file, line)) = prev {
                    self.output.push('\n');
                    self.output_next = Some((file, line + 1));
                }
                // Like Clang, write up to 8 newlines rather than a marker.
                match self.output_next {
                    Some((file, next)) if file == t.file && (next..=next + 8).contains(&t.line) => {
                        for _ in next..t.line {
                            self.output.push('\n');
                        }
                    }
                    _ => {
                        write!(self.output, "# {} ", t.line).unwrap();
                        quoted_to(&mut self.output, t.file);
                        self.output.push('\n');
                    }
                }
                prev = Some((t.file, t.line));
            }
            self.output.push_str(t.text);
            self.output.push(' ');
            if t.text.contains('\n') {
                prev = None;
                self.output_next = None;
            }
        }
        if let Some((file, line)) = prev {
            self.output.push('\n');
            self.output_next = Some((file, line + 1));
        }
    }
    fn flush(&mut self, pending: &mut Vec<Token<'a>>) -> Result<(), Diagnostic> {
        if pending.is_empty() {
            return Ok(());
        }
        let mut expanded = self.expand(pending, 0)?;
        self.emit(&expanded);
        pending.clear();
        // The next expansion reuses a small output buffer.
        if expanded.capacity() <= 4096 {
            expanded.clear();
            self.expansion_scratch = expanded;
        }
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
        // Only disk/embedded includes participate in the header caches.
        let key = (depth > 0 && CACHE_HEADERS).then(|| (canonical.clone(), embedded));
        let known = key.as_ref().and_then(|key| self.included.get(key).cloned());
        let mut tokens = None;
        if let Some(known) = &known {
            match known.guard {
                Some(guard) if !self.macros.contains_key(guard) => {}
                Some(_) if known.whole => {
                    if TRACK {
                        self.stats.guard_skips += 1;
                    }
                    return Ok(());
                }
                _ => {
                    tokens = known.rest.clone();
                    if TRACK && tokens.is_some() {
                        self.stats.token_cache_hits += 1;
                    }
                }
            }
        }
        self.include_stack.push((path, origin));
        let mut state = FileState::new(path, canonical, depth);
        if key.is_some() && known.is_none() {
            state.guard = Some(GuardScan::new());
        }
        if tokens.is_none()
            && let Some(key) = &key
        {
            tokens = self.restore(key, src, path).map(|t| Rc::new(t.into_vec()));
        }
        let mut kept = None;
        if let Some(tokens) = tokens {
            if TRACK {
                self.stats.token_bytes += std::mem::size_of_val(tokens.as_slice());
            }
            self.lines(&tokens, &mut state)?;
        } else {
            if TRACK {
                self.stats.scanned_files += 1;
            }
            let mut scanner = Scanner::new(src, path, self.arena);
            let mut chunk = self.chunks.pop().unwrap_or_default();
            // The batch cache keeps every token of a header; otherwise only
            // those that its later inclusions process.
            let batch = key.is_some() && self.batch_cache.is_some();
            kept = state.guard.is_some().then(Vec::new);
            self.stream(&mut scanner, &mut chunk, &mut state, &mut kept, batch)?;
            if TRACK {
                self.stats.token_bytes += chunk.capacity() * std::mem::size_of::<Token>();
            }
            chunk.clear();
            self.chunks.push(chunk);
            if batch
                && let (Some(tokens), Some(key), Some(cache)) =
                    (kept.take(), &key, &mut self.batch_cache)
            {
                cache
                    .headers
                    .insert(key.clone(), CachedHeader::new(src, &tokens));
            }
        }
        let included = state.guard.as_ref().map(|scan| scan.included(kept));
        self.finish_file(state)?;
        if let (Some(key), Some(included)) = (key, included) {
            self.included.insert(key, included);
        }
        Ok(())
    }

    /// A header's tokens from the batch cache, if it holds this source.
    fn restore(
        &mut self,
        key: &(PathBuf, bool),
        src: &str,
        path: &'a str,
    ) -> Option<Box<[Token<'a>]>> {
        let cache = self.batch_cache.as_mut()?;
        let header = cache.headers.get(key)?;
        if header.source.as_ref() != src {
            return None;
        }
        cache.lexical_hits += 1;
        Some(header.restore(self.arena, path))
    }

    /// Processes `scanner`'s tokens in line-aligned chunks of `chunk`. While
    /// a header's guard is scanned, `kept` collects the tokens later
    /// inclusions process, or all of them if `keep_all`.
    fn stream(
        &mut self,
        scanner: &mut Scanner<'a>,
        chunk: &mut Vec<Token<'a>>,
        state: &mut FileState<'a>,
        kept: &mut Option<Vec<Token<'a>>>,
        keep_all: bool,
    ) -> Result<(), Diagnostic> {
        loop {
            chunk.clear();
            // Inactive lines can be skipped unless their tokens are kept.
            // Before a guard group opens, scan line by line so that skipping
            // can start inside it.
            let (skippable, min) = match state.guard.as_ref().map(|scan| scan.state) {
                _ if keep_all => (false, self.stream_chunk),
                None | Some(GuardState::Open(_) | GuardState::Malformed) => {
                    (true, self.stream_chunk)
                }
                Some(GuardState::Start | GuardState::Ifndef(_)) => (false, 1),
                Some(_) => (false, self.stream_chunk),
            };
            let done = if skippable && state.stack.last().is_some_and(|c| !c.active) {
                scanner.skip(chunk)?
            } else {
                scanner.fill(chunk, min, skippable)?
            };
            let was = state.guard.as_ref().map(|scan| scan.state);
            if let Err(error) = self.lines(chunk, state) {
                // Report a lexical error anywhere in the file before any
                // directive error, as when the whole file was scanned first.
                loop {
                    chunk.clear();
                    if scanner.fill(chunk, self.stream_chunk, false)? {
                        return Err(error);
                    }
                }
            }
            if keep_all {
                if let Some(tokens) = kept {
                    tokens.extend_from_slice(chunk);
                }
            } else if let (Some(scan), Some(was)) = (&state.guard, was) {
                scan.keep(was, chunk, state.guard_closed_at, kept);
            }
            state.guard_closed_at = None;
            if done {
                return Ok(());
            }
        }
    }

    /// Processes the physical lines of `tokens`, each ending with a newline
    /// token except perhaps the last.
    fn lines(&mut self, tokens: &[Token<'a>], state: &mut FileState<'a>) -> Result<(), Diagnostic> {
        let mut end = 0;
        for line in tokens.split_inclusive(|t| t.text == "\n") {
            end += line.len();
            let line = match line.split_last() {
                Some((last, line)) if last.text == "\n" => line,
                _ => line,
            };
            if let Some(scan) = &mut state.guard
                && !line.is_empty()
                && scan.line(line)
            {
                state.guard_closed_at = Some(end);
            }
            self.physical_line(line, state)?;
        }
        Ok(())
    }

    fn finish_file(&mut self, mut state: FileState<'a>) -> Result<(), Diagnostic> {
        if let Some(c) = state.stack.last() {
            return Err(c.start.error("unterminated conditional directive"));
        }
        self.flush(&mut state.pending)?;
        self.include_stack.pop();
        Ok(())
    }

    /// Processes one physical line: text joins the pending run, and a
    /// directive first expands the pending run.
    fn physical_line(
        &mut self,
        physical: &[Token<'a>],
        state: &mut FileState<'a>,
    ) -> Result<(), Diagnostic> {
        let active = state.stack.last().is_none_or(|c| c.active);
        if !physical.first().is_some_and(|t| t.text == "#") {
            if active {
                let start = state.pending.len();
                state.pending.extend_from_slice(physical);
                let added = &mut state.pending[start..];
                if state.line_delta != 0
                    || added.first().is_some_and(|t| t.file != state.logical_file)
                {
                    for t in added.iter_mut() {
                        t.file = state.logical_file;
                        t.line = (t.line as i64 + state.line_delta) as u32;
                    }
                }
                if let Some(first) = added.first_mut() {
                    first.space = true;
                }
            }
            return Ok(());
        }
        self.flush(&mut state.pending)?;
        // Borrow directive lines unless #line or a cached path alias needs
        // new locations. Inactive ordinary lines require no token copies.
        let physical_line = physical.get(1).map_or(0, |t| t.line);
        let mut remapped;
        let line = if state.line_delta != 0
            || physical
                .first()
                .is_some_and(|t| t.file != state.logical_file)
        {
            remapped = physical.to_vec();
            for t in &mut remapped {
                t.file = state.logical_file;
                t.line = (t.line as i64 + state.line_delta) as u32;
            }
            remapped.as_slice()
        } else {
            physical
        };
        let Some(directive) = line.get(1) else {
            return Ok(());
        };
        let args = &line[2..];
        match directive.text {
            "if" | "ifdef" | "ifndef" => {
                let yes = if !active {
                    false
                } else if directive.text == "if" {
                    self.condition(args)?
                } else {
                    if args.len() != 1 || !ident(args[0].text) {
                        return Err(directive.error("expected one macro name"));
                    }
                    self.is_defined(args[0].text) != (directive.text == "ifndef")
                };
                state.stack.push(Conditional {
                    parent: active,
                    active: active && yes,
                    taken: yes,
                    otherwise: false,
                    start: *directive,
                });
            }
            "elif" | "else" => {
                let Some(c) = state.stack.last_mut() else {
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
                if state.stack.pop().is_none() {
                    return Err(directive.error("#endif without #if"));
                }
            }
            _ if !active => {}
            "define" => self.define(args)?,
            "undef" => {
                if args.len() != 1 || !ident(args[0].text) {
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
                    self.expand(args, 0)?
                };
                let (name, quoted) =
                    header(&expanded).ok_or_else(|| directive.error("expected header name"))?;
                let include = self
                    .find_header(&name, quoted, directive.text == "include_next")
                    .ok_or_else(|| directive.error(format!("header not found: {name}")))?;
                self.include_file(include, state.depth + 1, directive)?;
            }
            "pragma" => {
                if args.first().is_some_and(|t| t.text == "once") {
                    self.once.insert(state.canonical.clone());
                } else {
                    self.output.push_str("#pragma ");
                    self.output.push_str(&spelling(args));
                    self.output.push('\n');
                    self.output_next = None;
                }
            }
            "line" => {
                let expanded = self.expand(args, 0)?;
                let n = expanded
                    .first()
                    .and_then(|t| t.text.parse::<u32>().ok())
                    .filter(|&n| n > 0)
                    .ok_or_else(|| directive.error("invalid #line number"))?;
                // The directive's physical line, before any previous #line adjustment.
                state.line_delta = n as i64 - physical_line as i64 - 1;
                if let Some(t) = expanded.get(1) {
                    state.logical_file = self.arena.alloc_str(
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
        Ok(())
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
            let loaded = if let Some(header) = self
                .batch_cache
                .as_ref()
                .and_then(|cache| cache.headers.get(&key))
                && header.source.as_ref()
                    == loaded
                        .text()
                        .map_err(|e| at.error(format!("{}: {e}", include.path.display())))?
            {
                FileSource::Shared(Arc::clone(&header.source))
            } else {
                loaded
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
        if self.options.target == keel_ir::Target::Wasm32UnknownUnknown
            && Path::new(name).is_absolute()
        {
            return None;
        }
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
                crate::headers::get_for_target(Path::new(name), self.options.target)
            } else {
                None
            };
            let path = dir.join(name);
            if contents.is_some()
                || ((i != builtin_index
                    || self.options.target != keel_ir::Target::Wasm32UnknownUnknown)
                    && path.is_file())
            {
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
/// The state of one file being processed.
struct FileState<'a> {
    pending: Vec<Token<'a>>,
    stack: Vec<Conditional<'a>>,
    logical_file: &'a str,
    line_delta: i64,
    canonical: PathBuf,
    depth: usize,
    /// Present while a header's include guard is unknown.
    guard: Option<GuardScan<'a>>,
    /// The end of the guard group's `#endif` line in the current chunk.
    guard_closed_at: Option<usize>,
}
impl<'a> FileState<'a> {
    fn new(path: &'a str, canonical: PathBuf, depth: usize) -> Self {
        Self {
            pending: Vec::new(),
            stack: Vec::new(),
            logical_file: path,
            line_delta: 0,
            canonical,
            depth,
            guard: None,
            guard_closed_at: None,
        }
    }
}
struct Conditional<'a> {
    parent: bool,
    active: bool,
    taken: bool,
    otherwise: bool,
    start: Token<'a>,
}
fn spelling<'t, 'a: 't>(ts: impl IntoIterator<Item = &'t Token<'a>>) -> String {
    let mut s = String::new();
    for t in ts {
        if t.space && !s.is_empty() {
            s.push(' ');
        }
        s.push_str(t.raw_spelling.unwrap_or(t.text));
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
        return unquote(ts[0].text).map(|s| (s, true));
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

/// The tokens an expansion has yet to consume: replacement tokens awaiting
/// rescanning, stored last first so that replacing a prefix pops the
/// invocation and pushes its replacement, then the unread original tokens.
struct ExpansionInput<'t, 'a> {
    stack: Vec<Token<'a>>,
    rest: &'t [Token<'a>],
}
impl<'a> ExpansionInput<'_, 'a> {
    fn len(&self) -> usize {
        self.stack.len() + self.rest.len()
    }
    fn is_empty(&self) -> bool {
        self.stack.is_empty() && self.rest.is_empty()
    }
    fn get(&self, index: usize) -> Option<&Token<'a>> {
        match self.stack.len().checked_sub(index + 1) {
            Some(i) => Some(&self.stack[i]),
            None => self.rest.get(index - self.stack.len()),
        }
    }
    fn iter(&self) -> impl Iterator<Item = &Token<'a>> {
        self.stack.iter().rev().chain(self.rest)
    }
    /// The tokens at `range`, in order.
    fn range(&self, range: Range<usize>) -> impl Iterator<Item = &Token<'a>> {
        self.iter().skip(range.start).take(range.len())
    }
    /// Appends the tokens at `range` to `out`, in order.
    fn copy_range(&self, range: Range<usize>, out: &mut Vec<Token<'a>>) {
        let n = self.stack.len();
        if range.start < n {
            out.extend(
                self.stack[n - range.end.min(n)..n - range.start]
                    .iter()
                    .rev(),
            );
        }
        if range.end > n {
            out.extend_from_slice(&self.rest[range.start.max(n) - n..range.end - n]);
        }
    }
    fn collect_range(&self, range: Range<usize>) -> Vec<Token<'a>> {
        let mut out = Vec::with_capacity(range.len());
        self.copy_range(range, &mut out);
        out
    }
    fn pop(&mut self) -> Option<Token<'a>> {
        self.stack.pop().or_else(|| {
            let (first, rest) = self.rest.split_first()?;
            self.rest = rest;
            Some(*first)
        })
    }
    fn replace<R>(&mut self, end: usize, replacement: R)
    where
        R: IntoIterator<Item = Token<'a>>,
        R::IntoIter: DoubleEndedIterator,
    {
        let popped = end.min(self.stack.len());
        self.stack.truncate(self.stack.len() - popped);
        self.rest = &self.rest[end - popped..];
        self.stack.extend(replacement.into_iter().rev());
    }
}
impl<'a> std::ops::Index<usize> for ExpansionInput<'_, 'a> {
    type Output = Token<'a>;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).unwrap()
    }
}

/// The index ranges of an invocation's arguments, and the index after its
/// `)`. Single comma tokens separate consecutive ranges.
type Arguments = (Vec<Range<usize>>, usize);

/// The arguments of the invocation whose `(` is at `open`.
fn arguments(ts: &ExpansionInput<'_, '_>, open: usize) -> Result<Arguments, Diagnostic> {
    let mut args = Vec::new();
    let mut start = open + 1;
    let mut depth = 0;
    let n = ts.stack.len();
    let stacked = ts.stack.iter().rev().enumerate();
    let original = ts.rest.iter().enumerate().map(|(i, t)| (i + n, t));
    let end = stacked
        .chain(original)
        .skip(open + 1)
        .try_for_each(|(i, t)| {
            match t.text {
                "(" => depth += 1,
                ")" if depth == 0 => {
                    args.push(start..i);
                    return Err(i + 1);
                }
                ")" => depth -= 1,
                "," if depth == 0 => {
                    args.push(start..i);
                    start = i + 1;
                }
                _ => {}
            }
            Ok(())
        });
    match end {
        Err(end) => Ok((args, end)),
        Ok(()) => Err(ts[open].error("unterminated macro invocation")),
    }
}

impl<'a, const TRACK: bool> Preprocessor<'a, TRACK> {
    /// `None` if `t` expands to itself, otherwise its macro definition, if
    /// any; a builtin has none.
    #[inline(always)]
    fn candidate(&self, t: &Token<'a>) -> Option<Option<&'a Macro<'a>>> {
        let name = t.text;
        let first = name.as_bytes().first().copied().unwrap_or(0);
        if !(first.is_ascii_alphabetic() || first == b'_' || first >= 128)
            || (t.hide.contains(&name) && !(self.in_condition && name == "defined"))
        {
            return None;
        }
        let macro_definition = self.macros.get(name).copied();
        // Every builtin name except `defined` starts with '_'.
        let special = || {
            (self.in_condition && name == "defined")
                || (first == b'_'
                    && (matches!(
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
                    ) || query_builtin(name)))
        };
        (macro_definition.is_some() || special()).then_some(macro_definition)
    }
    fn expand(&mut self, ts: &[Token<'a>], depth: usize) -> Result<Vec<Token<'a>>, Diagnostic> {
        if depth > 200 {
            return Err(self.error("macro argument nesting exceeds 200"));
        }
        let mut input = ExpansionInput {
            stack: std::mem::take(&mut self.expansion_stack),
            rest: ts,
        };
        let mut out = std::mem::take(&mut self.expansion_scratch);
        out.reserve(input.len());
        // Bound macro invocations and the tokens they add, not the input:
        // a large array initializer has many tokens and no macros.
        let mut steps = 0;
        let limit = input.len().saturating_add(1_000_000);
        while !input.is_empty() {
            if steps > 1_000_000 || input.len().saturating_add(out.len()) > limit {
                return Err(input[0].error("macro expansion limit exceeded"));
            }
            let macro_definition = if input.stack.is_empty() {
                // Copy original tokens up to the next candidate in one run.
                let mut candidate = None;
                let run = (input.rest.iter())
                    .position(|t| {
                        candidate = self.candidate(t);
                        candidate.is_some()
                    })
                    .unwrap_or(input.rest.len());
                out.extend_from_slice(&input.rest[..run]);
                input.rest = &input.rest[run..];
                match candidate {
                    Some(m) => m,
                    None => continue,
                }
            } else {
                match self.candidate(input.stack.last().unwrap()) {
                    Some(m) => m,
                    None => {
                        out.push(input.stack.pop().unwrap());
                        continue;
                    }
                }
            };
            steps += 1;
            let t = input[0];
            let name = t.text;
            if name == "defined" && self.in_condition {
                let mut end = 1;
                let paren = input.get(end).is_some_and(|t| t.text == "(");
                if paren {
                    end += 1;
                }
                let operand = input
                    .get(end)
                    .filter(|t| ident(t.text))
                    .ok_or_else(|| t.error("expected identifier after defined"))?;
                let value = self.is_defined(operand.text);
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
                    "__FILE__" => Some(quote(t.file)),
                    "__BASE_FILE__" => Some(quote(self.include_stack[0].0)),
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
                let mut t = input.pop().unwrap();
                t.text = self.arena.alloc_str(&value);
                out.push(t);
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
                let arg = input.collect_range(args[0].clone());
                let value = if matches!(name, "__has_include" | "__has_include_next") {
                    let expanded = self.expand(&arg, depth + 1)?;
                    let (h, q) = header(&expanded)
                        .ok_or_else(|| t.error("expected header name in __has_include"))?;
                    (self
                        .find_header(&h, q, name == "__has_include_next")
                        .is_some() as u8)
                        .to_string()
                } else {
                    if arg.len() != 1 || !ident(arg[0].text) {
                        return Err(t.error("expected identifier in feature query"));
                    }
                    crate::features::query(name, arg[0].text, self.options.target).to_string()
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
                let arg = input.collect_range(args[0].clone());
                let expanded = self.expand(&arg, depth + 1)?;
                let value = if expanded.len() == 1 {
                    unquote(expanded[0].text)
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
            let (end, replacement, hide) = if let Some(params) = &m.params {
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
                    // Keep the invocation's commas and their spacing.
                    let rest = match args.get(fixed..) {
                        Some([first, .., last]) => first.start..last.end,
                        Some([only]) => only.clone(),
                        _ => end - 1..end - 1,
                    };
                    args.truncate(fixed);
                    args.push(rest);
                }
                // Each parameter's expansion, made on its first unpasted use.
                let mut expanded = Vec::new();
                let mut body = Vec::new();
                let mut operators = Vec::new();
                let mut j = 0;
                while j < m.body.len() {
                    let b = &m.body[j];
                    if b.text == "#" {
                        let p = params
                            .iter()
                            .position(|p| p == &m.body[j + 1].text)
                            .unwrap();
                        let arg = input.range(args[p].clone());
                        body.push(b.with_text(self.arena, quote(&spelling(arg))));
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
                            body.push(*b);
                            let arg = input.collect_range(args.last().unwrap().clone());
                            body.extend(self.expand(&arg, depth + 1)?);
                        }
                        j += 3;
                        continue;
                    }
                    if let Some(p) = params.iter().position(|p| *p == b.text) {
                        let paste = (j > 0 && m.body[j - 1].text == "##")
                            || m.body.get(j + 1).is_some_and(|t| t.text == "##");
                        // Argument tokens spelling ## are ordinary tokens,
                        // never operators in this replacement list.
                        let start = body.len();
                        if paste {
                            input.copy_range(args[p].clone(), &mut body);
                            if body.len() == start {
                                body.push(b.with_text(self.arena, ""));
                            }
                        } else {
                            if expanded.is_empty() {
                                expanded.resize_with(params.len(), || None);
                            }
                            if expanded[p].is_none() {
                                let arg = input.collect_range(args[p].clone());
                                expanded[p] = Some(self.expand(&arg, depth + 1)?);
                            }
                            body.extend_from_slice(expanded[p].as_deref().unwrap_or_default());
                        }
                        if let Some(first) = body.get_mut(start) {
                            first.space = b.space;
                        }
                    } else {
                        if b.text == "##" {
                            operators.push(body.len());
                        }
                        body.push(*b);
                    }
                    j += 1;
                }
                let hide = hide_intersection(self.arena, t.hide, input[end - 1].hide);
                (end, Some(self.paste(body, &operators)?), hide)
            } else if m.has_paste {
                let operators: Vec<usize> = (m.body.iter().enumerate())
                    .filter_map(|(i, t)| (t.text == "##").then_some(i))
                    .collect();
                (1, Some(self.paste(m.body.to_vec(), &operators)?), t.hide)
            } else {
                (1, None, t.hide)
            };
            let hide = hide_insert(self.arena, hide, name);
            let arena = self.arena;
            let relocate = |r: &Token<'a>| Token {
                hide: if r.hide.is_empty() {
                    hide
                } else {
                    hide_union(arena, r.hide, hide)
                },
                file: t.file,
                line: t.line,
                col: t.col,
                ..*r
            };
            let replacement = replacement.as_deref().unwrap_or(m.body);
            input.replace(end, replacement.iter().map(relocate));
            if !replacement.is_empty() {
                input.stack.last_mut().unwrap().space = t.space;
            }
        }
        // Recycle a small drained stack across conditions/arguments. Retaining
        // a large buffer here would raise per-TU peak memory.
        let stack = input.stack;
        if stack.capacity() <= 4096 && stack.capacity() >= self.expansion_stack.capacity() {
            self.expansion_stack = stack;
        }
        Ok(out)
    }
    /// Applies the `##` operators at the ascending `operators` indices of
    /// `body`. Only pasting creates empty placemarker tokens.
    fn paste(
        &self,
        body: Vec<Token<'a>>,
        operators: &[usize],
    ) -> Result<Vec<Token<'a>>, Diagnostic> {
        if operators.is_empty() {
            debug_assert!(body.iter().all(|t| !t.text.is_empty()));
            return Ok(body);
        }
        let mut out: Vec<Token<'a>> = Vec::with_capacity(body.len());
        let mut operators = operators.iter().copied().peekable();
        let mut body = body.into_iter().enumerate();
        while let Some((i, t)) = body.next() {
            // An operator consumed as a right operand is an ordinary token.
            while operators.next_if(|&k| k < i).is_some() {}
            if operators.next_if_eq(&i).is_some() {
                let left = out
                    .pop()
                    .ok_or_else(|| t.error("missing left operand to ##"))?;
                let (_, right) = body
                    .next()
                    .ok_or_else(|| t.error("missing right operand to ##"))?;
                let text = format!(
                    "{}{}",
                    left.raw_spelling.unwrap_or(left.text),
                    right.raw_spelling.unwrap_or(right.text)
                );
                let mut result = left.with_text(self.arena, &text);
                if !text.is_empty() {
                    let check = scan(self.arena.alloc_str(&text), t.file, self.arena)?;
                    if check.len() != 1 || check[0].raw_spelling.unwrap_or(check[0].text) != text {
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
        let expanded = self.expand(ts, 0);
        self.in_condition = false;
        let mut tokens = expanded?;
        for t in &mut tokens {
            if ident(t.text) {
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
        Ok(!value.is_zero())
    }
}

/// An `#if` operand: a value of `intmax_t` or `uintmax_t`.
type Integer = IntConst;

fn boolean(v: bool) -> Integer {
    IntConst::wrap(IntTy::INTMAX, u128::from(v))
}

/// `intmax_t`, or `uintmax_t` if either operand is unsigned.
fn common(a: Integer, b: Integer) -> IntTy {
    if a.ty().signed() && b.ty().signed() {
        IntTy::INTMAX
    } else {
        IntTy::UINTMAX
    }
}

/// `-v` or `~v`; both wrap.
fn unary(op: UnOp, v: Integer) -> Integer {
    int_const::unary(op, v, Policy::CONSTANT).expect("#if negation wraps")
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
                    "-" => unary(UnOp::Neg, v),
                    "~" => unary(UnOp::BitNot, v),
                    _ => boolean(v.is_zero()),
                }
            }
            _ => integer(t)?,
        };
        while let Some(op) = self.ts.get(self.pos) {
            if op.text == "?" && min <= 1 {
                self.pos += 1;
                let yes = self.eval(0, live && !lhs.is_zero())?;
                self.expect(":", op)?;
                let no = self.eval(1, live && lhs.is_zero())?;
                let ty = common(yes, no);
                lhs = if lhs.is_zero() { no } else { yes }.convert(ty);
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
                    "&&" => !lhs.is_zero(),
                    "||" => lhs.is_zero(),
                    _ => true,
                },
            )?;
            let ty = common(lhs, rhs);
            let (a, b) = (lhs.convert(ty), rhs.convert(ty));
            let value = match op.text {
                "||" => boolean(!a.is_zero() || !b.is_zero()),
                "&&" => boolean(!a.is_zero() && !b.is_zero()),
                _ => {
                    let op_kind = match op.text {
                        "|" => BinOp::Or,
                        "^" => BinOp::Xor,
                        "&" => BinOp::And,
                        "==" => BinOp::Eq,
                        "!=" => BinOp::Ne,
                        "<" => BinOp::Lt,
                        "<=" => BinOp::Le,
                        ">" => BinOp::Gt,
                        ">=" => BinOp::Ge,
                        "<<" => BinOp::Shl,
                        ">>" => BinOp::Shr,
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        "/" => BinOp::Div,
                        _ => BinOp::Rem,
                    };
                    // A shift has the left operand's type.
                    let (a, b) = if matches!(op_kind, BinOp::Shl | BinOp::Shr) {
                        (lhs, rhs.convert(lhs.ty()))
                    } else {
                        (a, b)
                    };
                    match int_const::binary(op_kind, a, b, Policy::CONSTANT) {
                        Ok(v) if op_kind.is_comparison() => boolean(!v.is_zero()),
                        Ok(v) => v,
                        // A skipped operand's value is never used.
                        Err(_) if !live => IntConst::wrap(a.ty(), 0),
                        Err(Ub::ShiftCount) => {
                            return Err(op.error("invalid shift count in #if"));
                        }
                        Err(Ub::DivideByZero) => {
                            return Err(op.error("division by zero in #if"));
                        }
                        Err(ub) => unreachable!("{ub:?} wraps in #if"),
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
        let arena = Bump::new();
        let mut strings = crate::strings::Strings::new(&arena);
        let mut files = Vec::new();
        let tokens = crate::lex::tokenize(t.text, t.file, &mut files, &mut strings)
            .map_err(|e| t.error(e.message))?;
        if let crate::lex::Tok::Char(value) = tokens[0].tok {
            return Ok(IntConst::from_literal(IntTy::INTMAX, value));
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
    let unsigned = suffix.contains('u') || bits > i64::MAX as u64;
    let ty = if unsigned {
        IntTy::UINTMAX
    } else {
        IntTy::INTMAX
    };
    Ok(IntConst::wrap(ty, u128::from(bits)))
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
                removed: input.removed,
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
            .map(|t| t.raw_spelling.unwrap_or(t.text))
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
                    .map(|t| t.raw_spelling.unwrap_or(t.text))
                    .collect::<String>(),
                source
            );
        }
    }

    fn chunked(src: &str, chunk: usize) -> String {
        match preprocess_inner::<false>(src, "chunk.c", &PreprocessOptions::default(), None, chunk)
        {
            Ok((text, _)) => text,
            Err(e) => format!("error {}:{}:{}: {}", e.file, e.line, e.col, e.message),
        }
    }

    /// Small chunks put a boundary after every line; one unbounded chunk
    /// scans the whole file first. Output and diagnostics must agree.
    #[test]
    fn streamed_main_file_matches_whole_file_scan() {
        for src in [
            "",
            "\n",
            "a",
            "a\r\nb\r\n",
            // Splices inside tokens, directives and conditions.
            "#define A 1 \\\n + 2\nA __LINE__\nin\\\nt x = A;\n#if A == \\\n3\nyes __LINE__\n#endif\n",
            "x\\\n",
            // Comments that span chunk boundaries.
            "int a; /* one\ntwo\nthree */ int b; __LINE__\n/*\n*/#define X 4\nX __LINE__\n// c \\\nstill comment\nX\n",
            // Conditionals whose branches span chunks.
            "#if 0\nno\n#if 1\nnested\n#endif\n#elif 1\nyes __LINE__\n#else\nno\n#endif\n#ifdef X\nno\n#endif\n",
            // Directives on a last line without a newline.
            "#define X 1\nX\n#undef X\nX\n#define Y 2",
            "#if 1\nint a;\n#endif",
            "int x;\n#pragma pack(1)",
            // Macro invocations and `#line` state across chunks.
            "#define F(a, b) a + b\nF(1,\n2) F\n(3,\n\n4)\n__LINE__\n",
            "#line 100 \"v.c\"\n__LINE__ __FILE__\n#line 7\n__LINE__\nlast\n",
            "int a;\n\n\n\n\n\n\n\n\n\n\nint b;\n",
            // Errors keep their locations and lexical errors stay first.
            "#if 1\nint a;\n",
            "#else\n",
            "#error stop\n/* unterminated",
            "#if 1\n#endif\n\"unterminated",
            "int a;\n#include \"missing-header.h\"\n'x",
            "#include \"missing-header.h\"\n",
        ] {
            let whole = chunked(src, usize::MAX);
            for chunk in [1, 2, 3, 7] {
                assert_eq!(chunked(src, chunk), whole, "{src:?} in chunks of {chunk}");
            }
        }
        assert_eq!(
            chunked("#error stop\nok\n/* unterminated", 1),
            "error chunk.c:3:1: unterminated comment"
        );
    }

    /// Later inclusions skip a defined guard group and process the kept
    /// tokens after it, wherever chunk boundaries fall.
    #[test]
    fn reinclusions_process_the_tokens_after_a_defined_guard_group() {
        let dir = std::env::temp_dir().join(format!("keel-pp-reinclude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in [
            (
                "lead.h",
                "#ifndef LEAD_H\n#define LEAD_H\nhead\n#endif\n\n// c\ntail MODE __LINE__\n",
            ),
            ("plain.h", "#if 1\nplain MODE\n#endif\n"),
            ("else.h", "#ifndef E\n#define E\na\n#else\nb\n#endif\n"),
            ("whole.h", "#ifndef W\n#define W\nw MODE\n#endif\n"),
        ] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let src = "#define MODE 1\n#include \"lead.h\"\n#include \"plain.h\"\n#include \"else.h\"\n#include \"whole.h\"\n#undef MODE\n#define MODE 2\n#include \"lead.h\"\n#include \"plain.h\"\n#include \"else.h\"\n#include \"whole.h\"\n#undef LEAD_H\n#undef W\n#include \"lead.h\"\n#include \"whole.h\"\n";
        let path = dir.join("main.c");
        let run = |chunk| {
            let opts = PreprocessOptions::default();
            let (text, _) =
                preprocess_inner::<false>(src, path.to_str().unwrap(), &opts, None, chunk).unwrap();
            text.lines()
                .filter(|line| !line.starts_with('#'))
                .flat_map(str::split_whitespace)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let whole = run(usize::MAX);
        assert_eq!(
            whole,
            "head tail 1 7 plain 1 a w 1 tail 2 7 plain 2 b head tail 2 7 w 2"
        );
        for chunk in [1, 2, 3, 5] {
            assert_eq!(run(chunk), whole, "chunks of {chunk}");
        }
        std::fs::remove_dir_all(dir).unwrap();
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
