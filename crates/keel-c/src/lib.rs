//! A C frontend for keel: preprocessing and compilation to keel IR.
//! `Wasm32UnknownUnknown` selects a bounded freestanding ILP32 profile;
//! see `docs/wasm-c-freestanding.md` for accepted storage and call contracts.
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
//! The language is C11-oriented, with bounded float/double complex values
//! and automatic multidimensional VLA/VM support, explicitly typed fixed-size
//! old-style (K&R) function definitions, plus GNU extensions: those the system
//! headers (Apple's, glibc's) use and more.
//! `__auto_type` follows GNU's single-variable inference and initializer
//! scope rules. Atomic pointer prefix/postfix `++` and `--` are supported.
//! Attribute syntax is accepted more broadly than its semantics: many
//! attributes are ignored. `visibility("hidden")` and `("internal")` emit
//! hidden symbols; ordinary `weak` supports external function/object definitions
//! and undefined references on both native targets. Default-priority executable
//! constructor/destructor callbacks, compiler-only `used`, bounded custom `section`
//! placement and distinct linker `retain` are supported;
//! Darwin finalization requires libSystem atexit, ELF requires runtime array dispatch.
//! Other unimplemented linkage
//! and lifetime attributes are diagnosed; see `docs/c-frontend-coverage.md` and
//! `docs/design.md`'s C frontend section for compatibility limits.

mod arena;
mod ast;
mod complex_eval;
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
/// Wasm accepts only the bounded freestanding integer/storage profile and
/// verifies against WASM32. The caller must retain that layout and avoid native
/// code generation and target-sensitive optimizer passes.
pub fn compile(src: &str, path: &str, opts: Options) -> Result<keel_ir::Module, Diagnostic> {
    compile_with_binding(src, path, opts, FunctionBinding::SemanticInterposition)
}

/// Direct-call optimization policy for strong C definitions. Neither mode changes
/// exports, function-address binding, or calls left unexpanded by the optimizer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FunctionBinding {
    #[default]
    SemanticInterposition,
    /// Replacements must preserve the definition's semantics and side effects.
    NoSemanticInterposition,
}

/// Compiles C with an explicit function binding policy. The ordinary `compile`
/// API retains semantic interposition for default-visibility public definitions.
pub fn compile_with_binding(
    src: &str,
    path: &str,
    opts: Options,
    binding: FunctionBinding,
) -> Result<keel_ir::Module, Diagnostic> {
    if opts.target == keel_ir::Target::Wasm32UnknownUnknown
        && binding == FunctionBinding::NoSemanticInterposition
    {
        return Err(Diagnostic {
            file: path.into(),
            line: 1,
            col: 1,
            message: "C function binding modes require a native target".into(),
        });
    }
    let arena = bumpalo::Bump::new();
    let text = crate::strings::Strings::new(&arena);
    let diagnostic = |files: &[&str], e: Error| Diagnostic {
        file: files
            .get(e.loc.file as usize)
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.to_string()),
        line: e.loc.line,
        col: e.loc.col,
        message: e.message,
    };
    let mut parser = parse::Parser::new(src, path, opts.target, text);
    let parsed = parser.translation_unit();
    let files = parser.take_files();
    parsed.map_err(|e| diagnostic(&files, e))?;
    let funcs = std::mem::take(&mut parser.funcs);
    let mut module = irgen::Gen::new(&parser, funcs, &files, opts.debug_info)
        .run()
        .map_err(|e| diagnostic(&files, e))?;
    apply_function_binding(&mut module, opts.target, binding);
    if opts.target == keel_ir::Target::Wasm32UnknownUnknown {
        keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).map_err(|e| {
            diagnostic(
                &files,
                Error::new(Loc::default(), format!("wasm32 IR verification: {e:?}")),
            )
        })?;
    }
    Ok(module)
}

/// Profiling compilation has no Wasm implementation; reject before lexing.
#[cfg(feature = "profile")]
fn require_public_target(target: keel_ir::Target, path: &str) -> Result<(), Diagnostic> {
    if target == keel_ir::Target::Wasm32UnknownUnknown {
        return Err(Diagnostic {
            file: path.into(),
            line: 1,
            col: 1,
            message:
                "unsupported target wasm32-unknown-unknown: C compile profiling is unavailable"
                    .into(),
        });
    }
    Ok(())
}

// Visibility is a target-specific proof supplied by the C producer. A target-free
// IR `hidden` flag alone does not establish ELF binding for the optimizer.
fn apply_function_binding(
    module: &mut keel_ir::Module,
    target: keel_ir::Target,
    binding: FunctionBinding,
) {
    let elf = matches!(
        target,
        keel_ir::Target::X86_64LinuxGnu | keel_ir::Target::Aarch64LinuxGnu
    );
    let mut changed = false;
    for f in &mut module.functions {
        if !f.local
            && !f.attrs.weak
            && !f.weak_odr
            && ((elf && f.hidden) || binding == FunctionBinding::NoSemanticInterposition)
        {
            f.attrs.no_semantic_interposition = true;
            changed = true;
        }
    }
    if changed {
        module.sync_function_attrs();
    }
}
