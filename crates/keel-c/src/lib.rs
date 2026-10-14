//! A C frontend for keel: one preprocessed C file (the output of
//! `clang -E`) to one keel module.
//!
//! The pipeline is lexer (`lex.rs`) → parser, which resolves names and
//! types as it goes and builds a typed tree (`parse.rs`, `expr.rs`,
//! `init.rs`, `ast.rs`) → IR generation (`irgen.rs`). Target facts are Apple
//! arm64's: LP64, signed `char`, `long double` = `double`, `va_list` a
//! `char *`.
//!
//! The language is C11 without `_Generic`, `_Atomic`, `_Complex`,
//! variable-length arrays and old-style (K&R) function definitions, plus
//! the GNU extensions Apple's headers use.

mod ast;
mod debug;
mod eval;
mod expr;
mod init;
mod irgen;
mod lex;
mod parse;
mod types;

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
}

/// Compiles preprocessed C source, read from `path`, to a keel module.
pub fn compile(src: &str, path: &str, opts: Options) -> Result<keel_ir::Module, Diagnostic> {
    let mut files = Vec::new();
    let diagnostic = |files: &[String], e: Error| Diagnostic {
        file: files
            .get(e.loc.file as usize)
            .cloned()
            .unwrap_or_else(|| path.to_string()),
        line: e.loc.line,
        col: e.loc.col,
        message: e.message,
    };
    let toks = lex::tokenize(src, path, &mut files).map_err(|e| diagnostic(&files, e))?;
    let mut parser = parse::Parser::new(toks);
    parser
        .translation_unit()
        .map_err(|e| diagnostic(&files, e))?;
    irgen::Gen::new(&parser, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))
}
