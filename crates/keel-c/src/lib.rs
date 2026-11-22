//! A C frontend for keel: preprocessing and compilation to keel IR.
//! `Wasm32UnknownUnknown` selects a bounded freestanding ILP32 profile;
//! see `docs/targets/wasm/c-frontend.md` for accepted storage and call contracts.
//! `preprocess` accepts raw C; `compile` accepts its output or `clang -E` text.
//!
//! The pipeline is lexer (`lex.rs`) → parser, which resolves names and
//! types as it goes and builds a typed tree (`parse.rs`, `expr.rs`,
//! `init.rs`, `ast.rs`) → IR generation (`irgen.rs`). The native targets
//! are LP64. Plain `char` is signed on Apple arm64 and x86-64 Linux and
//! unsigned on AArch64 Linux. `va_list` is the target's: a `char *` on
//! Apple arm64, SysV's `struct __va_list_tag[1]` on x86-64 Linux and the
//! 32-byte AAPCS64 struct on AArch64 Linux. `long double` is `double` on
//! Apple arm64. On the Linux targets it has the x87 80-bit or binary128
//! layout, which keel does not compute with: declarations and layouts are
//! right, but its values are refused.
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
//! and lifetime attributes are diagnosed; see `docs/frontends/c/README.md`
//! for compatibility limits.

mod arena;
mod ast;
mod batch;
mod cleanup_jumps;
mod complex_eval;
mod debug;
mod eval;
mod expr;
mod features;
mod headers;
mod init;
mod int_const;
mod irgen;
mod lex;
mod library_builtins;
mod parse;
mod predefined;
mod preprocess;
mod strings;
mod types;
mod unit;
#[path = "generated/x86_builtins.rs"]
mod x86_builtins;
mod x86_cpu;

pub use batch::{BatchCompiler, BatchCompilerError, Input, RawInput};
pub use library_builtins::{LibraryBuiltin, LibraryBuiltins};

pub use predefined::predefined_macros;
pub use preprocess::{
    PreprocessArtifacts, PreprocessCache, PreprocessDialect, PreprocessOptions, preprocess,
    preprocess_with_artifacts,
};
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Default relaxed floating-point mode; explicit function policies override it.
    pub fast_math: bool,
    /// Debug info: source locations for instructions, and types and
    /// variables. Named locals then live in memory, as without
    /// optimization in C compilers, so a debugger can show and change
    /// every one of them anywhere in the function.
    pub debug_info: bool,
    /// The target, for the types its ABI fixes.
    pub target: keel_ir::Target,
    /// The x86 feature baseline code generation will use. With it, a call to
    /// an x86 builtin whose features the enclosing function does not enable
    /// is an error at the call, if the function is emitted; without it the
    /// backend reports the intrinsic. Pass it only when no optimization runs
    /// before code generation: a call the optimizer would delete, such as one
    /// under `if (0)`, is accepted there but reported here.
    pub x86_features: Option<keel_asm_ir::x86::Features>,
    /// The x86 features every function has, as code generation will use them.
    /// Unlike `x86_features`, it never rejects a call: it only decides whether
    /// 16-byte atomics are available, which needs CX16. Pass it whether or not
    /// the optimizer runs, with the same value given to `PreprocessOptions`.
    pub x86_baseline: Option<keel_asm_ir::x86::Features>,
}

/// C character representation overrides for one translation unit.
#[derive(Clone, Copy, Debug, Default)]
pub struct CharacterTypes {
    /// Signedness of plain `char`. `None` uses the target's default.
    pub plain_char_signed: Option<bool>,
    /// Use a 16-bit unsigned `wchar_t` and UTF-16 wide strings.
    pub short_wchar: bool,
}

