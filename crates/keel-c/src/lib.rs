//! A C frontend for keel: native preprocessing and compilation to keel IR.
//! `preprocess` accepts raw C; `compile` accepts its output or `clang -E` text.
//!
//! The pipeline is lexer (`lex.rs`) → parser, which resolves names and
//! types as it goes and builds a typed tree (`parse.rs`, `expr.rs`,
//! `init.rs`, `ast.rs`) → IR generation (`irgen.rs`). Both targets are
//! LP64 with a signed `char`; `va_list` is the target's (a `char *` on
//! Apple arm64, SysV's `struct __va_list_tag[1]` on x86-64 Linux).
//! `long double` is `double` on Apple arm64. On x86-64 Linux it is the
//! x87 80-bit format, which keel does not have: it has its size, so
//! declarations and layouts are right, but its values are refused.
//!
//! The language is C11 without `_Complex` (which only parses, in
//! declarations), variable-length arrays and old-style (K&R) function
//! definitions, plus GNU extensions: those the system headers (Apple's,
//! glibc's) use and more.
//! `__auto_type` follows GNU's single-variable inference and initializer
//! scope rules. Atomic pointer prefix/postfix `++` and `--` are supported.
//! Attribute syntax is accepted more broadly than its semantics: many
//! attributes are ignored. Selected unimplemented linkage and lifetime
//! attributes are diagnosed; see `docs/c-frontend-coverage.md` and
//! `docs/design.md`'s C frontend section for compatibility limits.

mod ast;
mod debug;
mod eval;
mod expr;
mod features;
mod headers;
mod init;
mod irgen;
mod lex;
mod parse;
mod predefined;
mod preprocess;
mod strings;
mod types;

pub use predefined::predefined_macros;
pub use preprocess::{PreprocessOptions, preprocess};
#[cfg(feature = "profile")]
pub use preprocess::{PreprocessStats, preprocess_with_stats};

#[cfg(feature = "profile")]
pub mod profile;

use std::fmt;

/// A position in the preprocessed input's original files.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct Loc {
    /// Index into the file names the line markers gave.
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
pub struct Error {
    pub loc: Loc,
    pub message: String,
}

impl Error {
    pub fn new(loc: Loc, message: impl Into<String>) -> Self {
        Self {
            loc,
            message: message.into(),
        }
    }
}

/// An error with its file name resolved.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: error: {}",
            self.file, self.line, self.col, self.message
        )
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Default)]
pub struct Options {
    /// Debug info: source locations for instructions, and types and
    /// variables. Named locals then live in memory, as without
    /// optimization in C compilers, so a debugger can show and change
    /// every one of them anywhere in the function.
    pub debug_info: bool,
    /// The target, for the types its ABI fixes.
    pub target: keel_ir::Target,
}

/// Compiles preprocessed C source, read from `path`, to a keel module.
pub fn compile(src: &str, path: &str, opts: Options) -> Result<keel_ir::Module, Diagnostic> {
    let arena = bumpalo::Bump::new();
    let mut text = crate::strings::Strings::new(&arena);
    let mut files = Vec::new();
    let diagnostic = |files: &[&str], e: Error| Diagnostic {
        file: files
            .get(e.loc.file as usize)
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.to_string()),
        line: e.loc.line,
        col: e.loc.col,
        message: e.message,
    };
    let toks =
        lex::tokenize(src, path, &mut files, &mut text).map_err(|e| diagnostic(&files, e))?;
    let mut parser = parse::Parser::new(toks, opts.target, text);
    parser
        .translation_unit()
        .map_err(|e| diagnostic(&files, e))?;
    irgen::Gen::new(&parser, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))
}
