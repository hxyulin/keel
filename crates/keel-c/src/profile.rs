//! Optional stage measurements. Input reading and IR destruction are separate
//! from `compile`'s boundary. Parsing includes name resolution and type checking.
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
    let mut files = Vec::new();
    let diagnostic = |files: &[String], e: crate::Error| Diagnostic {
        file: files
            .get(e.loc.file as usize)
            .cloned()
            .unwrap_or_else(|| path.into()),
        line: e.loc.line,
        col: e.loc.col,
        message: e.message,
    };
    let start = Instant::now();
    let toks = lex::tokenize(src, path, &mut files).map_err(|e| diagnostic(&files, e))?;
    let tokens = toks.len();
    let lex_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let mut parser = parse::Parser::new(toks, opts.target);
    parser
        .translation_unit()
        .map_err(|e| diagnostic(&files, e))?;
    let parse_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let module = irgen::Gen::new(&parser, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))?;
    let irgen_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    drop(parser);
    drop(files);
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