/// C language choices that affect lowering to IR.
#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageOptions {
    /// The character types used by this translation unit.
    pub character_types: CharacterTypes,
    /// Zero uninitialized automatic objects at their declarations.
    pub zero_auto_vars: bool,
    /// Conservatively retain potential faults from C memory accesses.
    pub preserve_memory_faults: bool,
    /// Prove integer constant queries after mandatory inlining.
    pub deferred_constant_queries: bool,
    /// Require an explicit type specifier on declarations.
    pub error_implicit_int: bool,
    /// Require parameter types in function declarators.
    pub error_strict_prototypes: bool,
    /// Diagnose returns that do not match the function's result type.
    pub error_return_type: bool,
    /// Reject implicit conversions between incompatible pointer types.
    pub error_incompatible_pointers: bool,
    /// Require designated initializers for records marked `designated_init`.
    pub error_designated_init: bool,
    /// Warn about runtime-sized array declarators.
    pub warn_vla: bool,
    /// Warn about externally visible functions defined without prior declarations.
    pub warn_missing_declarations: bool,
    /// Warn about externally visible functions defined without prior prototypes.
    pub warn_missing_prototypes: bool,
    /// Warn on casts between incompatible function pointer types.
    pub warn_cast_function_type: bool,
    /// Warn on implicit conversions between distinct enumeration types.
    pub warn_enum_conversion: bool,
    /// Warn when a switch arm reaches another label without an annotation.
    pub warn_implicit_fallthrough: bool,
    /// Warn on the same invalid returns that the error policy rejects.
    pub warn_return_type: bool,
    /// Warn when a declaration uses implicit int.
    pub warn_implicit_int: bool,
    /// Warn when a comparison uses distinct enumeration types.
    pub warn_enum_compare: bool,
    /// Warn when the two arms of a conditional use distinct enumeration types.
    pub warn_enum_compare_conditional: bool,
    /// Warn when a comparison mixes integer signedness.
    pub warn_sign_compare: bool,
    /// Warn about an empty body on an `if` or `else`.
    pub warn_empty_body: bool,
    /// Diagnostics for unused entities and discarded values.
    pub unused: UnusedWarnings,
}

/// Individual members of the C unused-warning group.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnusedWarnings {
    pub functions: bool,
    pub variables: bool,
    pub const_variables: bool,
    pub set_variables: bool,
    pub parameters: bool,
    pub set_parameters: bool,
    pub labels: bool,
    pub local_typedefs: bool,
    pub values: bool,
}

impl UnusedWarnings {
    fn track_locals(self) -> bool {
        self.variables || self.set_variables || self.parameters || self.set_parameters
    }

    fn track_globals(self) -> bool {
        self.functions || self.variables || self.const_variables
    }
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
    let builtins = if opts.target == keel_ir::Target::Wasm32UnknownUnknown {
        LibraryBuiltins::disabled()
    } else {
        LibraryBuiltins::hosted()
    };
    compile_with_policies(src, path, opts, binding, builtins)
}

/// Compiles C with explicit definition binding and library builtin policies.
/// Library recognition is native-only; the bounded Wasm profile stays freestanding.
pub fn compile_with_policies(
    src: &str,
    path: &str,
    opts: Options,
    binding: FunctionBinding,
    builtins: LibraryBuiltins,
) -> Result<keel_ir::Module, Diagnostic> {
    compile_with_language(
        src,
        path,
        opts,
        binding,
        builtins,
        LanguageOptions::default(),
    )
}

/// Compiles C with explicit language, binding and builtin policies. Use
/// the same character types during preprocessing.
pub fn compile_with_language(
    src: &str,
    path: &str,
    opts: Options,
    binding: FunctionBinding,
    builtins: LibraryBuiltins,
    language: LanguageOptions,
) -> Result<keel_ir::Module, Diagnostic> {
    compile_with_diagnostics(
        src,
        path,
        opts,
        binding,
        builtins,
        language,
        &mut Vec::new(),
    )
}

