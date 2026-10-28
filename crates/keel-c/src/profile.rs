//! Optional stage measurements. Input reading and IR destruction are separate
//! from `compile`'s boundary. Parsing includes lexing, name resolution and
//! type checking; `lex_ms` times a separate streaming lexing pass, using
//! the parser's 256-token batch size. It excludes lexer destruction.
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
    crate::require_public_target(opts.target, path)?;
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
    let mut lex_text = crate::strings::Strings::new(&lex_arena);
    let mut lexer = lex::Lexer::new(src, path, &mut lex_text);
    let mut chunk = Vec::new();
    let mut tokens = 0;
    loop {
        chunk.clear();
        let end = lexer
            .tokens(&mut lex_text, &mut chunk, 256)
            .map_err(|e| diagnostic(&lexer.files, e))?;
        tokens += chunk.len();
        if end {
            break;
        }
    }
    let lex_ms = start.elapsed().as_secs_f64() * 1000.;
    drop(chunk);
    drop(lexer);
    drop(lex_text);
    drop(lex_arena);
    let start = Instant::now();
    let mut parser = parse::Parser::new(src, path, opts.target, text);
    let parsed = parser.translation_unit();
    let files = parser.take_files();
    parsed.map_err(|e| diagnostic(&files, e))?;
    let funcs = std::mem::take(&mut parser.funcs);
    let parse_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let mut module = irgen::Gen::new(&parser, funcs, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))?;
    crate::apply_function_binding(&mut module, opts.target, crate::FunctionBinding::default());
    crate::finalize_inline_asm(&mut module, &parser)
        .map_err(|message| diagnostic(&files, crate::Error::new(crate::Loc::default(), message)))?;
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
