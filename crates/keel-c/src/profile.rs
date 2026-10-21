//! Optional stage measurements. Input reading and IR destruction are separate
//! from `compile`'s boundary. Parsing includes lexing, name resolution and
//! type checking; `lex_ms` times a separate lexing pass.
use crate::{Diagnostic, Options, irgen, lex, parse};
use std::time::Instant;

#[derive(Debug)]
pub struct Timings {
    pub lex_ms: f64,
    pub parse_ms: f64,
    pub irgen_ms: f64,
    pub cleanup_ms: f64,
    pub tokens: usize,
}

pub fn compile(
    src: &str,
    path: &str,
    opts: Options,
) -> Result<(keel_ir::Module, Timings), Diagnostic> {
    let arena = bumpalo::Bump::new();
    let text = crate::strings::Strings::new(&arena);
    let diagnostic = |files: &[&str], e: crate::Error| Diagnostic {
        file: files
            .get(e.loc.file as usize)
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.into()),
        line: e.loc.line,
        col: e.loc.col,
        message: e.message,
    };
    // The parser lexes as it goes; lexing alone is measured separately,
    // with its own strings.
    let start = Instant::now();
    let lex_arena = bumpalo::Bump::new();
    let mut files = Vec::new();
    let toks = lex::tokenize(
        src,
        path,
        &mut files,
        &mut crate::strings::Strings::new(&lex_arena),
    )
    .map_err(|e| diagnostic(&files, e))?;
    let tokens = toks.len();
    let lex_ms = start.elapsed().as_secs_f64() * 1000.;
    drop(toks);
    drop(lex_arena);
    let start = Instant::now();
    let mut parser = parse::Parser::new(src, path, opts.target, text);
    let parsed = parser.translation_unit();
    let files = parser.take_files();
    parsed.map_err(|e| diagnostic(&files, e))?;
    let parse_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let module = irgen::Gen::new(&parser, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))?;
    let irgen_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    drop(parser);
    drop(files);
    drop(arena);
    let cleanup_ms = start.elapsed().as_secs_f64() * 1000.;
    Ok((
        module,
        Timings {
            lex_ms,
            parse_ms,
            irgen_ms,
            cleanup_ms,
            tokens,
        },
    ))
}