/// Compiles C with explicit policies and collects enabled source warnings.
pub fn compile_with_diagnostics(
    src: &str,
    path: &str,
    opts: Options,
    binding: FunctionBinding,
    builtins: LibraryBuiltins,
    language: LanguageOptions,
    warnings: &mut Vec<Diagnostic>,
) -> Result<keel_ir::Module, Diagnostic> {
    if opts.target == keel_ir::Target::Wasm32UnknownUnknown && opts.fast_math {
        return Err(Diagnostic {
            file: path.into(),
            line: 1,
            col: 1,
            message: "fast math requires a native target".into(),
        });
    }
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
    let mut parser = parse::Parser::with_language(src, path, opts.target, text, language);
    parser.set_x86_features(opts.x86_features);
    parser.set_x86_baseline(opts.x86_baseline);
    let parsed = parser.translation_unit();
    let files = parser.take_files();
    warnings.extend(parser.warnings.drain(..).map(|e| diagnostic(&files, e)));
    parsed.map_err(|e| diagnostic(&files, e))?;
    let constant_query_marker = language.deferred_constant_queries.then(|| {
        let mut marker = std::borrow::Cow::Borrowed("\u{1}keel.constant.query");
        while parser.globals.iter().any(|global| global.symbol == marker) {
            marker.to_mut().push('_');
        }
        marker
    });
    let funcs = std::mem::take(&mut parser.funcs);
    let mut lowering_warnings = Vec::new();
    let lowered = irgen::Gen::new(&parser, funcs, &files, opts.debug_info)
        .fast_math(opts.fast_math)
        .with_language(language)
        .with_constant_query_marker(constant_query_marker.as_deref())
        .with_library_builtins(builtins)
        .run(&mut lowering_warnings);
    warnings.extend(lowering_warnings.into_iter().map(|e| diagnostic(&files, e)));
    let mut module = lowered.map_err(|e| diagnostic(&files, e))?;
    apply_function_binding(&mut module, opts.target, binding);
    finalize_inline_asm(&mut module, &parser, constant_query_marker.as_deref())
        .map_err(|message| diagnostic(&files, Error::new(Loc::default(), message)))?;
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

// Pending immediate operands are internal to C lowering. Public compile APIs
// return only finalized IR, so ordinary verification and codegen stay strict.
fn finalize_inline_asm(
    module: &mut keel_ir::Module,
    parser: &parse::Parser<'_>,
    constant_query_marker: Option<&str>,
) -> Result<(), String> {
    let target = parser.target;
    let constant_query_marker = constant_query_marker.filter(|marker| {
        module
            .functions
            .iter()
            .any(|f| f.callees().any(|(_, decl)| decl.name == *marker))
    });
    let inline_only: Vec<_> = parser
        .globals
        .iter()
        .filter(|g| g.gnu_inline_only)
        .map(|g| g.symbol)
        .collect();
    let definitions: Vec<_> = parser
        .globals
        .iter()
        .filter(|g| {
            g.func.is_some()
                && !g.internal
                && !g.gnu_inline_only
                && g.weak.is_none()
                && g.function_attrs.alwaysinline
        })
        .map(|g| g.symbol)
        .collect();
    if inline_only.is_empty()
        && definitions.is_empty()
        && !module.functions.iter().any(|f| f.attrs.alwaysinline)
        && constant_query_marker.is_none()
        && (target != keel_ir::Target::X86_64LinuxGnu
            || !module.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::InlineAsm { .. }))
            }))
    {
        return Ok(());
    }
    keel_opt::required_inline_asm_with_c_definitions(module, &inline_only, &definitions)?;
    if let Some(marker) = constant_query_marker {
        keel_opt::finalize_c_constant_queries(module, marker);
    }
    // Release builds trust the expansion; the driver checks frontend output
    // under its verification policy.
    if cfg!(debug_assertions) {
        keel_ir::verify_module(module)
            .map_err(|errors| format!("inline asm finalization: {errors:?}"))?;
    }
    Ok(())
}
