//! The parser: declarations and statements. Expressions are in
//! `expr.rs`, initializers in `init.rs`.
//!
//! C cannot be parsed without knowing which names are types (`T * x;` is a
//! declaration if `T` is a typedef, a multiplication otherwise), so the
//! parser keeps the scopes, and since it already knows every name's type
//! it type-checks as it goes, producing the typed tree in `ast.rs`.
//!
//! Beyond ISO C it accepts what Apple's headers use: `__attribute__((..))`
//! (`aligned`, `packed`, `returns_twice`, `noreturn` and `visibility` have
//! semantics), `__asm` labels that rename a symbol, `__inline`, `__restrict`,
//! nullability qualifiers, `__extension__`, `__int128` and a few `__builtin_`s.

mod aliases;
mod asm;
mod registers;

use crate::arena::BodyArena;
use crate::ast::*;
use crate::lex::{Lexer, Tok, Token};
use crate::strings::Strings;
use crate::types::*;
use crate::{Error, Loc};
use keel_ir::Target;
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use smallvec::SmallVec;

pub type R<T> = Result<T, Error>;

/// A string literal: its bytes with the terminator, and whether it is wide.
#[derive(Clone, Debug)]
pub struct StrLit<'a> {
    pub bytes: &'a [u8],
    pub wide: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Sym {
    Local(LocalId),
    Global(usize),
    /// Read-only hardware RSP, without an addressable global object.
    StackPointer,
    /// By index in the typedef table.
    Typedef(usize),
    Enum(i64, IntKind),
}

#[derive(Default, Clone)]
struct Scope<'a> {
    /// Names first bound in this scope, restored in reverse order on exit.
    names: Vec<&'a str>,
    /// GNU local labels, in their separate lexical namespace.
    local_labels: HashMap<&'a str, &'a str>,
    /// Struct and union definitions, or `None` for an enum tag.
    tags: HashMap<&'a str, Option<usize>>,
    /// Enum tags, by index in the enum table, for debug info.
    enums: HashMap<&'a str, usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Storage {
    None,
    Typedef,
    Extern,
    Static,
}

struct DeclSpec<'a> {
    ty: Ty<'a>,
    sugar: Sugar<'a>,
    infer: bool,
    implicit_int: bool,
    storage: Storage,
    auto_storage: bool,
    register_storage: bool,
    is_const: bool,
    is_volatile: bool,
    inline: bool,
    thread_local: bool,
    attrs: Attrs,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TypeMode {
    Integer(u64),
    ComplexBinary128,
}

#[derive(Default, Clone, PartialEq)]
pub struct Attrs {
    pub nonstring: bool,
    pub alias: Option<(usize, Loc)>,
    pub alias_conflict: Option<Loc>,
    pub mode: Option<(TypeMode, Loc)>,
    pub vector_size: Option<(u64, Loc)>,
    pub aligned: Option<u64>,
    pub alignas: Option<u64>,
    pub packed: bool,
    pub transparent_union: Option<Loc>,
    pub microsoft_x64: Option<Loc>,
    pub gnu_inline: bool,
    pub returns_twice: bool,
    pub noreturn: bool,
    pub function_attrs: keel_ir::FunctionAttrs,
    pub function_policy: Option<Loc>,
    /// `visibility(..)` and where it was written; see `Global::visibility`.
    pub visibility: Option<(Visibility, Loc)>,
    /// Where a different visibility than `visibility` was written.
    pub visibility_conflict: Option<Loc>,
    /// Where `weak` was written; see `Global::weak`.
    pub weak: Option<Loc>,
    pub lifecycle: Option<Loc>,
    pub used: Option<Loc>,
    pub retain: Option<Loc>,
    pub section: Option<(keel_ir::SectionName, Loc)>,
    pub section_conflict: Option<Loc>,
    pub cleanup: Option<(usize, Loc)>,
    pub cleanup_conflict: Option<Loc>,
}

impl Attrs {
    fn merge(&mut self, other: Self) {
        if let Some((visibility, loc)) = other.visibility {
            self.set_visibility(visibility, loc);
        }
        self.visibility_conflict = self.visibility_conflict.or(other.visibility_conflict);
        if self.alias.is_some() && other.alias.is_some() {
            self.alias_conflict = other.alias.map(|(_, loc)| loc);
        }
        self.alias_conflict = self.alias_conflict.or(other.alias_conflict);
        self.alias = self.alias.or(other.alias);
        self.weak = self.weak.or(other.weak);
        self.lifecycle = self.lifecycle.or(other.lifecycle);
        self.used = self.used.or(other.used);
        self.retain = self.retain.or(other.retain);
        if self
            .section
            .zip(other.section)
            .is_some_and(|(a, b)| a.0 != b.0)
        {
            self.section_conflict = other.section.map(|s| s.1);
        }
        self.section_conflict = self.section_conflict.or(other.section_conflict);
        self.section = self.section.or(other.section);
        if self.cleanup.is_some() && other.cleanup.is_some() {
            self.cleanup_conflict = other.cleanup.map(|(_, loc)| loc);
        }
        self.cleanup_conflict = self.cleanup_conflict.or(other.cleanup_conflict);
        self.cleanup = other.cleanup.or(self.cleanup);
        self.mode = other.mode.or(self.mode);
        self.vector_size = other.vector_size.or(self.vector_size);
        if let Some(a) = other.aligned {
            self.aligned = Some(self.aligned.unwrap_or(1).max(a));
        }
        if let Some(a) = other.alignas {
            self.alignas = Some(self.alignas.unwrap_or(0).max(a));
        }
        self.nonstring |= other.nonstring;
        self.packed |= other.packed;
        self.transparent_union = self.transparent_union.or(other.transparent_union);
        self.microsoft_x64 = self.microsoft_x64.or(other.microsoft_x64);
        self.gnu_inline |= other.gnu_inline;
        self.returns_twice |= other.returns_twice;
        self.noreturn |= other.noreturn;
        self.function_attrs.merge(other.function_attrs);
        self.function_policy = self.function_policy.or(other.function_policy);
    }
    fn set_visibility(&mut self, visibility: Visibility, loc: Loc) {
        match self.visibility {
            None => self.visibility = Some((visibility, loc)),
            Some((v, _)) if v != visibility => {
                self.visibility_conflict.get_or_insert(loc);
            }
            Some(_) => {}
        }
    }
    /// Rejects the attributes that only a function or an object with
    /// linkage can carry, written where no such symbol is declared.
    fn no_type_attrs(&self) -> R<()> {
        if let Some(loc) = self.microsoft_x64 {
            return Err(unsupported_attribute(loc, "ms_abi"));
        }
        self.no_cleanup()?;
        if self.nonstring {
            return Err(Error::new(
                Loc::default(),
                "nonstring requires a character array or pointer declaration",
            ));
        }
        if let Some(loc) = self
            .mode
            .map(|(_, loc)| loc)
            .or(self.vector_size.map(|(_, loc)| loc))
        {
            return Err(Error::new(
                loc,
                "type-changing attribute is not supported in this position",
            ));
        }
        Ok(())
    }
    fn no_cleanup(&self) -> R<()> {
        if let Some((_, loc)) = self.cleanup {
            return Err(Error::new(
                loc,
                "cleanup applies only to automatic local objects",
            ));
        }
        Ok(())
    }
    fn no_symbol_attrs(&self) -> R<()> {
        if let Some(loc) = self.function_policy {
            return Err(Error::new(
                loc,
                "function policy attributes require a function declaration",
            ));
        }
        if let Some(loc) = self.microsoft_x64 {
            return Err(unsupported_attribute(loc, "ms_abi"));
        }
        self.no_cleanup()?;
        if let Some((_, loc)) = self.alias {
            return Err(Error::new(
                loc,
                "alias requires a file-scope function declaration",
            ));
        }
        if let Some(loc) = self
            .lifecycle
            .or(self.used)
            .or(self.retain)
            .or(self.section.map(|s| s.1))
        {
            return Err(Error::new(
                loc,
                "lifecycle/used/retain/section attributes require a function or static-storage object",
            ));
        }
        if let Some(loc) = self.weak {
            return Err(Error::new(
                loc,
                "weak applies only to functions and variables with external linkage",
            ));
        }
        if let Some(loc) = self.visibility_conflict.or(self
            .visibility
            .filter(|(v, _)| v.hidden())
            .map(|(_, loc)| loc))
        {
            return Err(Error::new(
                loc,
                "non-default visibility applies only to functions and variables with linkage",
            ));
        }
        Ok(())
    }
    fn alignment(&self) -> Option<u64> {
        self.aligned
            .into_iter()
            .chain(self.alignas.filter(|a| *a != 0))
            .max()
    }
}

/// Known nonlocal calls cannot participate in lexical cleanup unwinding.
pub(crate) fn nonlocal_exit(global: &Global<'_>) -> bool {
    let known = |name| {
        matches!(
            name,
            "setjmp"
                | "_setjmp"
                | "sigsetjmp"
                | "__sigsetjmp"
                | "savectx"
                | "vfork"
                | "longjmp"
                | "_longjmp"
                | "siglongjmp"
                | "__longjmp_chk"
                | "__builtin_longjmp"
                | "__builtin_setjmp"
        )
    };
    // A local C name can still call a known nonlocal runtime entry through
    // an asm linker label, including one supplied by a later declaration.
    global.returns_twice || known(global.name) || known(global.symbol)
}

pub(crate) fn unsupported_attribute(loc: Loc, name: &str) -> Error {
    Error::new(
        loc,
        format!("attribute `{name}` requires semantics that are not supported"),
    )
}

/// A parsed declarator: the name (if not abstract) and the full type.
pub struct Declarator<'a> {
    pub name: Option<(&'a str, Loc)>,
    pub ty: Ty<'a>,
    pub sugar: Sugar<'a>,
    /// `const` or `volatile` on the declared object itself.
    pub is_const: bool,
    pub is_volatile: bool,
    pub attrs: Attrs,
}

pub(crate) struct SwitchCtx<'a> {
    pub ty: Ty<'a>,
    pub cases: Vec<SwitchCase>,
    pub default: Option<usize>,
    pub labels: usize,
}

/// Per-function state.
pub(crate) struct FuncState<'a> {
    pub locals: Vec<Local<'a>>,
    /// Pending captures, consumed by the declaration or type-name expression.
    pub vla_bounds: Vec<(LocalId, Expr<'a>)>,
    pub has_vm: bool,
    pub cleanup_unsupported_exit: Option<(Loc, &'static str)>,
    /// Direct calls retained for cleanup contract checks after all redeclarations.
    pub direct_calls: Vec<(usize, Loc)>,
    /// Block scopes, by their parent, and the one being parsed.
    pub scopes: Vec<Option<usize>>,
    pub scope: Option<usize>,
    pub label_addrs: Vec<&'a str>,
    pub ret: Ty<'a>,
    pub name: &'a str,
    pub switches: Vec<SwitchCtx<'a>>,
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    /// Tokens of the file-scope declaration being parsed, and those read
    /// ahead. Earlier ones are dropped between declarations; only
    /// backtracking within a declaration revisits tokens.
    toks: Vec<Token<'a>>,
    /// The lexer has given `Eof`, the last of `toks`.
    lexed_all: bool,
    /// The first lexical error, which wins over any parse error.
    lex_error: Option<Error>,
    /// Token positions are saved to come back to: earlier tokens must stay.
    pinned: u32,
    pub(crate) text: Strings<'a>,
    pos: usize,
    scopes: Vec<Scope<'a>>,
    /// One spelling lookup, followed by its innermost binding. The short
    /// stack also supports file-scope declarations introduced inside blocks.
    /// Names are interned, so they are keyed by address (see `name_key`).
    names: HashMap<usize, SmallVec<[(usize, Sym); 2]>>,
    pub(crate) arena: &'a bumpalo::Bump,
    /// Where expressions and statements go: the body arena of the function
    /// being parsed, else `arena`.
    nodes: &'a bumpalo::Bump,
    type_nodes: TypeNodes<'a>,
    pub structs: Vec<StructDef<'a>>,
    pub typedefs: Vec<TypedefDef<'a>>,
    pub enums: Vec<EnumDef<'a>>,
    pub globals: Vec<Global<'a>>,
    pub funcs: Vec<FuncDef<'a>>,
    pub strings: Vec<StrLit<'a>>,
    pub(crate) assembly: Vec<String>,
    alias_names: Vec<&'a str>,
    aliases: HashMap<usize, (usize, Loc)>,
    literal_ids: [HashMap<&'a [u8], usize>; 2],
    /// Emptied statement buffers, for blocks to reuse.
    stmt_bufs: Vec<Vec<Stmt<'a>>>,
    /// Emptied argument buffers, for calls to reuse.
    pub(crate) expr_bufs: Vec<Vec<Expr<'a>>>,
    pub(crate) fs: Option<FuncState<'a>>,
    /// Functions declared at file scope at least once without `inline` (or with
    /// `extern`), for which inline-only emission does not apply.
    /// Explicit static linkage, recorded in `Global::internal`, still wins.
    external_def: HashSet<usize>,
    /// Runtime bounds are admitted in automatic and parameter declarators.
    vla_allowed: bool,
    in_parameter_list: bool,
    /// Parameters of the last parameter list parsed: those of a function
    /// definition's declarator, when its body follows.
    last_param_function: Option<&'a FuncTy<'a>>,
    last_param_scope: Option<Scope<'a>>,
    last_param_bindings: Vec<(&'a str, Sym)>,
    last_param_state: Option<FuncState<'a>>,
    last_param_ids: Vec<LocalId>,
    last_param_star: bool,
    parameter_arrays: Vec<(&'a ArrayTy<'a>, (bool, bool, bool, bool))>,
    parameter_star: bool,
    last_params: Vec<(Option<&'a str>, Ty<'a>, Sugar<'a>, Loc)>,
    /// Counts names made up for static locals and compound literals.
    unique: usize,
    pub(crate) target: Target,
    pub(crate) c_layout: CDataLayout,
    /// SysV's `struct __va_list_tag`, once a `va_list` needs it.
    va_list_tag: Option<usize>,
}

/// A keyword that starts a type name or declaration.
const TYPE_WORD: u8 = 1;
/// A word that cannot be an identifier.
const RESERVED: u8 = 2;

/// Keywords and what they are. The few type keywords that are typedef-like
/// names (`__builtin_va_list`, `__int128_t`, `__uint128_t`) are not
/// reserved.
const KEYWORDS: [(&str, u8); 78] = [
    ("void", TYPE_WORD | RESERVED),
    ("_Bool", TYPE_WORD | RESERVED),
    ("char", TYPE_WORD | RESERVED),
    ("short", TYPE_WORD | RESERVED),
    ("int", TYPE_WORD | RESERVED),
    ("long", TYPE_WORD | RESERVED),
    ("float", TYPE_WORD | RESERVED),
    ("double", TYPE_WORD | RESERVED),
    ("signed", TYPE_WORD | RESERVED),
    ("unsigned", TYPE_WORD | RESERVED),
    ("__signed", TYPE_WORD | RESERVED),
    ("__signed__", TYPE_WORD | RESERVED),
    ("struct", TYPE_WORD | RESERVED),
    ("union", TYPE_WORD | RESERVED),
    ("enum", TYPE_WORD | RESERVED),
    ("typedef", TYPE_WORD | RESERVED),
    ("extern", TYPE_WORD | RESERVED),
    ("static", TYPE_WORD | RESERVED),
    ("auto", TYPE_WORD | RESERVED),
    ("register", TYPE_WORD | RESERVED),
    ("const", TYPE_WORD | RESERVED),
    ("volatile", TYPE_WORD | RESERVED),
    ("restrict", TYPE_WORD | RESERVED),
    ("__restrict", TYPE_WORD | RESERVED),
    ("__restrict__", TYPE_WORD | RESERVED),
    ("__const", TYPE_WORD | RESERVED),
    ("__volatile", TYPE_WORD | RESERVED),
    ("__volatile__", TYPE_WORD | RESERVED),
    ("__label__", RESERVED),
    ("inline", TYPE_WORD | RESERVED),
    ("__inline", TYPE_WORD | RESERVED),
    ("__inline__", TYPE_WORD | RESERVED),
    ("_Noreturn", TYPE_WORD | RESERVED),
    ("_Thread_local", TYPE_WORD | RESERVED),
    ("__thread", TYPE_WORD | RESERVED),
    ("_Alignas", TYPE_WORD | RESERVED),
    ("__attribute__", TYPE_WORD | RESERVED),
    ("__attribute", TYPE_WORD | RESERVED),
    ("_Nullable", TYPE_WORD | RESERVED),
    ("_Nonnull", TYPE_WORD | RESERVED),
    ("_Null_unspecified", TYPE_WORD | RESERVED),
    ("__nullable", TYPE_WORD | RESERVED),
    ("__nonnull", TYPE_WORD | RESERVED),
    ("__int128", TYPE_WORD | RESERVED),
    ("__int128_t", TYPE_WORD),
    ("__uint128_t", TYPE_WORD),
    ("_Float16", TYPE_WORD | RESERVED),
    ("__float128", TYPE_WORD | RESERVED),
    ("__builtin_va_list", TYPE_WORD),
    ("__SVFloat32_t", TYPE_WORD),
    ("__SVFloat64_t", TYPE_WORD),
    ("__SVBool_t", TYPE_WORD),
    ("typeof", TYPE_WORD | RESERVED),
    ("__typeof__", TYPE_WORD | RESERVED),
    ("__typeof", TYPE_WORD | RESERVED),
    ("__auto_type", TYPE_WORD | RESERVED),
    ("_Complex", TYPE_WORD | RESERVED),
    ("_Atomic", TYPE_WORD | RESERVED),
    ("if", RESERVED),
    ("else", RESERVED),
    ("while", RESERVED),
    ("do", RESERVED),
    ("for", RESERVED),
    ("switch", RESERVED),
    ("case", RESERVED),
    ("default", RESERVED),
    ("break", RESERVED),
    ("continue", RESERVED),
    ("return", RESERVED),
    ("goto", RESERVED),
    ("sizeof", RESERVED),
    ("_Alignof", RESERVED),
    ("__alignof__", RESERVED),
    ("_Static_assert", RESERVED),
    ("_Generic", RESERVED),
    ("asm", RESERVED),
    ("__asm", RESERVED),
    ("__asm__", RESERVED),
];

/// Where a word's keyword search starts: its length and three of its bytes.
const fn keyword_slot(word: &[u8]) -> usize {
    let n = word.len();
    (n * 61 + word[0] as usize * 7 + word[n - 1] as usize * 13 + word[n / 2] as usize * 3) & 255
}

/// `KEYWORDS` by open addressing: an index plus one, 0 for none.
const KEYWORD_SLOTS: [u8; 256] = {
    let mut slots = [0u8; 256];
    let mut k = 0;
    while k < KEYWORDS.len() {
        let mut i = keyword_slot(KEYWORDS[k].0.as_bytes());
        while slots[i] != 0 {
            i = (i + 1) & 255;
        }
        slots[i] = k as u8 + 1;
        k += 1;
    }
    slots
};

/// What `s` is as a keyword: `TYPE_WORD` and `RESERVED` bits.
#[inline]
fn keyword_class(s: &str) -> u8 {
    if s.is_empty() {
        return 0;
    }
    let mut i = keyword_slot(s.as_bytes());
    loop {
        let (word, class) = match KEYWORD_SLOTS[i] {
            0 => return 0,
            k => KEYWORDS[k as usize - 1],
        };
        if word == s {
            return class;
        }
        i = (i + 1) & 255;
    }
}

fn is_type_keyword(s: &str) -> bool {
    keyword_class(s) & TYPE_WORD != 0
}

#[cfg(test)]
const TYPE_KEYWORDS: &[&str] = &[
    "void",
    "_Bool",
    "char",
    "short",
    "int",
    "long",
    "float",
    "double",
    "signed",
    "unsigned",
    "__signed",
    "__signed__",
    "struct",
    "union",
    "enum",
    "typedef",
    "extern",
    "static",
    "auto",
    "register",
    "const",
    "volatile",
    "restrict",
    "__restrict",
    "__restrict__",
    "__const",
    "__volatile",
    "__volatile__",
    "inline",
    "__inline",
    "__inline__",
    "_Noreturn",
    "_Thread_local",
    "__thread",
    "_Alignas",
    "__attribute__",
    "__attribute",
    "_Nullable",
    "_Nonnull",
    "_Null_unspecified",
    "__nullable",
    "__nonnull",
    "__int128",
    "__int128_t",
    "__uint128_t",
    "_Float16",
    "__float128",
    "__builtin_va_list",
    "__SVFloat32_t",
    "__SVFloat64_t",
    "__SVBool_t",
    "typeof",
    "__typeof__",
    "__typeof",
    "__auto_type",
    "_Complex",
    "_Atomic",
];

/// Qualifiers that change nothing for code generation.
const IGNORED_QUALIFIERS: &[&str] = &[
    "_Nullable",
    "_Nonnull",
    "_Null_unspecified",
    "__nullable",
    "__nonnull",
];

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, path: &str, target: Target, mut text: Strings<'a>) -> Self {
        let lexer = Lexer::new(src, path, &mut text);
        let arena = text.arena();
        let mut p = Self {
            lexer,
            toks: Vec::new(),
            lexed_all: false,
            lex_error: None,
            pinned: 0,
            text,
            pos: 0,
            scopes: vec![Scope::default()],
            names: HashMap::default(),
            arena,
            nodes: arena,
            type_nodes: TypeNodes::new(arena),
            structs: Vec::new(),
            typedefs: Vec::new(),
            enums: Vec::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            strings: Vec::new(),
            assembly: Vec::new(),
            alias_names: Vec::new(),
            aliases: HashMap::default(),
            literal_ids: [HashMap::default(), HashMap::default()],
            stmt_bufs: Vec::new(),
            expr_bufs: Vec::new(),
            fs: None,
            external_def: HashSet::default(),
            vla_allowed: false,
            in_parameter_list: false,
            last_param_function: None,
            last_param_scope: None,
            last_param_bindings: Vec::new(),
            last_param_state: None,
            last_param_ids: Vec::new(),
            last_param_star: false,
            parameter_arrays: Vec::new(),
            parameter_star: false,
            last_params: Vec::new(),
            unique: 0,
            target,
            c_layout: if target == Target::Wasm32UnknownUnknown {
                CDataLayout::WASM32
            } else {
                CDataLayout::NATIVE_LP64
            },
            va_list_tag: None,
        };
        p.fill();
        p
    }

    /// Private model seam for declaration tests and the separately validated
    /// integer-only lowering checkpoint. Never pass its AST to a native backend;
    /// production construction selects a model from the explicit target.
    #[cfg(test)]
    pub(crate) fn declarations_with_layout(
        src: &'a str,
        target: Target,
        text: Strings<'a>,
        layout: CDataLayout,
    ) -> R<Self> {
        let mut p = Self::new(src, "layout-declarations.c", target, text);
        p.c_layout = layout;
        p.translation_unit()?;
        Ok(p)
    }

    /// The file names line markers mentioned; a `Loc::file` indexes them.
    pub fn take_files(&mut self) -> Vec<&'a str> {
        std::mem::take(&mut self.lexer.files)
    }

    pub(crate) fn require_native_layout(&self, loc: Loc, subject: &str) -> R<()> {
        if self.c_layout != CDataLayout::NATIVE_LP64 {
            return Err(Error::new(
                loc,
                format!("{subject} requires the native C layout"),
            ));
        }
        Ok(())
    }

    /// `__builtin_va_list`. Apple's is a pointer to the next argument.
    /// SysV's is `struct __va_list_tag[1]`: the offsets of the next
    /// general and vector register in the callee's register save area,
    /// the next stack argument, and the save area. Being an array, it
    /// decays to a pointer when passed, and a `va_list` parameter is one.
    fn va_list(&mut self) -> Ty<'a> {
        if self.target == Target::Wasm32UnknownUnknown {
            // Declaration-only: do not borrow a native variadic representation.
            return Ty::Opaque(&OpaqueTy {
                name: "__builtin_va_list",
                size: 4,
                align: 4,
            });
        }
        if self.target == Target::Aarch64AppleDarwin {
            return self.ptr_type(crate::types::CHAR);
        }
        let id = *self.va_list_tag.get_or_insert_with(|| {
            let member = |name: &'static str, ty: Ty<'a>, offset| Member {
                nonstring: false,
                align: Types::new(&self.structs, self.c_layout).align(&ty),
                name: Some(name.into()),
                ty,
                sugar: Sugar::Plain,
                offset,
                bits: None,
            };
            let void_ptr = Ty::Ptr(&Ty::Void);
            self.structs.push(StructDef {
                transparent_union: false,
                tag: Some("__va_list_tag".into()),
                is_union: false,
                members: if self.target == Target::Aarch64LinuxGnu {
                    vec![
                        member("__stack", void_ptr, 0),
                        member("__gr_top", void_ptr, 8),
                        member("__vr_top", void_ptr, 16),
                        member("__gr_offs", crate::types::INT, 24),
                        member("__vr_offs", crate::types::INT, 28),
                    ]
                } else {
                    vec![
                        member("gp_offset", crate::types::UINT, 0),
                        member("fp_offset", crate::types::UINT, 4),
                        member("overflow_arg_area", void_ptr, 8),
                        member("reg_save_area", void_ptr, 16),
                    ]
                },
                size: if self.target == Target::Aarch64LinuxGnu {
                    32
                } else {
                    24
                },
                align: 8,
                complete: true,
            });
            self.structs.len() - 1
        });
        let elem = Ty::Struct(id);
        if self.target == Target::Aarch64LinuxGnu {
            elem
        } else {
            self.array_type(elem, Some(1))
        }
    }

    // ---- tokens ----

    pub(crate) fn peek(&self) -> Tok<'a> {
        self.toks[self.pos].tok
    }

    pub(crate) fn peek_at(&self, n: usize) -> Tok<'a> {
        self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }

    pub(crate) fn loc(&self) -> Loc {
        self.toks[self.pos].loc
    }

    pub(crate) fn advance(&mut self) -> Tok<'a> {
        let tok = self.toks[self.pos].tok;
        self.bump();
        tok
    }

    /// Moves to the next token, staying on `Eof`. At least the token after
    /// the current one is always lexed.
    #[inline]
    fn bump(&mut self) {
        // Statement attributes inspect through the fourth token. Keep that
        // lookahead valid even when the lexer refills its token chunk.
        if self.pos + 4 < self.toks.len() {
            self.pos += 1;
        } else {
            self.bump_slow();
        }
    }

    #[cold]
    fn bump_slow(&mut self) {
        self.fill();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
    }

    /// Lexes the next tokens. A lexical error ends the input there, and
    /// is reported in place of whatever the parser makes of it.
    fn fill(&mut self) {
        const CHUNK: usize = 256;
        if self.lexed_all {
            return;
        }
        self.lexed_all = match self.lexer.tokens(&mut self.text, &mut self.toks, CHUNK) {
            Ok(eof) => eof,
            Err(e) => {
                self.toks.push(Token {
                    tok: Tok::Eof,
                    loc: e.loc,
                    pack: 0,
                });
                self.lex_error = Some(e);
                true
            }
        };
    }

    /// Drops the tokens before the current one, between declarations,
    /// statements and initializers, unless a position is saved.
    pub(crate) fn release_tokens(&mut self) {
        if self.pos >= 4096 && self.pinned == 0 {
            self.toks.drain(..self.pos);
            self.pos = 0;
        }
    }

    pub(crate) fn pos_save(&self) -> usize {
        self.pos
    }

    pub(crate) fn pos_restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    // Inlined, so each comparison is with a known spelling and needs no
    // general string comparison.
    #[inline]
    pub(crate) fn is(&self, p: &str) -> bool {
        match self.peek() {
            Tok::Punct(q) => q == p,
            Tok::Ident(s) => s == p,
            _ => false,
        }
    }

    #[inline]
    pub(crate) fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    #[inline]
    pub(crate) fn expect(&mut self, p: &str) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.expected(p))
        }
    }

    #[cold]
    fn expected(&self, p: &str) -> Error {
        self.unexpected(&format!("`{p}`"))
    }

    pub(crate) fn unexpected(&self, wanted: &str) -> Error {
        let found = match self.peek() {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Punct(p) => format!("`{p}`"),
            Tok::Int { value, .. } => format!("`{value}`"),
            Tok::Float { value, .. } | Tok::Imaginary { value, .. } => format!("`{value}`"),
            Tok::Char(_) => "a character constant".into(),
            Tok::BadNumber(text) => format!("invalid number `{text}`"),
            Tok::Str { .. } => "a string".into(),
            Tok::Eof => "the end of the file".into(),
        };
        Error::new(self.loc(), format!("expected {wanted}, found {found}"))
    }

    pub(crate) fn ident(&mut self) -> R<&'a str> {
        match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let s = s;
                self.advance();
                Ok(s)
            }
            _ => Err(self.unexpected("an identifier")),
        }
    }

    /// Skips tokens up to and including the `)` matching the `(` just
    /// consumed.
    fn skip_parens(&mut self) -> R<()> {
        let mut depth = 1;
        while depth > 0 {
            match self.advance() {
                Tok::Punct("(") => depth += 1,
                Tok::Punct(")") => depth -= 1,
                Tok::Eof => return Err(self.unexpected("`)`")),
                _ => {}
            }
        }
        Ok(())
    }

    // ---- scopes ----

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    pub(crate) fn pop_scope(&mut self) {
        let depth = self.scopes.len() - 1;
        for name in self.scopes.pop().unwrap().names.into_iter().rev() {
            let key = self.name_key(name);
            let bindings = self.names.get_mut(&key).unwrap();
            debug_assert_eq!(bindings.last().unwrap().0, depth);
            bindings.pop();
            if bindings.is_empty() {
                self.names.remove(&key);
            }
        }
    }

    /// Opens a block scope in a function, giving its index.
    pub(crate) fn push_block(&mut self) -> usize {
        self.push_scope();
        let fs = self.fs.as_mut().unwrap();
        fs.scopes.push(fs.scope);
        let id = fs.scopes.len() - 1;
        fs.scope = Some(id);
        id
    }

    pub(crate) fn pop_block(&mut self) {
        self.pop_scope();
        let fs = self.fs.as_mut().unwrap();
        fs.scope = fs.scopes[fs.scope.unwrap()];
    }

    /// The scope tables' key for interned `name`: interning gives each
    /// spelling one address.
    fn name_key(&self, name: &str) -> usize {
        debug_assert!(
            self.text.get(name).is_some_and(|n| std::ptr::eq(n, name)),
            "`{name}` is not interned"
        );
        name.as_ptr() as usize
    }

    fn lookup(&self, name: &str) -> Option<&Sym> {
        self.names
            .get(&self.name_key(name))
            .and_then(|bindings| bindings.last())
            .map(|(_, sym)| sym)
    }

    fn bind(&mut self, name: &'a str, sym: Sym) {
        self.bind_at(self.scopes.len() - 1, name, sym);
    }

    fn bind_at(&mut self, depth: usize, name: &'a str, sym: Sym) {
        let bindings = self.names.entry(self.name_key(name)).or_default();
        match bindings.binary_search_by_key(&depth, |&(depth, _)| depth) {
            Ok(i) => bindings[i].1 = sym,
            Err(i) => {
                bindings.insert(i, (depth, sym));
                self.scopes[depth].names.push(name);
            }
        }
    }

    pub(crate) fn type_ref(&mut self, ty: Ty<'a>) -> &'a Ty<'a> {
        self.type_nodes.node(ty)
    }

    pub(crate) fn ptr_type(&mut self, ty: Ty<'a>) -> Ty<'a> {
        Ty::Ptr(self.type_ref(ty))
    }

    /// An expression node, in the arena.
    pub(crate) fn node(&self, e: Expr<'a>) -> &'a Expr<'a> {
        self.nodes.alloc(e)
    }

    pub(crate) fn stmt_node(&self, s: Stmt<'a>) -> &'a Stmt<'a> {
        self.nodes.alloc(s)
    }

    pub(crate) fn init_node(&self, init: Init<'a>) -> &'a Init<'a> {
        self.nodes.alloc(init)
    }

    /// A copy of `items` (parts of the tree), with the nodes.
    pub(crate) fn slice<T: Copy>(&self, items: &[T]) -> &'a [T] {
        self.nodes.alloc_slice_copy(items)
    }

    pub(crate) fn array_type(&self, elem: Ty<'a>, len: Option<u64>) -> Ty<'a> {
        Ty::Array(self.arena.alloc(ArrayTy {
            elem,
            len,
            vla: None,
        }))
    }

    pub(crate) fn sugar_ref(&self, sugar: Sugar<'a>) -> &'a Sugar<'a> {
        match sugar {
            Sugar::Plain => &PLAIN_SUGAR,
            other => self.arena.alloc(other),
        }
    }

    /// `Plain` when nothing is known, else `wrap` of `sugar`.
    pub(crate) fn wrap_sugar(
        &self,
        sugar: Sugar<'a>,
        wrap: fn(&'a Sugar<'a>) -> Sugar<'a>,
    ) -> Sugar<'a> {
        sugar.wrap(wrap, self.arena)
    }

    pub(crate) fn types(&self) -> Types<'_> {
        Types::new(&self.structs, self.c_layout)
    }

    pub(crate) fn size_of(&self, ty: &Ty<'a>) -> u64 {
        self.types().size(ty)
    }

    pub(crate) fn align_of(&self, ty: &Ty<'a>) -> u64 {
        self.types().align(ty)
    }

    pub(crate) fn type_alignment(&self, ty: &Ty<'a>, sugar: &Sugar<'a>) -> u64 {
        sugar
            .alignment(&self.typedefs)
            .unwrap_or_else(|| self.align_of(ty))
    }

    fn object_alignment(&self, ty: &Ty<'a>, sugar: &Sugar<'a>, attrs: &Attrs, loc: Loc) -> R<u64> {
        let natural = self.type_alignment(ty, sugar);
        if let Some(a) = attrs.alignas
            && a != 0
            && a < natural
        {
            return Err(Error::new(
                loc,
                "`_Alignas` cannot weaken natural alignment",
            ));
        }
        Ok(natural.max(attrs.alignment().unwrap_or(1)))
    }

    pub(crate) fn expr_alignment(&self, e: &Expr<'a>) -> u64 {
        match &e.kind {
            ExprKind::Local(id) | ExprKind::CompoundLit(id, _) => {
                self.fs.as_ref().unwrap().locals[*id].align
            }
            ExprKind::Global(g) => self.globals[*g].align,
            ExprKind::Member { align, .. } => *align,
            _ => self.type_alignment(&e.ty, &e.sugar),
        }
    }

    pub(crate) fn is_typedef(&self, name: &str) -> bool {
        matches!(self.lookup(name), Some(Sym::Typedef(_)))
    }

    /// Whether the next token starts a type name or declaration.
    pub(crate) fn is_typename(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => {
                is_type_keyword(s)
                    || self.is_typedef(s)
                    // Recognize the unsupported builtin only when no header
                    // typedef or ordinary identifier supplies this spelling.
                    || (s == "_Float32" && self.lookup(s).is_none())
            }
            _ => false,
        }
    }

    /// The value of an identifier in an expression.
    pub(crate) fn lookup_value(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        match self.lookup(name).cloned() {
            Some(Sym::Local(id)) => {
                let ty = self.fs.as_ref().unwrap().locals[id].ty;
                Ok(Expr {
                    kind: ExprKind::Local(id),
                    sugar: self.fs.as_ref().unwrap().locals[id].sugar,
                    ty,
                    loc,
                })
            }
            Some(Sym::Global(g)) => Ok(Expr {
                kind: ExprKind::Global(g),
                sugar: self.globals[g].sugar,
                ty: self.globals[g].ty,
                loc,
            }),
            Some(Sym::StackPointer) => Ok(Expr {
                kind: ExprKind::Builtin(Builtin::StackPointer, &[]),
                sugar: Sugar::Plain,
                ty: ULONG,
                loc,
            }),
            Some(Sym::Enum(v, kind)) => Ok(Expr {
                kind: ExprKind::Int(v),
                sugar: Sugar::Plain,
                ty: Ty::Int(kind),
                loc,
            }),
            Some(Sym::Typedef(_)) => Err(Error::new(loc, format!("`{name}` is a type"))),
            None => Err(Error::new(loc, format!("`{name}` is not declared"))),
        }
    }

    pub(crate) fn mark_in_memory(&mut self, id: LocalId) {
        self.fs.as_mut().unwrap().locals[id].in_memory = true;
    }

    pub(crate) fn new_local(
        &mut self,
        name: &'a str,
        ty: Ty<'a>,
        sugar: Sugar<'a>,
        volatile: bool,
    ) -> LocalId {
        let in_memory =
            volatile || sugar.is_volatile(&self.typedefs) || !ty.is_scalar() || ty.is_complex();
        let align = self.type_alignment(&ty, &sugar);
        let fs = self.fs.as_mut().unwrap();
        fs.locals.push(Local {
            asm_register: None,
            nonstring: false,
            name: name,
            ty,
            sugar,
            align,
            scope: fs.scope,
            in_memory,
            compound_literal: false,
            cleanup: None,
        });
        let id = fs.locals.len() - 1;
        self.bind(name, Sym::Local(id));
        id
    }

    /// A made-up global name, unique in this file.
    pub(crate) fn unique_name(&mut self, base: &str) -> &'a str {
        self.unique += 1;
        self.text.intern(&format!("{base}.{}", self.unique))
    }

    /// Adds an unnamed object with static storage, like a file-scope
    /// compound literal.
    pub(crate) fn add_static(
        &mut self,
        name: &'a str,
        ty: Ty<'a>,
        init: Init<'a>,
        is_const: bool,
    ) -> usize {
        let align = self.align_of(&ty);
        self.globals.push(Global {
            gnu_inline: false,
            gnu_inline_only: false,
            nonstring: false,
            symbol: name,
            name,
            ty,
            sugar: Sugar::Plain,
            align,
            internal: true,
            defined: true,
            compound_literal: true,
            init: Some(init),
            is_const,
            thread_local: false,
            returns_twice: false,
            incompatible_return: false,
            function_attrs: keel_ir::FunctionAttrs::default(),
            visibility: None,
            weak: None,
            lifecycle: None,
            used: false,
            retain: None,
            section: None,
            func: None,
        });
        self.globals.len() - 1
    }

    pub(crate) fn add_string(&mut self, bytes: Vec<u8>, wide: bool) -> usize {
        let ids = &mut self.literal_ids[usize::from(wide)];
        if let Some(&i) = ids.get(bytes.as_slice()) {
            return i;
        }
        let bytes = self.text.bytes(&bytes);
        let i = self.strings.len();
        ids.insert(bytes, i);
        self.strings.push(StrLit { bytes, wide });
        i
    }

    // ---- translation unit ----

    pub fn translation_unit(&mut self) -> R<()> {
        let parsed = self.declarations();
        // A lexical error anywhere comes first, as if the whole input had
        // been lexed before parsing.
        if parsed.is_err() && !self.lexed_all {
            loop {
                match self.lexer.token(&mut self.text) {
                    Ok(t) if t.tok == Tok::Eof => break,
                    Ok(_) => {}
                    Err(e) => {
                        self.lex_error = Some(e);
                        break;
                    }
                }
            }
        }
        self.toks = Vec::new();
        if let Some(e) = self.lex_error.take() {
            return Err(e);
        }
        parsed?;
        // Until now `internal` records explicit static linkage only. C99
        // inline-only emission depends on all file-scope declarations in this
        // unit, including declarations that follow the definition.
        for f in &self.funcs {
            if self.globals[f.global].gnu_inline_only {
                let global = &self.globals[f.global];
                if global.internal || global.weak.is_some() || global.visibility.is_some_and(Visibility::hidden)
                    || global.used || global.retain.is_some() || global.lifecycle.is_some()
                {
                    return Err(Error::new(f.loc, "GNU extern-inline bodies with binding or retention attributes are not supported"));
                }
                self.globals[f.global].internal = true;
            } else if !self.external_def.contains(&f.global) {
                self.globals[f.global].internal = true;
            }
        }
        self.resolve_aliases()?;
        Ok(())
    }

    fn declarations(&mut self) -> R<()> {
        while self.peek() != Tok::Eof {
            if self.is("asm") || self.is("__asm") || self.is("__asm__") {
                self.asm_declaration()?;
            } else if !self.eat(";") {
                // File-scope declarations initialize no locals.
                self.declaration(true, &mut Vec::new())?;
            }
            self.release_tokens();
        }
        Ok(())
    }

    /// A declaration, at file scope or in a block. Adds the statements
    /// initializing the locals it declares to `out`.
    pub(crate) fn declaration(&mut self, file_scope: bool, out: &mut Vec<Stmt<'a>>) -> R<()> {
        if self.static_assert()? {
            return Ok(());
        }
        let spec = self.decl_spec()?;
        if file_scope && spec.register_storage && self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(
                self.loc(),
                "global register variables require x86-64 Linux",
            ));
        }
        if spec.infer && spec.storage == Storage::Typedef {
            return Err(Error::new(
                self.loc(),
                "`__auto_type` cannot declare a typedef",
            ));
        }
        if !spec.infer && self.eat(";") {
            if spec.attrs.nonstring {
                return Err(Error::new(
                    self.loc(),
                    "nonstring requires a character array or pointer declaration",
                ));
            }
            spec.attrs.no_symbol_attrs()?;
            return Ok(());
        }
        // A block-scope `static` or `extern` declares a global, whose
        // initializer must outlive the function's body arena.
        let nodes = self.nodes;
        if !file_scope && matches!(spec.storage, Storage::Static | Storage::Extern) {
            self.nodes = self.arena;
        }
        let declared = self.declarators(file_scope, &spec, out);
        self.nodes = nodes;
        declared
    }

    /// A buffer for the statements of a block, from those recycled.
    fn stmt_buf(&mut self) -> Vec<Stmt<'a>> {
        self.stmt_bufs.pop().unwrap_or_default()
    }

    /// The statements in `buf`, with the nodes; recycles `buf`.
    fn finish_stmts(&mut self, mut buf: Vec<Stmt<'a>>) -> &'a [Stmt<'a>] {
        let stmts = self.slice(&buf);
        buf.clear();
        self.stmt_bufs.push(buf);
        stmts
    }

    /// The declarators of a declaration with specifiers `spec`, through
    /// the `;`.
    fn declarators(
        &mut self,
        file_scope: bool,
        spec: &DeclSpec<'a>,
        out: &mut Vec<Stmt<'a>>,
    ) -> R<()> {
        loop {
            let loc = self.loc();
            self.last_param_function = None;
            let bound_start = self.vm_bound_mark();
            let mut d = if spec.infer {
                let name = self.ident()?;
                Declarator {
                    name: Some((name, loc)),
                    ty: spec.ty,
                    sugar: spec.sugar,
                    is_const: false,
                    is_volatile: false,
                    attrs: Attrs::default(),
                }
            } else {
                let outer = self.vla_allowed;
                self.vla_allowed =
                    !file_scope && matches!(spec.storage, Storage::None | Storage::Typedef);
                let result = self.declarator_sugar(spec.ty, spec.sugar);
                self.vla_allowed = outer;
                result?
            };
            if d.ty.variably_modified()
                && (file_scope || !matches!(spec.storage, Storage::None | Storage::Typedef))
            {
                return Err(Error::new(
                    loc,
                    "variably modified types require automatic block storage in this milestone",
                ));
            }
            if d.ty.variably_modified() {
                self.fs.as_mut().unwrap().has_vm = true;
            }
            // Evaluate only bounds introduced here. A VM typedef reuses its
            // captured locals, even when its source variables have changed.
            let bounds = self.take_vm_bounds(bound_start);
            for (id, bound) in bounds {
                out.push(Stmt::Init(
                    id,
                    Init {
                        items: self.slice(&[InitItem::Expr {
                            offset: 0,
                            expr: self.node(bound),
                            bits: None,
                        }]),
                    },
                    loc,
                ));
            }
            let mut attrs = spec.attrs.clone();
            attrs.merge(d.attrs);
            let label = self.asm_label_and_attrs(&mut attrs)?;
            d.ty = self.apply_type_attrs(d.ty, &mut attrs)?;
            d.attrs = attrs.clone();
            let Some((name, name_loc)) = d.name else {
                return Err(Error::new(loc, "declaration needs a name"));
            };
            if file_scope
                && !spec.register_storage
                && matches!(self.lookup(name), Some(Sym::StackPointer))
            {
                return Err(Error::new(
                    name_loc,
                    "stack register variable cannot be redeclared as an ordinary symbol",
                ));
            }
            if self.names.get(&self.name_key(name)).is_some_and(|b|
                matches!(b.last(), Some((depth, Sym::Local(_))) if *depth == self.scopes.len() - 1)) {
                return Err(Error::new(name_loc, "duplicate local or parameter name"));
            }
            let inferred_init = if spec.infer {
                if !self.eat("=") {
                    return Err(Error::new(
                        self.loc(),
                        "`__auto_type` needs an initializer and a plain identifier",
                    ));
                }
                // Unlike ordinary declarations, the name is not in scope
                // until after its initializer has been parsed.
                let expr = self.assign()?;
                if matches!(expr.kind, ExprKind::Member { bits: Some(_), .. }) {
                    return Err(Error::new(
                        expr.loc,
                        "`__auto_type` cannot infer from a bit-field",
                    ));
                }
                let expr = self.rvalue(expr);
                if self.incomplete(&expr.ty) {
                    return Err(Error::new(
                        expr.loc,
                        "`__auto_type` initializer has an incomplete type",
                    ));
                }
                d.ty = if matches!(spec.ty, Ty::Atomic(_)) {
                    self.atomic_type(expr.ty, name_loc)?
                } else {
                    expr.ty
                };
                // Inference drops top-level object qualifiers but retains
                // qualified pointees. Explicit qualifiers remain outermost.
                let inferred = *expr.sugar.unqualified(&self.typedefs);
                d.sugar = if spec.is_volatile {
                    Sugar::Volatile(self.sugar_ref(inferred))
                } else {
                    inferred
                };
                if spec.is_const {
                    d.sugar = Sugar::Const(self.sugar_ref(d.sugar));
                }
                Some(Init {
                    items: self.slice(&[InitItem::Expr {
                        offset: 0,
                        expr: self.node(expr),
                        bits: None,
                    }]),
                })
            } else {
                None
            };
            if d.ty.variably_modified()
                && (file_scope || !matches!(spec.storage, Storage::None | Storage::Typedef))
            {
                return Err(Error::new(
                    loc,
                    "variably modified types require automatic block storage in this milestone",
                ));
            }
            // The specifiers' qualifiers are on the object only if the
            // declarator adds nothing but arrays: `const char *a[2]` is an
            // array of mutable pointers.
            let mut elem = &d.ty;
            while let Ty::Array(inner) = elem {
                elem = &inner.elem;
            }
            let base = spec.infer
                || *elem == spec.ty
                || ((elem.is_arith() || elem.is_vector())
                    && (spec.ty.is_arith() || spec.ty.is_vector()));
            if let Some(what) = self.types().unsupported_value(&d.ty)
                && spec.storage != Storage::Typedef
                && (spec.storage != Storage::Extern || self.is("="))
            {
                return Err(Error::new(
                    name_loc,
                    format!("`{name}`: objects of type `{what}` are not supported"),
                ));
            }
            let is_const = d.is_const || (spec.is_const && base);
            let is_volatile = d.is_volatile || (spec.is_volatile && base);
            if file_scope && spec.register_storage {
                self.stack_pointer_declaration(spec, &d, label)?;
            } else if spec.storage == Storage::Typedef {
                if attrs.nonstring {
                    return Err(Error::new(name_loc, "nonstring cannot annotate a typedef"));
                }
                attrs.no_symbol_attrs()?;
                if attrs.alignas.is_some() {
                    return Err(Error::new(name_loc, "`_Alignas` cannot apply to a typedef"));
                }
                if let Some((depth, Sym::Typedef(id))) = self
                    .names
                    .get(&self.name_key(name))
                    .and_then(|bindings| bindings.last())
                    && *depth == self.scopes.len() - 1
                    && self.typedefs[*id].ty.numeric_redeclaration_conflict(&d.ty)
                {
                    return Err(Error::new(
                        name_loc,
                        "incompatible types in typedef redeclaration",
                    ));
                }
                self.typedefs.push(TypedefDef {
                    name: name,
                    ty: d.ty,
                    sugar: d.sugar,
                    align: attrs.aligned,
                });
                self.bind(name, Sym::Typedef(self.typedefs.len() - 1));
            } else if let Ty::Func(mut fty) = d.ty {
                if spec.thread_local {
                    return Err(Error::new(
                        name_loc,
                        "thread-local storage requires an object",
                    ));
                }
                if !fty.prototyped && !fty.old_style && file_scope && self.is("{") {
                    let direct = self
                        .last_param_function
                        .is_some_and(|f| std::ptr::eq(f, fty));
                    fty = self.arena.alloc(FuncTy {
                        old_style: true,
                        ..*fty
                    });
                    if direct {
                        self.last_param_function = Some(fty);
                    }
                }
                if fty.old_style {
                    if !file_scope
                        || !self
                            .last_param_function
                            .is_some_and(|f| std::ptr::eq(f, fty))
                    {
                        return Err(Error::new(
                            name_loc,
                            "identifier list requires a function definition",
                        ));
                    }
                    fty = self.old_style_parameters(fty)?;
                } else if file_scope && self.is_typename() {
                    return Err(Error::new(
                        name_loc,
                        "parameter declaration list requires an identifier-list definition",
                    ));
                }
                self.check_nonstring(d.ty, &attrs, name_loc)?;
                if attrs.alignas.is_some() {
                    return Err(Error::new(
                        name_loc,
                        "`_Alignas` cannot apply to a function",
                    ));
                }
                let g = self
                    .declare_function(&name, label, fty, &spec, &attrs, file_scope)
                    .map_err(|message| Error::new(name_loc, message))?;
                self.globals[g].sugar = d.sugar;
                self.symbol_attrs(g, &attrs)?;
                self.function_alias(g, attrs, file_scope)?;
                if file_scope && self.is("{") {
                    if fty.microsoft_x64 {
                        return Err(Error::new(name_loc, "ms_abi definitions require unsupported Microsoft x64 ABI lowering"));
                    }
                    self.globals[g].gnu_inline_only = self.globals[g].gnu_inline
                        && spec.inline && spec.storage == Storage::Extern;
                    self.function_body(g, fty, name_loc)?;
                    return Ok(());
                }
            } else if !file_scope && spec.storage == Storage::None {
                let asm_register = label
                    .map(|label| self.local_register_declaration(spec, &d, label))
                    .transpose()?;
                let cleanup = attrs.cleanup.take();
                if let Some((_, loc)) = cleanup
                    && spec.thread_local
                {
                    return Err(Error::new(
                        loc,
                        "cleanup applies only to automatic local objects",
                    ));
                }
                if spec.thread_local {
                    return Err(Error::new(
                        name_loc,
                        "block-scope thread-local storage requires static or extern",
                    ));
                }
                if let Some(loc) = attrs.cleanup_conflict {
                    return Err(Error::new(
                        loc,
                        "multiple cleanup attributes on one declaration",
                    ));
                }
                attrs.no_symbol_attrs()?;
                if self
                    .names
                    .get(&self.name_key(name))
                    .and_then(|b| b.last())
                    .is_some_and(|(depth, _)| *depth == self.scopes.len() - 1)
                {
                    return Err(Error::new(name_loc, "redeclaration of automatic local"));
                }
                // In scope from the end of its declarator: `T *p =
                // malloc(sizeof *p)` refers to itself.
                self.check_nonstring(d.ty, &attrs, name_loc)?;
                let id = self.new_local(name, d.ty, d.sugar, is_volatile);
                self.fs.as_mut().unwrap().locals[id].asm_register = asm_register;
                self.fs.as_mut().unwrap().locals[id].nonstring = attrs.nonstring;
                let align = self.object_alignment(&d.ty, &d.sugar, &attrs, name_loc)?;
                self.fs.as_mut().unwrap().locals[id].align = align;
                let mut ty = d.ty;
                let mut init = inferred_init;
                if ty.runtime_sized() && self.is("=") {
                    return Err(Error::new(
                        name_loc,
                        "variable-length arrays cannot have initializers",
                    ));
                }
                if init.is_none() && self.eat("=") {
                    let (i, complete) = self.initializer(&ty)?;
                    ty = complete;
                    init = Some(i);
                }
                let incomplete = match &ty {
                    Ty::Array(ArrayTy {
                        len: None,
                        vla: None,
                        ..
                    })
                    | Ty::Void => true,
                    Ty::Struct(id) => !self.structs[*id].complete,
                    _ => false,
                };
                if incomplete {
                    return Err(Error::new(
                        name_loc,
                        format!("`{name}` has an incomplete type"),
                    ));
                }
                let local = &mut self.fs.as_mut().unwrap().locals[id];
                local.in_memory |= !ty.is_scalar() || ty.is_complex();
                local.ty = ty;
                if ty.runtime_sized() && align > 16 {
                    return Err(Error::new(
                        name_loc,
                        "VLA alignment greater than 16 bytes is not supported yet",
                    ));
                }
                if let Some((g, loc)) = cleanup {
                    let Ty::Func(fty) = self.globals[g].ty else {
                        unreachable!()
                    };
                    if !fty.prototyped
                        || fty.variadic
                        || fty.params.len() != 1
                        || !matches!(fty.params[0], Ty::Ptr(t) if *t == Ty::Void || t.compatible(&ty))
                    {
                        return Err(Error::new(
                            loc,
                            "cleanup function must have a prototype with exactly one compatible object pointer parameter",
                        ));
                    }
                    if self.globals[g].incompatible_return {
                        return Err(Error::new(
                            loc,
                            "cleanup function has incompatible return-type redeclarations",
                        ));
                    }
                    if fty.ret != Ty::Void && self.incomplete(&fty.ret) {
                        return Err(Error::new(
                            loc,
                            "cleanup function returns an incomplete type",
                        ));
                    }
                    if nonlocal_exit(&self.globals[g]) || self.globals[g].function_attrs.noreturn {
                        return Err(Error::new(loc, "cleanup callback must return normally"));
                    }
                    let local = &mut self.fs.as_mut().unwrap().locals[id];
                    local.in_memory = true;
                    local.cleanup = cleanup;
                }
                out.push(match init {
                    Some(init) => Stmt::Init(id, init, loc),
                    None => Stmt::Declare(id),
                });
            } else {
                self.object_declaration(&spec, d, label, is_const, file_scope, inferred_init)?;
            }
            if spec.infer && self.is(",") {
                return Err(Error::new(
                    self.loc(),
                    "`__auto_type` declares only one variable",
                ));
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")?;
        Ok(())
    }

    fn static_assert(&mut self) -> R<bool> {
        if !self.eat("_Static_assert") {
            return Ok(false);
        }
        let loc = self.loc();
        self.expect("(")?;
        let value = self.const_int()?;
        if self.eat(",") {
            if !matches!(self.peek(), Tok::Str { .. }) {
                return Err(self.unexpected("a static assertion string literal"));
            }
            while matches!(self.peek(), Tok::Str { .. }) {
                self.advance();
            }
        }
        self.expect(")")?;
        self.expect(";")?;
        if value == 0 {
            return Err(Error::new(loc, "static assertion failed"));
        }
        Ok(true)
    }

    /// A file-scope object, a block-scope `extern`, or a `static` local.
    fn object_declaration(
        &mut self,
        spec: &DeclSpec<'a>,
        d: Declarator<'a>,
        label: Option<&'a str>,
        is_const: bool,
        file_scope: bool,
        inferred_init: Option<Init<'a>>,
    ) -> R<()> {
        let (name, name_loc) = *d.name.as_ref().unwrap();
        if d.attrs.alias.is_some() {
            return Err(Error::new(
                name_loc,
                "alias requires a file-scope function declaration",
            ));
        }
        self.check_nonstring(d.ty, &d.attrs, name_loc)?;
        let align = self.object_alignment(&d.ty, &d.sugar, &d.attrs, name_loc)?;
        let local_static = !file_scope && spec.storage == Storage::Static;
        let g = if local_static {
            let func = &self.fs.as_ref().unwrap().name;
            let symbol = format!("{func}.{name}");
            let symbol = if self.globals.iter().any(|g| g.symbol == symbol) {
                self.unique_name(&symbol)
            } else {
                self.text.intern(&symbol)
            };
            self.globals.push(Global {
                gnu_inline: false,
                gnu_inline_only: false,
                nonstring: false,
                name: name,
                symbol,
                ty: d.ty,
                sugar: d.sugar,
                align,
                internal: true,
                defined: true,
                compound_literal: false,
                init: None,
                is_const,
                thread_local: spec.thread_local,
                returns_twice: false,
                incompatible_return: false,
                function_attrs: keel_ir::FunctionAttrs::default(),
                visibility: None,
                weak: None,
                lifecycle: None,
                used: false,
                retain: None,
                section: None,
                func: None,
            });
            let g = self.globals.len() - 1;
            self.bind(name, Sym::Global(g));
            g
        } else {
            let previous_count = self.globals.len();
            let g = self
                .global(&name, label, d.ty)
                .map_err(|message| Error::new(d.name.as_ref().unwrap().1, message))?;
            let global = &mut self.globals[g];
            if g < previous_count && global.thread_local != spec.thread_local {
                return Err(Error::new(
                    name_loc,
                    "TLS and non-TLS object redeclarations conflict",
                ));
            }
            global.sugar = d.sugar;
            if spec.storage == Storage::Static {
                global.internal = true;
            }
            global.thread_local |= spec.thread_local;
            global.is_const = is_const;
            if file_scope && spec.storage != Storage::Extern {
                // A tentative definition, unless an initializer follows.
                global.defined = true;
            }
            g
        };
        self.globals[g].nonstring |= d.attrs.nonstring;
        self.symbol_attrs(g, &d.attrs)?;
        self.globals[g].align = self.globals[g].align.max(align);
        if inferred_init.is_some() || self.eat("=") {
            let loc = self.loc();
            if !file_scope && !local_static {
                return Err(Error::new(
                    loc,
                    "a block-scope `extern` cannot be initialized",
                ));
            }
            let ty = self.globals[g].ty;
            let (init, ty) = match inferred_init {
                Some(init) => (init, ty),
                None => self.initializer(&ty)?,
            };
            let init = self.static_initializer(init);
            let global = &mut self.globals[g];
            global.ty = ty;
            global.init = Some(init);
            global.defined = true;
        }
        Ok(())
    }

    /// Finds or adds the file-scope global `name`, merging this
    /// declaration's type into it (a later one may complete an array).
    fn global(
        &mut self,
        name: &str,
        label: Option<&'a str>,
        ty: Ty<'a>,
    ) -> Result<usize, &'static str> {
        if self
            .names
            .get(&self.name_key(name))
            .and_then(|bindings| bindings.first())
            .is_some_and(|(depth, s)| *depth == 0 && matches!(s, Sym::StackPointer))
        {
            return Err("stack register variable cannot be redeclared as an ordinary symbol");
        }
        let existing = self
            .names
            .get(&self.name_key(name))
            .and_then(|bindings| bindings.first())
            .and_then(|(depth, s)| match s {
                Sym::Global(g) if *depth == 0 => Some(*g),
                _ => None,
            });
        let checked = crate::expr::checked_memory_intrinsic(name)
            .or_else(|| label.and_then(crate::expr::checked_memory_intrinsic))
            .or_else(|| {
                existing.and_then(|g| crate::expr::checked_memory_intrinsic(self.globals[g].symbol))
            });
        if let Some(id) = checked
            && !crate::expr::checked_memory_prototype(id, &ty)
        {
            return Err("invalid checked memory builtin prototype");
        }
        let g = match existing {
            Some(g) => {
                let global = &mut self.globals[g];
                if global.ty.numeric_redeclaration_conflict(&ty)
                    || matches!((global.ty, ty), (Ty::Func(a), Ty::Func(b))
                        if !a.parameters_compatible(b))
                {
                    return Err("incompatible types in redeclaration");
                }
                // Ordinary unrelated return-type redeclarations keep their existing
                // permissive policy; cleanup callbacks must use one compatible ABI.
                global.incompatible_return |= matches!((global.ty, ty),
                    (Ty::Func(a), Ty::Func(b)) if !a.ret.compatible(&b.ret));
                let more_complete = match (&global.ty, &ty) {
                    (
                        Ty::Array(ArrayTy { len: None, .. }),
                        Ty::Array(ArrayTy { len: Some(_), .. }),
                    ) => true,
                    (Ty::Func(old), Ty::Func(new)) => {
                        !old.prototyped && (new.prototyped || new.old_style)
                    }
                    _ => false,
                };
                if more_complete {
                    global.ty = ty;
                }
                g
            }
            None => {
                self.globals.push(Global {
                    gnu_inline: false,
                    gnu_inline_only: false,
                    nonstring: false,
                    name: self.text.intern(name),
                    symbol: self.text.intern(name),
                    ty,
                    sugar: Sugar::Plain,
                    align: 1,
                    internal: false,
                    defined: false,
                    compound_literal: false,
                    init: None,
                    is_const: false,
                    thread_local: false,
                    returns_twice: false,
                    incompatible_return: false,
                    function_attrs: keel_ir::FunctionAttrs::default(),
                    visibility: None,
                    weak: None,
                    lifecycle: None,
                    used: false,
                    retain: None,
                    section: None,
                    func: None,
                });
                let g = self.globals.len() - 1;
                let name = self.text.intern(name);
                self.bind_at(0, name, Sym::Global(g));
                g
            }
        };
        if let Some(label) = label {
            self.globals[g].symbol = label;
        }
        // A block-scope declaration is visible in its block only.
        if self.scopes.len() > 1 {
            let name = self.text.intern(name);
            self.bind(name, Sym::Global(g));
        }
        Ok(g)
    }

    fn declare_function(
        &mut self,
        name: &str,
        label: Option<&'a str>,
        fty: &'a FuncTy<'a>,
        spec: &DeclSpec<'a>,
        attrs: &Attrs,
        file_scope: bool,
    ) -> Result<usize, &'static str> {
        let g = self.global(name, label, Ty::Func(fty))?;
        if spec.storage == Storage::Static {
            self.globals[g].internal = true;
        }
        self.globals[g].gnu_inline |= attrs.gnu_inline;
        if file_scope
            && (!spec.inline
                || (spec.storage == Storage::Extern && !self.globals[g].gnu_inline)
                || (self.globals[g].gnu_inline && spec.storage != Storage::Extern))
        {
            self.external_def.insert(g);
        }
        let returns_twice = attrs.returns_twice
            || matches!(
                name,
                "setjmp" | "_setjmp" | "sigsetjmp" | "__sigsetjmp" | "savectx" | "vfork"
            );
        self.globals[g].returns_twice |= returns_twice;
        let mut policies = attrs.function_attrs.clone();
        policies.align = attrs.aligned.map(|a| a as u32);
        policies.inlinehint |= spec.inline;
        policies.noreturn |= attrs.noreturn;
        self.globals[g].function_attrs.merge(policies);
        self.globals[g]
            .function_attrs
            .validate(self.globals[g].returns_twice)?;
        Ok(g)
    }

    /// Merges a declaration's `visibility` and `weak` into those of the
    /// symbol it declares.
    fn symbol_attrs(&mut self, g: usize, attrs: &Attrs) -> R<()> {
        attrs.no_cleanup()?;
        if let Some(loc) = attrs.visibility_conflict {
            return Err(Error::new(loc, "conflicting visibility attributes"));
        }
        if let Some(loc) = attrs.function_policy
            && !matches!(self.globals[g].ty, Ty::Func(_))
        {
            return Err(Error::new(
                loc,
                "function policy attributes require a function declaration",
            ));
        }
        let global = &mut self.globals[g];
        if let Some(loc) = attrs.lifecycle {
            if !matches!(global.ty, Ty::Func(_)) {
                return Err(Error::new(
                    loc,
                    "constructor/destructor requires a function",
                ));
            }
            global.lifecycle.get_or_insert(loc);
        }
        if let Some(loc) = attrs.section_conflict {
            return Err(Error::new(loc, "conflicting section attributes"));
        }
        if let Some((section, loc)) = attrs.section {
            if global.section.is_some_and(|s| s != section) {
                return Err(Error::new(loc, "conflicting section redeclarations"));
            }
            global.section = Some(section);
        }
        global.retain = global.retain.or(attrs.retain);
        global.used |= attrs.used.is_some();
        if matches!(global.ty, Ty::Func(_)) {
            global.function_attrs.used |= attrs.used.is_some();
            global.function_attrs.retain |= attrs.retain.is_some();
            global.function_attrs.section = global.section;
        }
        if let Some((visibility, loc)) = attrs.visibility {
            if global.visibility.is_some_and(|v| v != visibility) {
                return Err(Error::new(
                    loc,
                    format!(
                        "visibility of `{}` does not match a previous declaration",
                        global.name
                    ),
                ));
            }
            global.visibility = Some(visibility);
        }
        if let Some(loc) = attrs.weak {
            global.weak.get_or_insert(loc);
        }
        Ok(())
    }

    /// Bind the declaration list in the retained definition scope. Nested
    /// function declarators may replace `last_param_*`, so own all outer state
    /// until the list is complete, then publish it for `function_body`.
    fn old_style_parameters(&mut self, fty: &'a FuncTy<'a>) -> R<&'a FuncTy<'a>> {
        let mut params = std::mem::take(&mut self.last_params);
        let ids = std::mem::take(&mut self.last_param_ids);
        let scope = self.last_param_scope.take().unwrap();
        let bindings = std::mem::take(&mut self.last_param_bindings);
        let outer = self.fs.take();
        self.fs = self.last_param_state.take();
        self.push_scope();
        self.scopes.last_mut().unwrap().tags = scope.tags;
        self.scopes.last_mut().unwrap().enums = scope.enums;
        for (name, sym) in bindings {
            self.bind(name, sym);
        }
        let mut declared = vec![false; params.len()];
        while !self.is("{") {
            if !self.is_typename() {
                return Err(Error::new(
                    self.loc(),
                    "identifier list requires a function definition and explicit parameter declarations",
                ));
            }
            let tags = self.structs.len();
            let enums = self.enums.len();
            let spec = self.decl_spec()?;
            if spec.implicit_int {
                return Err(Error::new(
                    self.loc(),
                    "old-style parameter needs an explicit type declaration",
                ));
            }
            if self.structs.len() != tags || self.enums.len() != enums {
                return Err(Error::new(
                    self.loc(),
                    "new tags and enumerators in old-style parameter declarations are not supported",
                ));
            }
            if spec.storage != Storage::None
                || spec.thread_local
                || spec.auto_storage
                || spec.inline
                || spec.infer
            {
                return Err(Error::new(
                    self.loc(),
                    "invalid old-style parameter storage class",
                ));
            }
            loop {
                // VM declaration-list bounds have different ordering semantics;
                // do not evaluate or capture them in this fixed-size milestone.
                let declaration_tags = self.structs.len();
                let declaration_enums = self.enums.len();
                let declaration_names: Vec<_> = self
                    .scopes
                    .last()
                    .unwrap()
                    .names
                    .iter()
                    .map(|&name| (name, *self.lookup(name).unwrap()))
                    .collect();
                let mut d = self.declarator_sugar(spec.ty, spec.sugar)?;
                // Nested callback prototypes own their tags and enumerators.
                // New bindings in this scope instead belong to the unsupported
                // declaration-list scope, which must not leak into the body.
                let current = self.scopes.last().unwrap();
                if current
                    .tags
                    .values()
                    .any(|id| id.is_some_and(|id| id >= declaration_tags))
                    || current.enums.values().any(|&id| id >= declaration_enums)
                    || current.names.len() != declaration_names.len()
                    || declaration_names
                        .iter()
                        .any(|(name, sym)| self.lookup(name) != Some(sym))
                {
                    return Err(Error::new(
                        self.loc(),
                        "new tags and enumerators in old-style parameter declarations are not supported",
                    ));
                }
                if let Ty::Array(a) = d.ty
                    && !self.complete_vm_element(&a.elem)
                {
                    return Err(Error::new(
                        self.loc(),
                        "parameter array element must be a complete object type",
                    ));
                }
                let Some((name, ploc)) = d.name else {
                    return Err(Error::new(
                        self.loc(),
                        "old-style parameter declaration needs a name",
                    ));
                };
                let Some(index) = params.iter().position(|p| p.0 == Some(name)) else {
                    return Err(Error::new(
                        ploc,
                        "declaration names no identifier-list parameter",
                    ));
                };
                if declared[index] {
                    return Err(Error::new(
                        ploc,
                        "duplicate old-style parameter declaration",
                    ));
                }
                let mut attrs = spec.attrs.clone();
                attrs.merge(d.attrs);
                self.attributes(&mut attrs)?;
                d.ty = self.apply_type_attrs(d.ty, &mut attrs)?;
                attrs.no_symbol_attrs()?;
                if attrs.alignment().is_some() || attrs.alignas.is_some() {
                    return Err(Error::new(
                        ploc,
                        "alignment attributes on parameters are not supported",
                    ));
                }
                if d.ty.variably_modified() || !self.parameter_arrays.is_empty() {
                    return Err(Error::new(
                        ploc,
                        "VM or qualified array old-style parameters are not supported",
                    ));
                }
                let (ty, sugar) = match d.ty {
                    Ty::Array(a) => (
                        Ty::Ptr(&a.elem),
                        Sugar::Ptr(
                            self.sugar_ref(d.sugar.array_element(&self.typedefs, self.arena)),
                        ),
                    ),
                    Ty::Func(_) => (self.ptr_type(d.ty), self.wrap_sugar(d.sugar, Sugar::Ptr)),
                    ty => (ty, d.sugar),
                };
                let supported = matches!(ty, Ty::Int(k) if k.size() <= 8)
                    || matches!(ty, Ty::Float | Ty::Double | Ty::Ptr(_));
                if !supported || self.incomplete(&ty) {
                    return Err(Error::new(ploc, "unsupported old-style parameter type"));
                }
                if self.eat("=") {
                    return Err(Error::new(ploc, "parameter cannot have an initializer"));
                }
                self.check_nonstring(ty, &attrs, ploc)?;
                let align = self.type_alignment(&ty, &sugar);
                let local = &mut self.fs.as_mut().unwrap().locals[ids[index]];
                local.nonstring = attrs.nonstring;
                local.ty = ty;
                local.sugar = sugar;
                local.align = align;
                local.in_memory = d.is_volatile || sugar.is_volatile(&self.typedefs);
                params[index] = (Some(name), ty, sugar, ploc);
                declared[index] = true;
                self.bind(name, Sym::Local(ids[index]));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        if let Some(index) = declared.iter().position(|d| !d) {
            return Err(Error::new(
                params[index].3,
                "old-style parameter needs an explicit type declaration",
            ));
        }
        let abi: Vec<_> = params.iter().map(|p| p.1.default_promoted()).collect();
        let quals: Vec<_> = params
            .iter()
            .map(|p| self.parameter_qualifiers(p.1, p.2))
            .collect();
        let fty = self.arena.alloc(FuncTy {
            params: self.arena.alloc_slice_copy(&abi),
            param_quals: self.arena.alloc_slice_copy(&quals),
            ..*fty
        });
        let scope = self.scopes.last().unwrap().clone();
        self.last_param_bindings = scope
            .names
            .iter()
            .map(|&name| (name, *self.lookup(name).unwrap()))
            .collect();
        self.last_param_scope = Some(scope);
        self.pop_scope();
        self.last_param_state = self.fs.take();
        self.fs = outer;
        self.last_params = params;
        self.last_param_ids = ids;
        self.last_param_star = false;
        self.last_param_function = Some(fty);
        Ok(fty)
    }

    fn function_body(&mut self, g: usize, fty: &'a FuncTy<'a>, loc: Loc) -> R<()> {
        if self.globals[g].func.is_some() {
            return Err(Error::new(
                loc,
                format!("`{}` is defined twice", self.globals[g].name),
            ));
        }
        if !self
            .last_param_function
            .is_some_and(|f| std::ptr::eq(f, fty))
        {
            return Err(Error::new(
                loc,
                "function definition requires a direct parameter list",
            ));
        }
        let params = std::mem::take(&mut self.last_params);
        let name = self.globals[g].name;
        if self.last_param_star {
            return Err(Error::new(
                loc,
                "VLA star bounds are only allowed in function prototypes",
            ));
        }
        let mut state = self.last_param_state.take().ok_or_else(|| {
            Error::new(loc, "function definition requires a direct parameter list")
        })?;
        state.ret = fty.ret;
        state.name = name;
        let bounds = std::mem::take(&mut state.vla_bounds);
        self.fs = Some(state);
        self.push_scope();
        let scope = self.last_param_scope.take().unwrap();
        self.scopes.last_mut().unwrap().tags = scope.tags;
        self.scopes.last_mut().unwrap().enums = scope.enums;
        for (name, sym) in std::mem::take(&mut self.last_param_bindings) {
            self.bind(name, sym);
        }
        let ids = std::mem::take(&mut self.last_param_ids);
        for ((name, _, _, ploc), &id) in params.into_iter().zip(&ids) {
            let Some(name) = name else {
                return Err(Error::new(ploc, "parameter needs a name"));
            };
            debug_assert_eq!(self.lookup(name), Some(&Sym::Local(id)));
        }
        self.expect("{")?;
        let nodes = BodyArena::new();
        // SAFETY: the body's nodes all end up in its `FuncDef`, which owns
        // the arena. Nothing else keeps them: declarations that outlive the
        // function switch back to `arena` (see `declaration`), and on an
        // error the whole tree is dropped.
        self.nodes = unsafe { nodes.get() };
        let body = self.block_items().map(|body| {
            if bounds.is_empty() {
                return body;
            }
            let mut entry = Vec::with_capacity(bounds.len() + body.len());
            for (id, bound) in bounds {
                entry.push(Stmt::Init(
                    id,
                    Init {
                        items: self.slice(&[InitItem::Expr {
                            offset: 0,
                            expr: self.node(bound),
                            bits: None,
                        }]),
                    },
                    loc,
                ));
            }
            entry.extend_from_slice(body);
            self.slice(&entry)
        });
        self.nodes = self.arena;
        let body = body?;
        self.pop_scope();
        let fs = self.fs.take().unwrap();
        if fs.locals.iter().any(|l| l.cleanup.is_some())
            && let Some((loc, kind)) = fs.cleanup_unsupported_exit
        {
            return Err(Error::new(
                loc,
                format!("{kind} in a function containing cleanup locals is not supported yet"),
            ));
        }
        let def = FuncDef {
            global: g,
            ty: fty,
            params: ids,
            locals: fs.locals,
            has_vm: fs.has_vm,
            direct_calls: fs.direct_calls,
            scopes: fs.scopes,
            label_addrs: fs.label_addrs,
            body,
            loc,
            nodes,
        };
        crate::cleanup_jumps::CleanupJumps::analyze(&def)?;
        self.funcs.push(def);
        let global = &mut self.globals[g];
        global.func = Some(self.funcs.len() - 1);
        global.defined = true;
        Ok(())
    }

    /// `__asm("label")` and attributes after a declarator.
    fn asm_label_and_attrs(&mut self, attrs: &mut Attrs) -> R<Option<&'a str>> {
        let mut label = None;
        loop {
            if self.is("__asm") || self.is("__asm__") || self.is("asm") {
                self.require_native_layout(self.loc(), "asm labels")?;
                self.advance();
                self.expect("(")?;
                let mut text = Vec::new();
                while let Tok::Str { bytes, .. } = self.peek() {
                    text.extend_from_slice(bytes);
                    self.advance();
                }
                self.expect(")")?;
                let text = String::from_utf8_lossy(&text).into_owned();
                // Keel adds the `_` every Darwin symbol starts with; ELF
                // names are as written.
                let name = match self.target {
                    Target::Aarch64AppleDarwin => match text.strip_prefix('_') {
                        Some(name) => name,
                        None => {
                            return Err(Error::new(
                                self.loc(),
                                format!("asm label `{text}` does not start with `_`"),
                            ));
                        }
                    },
                    Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => &text,
                    Target::Wasm32UnknownUnknown => {
                        return Err(Error::new(
                            self.loc(),
                            "unsupported target wasm32-unknown-unknown: asm labels are unavailable",
                        ));
                    }
                };
                label = Some(self.text.intern(name));
            } else if !self.attributes(attrs)? {
                return Ok(label);
            }
        }
    }

    /// `__attribute__((..))` lists. Returns whether there were any.
    pub(crate) fn attributes(&mut self, attrs: &mut Attrs) -> R<bool> {
        let mut any = false;
        while self.is("__attribute__") || self.is("__attribute") {
            any = true;
            self.advance();
            self.expect("(")?;
            self.expect("(")?;
            while !self.is(")") {
                let loc = self.loc();
                let name = match self.advance() {
                    Tok::Ident(s) => s,
                    _ => return Err(self.unexpected("an attribute")),
                };
                let name = name.trim_matches('_');
                match name {
                    // Symbol metadata needs an explicit runtime import/export
                    // contract. Never discard it through the generic ignored
                    // attribute path, including on unused Wasm definitions.
                    "import_module" | "import_name" | "export_name"
                        if self.target == Target::Wasm32UnknownUnknown =>
                    {
                        return Err(unsupported_attribute(loc, name));
                    }
                    // These change types, linkage, retention, placement or execution.
                    // Accepting their syntax without implementing the contract
                    // would silently change the program, even on a prototype
                    // or on an otherwise unused declaration.
                    "ms_abi" if self.target == Target::X86_64LinuxGnu => {
                        attrs.microsoft_x64.get_or_insert(loc);
                    }
                    "weakref" | "weak_import" | "ifunc" | "ms_abi" | "sysv_abi" | "stdcall"
                    | "fastcall" | "thiscall" | "vectorcall" | "regcall" | "pcs"
                    | "aarch64_sve_pcs" | "aarch64_vector_pcs" | "preserve_most"
                    | "preserve_all" | "cdecl" | "regparm" | "sseregparm" | "swiftcall"
                    | "swiftasync" | "preserve_none" | "riscv_vector_cc" | "ext_vector_type" => {
                        return Err(unsupported_attribute(loc, name));
                    }
                    "optimize" => {
                        attrs.function_policy.get_or_insert(loc);
                        self.expect("(")?;
                        let mut options = Vec::new();
                        if self.is(")") {
                            return Err(self.unexpected("an optimization option"));
                        }
                        loop {
                            let option = if matches!(self.peek(), Tok::Str { .. }) {
                                keel_ir::OptimizationOption::Options(self.asm_string()?)
                            } else {
                                let expr = self.conditional()?;
                                let Some(crate::eval::Const::Int(n)) =
                                    crate::eval::eval_constant_with_layout(
                                        &expr,
                                        false,
                                        self.c_layout,
                                    )
                                else {
                                    return Err(Error::new(
                                        loc,
                                        "optimization level must be an integer constant",
                                    ));
                                };
                                let n = u32::try_from(n).map_err(|_| {
                                    Error::new(loc, "optimization level must fit u32")
                                })?;
                                keel_ir::OptimizationOption::Level(n)
                            };
                            options.push(option);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?;
                        }
                        attrs
                            .function_attrs
                            .optimize
                            .append(&keel_ir::FunctionOptimization::new(options));
                    }
                    "cold"
                    | "hot"
                    | "optnone"
                    | "flatten"
                    | "naked"
                    | "interrupt"
                    | "no_instrument_function"
                    | "no_profile_instrument_function" => {
                        if self.is("(") {
                            return Err(self.unexpected("an attribute without arguments"));
                        }
                        attrs.function_policy.get_or_insert(loc);
                        let a = &mut attrs.function_attrs;
                        match name {
                            "cold" => a.cold = true,
                            "hot" => a.hot = true,
                            "optnone" => a.optnone = true,
                            "flatten" => a.flatten = true,
                            "naked" => a.naked = true,
                            "interrupt" => a.interrupt = true,
                            "no_instrument_function" => a.no_instrument_function = true,
                            _ => a.no_profile_instrument_function = true,
                        }
                    }
                    "gnu_inline" => attrs.gnu_inline = true,
                    "transparent_union" => {
                        attrs.transparent_union.get_or_insert(loc);
                    }
                    "alias" => {
                        self.expect("(")?;
                        let name = self.asm_string()?;
                        self.expect(")")?;
                        if attrs.alias.is_some() {
                            return Err(Error::new(loc, "duplicate alias attribute"));
                        }
                        let index = self.alias_names.len();
                        self.alias_names.push(self.text.intern(&name));
                        attrs.alias = Some((index, loc));
                    }
                    "constructor" | "destructor" => {
                        if self.eat("(") {
                            let expression = self.conditional()?;
                            self.expect(")")?;
                            let priority = match crate::eval::eval_constant_with_layout(
                                &expression,
                                false,
                                self.c_layout,
                            ) {
                                Some(crate::eval::Const::Int(n)) if expression.ty.is_integer() => n,
                                _ => {
                                    return Err(Error::new(
                                        loc,
                                        "lifecycle priority requires an integer constant expression",
                                    ));
                                }
                            };
                            if priority != 65535 {
                                return Err(Error::new(
                                    loc,
                                    "only default lifecycle priority 65535 is supported",
                                ));
                            }
                        }
                        attrs.lifecycle.get_or_insert(loc);
                        if name == "constructor" {
                            attrs.function_attrs.constructor = true;
                        } else {
                            attrs.function_attrs.destructor = true;
                        }
                    }
                    "section" => {
                        self.expect("(")?;
                        let mut bytes = Vec::new();
                        if !matches!(self.peek(), Tok::Str { .. }) {
                            return Err(self.unexpected("a section string"));
                        }
                        while let Tok::Str { bytes: part, wide } = self.peek() {
                            if wide {
                                return Err(Error::new(loc, "section requires a narrow string"));
                            }
                            bytes.extend_from_slice(part);
                            self.advance();
                        }
                        self.expect(")")?;
                        let name = std::str::from_utf8(&bytes)
                            .map_err(|_| Error::new(loc, "section requires ASCII"))?;
                        let section =
                            keel_ir::SectionName::new(name).map_err(|e| Error::new(loc, e))?;
                        if attrs.section.is_some_and(|s| s.0 != section) {
                            attrs.section_conflict = Some(loc);
                        }
                        attrs.section = Some((section, loc));
                    }
                    "retain" => {
                        if self.is("(") {
                            return Err(self.unexpected("an attribute without arguments"));
                        }
                        attrs.retain.get_or_insert(loc);
                    }
                    "used" => {
                        if self.is("(") {
                            return Err(self.unexpected("an attribute without arguments"));
                        }
                        attrs.used.get_or_insert(loc);
                    }
                    "cleanup" => {
                        self.expect("(")?;
                        let callback = self.ident()?;
                        let g = match self.lookup(callback) {
                            Some(Sym::Global(g)) if matches!(self.globals[*g].ty, Ty::Func(_)) => {
                                *g
                            }
                            _ => {
                                return Err(Error::new(
                                    loc,
                                    "cleanup argument must name a declared function",
                                ));
                            }
                        };
                        self.expect(")")?;
                        if attrs.cleanup.is_some() {
                            attrs.cleanup_conflict = Some(loc);
                        }
                        attrs.cleanup = Some((g, loc));
                    }
                    // Ordinary weak binding is merged over the complete translation unit.
                    "weak" => {
                        if self.is("(") {
                            return Err(self.unexpected("an attribute without arguments"));
                        }
                        attrs.weak.get_or_insert(loc);
                    }
                    "visibility" => {
                        self.expect("(")?;
                        // Decode adjacent string tokens just as asm labels do.
                        let mut visibility = Vec::new();
                        if !matches!(self.peek(), Tok::Str { .. }) {
                            return Err(self.unexpected("a visibility string"));
                        }
                        while let Tok::Str { bytes, .. } = self.peek() {
                            visibility.extend_from_slice(bytes);
                            self.advance();
                        }
                        self.expect(")")?;
                        let visibility = match &visibility[..] {
                            b"default" => Visibility::Default,
                            b"hidden" => Visibility::Hidden,
                            b"internal" => Visibility::Internal,
                            _ => {
                                return Err(Error::new(
                                    loc,
                                    "non-default visibility is supported only as \"hidden\" or \"internal\"",
                                ));
                            }
                        };
                        attrs.set_visibility(visibility, loc);
                    }
                    "vector_size" => {
                        self.expect("(")?;
                        let n = self.const_int()?;
                        self.expect(")")?;
                        if n != 16 && n != 32 {
                            return Err(Error::new(
                                loc,
                                "vector_size supports only 16- and 32-byte vectors",
                            ));
                        }
                        attrs.vector_size = Some((n as u64, loc));
                    }
                    "mode" => {
                        self.expect("(")?;
                        let name = self.ident()?;
                        let mode = match name.trim_matches('_') {
                            "QI" | "byte" => TypeMode::Integer(1),
                            "HI" => TypeMode::Integer(2),
                            "SI" => TypeMode::Integer(4),
                            "DI" => TypeMode::Integer(8),
                            "word" | "pointer" => TypeMode::Integer(self.c_layout.pointer_bytes()),
                            "TI" => TypeMode::Integer(16),
                            "TC" => TypeMode::ComplexBinary128,
                            _ => {
                                return Err(Error::new(
                                    loc,
                                    format!("unsupported integer mode `{name}`"),
                                ));
                            }
                        };
                        self.expect(")")?;
                        if attrs.mode.is_some_and(|(old, _)| old != mode) {
                            return Err(Error::new(loc, "conflicting mode attributes"));
                        }
                        attrs.mode = Some((mode, loc));
                    }
                    "nonstring" => {
                        if self.is("(") {
                            return Err(self.unexpected("an attribute without arguments"));
                        }
                        attrs.nonstring = true;
                    }
                    "aligned" if self.eat("(") => {
                        let n = self.alignment_constant(false)?;
                        attrs.aligned = Some(attrs.aligned.unwrap_or(1).max(n));
                        self.expect(")")?;
                    }
                    "aligned" => attrs.aligned = Some(16),
                    // Other weak spellings/contracts cannot silently become strong.
                    name if name.starts_with("weak") => {
                        return Err(unsupported_attribute(loc, name));
                    }
                    _ => {
                        match name {
                            "packed" => attrs.packed = true,
                            "returns_twice" => attrs.returns_twice = true,
                            "noreturn" => attrs.noreturn = true,
                            "keel_noninterposable" => attrs.function_attrs.noninterposable = true,
                            "always_inline" => attrs.function_attrs.alwaysinline = true,
                            "noinline" => attrs.function_attrs.noinline = true,
                            "nothrow" => attrs.function_attrs.nounwind = true,
                            _ => {}
                        }
                        if self.eat("(") {
                            if matches!(
                                name,
                                "returns_twice"
                                    | "noreturn"
                                    | "keel_noninterposable"
                                    | "always_inline"
                                    | "noinline"
                                    | "nothrow"
                            ) {
                                return Err(self.unexpected("an attribute without arguments"));
                            }
                            self.skip_parens()?;
                        }
                    }
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
            self.expect(")")?;
        }
        Ok(any)
    }

    /// Attributes without meaning where they are written.
    fn ignored_attributes(&mut self) -> R<()> {
        let mut attrs = Attrs::default();
        self.attributes(&mut attrs)?;
        if let Some(loc) = attrs
            .mode
            .map(|(_, loc)| loc)
            .or(attrs.vector_size.map(|(_, loc)| loc))
        {
            return Err(Error::new(
                loc,
                "type-changing attribute has no type subject",
            ));
        }
        attrs.no_symbol_attrs()
    }

    // ---- types ----

    fn check_nonstring(&self, ty: Ty<'a>, attrs: &Attrs, loc: Loc) -> R<()> {
        if !attrs.nonstring {
            return Ok(());
        }
        let element = match ty {
            Ty::Array(a) => Some(a.elem),
            Ty::Ptr(t) => Some(*t),
            _ => None,
        };
        if !matches!(
            element,
            Some(Ty::Int(
                IntKind::Char | IntKind::UPlainChar | IntKind::SChar | IntKind::UChar
            ))
        ) {
            return Err(Error::new(
                loc,
                "nonstring requires a character array or pointer declaration",
            ));
        }
        Ok(())
    }

    fn alignment_constant(&mut self, allow_zero: bool) -> R<u64> {
        let loc = self.loc();
        let e = self.conditional()?;
        let Some(crate::eval::Const::Int(n)) =
            crate::eval::eval_constant_with_layout(&e, false, self.c_layout)
        else {
            return Err(Error::new(loc, "alignment must be an integer constant"));
        };
        if !e.ty.is_integer()
            || n < 0
            || (n == 0 && !allow_zero)
            || (n != 0 && !(n as u64).is_power_of_two())
        {
            return Err(Error::new(loc, "alignment must be a power of two"));
        }
        if n > i64::from(keel_ir::MAX_DATA_ALIGN) {
            return Err(Error::new(
                loc,
                format!(
                    "alignment above {} is not supported",
                    keel_ir::MAX_DATA_ALIGN
                ),
            ));
        }
        Ok(n as u64)
    }

    fn decl_spec(&mut self) -> R<DeclSpec<'a>> {
        const VOID: u32 = 1;
        const BOOL: u32 = 1 << 2;
        const CHAR: u32 = 1 << 4;
        const SHORT: u32 = 1 << 6;
        const INT: u32 = 1 << 8;
        const LONG: u32 = 1 << 10;
        const FLOAT: u32 = 1 << 12;
        const DOUBLE: u32 = 1 << 14;
        const OTHER: u32 = 1 << 16;
        const SIGNED: u32 = 1 << 17;
        const UNSIGNED: u32 = 1 << 18;
        const INT128: u32 = 1 << 19;
        const HALF: u32 = 1 << 20;
        const AUTO: u32 = 1 << 21;

        let loc = self.loc();
        let mut spec = DeclSpec {
            ty: Ty::Int(IntKind::Int),
            sugar: Sugar::Plain,
            infer: false,
            implicit_int: false,
            storage: Storage::None,
            auto_storage: false,
            register_storage: false,
            is_const: false,
            is_volatile: false,
            inline: false,
            thread_local: false,
            attrs: Attrs::default(),
        };
        let mut counter = 0u32;
        let mut other: Option<Ty<'a>> = None;
        let mut is_atomic = false;
        let mut is_restrict = false;
        let mut is_complex = false;
        while let Tok::Ident(word) = self.peek() {
            let storage = match word {
                "typedef" => Some(Storage::Typedef),
                "extern" => Some(Storage::Extern),
                "static" => Some(Storage::Static),
                "auto" | "register" => Some(Storage::None),
                _ => None,
            };
            if let Some(s) = storage {
                if (word == "register" && spec.storage != Storage::None)
                    || (spec.register_storage && s != Storage::None)
                {
                    return Err(Error::new(
                        loc,
                        "register cannot be combined with another storage class",
                    ));
                }
                self.advance();
                spec.storage = s;
                spec.auto_storage |= word == "auto";
                spec.register_storage |= word == "register";
                continue;
            }
            match word {
                "const" | "__const" => spec.is_const = true,
                "volatile" | "__volatile" | "__volatile__" => spec.is_volatile = true,
                "inline" | "__inline" | "__inline__" => spec.inline = true,
                "_Thread_local" | "__thread" => spec.thread_local = true,
                "_Noreturn" => spec.attrs.noreturn = true,
                "restrict" | "__restrict" | "__restrict__" => is_restrict = true,
                w if IGNORED_QUALIFIERS.contains(&w) => {}
                "_Atomic" if !matches!(self.peek_at(1), Tok::Punct("(")) => is_atomic = true,
                "_Complex" => is_complex = true,
                "__attribute__" | "__attribute" => {
                    let mut attrs = spec.attrs.clone();
                    self.attributes(&mut attrs)?;
                    spec.attrs = attrs;
                    continue;
                }
                "_Alignas" => {
                    self.advance();
                    self.expect("(")?;
                    let n = if self.is_typename() {
                        let (ty, sugar) = self.type_name_sugar()?;
                        self.type_alignment(&ty, &sugar)
                    } else {
                        self.alignment_constant(true)?
                    };
                    self.expect(")")?;
                    spec.attrs.alignas = Some(spec.attrs.alignas.unwrap_or(0).max(n));
                    continue;
                }
                _ => {
                    // A type specifier. A typedef name only counts if no
                    // other type specifier came before it (`T x` vs a
                    // redeclaration `int T`).
                    let ty = match word {
                        "struct" | "union" => {
                            self.advance();
                            Some(self.struct_spec(word == "union")?)
                        }
                        "enum" => {
                            self.advance();
                            let (ty, sugar) = self.enum_spec()?;
                            spec.sugar = sugar;
                            Some(ty)
                        }
                        "_Atomic" => {
                            self.advance();
                            self.expect("(")?;
                            let (ty, sugar) = self.type_name_sugar()?;
                            spec.sugar = sugar;
                            self.expect(")")?;
                            Some(self.atomic_type(ty, loc)?)
                        }
                        "typeof" | "__typeof__" | "__typeof" => {
                            self.advance();
                            self.expect("(")?;
                            let (ty, sugar) = if self.is_typename() {
                                self.type_name_sugar()?
                            } else {
                                let e = self.expr()?;
                                (e.ty, e.sugar)
                            };
                            if ty.variably_modified() {
                                return Err(Error::new(
                                    loc,
                                    "typeof of a variably modified type is not supported yet",
                                ));
                            }
                            spec.sugar = sugar;
                            self.expect(")")?;
                            Some(ty)
                        }
                        "__builtin_va_list" => {
                            if self.target != Target::Wasm32UnknownUnknown {
                                self.require_native_layout(loc, "va_list")?;
                            }
                            self.advance();
                            Some(self.va_list())
                        }
                        "__SVFloat32_t" | "__SVFloat64_t" | "__SVBool_t"
                            if self.target == Target::Aarch64LinuxGnu =>
                        {
                            self.advance();
                            let name = match word {
                                "__SVFloat32_t" => "__SVFloat32_t",
                                "__SVFloat64_t" => "__SVFloat64_t",
                                _ => "__SVBool_t",
                            };
                            Some(Ty::Opaque(self.arena.alloc(OpaqueTy {
                                name,
                                size: 0,
                                align: 1,
                            })))
                        }
                        "__float128" => {
                            self.advance();
                            Some(BINARY128)
                        }
                        "__int128_t" => {
                            self.advance();
                            Some(Ty::Int(IntKind::Int128))
                        }
                        "__uint128_t" => {
                            self.advance();
                            Some(Ty::Int(IntKind::UInt128))
                        }
                        _ if counter == 0 && other.is_none() => {
                            if let Some(Sym::Typedef(i)) = self.lookup(&word) {
                                let i = *i;
                                self.advance();
                                spec.sugar = Sugar::Typedef(i);
                                Some(self.typedefs[i].ty)
                            } else if word == "_Float32" && self.lookup(word).is_none() {
                                // glibc's Clang path can typedef this spelling to
                                // float. GCC's builtin is a distinct type; do not
                                // accept it via implicit int or invent an alias.
                                return Err(Error::new(
                                    self.loc(),
                                    "unsupported extended floating type `_Float32`",
                                ));
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(ty) = ty {
                        if counter != 0 || other.is_some() {
                            return Err(Error::new(loc, "two types in one declaration"));
                        }
                        other = Some(ty);
                        counter += OTHER;
                        continue;
                    }
                    let bit = match word {
                        "void" => VOID,
                        "_Bool" => BOOL,
                        "char" => CHAR,
                        "short" => SHORT,
                        "int" => INT,
                        "long" => LONG,
                        "float" => FLOAT,
                        "double" => DOUBLE,
                        "signed" | "__signed" | "__signed__" => SIGNED,
                        "unsigned" => UNSIGNED,
                        "__int128" => INT128,
                        "_Float16" => HALF,
                        "__auto_type" => AUTO,
                        _ => break,
                    };
                    if other.is_some() {
                        return Err(Error::new(loc, "two types in one declaration"));
                    }
                    counter += bit;
                }
            }
            self.advance();
        }
        use IntKind::*;
        spec.infer = counter == AUTO;
        spec.implicit_int = counter == 0;
        spec.ty = match counter {
            AUTO => Ty::Int(Int), // Replaced after parsing the initializer.
            OTHER => other.unwrap(),
            0 => {
                if spec.storage == Storage::None && !spec.is_const && !spec.inline {
                    return Err(self.unexpected("a type"));
                }
                Ty::Int(Int)
            }
            VOID => Ty::Void,
            BOOL => Ty::Int(Bool),
            CHAR => Ty::Int(if self.target == Target::Aarch64LinuxGnu {
                UPlainChar
            } else {
                Char
            }),
            c if c == SIGNED + CHAR => Ty::Int(SChar),
            c if c == UNSIGNED + CHAR => Ty::Int(UChar),
            c if c == SHORT
                || c == SHORT + INT
                || c == SIGNED + SHORT
                || c == SIGNED + SHORT + INT =>
            {
                Ty::Int(Short)
            }
            c if c == UNSIGNED + SHORT || c == UNSIGNED + SHORT + INT => Ty::Int(UShort),
            INT | SIGNED => Ty::Int(Int),
            c if c == SIGNED + INT => Ty::Int(Int),
            UNSIGNED => Ty::Int(UInt),
            c if c == UNSIGNED + INT => Ty::Int(UInt),
            c if c == LONG || c == LONG + INT || c == SIGNED + LONG || c == SIGNED + LONG + INT => {
                Ty::Int(Long)
            }
            c if c == UNSIGNED + LONG || c == UNSIGNED + LONG + INT => Ty::Int(ULong),
            c if c == 2 * LONG
                || c == 2 * LONG + INT
                || c == SIGNED + 2 * LONG
                || c == SIGNED + 2 * LONG + INT =>
            {
                Ty::Int(LLong)
            }
            c if c == UNSIGNED + 2 * LONG || c == UNSIGNED + 2 * LONG + INT => Ty::Int(ULLong),
            c if c == INT128 || c == SIGNED + INT128 => Ty::Int(Int128),
            c if c == UNSIGNED + INT128 => Ty::Int(UInt128),
            FLOAT => Ty::Float,
            DOUBLE => Ty::Double,
            // On x86-64 Linux it is the x87 80-bit format, which keel does
            // not have: its values are refused rather than miscompiled.
            c if c == LONG + DOUBLE => {
                if self.target != Target::Wasm32UnknownUnknown {
                    self.require_native_layout(loc, "long double declarations")?;
                }
                match self.target {
                    Target::Aarch64AppleDarwin => Ty::Double,
                    Target::X86_64LinuxGnu
                    | Target::Aarch64LinuxGnu
                    | Target::Wasm32UnknownUnknown => Ty::Opaque(&OpaqueTy {
                        name: "long double",
                        size: 16,
                        align: 16,
                    }),
                }
            }
            HALF => Ty::Half,
            _ => return Err(Error::new(loc, "invalid combination of type specifiers")),
        };
        if is_complex {
            if spec.infer {
                return Err(Error::new(
                    loc,
                    "`__auto_type` cannot be combined with `_Complex`",
                ));
            }
            if matches!(spec.ty, Ty::Float | Ty::Double) && counter != LONG + DOUBLE {
                spec.ty = Ty::Complex(self.type_ref(spec.ty));
            } else if counter == LONG + DOUBLE {
                let size = 2 * Types::new(&self.structs, self.c_layout).size(&spec.ty);
                let align = Types::new(&self.structs, self.c_layout).align(&spec.ty);
                spec.ty = Ty::Opaque(self.arena.alloc(OpaqueTy {
                    name: "_Complex long double",
                    size,
                    align,
                }));
            } else {
                return Err(Error::new(
                    loc,
                    "_Complex requires float or double components",
                ));
            }
        }
        spec.ty = self.apply_type_attrs(spec.ty, &mut spec.attrs)?;
        if is_atomic {
            spec.ty = self.atomic_type(spec.ty, loc)?;
        }
        if is_restrict {
            spec.sugar = Sugar::Restrict(self.sugar_ref(spec.sugar));
        }
        if spec.is_volatile {
            spec.sugar = Sugar::Volatile(self.sugar_ref(spec.sugar));
        }
        if spec.is_const {
            spec.sugar = Sugar::Const(self.sugar_ref(spec.sugar));
        }
        Ok(spec)
    }

    /// Type-changing attributes are consumed once, before layout and compatibility.
    /// Preserve foreign function-pointer declarations without authorizing calls.
    fn microsoft_function_type(&mut self, ty: Ty<'a>) -> Option<Ty<'a>> {
        match ty {
            Ty::Func(function) => Some(Ty::Func(self.arena.alloc(FuncTy {
                microsoft_x64: true,
                ..*function
            }))),
            Ty::Ptr(inner) => {
                let annotated = self.microsoft_function_type(*inner)?;
                Some(self.ptr_type(annotated))
            }
            Ty::Array(array) => {
                let elem = self.microsoft_function_type(array.elem)?;
                Some(Ty::Array(self.arena.alloc(ArrayTy { elem, ..*array })))
            }
            _ => None,
        }
    }

    fn apply_type_attrs(&mut self, ty: Ty<'a>, attrs: &mut Attrs) -> R<Ty<'a>> {
        let mut ty = ty;
        if attrs.microsoft_x64.is_some()
            && let Some(annotated) = self.microsoft_function_type(ty)
        {
            ty = annotated;
            attrs.microsoft_x64 = None;
        }
        if let Some(loc) = attrs.transparent_union.take() {
            self.require_native_layout(loc, "transparent unions")?;
            let Ty::Struct(id) = ty else {
                return Err(Error::new(loc, "transparent_union requires a union type"));
            };
            let def = &self.structs[id];
            if !def.is_union
                || !def.complete
                || def.members.is_empty()
                || def.size != 8
                || def.align != 8
                || def
                    .members
                    .iter()
                    .any(|m| !m.ty.is_ptr() || m.bits.is_some())
            {
                return Err(Error::new(
                    loc,
                    "transparent_union requires a complete naturally aligned pointer-only union",
                ));
            }
            self.structs[id].transparent_union = true;
        }
        if let Some((mode, loc)) = attrs.mode.take() {
            match mode {
                TypeMode::ComplexBinary128 => {
                    self.require_native_layout(loc, "TC mode")?;
                    if self.target != Target::X86_64LinuxGnu {
                        return Err(Error::new(
                            loc,
                            "TC mode declaration layout requires x86-64 Linux",
                        ));
                    }
                    if !matches!(ty, Ty::Complex(_))
                        && ty != COMPLEX_BINARY128
                        && !matches!(ty, Ty::Opaque(o) if o.name == "_Complex long double")
                    {
                        return Err(Error::new(
                            loc,
                            "TC mode requires a complex floating base type",
                        ));
                    }
                    ty = COMPLEX_BINARY128;
                }
                TypeMode::Integer(bytes) => {
                    let Ty::Int(kind) = ty else {
                        return Err(Error::new(
                            loc,
                            "integer mode requires an integer base type",
                        ));
                    };
                    if kind == IntKind::Bool {
                        return Err(Error::new(loc, "integer mode does not support _Bool"));
                    }
                    let signed = match bytes {
                        1 => IntKind::SChar,
                        2 => IntKind::Short,
                        4 => IntKind::Int,
                        8 if IntKind::Long.size_with_layout(self.c_layout) == 8 => IntKind::Long,
                        8 => IntKind::LLong,
                        _ => IntKind::Int128,
                    };
                    ty = Ty::Int(if kind.signed() {
                        signed
                    } else {
                        signed.to_unsigned()
                    });
                }
            }
        }
        if let Some((bytes, loc)) = attrs.vector_size.take() {
            ty = self.vector_type(ty, bytes, loc)?;
        }
        Ok(ty)
    }

    fn vector_type(&mut self, ty: Ty<'a>, bytes: u64, loc: Loc) -> R<Ty<'a>> {
        self.require_native_layout(loc, "C vector types")?;
        // Declaration-specifier attributes have already changed the base type.
        // A final declarator attribute requires a scalar subject, like Clang.
        if bytes == 32 && self.target != keel_ir::Target::X86_64LinuxGnu {
            return Err(Error::new(
                loc,
                "32-byte C vectors require the x86-64 target and AVX/AVX2 code generation",
            ));
        }
        match ty {
            Ty::Int(IntKind::Bool | IntKind::Int128 | IntKind::UInt128) => {
                Err(Error::new(loc, "unsupported vector_size element type"))
            }
            t if t.is_integer() || matches!(t, Ty::Float | Ty::Double) => {
                let shapes = if bytes == 32 {
                    keel_ir::VectorType::ALL_256
                } else {
                    keel_ir::VectorType::ALL_128
                };
                let shape = shapes
                    .into_iter()
                    .find(|shape| shape.lane_type() == t.ir())
                    .unwrap();
                Ok(Ty::Vector(self.type_ref(t), shape))
            }
            _ => Err(Error::new(
                loc,
                "vector_size requires a supported scalar element type",
            )),
        }
    }

    pub(crate) fn vm_bound_mark(&self) -> usize {
        self.fs.as_ref().map_or(0, |fs| fs.vla_bounds.len())
    }

    pub(crate) fn take_vm_bounds(&mut self, mark: usize) -> Vec<(LocalId, Expr<'a>)> {
        self.fs
            .as_mut()
            .map_or_else(Vec::new, |fs| fs.vla_bounds.split_off(mark))
    }

    /// A type name, as in casts and `sizeof`: specifiers and an abstract
    /// declarator.
    pub(crate) fn type_name(&mut self) -> R<Ty<'a>> {
        Ok(self.type_name_sugar()?.0)
    }

    pub(crate) fn type_name_sugar(&mut self) -> R<(Ty<'a>, Sugar<'a>)> {
        self.type_name_sugar_mode(false)
    }

    pub(crate) fn type_name_sugar_mode(&mut self, runtime: bool) -> R<(Ty<'a>, Sugar<'a>)> {
        let spec = self.decl_spec()?;
        if spec.infer {
            return Err(Error::new(
                self.loc(),
                "`__auto_type` is only allowed in variable declarations",
            ));
        }
        let outer = self.vla_allowed;
        self.vla_allowed = runtime && self.fs.is_some() && !self.in_parameter_list;
        let result = self.declarator_sugar(spec.ty, spec.sugar);
        self.vla_allowed = outer;
        let d = result?;
        if !runtime && d.ty.variably_modified() {
            return Err(Error::new(
                self.loc(),
                "variably modified type in this type-name context is not supported yet",
            ));
        }
        spec.attrs.no_symbol_attrs()?;
        d.attrs.no_symbol_attrs()?;
        if spec.attrs.nonstring || d.attrs.nonstring {
            return Err(Error::new(
                self.loc(),
                "nonstring cannot annotate a type name",
            ));
        }
        if spec.attrs.alignment().is_some()
            || d.attrs.alignment().is_some()
            || spec.attrs.alignas.is_some()
            || d.attrs.alignas.is_some()
        {
            return Err(Error::new(
                self.loc(),
                "alignment attributes on type names are not supported",
            ));
        }
        let mut attrs = d.attrs;
        Ok((self.apply_type_attrs(d.ty, &mut attrs)?, d.sugar))
    }

    /// `_Atomic(ty)`, with native-width scalar representations.
    fn atomic_type(&mut self, ty: Ty<'a>, loc: Loc) -> R<Ty<'a>> {
        if ty.variably_modified() {
            return Err(Error::new(
                loc,
                "atomic variably modified pointers are not supported yet",
            ));
        }
        match ty {
            Ty::Atomic(_) => Ok(ty),
            ty if (ty.is_integer() || ty.is_ptr() || matches!(ty, Ty::Float | Ty::Double))
                && self
                    .types()
                    .atomic_size(&ty, self.types().align(&ty))
                    .is_some() =>
            {
                Ok(Ty::Atomic(self.type_ref(ty)))
            }
            _ => Err(Error::new(
                loc,
                "`_Atomic` requires a supported 1/2/4/8-byte integer, pointer, float or double",
            )),
        }
    }

    /// Pointer qualifiers after a `*`. Returns (const, volatile, atomic, restrict).
    fn qualifiers(&mut self, attrs: &mut Attrs) -> R<(bool, bool, bool, bool)> {
        let (mut c, mut v, mut a, mut r) = (false, false, false, false);
        loop {
            match self.peek() {
                Tok::Ident(w) if w == "_Atomic" => a = true,
                Tok::Ident("restrict" | "__restrict" | "__restrict__") => r = true,
                Tok::Ident(w) if w == "const" || w == "__const" => c = true,
                Tok::Ident(w) if w == "volatile" || w == "__volatile__" || w == "__volatile" => {
                    v = true
                }
                Tok::Ident(w) if IGNORED_QUALIFIERS.contains(&w) => {}
                Tok::Ident(w) if w == "__attribute__" || w == "__attribute" => {
                    self.attributes(attrs)?;
                    continue;
                }
                _ => return Ok((c, v, a, r)),
            }
            self.advance();
        }
    }

    /// A declarator applied to the specifiers' type and its sugar.
    fn declarator_sugar(&mut self, ty: Ty<'a>, sugar: Sugar<'a>) -> R<Declarator<'a>> {
        let mut d = self.declarator_layer(ty, sugar, None)?;
        // Declaration attributes belong to the final named function, regardless
        // of the intervening pointer/array/function layers. Do not decide this
        // while composing those layers. Pointer objects, typedefs and parameters
        // do not acquire indirect-call contracts.
        if !matches!(d.ty, Ty::Func(_)) {
            d.attrs.returns_twice = false;
            d.attrs.function_attrs = keel_ir::FunctionAttrs::default();
        }
        Ok(d)
    }

    fn declarator_layer(
        &mut self,
        mut ty: Ty<'a>,
        mut sugar: Sugar<'a>,
        inherited_owner: Option<&'a FuncTy<'a>>,
    ) -> R<Declarator<'a>> {
        // Only functions constructed by enclosing declarator layers supply
        // inherited ownership. A typedef's hidden function is not that layer.
        let ambiguous_base = inherited_owner.is_none()
            && !matches!(ty, Ty::Func(_))
            && Self::contract_owner(&ty).is_some();
        let (mut is_const, mut is_volatile) = (false, false);
        let mut attrs = Attrs::default();
        self.attributes(&mut attrs)?;
        // `^` is a block pointer (Apple's closures, in headers when
        // `__BLOCKS__` is defined); as a value it is just a pointer.
        while self.eat("*") || self.eat("^") {
            let loc = self.loc();
            ty = self.ptr_type(ty);
            sugar = self.wrap_sugar(sugar, Sugar::Ptr);
            let (is_atomic, is_restrict);
            (is_const, is_volatile, is_atomic, is_restrict) = self.qualifiers(&mut attrs)?;
            if is_atomic {
                ty = self.atomic_type(ty, loc)?;
            }
            if is_restrict {
                sugar = Sugar::Restrict(self.sugar_ref(sugar));
            }
            if is_volatile {
                sugar = Sugar::Volatile(self.sugar_ref(sugar));
            }
            if is_const {
                sugar = Sugar::Const(self.sugar_ref(sugar));
            }
        }
        self.attributes(&mut attrs)?;
        if ambiguous_base
            && (attrs.returns_twice
                || attrs.noreturn
                || attrs.function_attrs != keel_ir::FunctionAttrs::default())
        {
            return Err(Error::new(
                self.loc(),
                "ambiguous function attribute ownership through a function-pointer typedef; use declaration-specifier or final named-declarator attributes",
            ));
        }
        if self.is("(") && self.nested_declarator_follows() {
            // `(*name)[3]`: the suffix after the parentheses applies first,
            // so skip the inner declarator, read the suffix, and come back.
            self.advance();
            let start = self.pos;
            self.pinned += 1;
            self.skip_parens()?;
            let (ty, sugar) = self.type_suffix(ty, sugar, &mut attrs)?;
            // The current layer's attributes retain their incoming owner.
            // A newly constructed suffix becomes the incoming owner for the
            // nested layer, which may carry separate named-function promises.
            let nested_owner = Self::named_contract_owner(&ty).or(inherited_owner);
            let owner = inherited_owner.or_else(|| Self::named_contract_owner(&ty));
            let end = self.pos;
            self.pos = start;
            let mut d = self.declarator_layer(ty, sugar, nested_owner)?;
            // A purely grouping layer constructs no function of its own;
            // its attributes follow the nested declarator's owner instead.
            let owner = owner.or_else(|| Self::named_contract_owner(&d.ty));
            Self::merge_declarator_attrs(&mut d.attrs, attrs, owner, &d.ty);
            self.expect(")")?;
            self.pos = end;
            self.pinned -= 1;
            return Ok(d);
        }
        let name = match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let loc = self.loc();
                let s = s;
                self.advance();
                Some((s, loc))
            }
            _ => None,
        };
        let (ty, sugar) = self.type_suffix(ty, sugar, &mut attrs)?;
        let owner = inherited_owner.or_else(|| Self::named_contract_owner(&ty));
        let mut named_attrs = Attrs::default();
        Self::merge_declarator_attrs(&mut named_attrs, attrs, owner, &ty);
        if !matches!(ty.unatomic(), Ty::Ptr(_)) {
            // Qualifiers apply to the pointer only when it is the object.
            is_const = false;
            is_volatile = false;
        }
        Ok(Declarator {
            name,
            ty,
            sugar,
            is_const,
            is_volatile,
            attrs: named_attrs,
        })
    }

    /// For the supported function-type promise `noreturn`, a declarator layer
    /// inherits the function constructed by its enclosing
    /// layer, or otherwise owns the function constructed by its own suffix (or nested
    /// declarator when this is a purely grouping layer). Parentheses apply
    /// that suffix before parsing the nested layer. Thus `* attr h(void)` owns
    /// h when returning a scalar pointer, but owns the returned function in
    /// `(* attr h(void))(void)`. Node identity (not ABI equality) distinguishes
    /// those functions through arbitrary parenthesized composition.
    fn named_contract_owner(ty: &Ty<'a>) -> Option<&'a FuncTy<'a>> {
        match *ty {
            Ty::Func(f) => Some(f),
            _ => None,
        }
    }

    fn contract_owner(ty: &Ty<'a>) -> Option<&'a FuncTy<'a>> {
        match *ty {
            Ty::Func(f) => Some(f),
            Ty::Ptr(t) | Ty::Atomic(t) => Self::contract_owner(t),
            Ty::Array(a) => Self::contract_owner(&a.elem),
            _ => None,
        }
    }

    fn merge_declarator_attrs(
        named: &mut Attrs,
        mut layer: Attrs,
        owner: Option<&'a FuncTy<'a>>,
        declared: &Ty<'a>,
    ) {
        // GNU noreturn can qualify a function type, including the returned
        // function of a factory. Only that promise uses function-node identity.
        // Policies, returns_twice and nothrow are declaration attributes here:
        // keep them until the complete declarator identifies the named subject.
        // Layout attributes merge independently of either contract category.
        let is_named =
            matches!((owner, declared), (Some(owner), Ty::Func(f)) if std::ptr::eq(owner, *f));
        if !is_named {
            layer.noreturn = false;
        }
        named.merge(layer);
    }

    /// After `(` in a declarator: a nested declarator, or parameters?
    fn nested_declarator_follows(&self) -> bool {
        match self.peek_at(1) {
            Tok::Punct("*") | Tok::Punct("(") | Tok::Punct("[") | Tok::Punct("^") => true,
            Tok::Ident(s) => !is_type_keyword(s) && !self.is_typedef(s) || s == "__attribute__",
            _ => false,
        }
    }

    fn type_suffix(
        &mut self,
        ty: Ty<'a>,
        sugar: Sugar<'a>,
        attrs: &mut Attrs,
    ) -> R<(Ty<'a>, Sugar<'a>)> {
        if self.eat("(") {
            let outer = self.vla_allowed;
            self.vla_allowed = true;
            let outer_parameter = self.in_parameter_list;
            self.in_parameter_list = true;
            let result = self.params(ty);
            self.in_parameter_list = outer_parameter;
            self.vla_allowed = outer;
            let fty = result?;
            let fty = self.arena.alloc(fty);
            self.last_param_function = Some(fty);
            let fty = Ty::Func(fty);
            return Ok((fty, self.wrap_sugar(sugar, Sugar::Func)));
        }
        if self.eat("[") {
            let mut array_attrs = Attrs::default();
            let mut has_static = self.eat("static");
            let quals = self.qualifiers(&mut array_attrs)?;
            if self.eat("static") {
                if has_static {
                    return Err(Error::new(self.loc(), "duplicate static array modifier"));
                }
                has_static = true;
            }
            if (has_static || quals != (false, false, false, false)) && !self.in_parameter_list {
                return Err(Error::new(
                    self.loc(),
                    "array static and qualifiers require a parameter",
                ));
            }
            if quals.2 {
                return Err(Error::new(
                    self.loc(),
                    "atomic array parameter qualification is not supported",
                ));
            }
            array_attrs.no_symbol_attrs()?;
            array_attrs.no_type_attrs()?;
            if array_attrs.alignment().is_some() || array_attrs.alignas.is_some() {
                return Err(Error::new(
                    self.loc(),
                    "alignment attributes in array declarators are not supported",
                ));
            }
            // Attributes in an already supported array suffix are still part
            // of this declarator, not the element type or an enclosing parameter.
            attrs.merge(Attrs {
                returns_twice: array_attrs.returns_twice,
                noreturn: array_attrs.noreturn,
                function_attrs: array_attrs.function_attrs,
                ..Default::default()
            });
            let loc = self.loc();
            let mut vla = None;
            let len = if self.is("]") {
                None
            } else if self.is("*") && matches!(self.peek_at(1), Tok::Punct("]")) {
                if !self.in_parameter_list || has_static {
                    return Err(Error::new(
                        loc,
                        "VLA star bounds require a prototype parameter without static",
                    ));
                }
                self.advance();
                self.parameter_star = true;
                // A disposable capture identity lets nested star dimensions
                // participate in completeness checks, without an expression.
                let name = self.unique_name(".prototype.star");
                vla = Some(self.new_local(name, ULONG, Sugar::Plain, false));
                None
            } else {
                let e = self.assign()?;
                if !e.ty.is_integer() {
                    return Err(Error::new(loc, "array bound must have integer type"));
                }
                // General constant folding can discard comma operands. Bounds
                // must retain runtime side effects even if the result is known.
                match crate::eval::eval_constant_with_layout(&e, false, self.c_layout) {
                    Some(crate::eval::Const::Int(n)) if n >= 0 => Some(n as u64),
                    Some(crate::eval::Const::Int(_)) => {
                        return Err(Error::new(loc, "array size is negative"));
                    }
                    _ => {
                        if !self.vla_allowed || self.fs.is_none() {
                            return Err(Error::new(
                                loc,
                                "runtime array bounds require a supported block declaration, parameter or type name",
                            ));
                        }
                        let e = self.cast(e, ULONG);
                        let name = self.unique_name(".vla.length");
                        let id = self.new_local(name, ULONG, Sugar::Plain, false);
                        self.fs.as_mut().unwrap().vla_bounds.push((id, e));
                        self.fs.as_mut().unwrap().has_vm = true;
                        vla = Some(id);
                        None
                    }
                }
            };
            if has_static && len.is_none() && vla.is_none() {
                return Err(Error::new(loc, "static array parameter requires a bound"));
            }
            self.expect("]")?;
            let (elem, sugar) = self.type_suffix(ty, sugar, attrs)?;
            if (vla.is_some() || elem.runtime_sized()) && !self.complete_vm_element(&elem) {
                return Err(Error::new(
                    loc,
                    "variable-length array element type must be a complete object type",
                ));
            }
            // Bound each completed dimension before an enclosing dimension,
            // record layout or sizeof query can multiply through Types::size.
            if self.c_layout.pointer_bytes() == 4
                && let Some(len) = len
                && self
                    .size_of(&elem)
                    .checked_mul(len)
                    .is_none_or(|size| size > u32::MAX as u64)
            {
                return Err(Error::new(
                    loc,
                    "non-native array storage exceeds wasm32 bounds",
                ));
            }
            if !self
                .size_of(&elem)
                .is_multiple_of(self.type_alignment(&elem, &sugar))
            {
                return Err(Error::new(
                    self.loc(),
                    "array element size is not a multiple of its alignment",
                ));
            }
            let array = self.arena.alloc(ArrayTy { elem, len, vla });
            if has_static || quals != (false, false, false, false) {
                self.parameter_arrays.push((array, quals));
            }
            return Ok((Ty::Array(array), self.wrap_sugar(sugar, Sugar::Array)));
        }
        Ok((ty, sugar))
    }

    /// A parameter list, after the `(`.
    fn params(&mut self, ret: Ty<'a>) -> R<FuncTy<'a>> {
        let outer = self.fs.take();
        let nodes = self.nodes;
        // Prototype expressions never live in an ephemeral enclosing body arena.
        self.nodes = self.arena;
        self.fs = Some(FuncState {
            locals: outer.as_ref().map_or_else(Vec::new, |fs| fs.locals.clone()),
            vla_bounds: Vec::new(),
            has_vm: false,
            cleanup_unsupported_exit: None,
            direct_calls: Vec::new(),
            scopes: Vec::new(),
            scope: None,
            label_addrs: Vec::new(),
            ret,
            name: "",
            switches: Vec::new(),
        });
        let arrays = std::mem::take(&mut self.parameter_arrays);
        let star = std::mem::replace(&mut self.parameter_star, false);
        self.push_scope();
        let result = self.params_inner(ret).and_then(|fty| {
            if fty.old_style
                && (outer.is_some()
                    || !(self.is("{")
                        || self.is_typename()
                        || self.is(")")
                        || self.is("__attribute__")
                        || self.is("__attribute")))
            {
                Err(Error::new(
                    self.loc(),
                    "identifier list requires a function definition",
                ))
            } else {
                Ok(fty)
            }
        });
        let scope = self.scopes.last().unwrap().clone();
        self.last_param_bindings = scope
            .names
            .iter()
            .map(|&name| (name, *self.lookup(name).unwrap()))
            .collect();
        self.last_param_scope = Some(scope);
        self.pop_scope();
        self.last_param_state = self.fs.take();
        self.last_param_star = self.parameter_star;
        self.parameter_arrays = arrays;
        self.parameter_star = star;
        self.fs = outer;
        self.nodes = nodes;
        result
    }

    /// Remove function-local bound identities from persistent ABI types.
    fn parameter_signature(&self, ty: Ty<'a>) -> Ty<'a> {
        match ty {
            Ty::Array(a) => Ty::Array(self.arena.alloc(ArrayTy {
                elem: self.parameter_signature(a.elem),
                len: a.len,
                vla: None,
            })),
            Ty::Ptr(t) => Ty::Ptr(self.arena.alloc(self.parameter_signature(*t))),
            _ => ty,
        }
    }

    /// Canonical qualifier identity for a function parameter. Typedefs and
    /// adjusted arrays retain their pointee qualifiers; only the parameter
    /// object's own qualifiers are ignored by C function compatibility.
    pub(crate) fn parameter_qualifiers(&self, mut ty: Ty<'a>, mut sugar: Sugar<'a>) -> &'a [u8] {
        let mut quals = Vec::new();
        loop {
            match ty {
                Ty::Ptr(t) => {
                    ty = *t;
                    sugar = sugar.pointee(&self.typedefs);
                }
                Ty::Array(a) => {
                    ty = a.elem;
                    sugar = sugar.array_element(&self.typedefs, self.arena);
                }
                _ => break,
            }
            quals.push(
                u8::from(sugar.is_const(&self.typedefs))
                    | (u8::from(sugar.is_volatile(&self.typedefs)) << 1)
                    | (u8::from(sugar.is_restrict(&self.typedefs)) << 2),
            );
        }
        self.arena.alloc_slice_copy(&quals)
    }

    fn params_inner(&mut self, ret: Ty<'a>) -> R<FuncTy<'a>> {
        if ret.variably_modified() {
            return Err(Error::new(
                self.loc(),
                "variably modified function return types are not supported yet",
            ));
        }
        let mut fty = FuncTy {
            microsoft_x64: false,
            ret,
            params: &[],
            param_quals: &[],
            variadic: false,
            prototyped: true,
            old_style: false,
        };
        let mut names = Vec::new();
        let mut ids = Vec::new();
        let mut params = Vec::new();
        let mut param_quals = Vec::new();
        self.last_param_ids.clear();
        if self.eat(")") {
            fty.prototyped = false;
            self.last_params = names;
            return Ok(fty);
        }
        if self.is("void") && matches!(self.peek_at(1), Tok::Punct(")")) {
            self.advance();
            self.advance();
            self.last_params = names;
            return Ok(fty);
        }
        // A typedef-name starts a prototype, even if followed by a declaration
        // list. Never backtrack and reinterpret it as an identifier.
        if !self.is_typename() {
            fty.prototyped = false;
            fty.old_style = true;
            loop {
                let loc = self.loc();
                if self.is_typename() {
                    return Err(Error::new(
                        loc,
                        "typedef-name or type in identifier-list parameters",
                    ));
                }
                let name = self.ident()?;
                if names.iter().any(|(n, _, _, _)| *n == Some(name)) {
                    return Err(Error::new(loc, "duplicate parameter name"));
                }
                // The identifier list establishes order, not visibility. A
                // parameter enters scope only after its declaration's declarator.
                let fs = self.fs.as_mut().unwrap();
                ids.push(fs.locals.len());
                fs.locals.push(Local {
                    asm_register: None,
                    nonstring: false,
                    name,
                    ty: INT,
                    sugar: Sugar::Plain,
                    align: 4,
                    scope: fs.scope,
                    in_memory: false,
                    compound_literal: false,
                    cleanup: None,
                });
                names.push((Some(name), INT, Sugar::Plain, loc));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
            self.last_params = names;
            self.last_param_ids = ids;
            return Ok(fty);
        }
        loop {
            if self.eat("...") {
                fty.variadic = true;
                break;
            }
            let loc = self.loc();
            if !self.is_typename() {
                return Err(Error::new(
                    loc,
                    "old-style parameter names cannot appear in a prototype parameter list",
                ));
            }
            let spec = self.decl_spec()?;
            if spec.infer {
                return Err(Error::new(loc, "`__auto_type` cannot declare a parameter"));
            }
            let mut d = self.declarator_sugar(spec.ty, spec.sugar)?;
            if let Ty::Array(a) = d.ty {
                if !self.complete_vm_element(&a.elem) {
                    return Err(Error::new(
                        loc,
                        "parameter array element must be a complete object type",
                    ));
                }
            }
            let arrays = std::mem::take(&mut self.parameter_arrays);
            let mut array_quals = (false, false, false, false);
            for (array, quals) in arrays {
                if !matches!(d.ty, Ty::Array(a) if std::ptr::eq(a, array)) {
                    return Err(Error::new(
                        loc,
                        "static and qualifiers require the outermost parameter array",
                    ));
                }
                array_quals = quals;
            }
            if spec.storage != Storage::None
                || spec.thread_local
                || spec.auto_storage
                || spec.inline
            {
                return Err(Error::new(loc, "invalid parameter storage class"));
            }
            let mut attrs = spec.attrs.clone();
            attrs.merge(d.attrs);
            self.attributes(&mut attrs)?;
            d.ty = self.apply_type_attrs(d.ty, &mut attrs)?;
            attrs.no_symbol_attrs()?;
            if attrs.alignment().is_some() || attrs.alignas.is_some() {
                return Err(Error::new(
                    loc,
                    "alignment attributes on parameters are not supported",
                ));
            }
            if d.ty == Ty::Void {
                if d.sugar.is_const(&self.typedefs)
                    || d.sugar.is_volatile(&self.typedefs)
                    || d.sugar.is_restrict(&self.typedefs)
                {
                    return Err(Error::new(
                        loc,
                        "void parameter must not have type qualifiers",
                    ));
                }
                if d.name.is_none() && params.is_empty() && self.eat(")") {
                    self.last_params = names;
                    self.last_param_ids = ids;
                    return Ok(fty);
                }
                return Err(Error::new(loc, "parameter cannot have void type"));
            }
            // Arrays and functions are passed as pointers.
            let mut sugar = match (&d.ty, d.sugar) {
                (Ty::Func(_), s) => self.wrap_sugar(s, Sugar::Ptr),
                (Ty::Array(_), s) => {
                    Sugar::Ptr(self.sugar_ref(s.array_element(&self.typedefs, self.arena)))
                }
                (_, s) => s,
            };
            let ty = match d.ty {
                Ty::Array(a) => Ty::Ptr(&a.elem),
                Ty::Func(_) => self.ptr_type(d.ty),
                ty => ty,
            };
            if array_quals.0 {
                sugar = Sugar::Const(self.sugar_ref(sugar));
            }
            if array_quals.1 {
                sugar = Sugar::Volatile(self.sugar_ref(sugar));
            }
            if array_quals.3 {
                sugar = Sugar::Restrict(self.sugar_ref(sugar));
            }
            if let Some((name, _)) = d.name {
                let key = self.name_key(name);
                if self
                    .names
                    .get(&key)
                    .is_some_and(|b| b.last().unwrap().0 == self.scopes.len() - 1)
                {
                    return Err(Error::new(loc, "duplicate parameter name"));
                }
            }
            let name = d
                .name
                .map(|(n, _)| n)
                .unwrap_or_else(|| self.unique_name(".parameter"));
            self.check_nonstring(ty, &attrs, loc)?;
            let id = self.new_local(name, ty, sugar, array_quals.1);
            self.fs.as_mut().unwrap().locals[id].nonstring = attrs.nonstring;
            ids.push(id);
            params.push(self.parameter_signature(ty));
            param_quals.push(self.parameter_qualifiers(ty, sugar));
            names.push((d.name.map(|(n, _)| n), ty, sugar, loc));
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        self.last_params = names;
        self.last_param_ids = ids;
        fty.params = self.arena.alloc_slice_copy(&params);
        fty.param_quals = self.arena.alloc_slice_copy(&param_quals);
        Ok(fty)
    }

    fn struct_spec(&mut self, is_union: bool) -> R<Ty<'a>> {
        let mut attrs = Attrs::default();
        self.attributes(&mut attrs)?;
        attrs.no_type_attrs()?;
        let loc = self.loc();
        let tag = match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let s = s;
                self.advance();
                Some(s)
            }
            _ => None,
        };
        let new_struct = |p: &mut Parser<'a>, tag: Option<&'a str>| {
            p.structs.push(StructDef {
                transparent_union: false,
                tag,
                is_union,
                members: Vec::new(),
                size: 0,
                align: 1,
                complete: false,
            });
            p.structs.len() - 1
        };
        if !self.is("{") {
            let Some(tag) = tag else {
                return Err(Error::new(loc, "struct needs a tag or a body"));
            };
            if let Some(id) = self.scopes.iter().rev().find_map(|s| s.tags.get(&tag)) {
                return match id {
                    Some(id) => self.apply_type_attrs(Ty::Struct(*id), &mut attrs),
                    None => Err(Error::new(loc, format!("`{tag}` is an enum"))),
                };
            }
            let id = new_struct(self, Some(tag));
            self.scopes.last_mut().unwrap().tags.insert(tag, Some(id));
            attrs.no_symbol_attrs()?;
            attrs.no_type_attrs()?;
            return Ok(Ty::Struct(id));
        }
        let pack = self.lexer.pack_limit(self.toks[self.pos].pack);
        self.advance();
        let id = match &tag {
            Some(t) => match self.scopes.last().unwrap().tags.get(t) {
                Some(Some(id)) if !self.structs[*id].complete => *id,
                _ => {
                    let id = new_struct(self, Some(*t));
                    self.scopes.last_mut().unwrap().tags.insert(*t, Some(id));
                    id
                }
            },
            None => new_struct(self, None),
        };
        let mut fields = Vec::new();
        while !self.eat("}") {
            if self.eat(";") {
                continue;
            }
            if self.static_assert()? {
                continue;
            }
            let floc = self.loc();
            let spec = self.decl_spec()?;
            if spec.infer {
                return Err(Error::new(
                    floc,
                    "`__auto_type` cannot declare a struct member",
                ));
            }
            if self.eat(";") {
                // An anonymous struct or union member.
                spec.attrs.no_symbol_attrs()?;
                if matches!(spec.ty, Ty::Struct(_)) {
                    self.object_alignment(&spec.ty, &spec.sugar, &spec.attrs, floc)?;
                    fields.push((
                        None,
                        spec.ty,
                        spec.sugar,
                        None,
                        spec.attrs.alignment(),
                        spec.attrs.packed,
                        false,
                    ));
                }
                continue;
            }
            loop {
                let mut member_attrs = spec.attrs.clone();
                let (name, mut ty, sugar) = if self.is(":") {
                    (None, spec.ty, spec.sugar)
                } else {
                    let outer = self.vla_allowed;
                    self.vla_allowed = false;
                    let result = self.declarator_sugar(spec.ty, spec.sugar);
                    self.vla_allowed = outer;
                    let d = result?;
                    member_attrs.merge(d.attrs);
                    (d.name.map(|(n, _)| n), d.ty, d.sugar)
                };
                if ty.variably_modified() {
                    return Err(Error::new(
                        floc,
                        "variably modified structure members are not supported",
                    ));
                }
                let width = if self.eat(":") {
                    let w = self.const_int()?;
                    if !ty.is_integer() || !(0..=self.size_of(&ty) as i64 * 8).contains(&w) {
                        return Err(Error::new(floc, "invalid bit-field"));
                    }
                    Some(w as u32)
                } else {
                    if self.incomplete(&ty) {
                        return Err(Error::new(floc, "member has an incomplete type"));
                    }
                    None
                };
                self.attributes(&mut member_attrs)?;
                ty = self.apply_type_attrs(ty, &mut member_attrs)?;
                if let Some(width) = width
                    && (!ty.is_integer() || u64::from(width) > self.size_of(&ty) * 8)
                {
                    return Err(Error::new(
                        floc,
                        "invalid bit-field after type-changing attribute",
                    ));
                }
                member_attrs.no_symbol_attrs()?;
                self.check_nonstring(ty, &member_attrs, floc)?;
                self.object_alignment(&ty, &sugar, &member_attrs, floc)?;
                if width.is_some() && member_attrs.alignas.is_some() {
                    return Err(Error::new(floc, "`_Alignas` cannot apply to a bit-field"));
                }
                fields.push((
                    name,
                    ty,
                    sugar,
                    width,
                    member_attrs.alignment(),
                    member_attrs.packed,
                    member_attrs.nonstring,
                ));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.attributes(&mut attrs)?;
        attrs.no_symbol_attrs()?;
        attrs.no_type_attrs()?;
        for field in &mut fields {
            field.5 |= attrs.packed;
        }
        // AAPCS64 Linux gives zero-width bit-fields their declared alignment
        // even in packed records. Darwin and x86 use them only as bit barriers.
        let zero_width_align = if self.target == Target::Aarch64LinuxGnu {
            fields
                .iter()
                .filter(|field| field.3 == Some(0))
                .map(|field| {
                    field
                        .2
                        .alignment(&self.typedefs)
                        .unwrap_or_else(|| self.align_of(&field.1))
                })
                .max()
                .unwrap_or(1)
        } else {
            1
        };
        let (members, mut size, mut align) =
            layout_struct(&self.types(), &self.typedefs, is_union, fields, pack)
                .map_err(|message| Error::new(loc, message))?;
        align = align.max(zero_width_align).max(attrs.aligned.unwrap_or(1));
        size = size.next_multiple_of(align);
        let def = &mut self.structs[id];
        def.members = members;
        def.size = size;
        def.align = align;
        def.complete = true;
        self.apply_type_attrs(Ty::Struct(id), &mut attrs)
    }

    /// An enum specifier: its values' type, and the enum for debug info.
    fn enum_spec(&mut self) -> R<(Ty<'a>, Sugar<'a>)> {
        let loc = self.loc();
        let mut attrs = Attrs::default();
        self.attributes(&mut attrs)?;
        let tag = if let Tok::Ident(name) = self.peek()
            && !is_keyword(name)
        {
            self.advance();
            Some(name)
        } else {
            None
        };
        self.attributes(&mut attrs)?;
        if let Some(loc) = attrs.transparent_union {
            return Err(Error::new(loc, "transparent_union requires a union type"));
        }
        let fixed = if self.eat(":") {
            let ty = self.type_name()?;
            if !ty.is_integer() || self.size_of(&ty) > 8 {
                return Err(Error::new(
                    loc,
                    "enum underlying type must be an integer of at most 64 bits",
                ));
            }
            Some(ty)
        } else {
            None
        };
        if !self.eat("{") {
            if attrs.packed {
                return Err(Error::new(loc, "packed requires an enum definition"));
            }
            if let Some(tag) = tag {
                for scope in self.scopes.iter().rev() {
                    if let Some(kind) = scope.tags.get(tag) {
                        if kind.is_some() {
                            return Err(Error::new(loc, format!("`{tag}` is not an enum")));
                        }
                        if let Some(&id) = scope.enums.get(tag) {
                            let ty = self.enums[id].ty;
                            if fixed.is_some_and(|fixed| fixed != ty) {
                                return Err(Error::new(loc, "conflicting enum underlying type"));
                            }
                            return Ok((ty, Sugar::Enum(id)));
                        }
                        break;
                    }
                }
                // The frontend represents incomplete ordinary enum extensions as int.
                // Do not let a later packed definition change already emitted types.
                self.scopes.last_mut().unwrap().tags.insert(tag, None);
            }
            if fixed.is_some() {
                return Err(Error::new(
                    loc,
                    "fixed enum forward declarations are not supported",
                ));
            }
            return Ok((INT, Sugar::Plain));
        }
        let prior_incomplete = tag.is_some_and(|tag| {
            self.scopes.last().unwrap().tags.get(tag) == Some(&None)
                && !self.scopes.last().unwrap().enums.contains_key(tag)
        });
        if let Some(tag) = tag
            && self.scopes.last().unwrap().tags.contains_key(tag)
            && !prior_incomplete
        {
            return Err(Error::new(loc, "enum tag is already defined in this scope"));
        }
        let mut values = Vec::new();
        let mut next = 0_i128;
        let mut next_kind = IntKind::Int;
        let mut minimum = 0_i128;
        let mut maximum = 0_i128;
        while !self.eat("}") {
            let name = self.ident()?;
            self.ignored_attributes()?;
            if self.eat("=") {
                let expr = self.conditional()?;
                let value = crate::eval::eval_constant_with_layout(&expr, true, self.c_layout);
                let Some(crate::eval::Const::Int(bits)) = value else {
                    return Err(Error::new(
                        expr.loc,
                        "expected an integer constant enumerator",
                    ));
                };
                if !expr.ty.is_integer() || self.size_of(&expr.ty) > 8 {
                    return Err(Error::new(
                        expr.loc,
                        "enumerators require an integer of at most 64 bits",
                    ));
                }
                next_kind = expr.ty.int_kind().expect("integer enumerator was checked");
                next = if expr.ty.is_signed() {
                    i128::from(bits)
                } else {
                    let width = self.size_of(&expr.ty) * 8;
                    i128::from((bits as u64) & (u64::MAX >> (64 - width)))
                };
            }
            if !(i128::from(i64::MIN)..=i128::from(u64::MAX)).contains(&next) {
                return Err(Error::new(
                    loc,
                    "enumerator exceeds the supported 64-bit range",
                ));
            }
            if values.is_empty() {
                minimum = next;
                maximum = next;
            } else {
                minimum = minimum.min(next);
                maximum = maximum.max(next);
            }
            values.push((name, next as i64));
            let bits = next_kind.size_with_layout(self.c_layout) * 8;
            let fits_previous = if next_kind.signed() {
                (-(1_i128 << (bits - 1))..(1_i128 << (bits - 1))).contains(&next)
            } else {
                (0..(1_i128 << bits)).contains(&next)
            };
            let kind = if let Some(ty) = fixed {
                ty.int_kind()
            } else if i32::try_from(next).is_ok() {
                None
            } else if fits_previous {
                Some(next_kind)
            } else if i64::try_from(next).is_ok() {
                Some(if IntKind::Long.size_with_layout(self.c_layout) == 8 {
                    IntKind::Long
                } else {
                    IntKind::LLong
                })
            } else {
                Some(IntKind::ULLong)
            };
            next_kind = kind.unwrap_or(IntKind::Int);
            self.bind(name, Sym::Enum(next as i64, next_kind));
            next += 1;
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
        }
        self.attributes(&mut attrs)?;
        if let Some(loc) = attrs.transparent_union {
            return Err(Error::new(loc, "transparent_union requires a union type"));
        }
        attrs.no_type_attrs()?;
        attrs.no_symbol_attrs()?;
        if attrs.aligned.is_some() {
            return Err(Error::new(loc, "aligned enum types are not supported"));
        }
        let ty = if let Some(ty) = fixed {
            let bits = self.size_of(&ty) * 8;
            let (low, high) = if ty.is_signed() {
                (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
            } else {
                (0, (1_i128 << bits) - 1)
            };
            if minimum < low || maximum > high {
                return Err(Error::new(
                    loc,
                    "enumerator does not fit the fixed underlying type",
                ));
            }
            ty
        } else {
            let candidates = if minimum < 0 {
                [
                    IntKind::SChar,
                    IntKind::Short,
                    IntKind::Int,
                    IntKind::Long,
                    IntKind::LLong,
                ]
            } else {
                [
                    IntKind::UChar,
                    IntKind::UShort,
                    IntKind::UInt,
                    IntKind::ULong,
                    IntKind::ULLong,
                ]
            };
            let kind = candidates
                .into_iter()
                .find(|kind| {
                    let bits = kind.size_with_layout(self.c_layout) * 8;
                    let (low, high) = if kind.signed() {
                        (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
                    } else {
                        (0, (1_i128 << bits) - 1)
                    };
                    (attrs.packed || bits >= 32) && minimum >= low && maximum <= high
                })
                .ok_or_else(|| Error::new(loc, "enum range needs more than 64 bits"))?;
            // Preserve the existing ordinary int representation for small enums.
            if !attrs.packed && minimum >= i128::from(i32::MIN) && maximum <= i128::from(i32::MAX) {
                INT
            } else {
                Ty::Int(kind)
            }
        };
        if prior_incomplete && ty != INT {
            return Err(Error::new(
                loc,
                "enum definition changes an incomplete enum representation",
            ));
        }
        for &(name, value) in &values {
            if fixed.is_some() || i32::try_from(value).is_err() || (minimum >= 0 && value < 0) {
                self.bind(
                    name,
                    Sym::Enum(
                        value,
                        ty.int_kind().expect("enum representation is integer"),
                    ),
                );
            }
        }
        self.enums.push(EnumDef { tag, ty, values });
        let id = self.enums.len() - 1;
        if let Some(tag) = tag {
            let scope = self.scopes.last_mut().unwrap();
            scope.tags.insert(tag, None);
            scope.enums.insert(tag, id);
        }
        Ok((ty, Sugar::Enum(id)))
    }

    /// An integer constant expression.
    pub(crate) fn const_int(&mut self) -> R<i64> {
        let loc = self.loc();
        let e = self.conditional()?;
        if e.ty.is_integer()
            && let Some(crate::eval::Const::Int(value)) =
                crate::eval::eval_constant_with_layout(&e, true, self.c_layout)
        {
            return Ok(value);
        }
        Err(Error::new(loc, "expected a constant expression"))
    }

    // ---- statements ----

    /// The items of a `{}` block, after the `{`, in a new scope.
    /// The statements of a block, through the `}`.
    pub(crate) fn block_items(&mut self) -> R<&'a [Stmt<'a>]> {
        while self.eat("__label__") {
            loop {
                let loc = self.loc();
                let name = self.ident()?;
                if self.scopes.last().unwrap().local_labels.contains_key(name) {
                    return Err(Error::new(loc, "duplicate local label declaration"));
                }
                let unique = self.unique_name(".local_label");
                self.scopes
                    .last_mut()
                    .unwrap()
                    .local_labels
                    .insert(name, unique);
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        let mut out = self.stmt_buf();
        while !self.eat("}") {
            if self.is_declaration_start() {
                self.declaration(false, &mut out)?;
            } else {
                out.push(self.statement()?);
            }
            self.release_tokens();
        }
        Ok(self.finish_stmts(out))
    }

    pub(crate) fn resolve_label(&self, name: &'a str) -> &'a str {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.local_labels.get(name).copied())
            .unwrap_or(name)
    }

    fn is_declaration_start(&self) -> bool {
        if self.is("_Static_assert") {
            return true;
        }
        if self.is_fallthrough_attribute() {
            return false;
        }
        // `T:` is a label, even if `T` is a typedef name.
        self.is_typename() && !matches!(self.peek_at(1), Tok::Punct(":"))
    }

    fn is_fallthrough_attribute(&self) -> bool {
        matches!(self.peek(), Tok::Ident("__attribute__" | "__attribute"))
            && matches!(self.peek_at(1), Tok::Punct("("))
            && matches!(self.peek_at(2), Tok::Punct("("))
            && matches!(self.peek_at(3), Tok::Ident(name) if name.trim_matches('_') == "fallthrough")
    }

    fn statement(&mut self) -> R<Stmt<'a>> {
        let loc = self.loc();
        if self.is_fallthrough_attribute() {
            if self.fs.as_ref().is_none_or(|f| f.switches.is_empty()) {
                return Err(Error::new(loc, "fallthrough statement outside a switch"));
            }
            self.advance();
            self.expect("(")?;
            self.expect("(")?;
            self.advance();
            if self.is("(") {
                return Err(Error::new(
                    loc,
                    "fallthrough requires an attribute without arguments",
                ));
            }
            if self.eat(",") && !self.is(")") {
                return Err(Error::new(
                    loc,
                    "fallthrough must be the only statement attribute",
                ));
            }
            self.expect(")")?;
            self.expect(")")?;
            if !self.eat(";") {
                return Err(Error::new(
                    loc,
                    "fallthrough must annotate a null statement",
                ));
            }
            return Ok(Stmt::Block(&[]));
        }
        if let Tok::Ident(word) = self.peek() {
            match word {
                "if" => {
                    self.advance();
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    let then = self.statement()?;
                    let els = if self.eat("else") {
                        {
                            let s = self.statement()?;
                            Some(self.stmt_node(s))
                        }
                    } else {
                        None
                    };
                    return Ok(Stmt::If(cond, self.stmt_node(then), els));
                }
                "while" => {
                    self.advance();
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    let body = self.statement()?;
                    return Ok(Stmt::While(cond, self.stmt_node(body)));
                }
                "do" => {
                    self.advance();
                    let body = self.statement()?;
                    self.expect("while")?;
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::DoWhile(self.stmt_node(body), cond));
                }
                "for" => {
                    self.advance();
                    self.expect("(")?;
                    let scope = self.push_block();
                    let init = if self.eat(";") {
                        None
                    } else if self.is_declaration_start() {
                        {
                            let mut decls = self.stmt_buf();
                            self.declaration(false, &mut decls)?;
                            let decls = self.finish_stmts(decls);
                            Some(self.stmt_node(Stmt::Block(decls)))
                        }
                    } else {
                        let e = self.expr()?;
                        self.expect(";")?;
                        Some(self.stmt_node(Stmt::Expr(e)))
                    };
                    let cond = if self.is(";") {
                        None
                    } else {
                        {
                            let c = self.condition()?;
                            Some(self.node(c))
                        }
                    };
                    self.expect(";")?;
                    let step = if self.is(")") {
                        None
                    } else {
                        {
                            let e = self.expr()?;
                            Some(self.node(e))
                        }
                    };
                    self.expect(")")?;
                    let body = self.statement()?;
                    self.pop_block();
                    let for_ = Stmt::For {
                        init,
                        cond,
                        step,
                        body: self.stmt_node(body),
                    };
                    return Ok(Stmt::Scoped(scope, self.slice(&[for_])));
                }
                "switch" => {
                    self.advance();
                    self.expect("(")?;
                    let cond = self.expr()?;
                    let cond = self.rvalue(cond);
                    if !cond.ty.is_integer() {
                        return Err(Error::new(loc, "switch needs an integer"));
                    }
                    let cond = self.promote(cond);
                    self.expect(")")?;
                    self.fs.as_mut().unwrap().switches.push(SwitchCtx {
                        ty: cond.ty,
                        cases: Vec::new(),
                        default: None,
                        labels: 0,
                    });
                    let body = self.statement()?;
                    let sw = self.fs.as_mut().unwrap().switches.pop().unwrap();
                    return Ok(Stmt::Switch {
                        cond: self.node(cond),
                        body: self.stmt_node(body),
                        cases: self.slice(&sw.cases),
                        default: sw.default,
                        labels: sw.labels,
                    });
                }
                "case" | "default" => {
                    self.advance();
                    // Keep GNU case ranges compact, including full-width intervals.
                    let values = if word == "case" {
                        let v = self.const_int()?;
                        let last = if self.eat("...") {
                            self.const_int()?
                        } else {
                            v
                        };
                        Some((v, last))
                    } else {
                        None
                    };
                    self.expect(":")?;
                    let Some(sw) = self.fs.as_mut().unwrap().switches.last_mut() else {
                        return Err(Error::new(loc, format!("`{word}` outside a switch")));
                    };
                    let label = sw.labels;
                    sw.labels += 1;
                    match values {
                        Some((first, last)) => {
                            let first =
                                crate::eval::normalize_with_layout(first, &sw.ty, self.c_layout);
                            let last =
                                crate::eval::normalize_with_layout(last, &sw.ty, self.c_layout);
                            // i64 stores unsigned constants as bits. Order those bits
                            // according to the controlling type, after conversion.
                            let key =
                                |v: i64| (v as u64) ^ if sw.ty.is_signed() { 1 << 63 } else { 0 };
                            if key(last) < key(first) {
                                return Err(Error::new(loc, "case range is empty"));
                            }
                            for case in &sw.cases {
                                if key(first) <= key(case.last) && key(case.first) <= key(last) {
                                    let duplicate = if key(first) >= key(case.first) {
                                        first
                                    } else {
                                        case.first
                                    };
                                    return Err(Error::new(
                                        loc,
                                        format!("duplicate case {duplicate}"),
                                    ));
                                }
                            }
                            sw.cases.push(SwitchCase { first, last, label });
                        }
                        None => {
                            if sw.default.is_some() {
                                return Err(Error::new(loc, "two `default` labels"));
                            }
                            sw.default = Some(label);
                        }
                    }
                    let stmt = self.labeled_statement()?;
                    return Ok(Stmt::Block(self.slice(&[Stmt::Case(label), stmt])));
                }
                "break" | "continue" => {
                    self.advance();
                    self.expect(";")?;
                    return Ok(if word == "break" {
                        Stmt::Break
                    } else {
                        Stmt::Continue
                    });
                }
                "return" => {
                    self.advance();
                    if self.eat(";") {
                        return Ok(Stmt::Return(None, loc));
                    }
                    let e = self.expr()?;
                    self.expect(";")?;
                    let ret = self.fs.as_ref().unwrap().ret;
                    if ret == Ty::Void {
                        // `return f();` in a void function, f returning void.
                        return Ok(Stmt::Block(
                            self.slice(&[Stmt::Expr(e), Stmt::Return(None, loc)]),
                        ));
                    }
                    let e = self.convert_for_assign(e, &ret)?;
                    return Ok(Stmt::Return(Some(e), loc));
                }
                "goto" => {
                    self.advance();
                    if self.eat("*") {
                        let e = self.expr()?;
                        let e = self.rvalue(e);
                        if !e.ty.is_ptr() {
                            return Err(Error::new(loc, "`goto *` needs a pointer"));
                        }
                        self.expect(";")?;
                        return Ok(Stmt::GotoIndirect(e));
                    }
                    let label = self.ident()?;
                    self.expect(";")?;
                    return Ok(Stmt::Goto(self.resolve_label(label)));
                }
                "asm" | "__asm" | "__asm__" => {
                    self.require_native_layout(loc, "inline assembly")?;
                    return self.asm_statement();
                }
                _ if matches!(self.peek_at(1), Tok::Punct(":")) && !is_keyword(&word) => {
                    self.advance();
                    self.advance();
                    if self.is_fallthrough_attribute() {
                        return Err(Error::new(loc, "fallthrough cannot annotate a label"));
                    }
                    self.ignored_attributes()?;
                    let label = self.resolve_label(word);
                    let stmt = self.labeled_statement()?;
                    return Ok(Stmt::Block(self.slice(&[Stmt::Label(label), stmt])));
                }
                _ => {}
            }
        }
        if self.eat("{") {
            let scope = self.push_block();
            let items = self.block_items()?;
            self.pop_block();
            return Ok(Stmt::Scoped(scope, items));
        }
        if self.eat(";") {
            return Ok(Stmt::Block(&[]));
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(Stmt::Expr(e))
    }

    /// The statement after a label. C23 allows a declaration there, and
    /// so do compilers in practice; so does this.
    fn labeled_statement(&mut self) -> R<Stmt<'a>> {
        if self.is("}") {
            return Ok(Stmt::Block(&[]));
        }
        if self.is_declaration_start() {
            let mut decls = self.stmt_buf();
            self.declaration(false, &mut decls)?;
            return Ok(Stmt::Block(self.finish_stmts(decls)));
        }
        self.statement()
    }

    /// A controlling expression: any scalar.
    fn condition(&mut self) -> R<Expr<'a>> {
        let loc = self.loc();
        let e = self.expr()?;
        let e = self.rvalue(e);
        if !e.ty.is_scalar() {
            return Err(Error::new(loc, "condition must be a scalar"));
        }
        Ok(e)
    }
}

/// A flexible array member, or GNU's zero-length array used as one.
impl<'a> Parser<'a> {
    fn complete_vm_element(&self, ty: &Ty<'a>) -> bool {
        match ty {
            Ty::Void | Ty::Func(_) => false,
            Ty::Struct(id) => self.structs[*id].complete,
            Ty::Array(a) => {
                (a.len.is_some() || a.vla.is_some()) && self.complete_vm_element(&a.elem)
            }
            _ => true,
        }
    }

    /// A type a member cannot have. A struct or union of size 0 is
    /// complete (GNU allows empty ones, and zero-length arrays); an array
    /// with no length is a flexible array member if its element is.
    fn incomplete(&self, ty: &Ty<'a>) -> bool {
        match ty {
            Ty::Void | Ty::Func(_) => true,
            Ty::Struct(id) => !self.structs[*id].complete,
            Ty::Array(a) => self.incomplete(&a.elem),
            _ => false,
        }
    }
}

/// Words that cannot be identifiers.
pub(crate) fn is_keyword(s: &str) -> bool {
    keyword_class(s) & RESERVED != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_dispatch_preserves_membership_and_identifier_exceptions() {
        let other = [
            "if",
            "else",
            "while",
            "do",
            "for",
            "switch",
            "case",
            "default",
            "break",
            "continue",
            "return",
            "goto",
            "sizeof",
            "_Alignof",
            "__alignof__",
            "_Static_assert",
            "_Generic",
            "asm",
            "__asm",
            "__asm__",
        ];
        for word in
            TYPE_KEYWORDS
                .iter()
                .copied()
                .chain(other)
                .chain(["", "value", "T", "__builtin_custom"])
        {
            for candidate in [
                word.to_owned(),
                format!("{word}_"),
                format!("_{word}"),
                word.to_uppercase(),
            ] {
                assert_eq!(
                    is_type_keyword(&candidate),
                    TYPE_KEYWORDS.contains(&candidate.as_str())
                );
                let expected = (TYPE_KEYWORDS.contains(&candidate.as_str())
                    && !candidate.starts_with("__builtin")
                    && !candidate.ends_with("_t"))
                    || other.contains(&candidate.as_str());
                assert_eq!(is_keyword(&candidate), expected, "{candidate}");
            }
        }
        assert!(!is_keyword("__builtin_va_list"));
        assert!(!is_keyword("__int128_t"));
    }

    #[test]
    fn binding_stacks_match_scope_search_through_shadowing_and_root_insertions() {
        let arena = bumpalo::Bump::new();
        let mut p = Parser::new(
            "",
            "test.c",
            crate::Options::default().target,
            Strings::new(&arena),
        );
        let words = ["a", "b", "c", "d", "e", "f", "g"].map(|w| p.text.intern(w));
        let mut model = vec![HashMap::default()];
        let mut state = 7u32;
        for step in 0..3_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let name = words[(state as usize >> 8) % words.len()];
            match (state >> 16) % 8 {
                0 if model.len() < 8 => {
                    p.push_scope();
                    model.push(HashMap::default());
                }
                1 if model.len() > 1 => {
                    p.pop_scope();
                    model.pop();
                }
                _ => {
                    let depth = if state % 5 == 0 { 0 } else { model.len() - 1 };
                    let sym = if step % 3 == 0 {
                        Sym::Typedef(step)
                    } else {
                        Sym::Enum(step as i64, IntKind::Int)
                    };
                    p.bind_at(depth, name, sym);
                    model[depth].insert(name, sym);
                }
            }
            for name in words {
                assert_eq!(
                    p.lookup(name),
                    model.iter().rev().find_map(|scope| scope.get(name))
                );
            }
        }
        while model.len() > 1 {
            p.pop_scope();
            model.pop();
        }
        for name in words {
            assert_eq!(p.lookup(name), model[0].get(name));
        }
    }

    #[test]
    fn inner_extern_restores_outer_shadow_and_file_binding() {
        let arena = bumpalo::Bump::new();
        let mut p = Parser::new(
            "",
            "test.c",
            crate::Options::default().target,
            Strings::new(&arena),
        );
        let object = p.text.intern("object");
        p.push_scope();
        p.bind(object, Sym::Local(3));
        p.push_scope();
        let global = p.global(object, None, INT).unwrap();
        assert_eq!(p.lookup(object), Some(&Sym::Global(global)));
        p.pop_scope();
        assert_eq!(p.lookup(object), Some(&Sym::Local(3)));
        p.pop_scope();
        assert_eq!(p.lookup(object), Some(&Sym::Global(global)));
        assert_eq!(p.global(object, None, INT).unwrap(), global);
        assert_eq!(p.globals.len(), 1);
        for depth in 0..6 {
            p.push_scope();
            p.bind(object, Sym::Enum(depth, IntKind::Int));
        }
        for depth in (0..6).rev() {
            assert_eq!(p.lookup(object), Some(&Sym::Enum(depth, IntKind::Int)));
            p.pop_scope();
        }
        assert_eq!(p.lookup(object), Some(&Sym::Global(global)));
        assert!(std::ptr::eq(
            p.sugar_ref(Sugar::Plain),
            p.sugar_ref(Sugar::Plain)
        ));
    }
}

#[cfg(test)]
mod declaration_layout_tests {
    use super::*;
    use crate::eval::{Const, eval_with_layout};
    use bumpalo::Bump;

    const FIXTURE: &str = include_str!("../tests/fixtures/wasm-scalar-declarations.c");

    fn declarations<'a>(src: &'a str, arena: &'a Bump, layout: CDataLayout) -> R<Parser<'a>> {
        Parser::declarations_with_layout(src, Target::default(), Strings::new(arena), layout)
    }

    #[test]
    fn maintained_declarations_build_records_under_each_model() {
        let arena = Bump::new();
        for (layout, sizes, offsets, bound, value) in [
            (
                CDataLayout::NATIVE_LP64,
                [24, 24, 80],
                [48, 64],
                2,
                4294967297,
            ),
            (CDataLayout::WASM32, [12, 16, 56], [32, 44], 3, 1),
            (
                CDataLayout::NATIVE_LP64,
                [24, 24, 80],
                [48, 64],
                2,
                4294967297,
            ),
        ] {
            let p = declarations(FIXTURE, &arena, layout).unwrap();
            for (record, size) in p.structs.iter().zip(sizes) {
                assert_eq!(record.size, size, "{:?}", record.tag);
            }
            let global = |name| p.globals.iter().find(|g| g.name == name).unwrap();
            assert_eq!(p.size_of(&global("grid").ty), 18 * layout.pointer_bytes());
            assert!(matches!(global("bounds").ty, Ty::Array(a) if a.len == Some(bound)));
            let items = global("initialized").init.unwrap().items;
            assert_eq!(items.len(), 2);
            for (item, offset) in items.iter().zip(offsets) {
                assert!(matches!(item, InitItem::Expr { offset: o, .. } if *o == offset));
            }
            let InitItem::Expr { expr, .. } = items[1] else {
                panic!("scalar item")
            };
            assert_eq!(expr.ty, LONG);
            assert_eq!(eval_with_layout(expr, layout), Some(Const::Int(value)));
            let scalar = global("scalar_values").init.unwrap().items;
            assert!(matches!(scalar, [InitItem::Expr { offset, expr, .. }]
                if *offset == 2 * IntKind::Long.size_with_layout(layout)
                && eval_with_layout(expr, layout) == Some(Const::Int(value))));
            for (name, size, bytes) in [("narrow", 4, 4), ("wide", 12, 12), ("clipped", 8, 8)] {
                let g = global(name);
                assert_eq!(p.size_of(&g.ty), size);
                assert!(
                    matches!(g.init.unwrap().items, [InitItem::Bytes { offset: 0, bytes: b }] if b.len() == bytes)
                );
            }
            assert!(matches!(global("wide").ty, Ty::Array(a) if a.elem == INT));
        }
    }

    #[test]
    fn modes_preserve_nominal_identity_signedness_and_attribute_positions() {
        use IntKind::*;
        let arena = Bump::new();
        for layout in [CDataLayout::WASM32, CDataLayout::NATIVE_LP64] {
            let p = declarations(
                "typedef unsigned int U __attribute__((mode(__DI__)));\n\
                 typedef signed char __attribute__((__mode__(word))) W;\n\
                 typedef unsigned short P __attribute__((mode(__pointer__)));\n\
                 typedef signed int H __attribute__((mode(__HI__)));\n\
                 typedef unsigned long Q __attribute__((mode(QI)));\n\
                 typedef unsigned int T __attribute__((mode(TI)));\n\
                 U object; U function(U parameter);\n\
                 _Static_assert(__builtin_types_compatible_p(U, __typeof__(object)), \"object identity\");",
                &arena, layout,
            ).unwrap();
            let wasm = layout == CDataLayout::WASM32;
            for (name, kind) in [
                ("U", if wasm { ULLong } else { ULong }),
                ("W", if wasm { Int } else { Long }),
                ("P", if wasm { UInt } else { ULong }),
                ("H", Short),
                ("Q", UChar),
                ("T", UInt128),
            ] {
                assert_eq!(
                    p.typedefs.iter().find(|t| t.name == name).unwrap().ty,
                    Ty::Int(kind)
                );
            }
            let Ty::Func(f) = p.globals.iter().find(|g| g.name == "function").unwrap().ty else {
                panic!("function type")
            };
            assert_eq!(f.ret, Ty::Int(if wasm { ULLong } else { ULong }));
            assert_eq!(f.params, &[f.ret]);
        }
        let aliases = "typedef int X __attribute__((mode(DI), mode(word)));";
        assert!(declarations(aliases, &arena, CDataLayout::NATIVE_LP64).is_ok());
        assert!(declarations(aliases, &arena, CDataLayout::WASM32).is_err());
        for layout in [CDataLayout::WASM32, CDataLayout::NATIVE_LP64] {
            assert!(
                declarations(
                    "typedef unsigned int X __attribute__((mode(__word__), __mode__(pointer)));",
                    &arena,
                    layout,
                )
                .is_ok()
            );
        }
        for src in [
            "typedef int X __attribute__((mode(DI))); typedef long X;",
            "typedef unsigned int X __attribute__((mode(pointer))); typedef int X;",
            "_Bool x __attribute__((mode(DI)));",
            "int *x __attribute__((mode(DI)));",
        ] {
            assert!(
                declarations(src, &arena, CDataLayout::WASM32).is_err(),
                "{src}"
            );
        }
    }

    #[test]
    fn nonnative_declarations_fence_native_contracts_and_string_identity() {
        let arena = Bump::new();
        for (src, message) in [
            (
                "typedef long V __attribute__((vector_size(16)));",
                "C vector types",
            ),
            ("__builtin_va_list ap;", "va_list"),
            (
                "int f(void) { return __builtin_ia32_aesimc128(0); }",
                "x86 builtins",
            ),
            ("long double x;", "long double"),
            ("_Complex double x __attribute__((mode(TC)));", "TC mode"),
            ("int f(void) { asm(\"\"); return 0; }", "inline assembly"),
            ("int x asm(\"x\");", "asm labels"),
            ("long text[] = L\"ab\";", "incompatible string"),
            ("unsigned int text[] = L\"ab\";", "incompatible string"),
            ("int text[] = \"ab\";", "wrong width"),
            ("char text[] = L\"ab\";", "wrong width"),
            ("_Bool text[] = \"ab\";", "incompatible string"),
            ("_Atomic(long) x;", "`_Atomic` requires"),
            ("long _Atomic x;", "`_Atomic` requires"),
            ("long * _Atomic x;", "`_Atomic` requires"),
            (
                "struct B { long :0; long x; };",
                "non-native packed records",
            ),
            ("union B { long bits:3; };", "non-native packed records"),
            (
                "struct B { long x __attribute__((packed)); };",
                "non-native packed records",
            ),
            (
                "#pragma pack(push, 1)\nstruct B { long x; };\n#pragma pack(pop)\n",
                "non-native packed records",
            ),
            ("struct B { long bits:3; };", "non-native packed records"),
            (
                "struct B { long x; } __attribute__((packed));",
                "non-native packed records",
            ),
        ] {
            let err = declarations(src, &arena, CDataLayout::WASM32)
                .err()
                .unwrap();
            assert!(err.message.contains(message), "{src}: {err:?}");
        }
        // Retain the existing permissive LP64 unsigned-wide initializer path.
        assert!(
            declarations(
                "unsigned int text[] = L\"ab\";",
                &arena,
                CDataLayout::NATIVE_LP64
            )
            .is_ok()
        );
        // A borrowed native target must not select its unsigned wchar_t in ILP32.
        let p = Parser::declarations_with_layout(
            "int text[] = L\"ab\";",
            Target::Aarch64LinuxGnu,
            Strings::new(&arena),
            CDataLayout::WASM32,
        )
        .unwrap();
        assert!(matches!(p.globals[0].ty, Ty::Array(a) if a.elem == INT));
    }

    #[test]
    fn recursive_records_and_nested_string_initializers_use_one_model() {
        let arena = Bump::new();
        for (layout, node_size, text_size, string_offsets) in [
            (CDataLayout::WASM32, 16, 24, [32, 44]),
            (CDataLayout::NATIVE_LP64, 32, 32, [48, 60]),
            (CDataLayout::WASM32, 16, 24, [32, 44]),
        ] {
            let p = declarations(FIXTURE, &arena, layout).unwrap();
            let global = |name| p.globals.iter().find(|g| g.name == name).unwrap();
            let nodes = global("nodes");
            assert_eq!(p.size_of(&nodes.ty), 2 * node_size);
            assert_eq!(p.align_of(&nodes.ty), 16);
            let Ty::Array(a) = nodes.ty else {
                panic!("node array")
            };
            let Ty::Struct(id) = a.elem else {
                panic!("node record")
            };
            let node = &p.structs[id];
            assert!(matches!(node.members[2].ty, Ty::Ptr(t) if *t == Ty::Struct(id)));
            for (item, offset) in nodes.init.unwrap().items.iter().zip([
                node_size + 2 * layout.pointer_bytes(),
                node_size + 3 * layout.pointer_bytes(),
            ]) {
                assert!(matches!(item, InitItem::Expr { offset: o, .. } if *o == offset));
            }
            assert_eq!(nodes.init.unwrap().items.len(), 2);
            let texts = global("texts");
            assert_eq!(p.size_of(&texts.ty), 2 * text_size);
            assert_eq!(texts.init.unwrap().items.len(), 2);
            for ((item, offset), len) in texts
                .init
                .unwrap()
                .items
                .iter()
                .zip(string_offsets)
                .zip([12, 4])
            {
                assert!(
                    matches!(item, InitItem::Bytes { offset: o, bytes } if *o == offset && bytes.len() == len)
                );
            }
        }
        // Native constructor provenance must remain independent of test parses
        // and of which native profile the private seam borrowed.
        for target in Target::NATIVE {
            let p = Parser::new("", "native.c", target, Strings::new(&arena));
            assert_eq!(p.c_layout, CDataLayout::NATIVE_LP64);
            assert!(p.structs.is_empty());
            assert_eq!(p.size_of(&LONG), 8);
        }
    }

    #[test]
    fn clang_checks_maintained_wasm_and_native_declaration_oracle() {
        for target in [Some("wasm32-unknown-unknown"), None] {
            let mut cmd = std::process::Command::new("clang");
            if let Some(target) = target {
                cmd.arg(format!("--target={target}"));
            }
            let output = cmd
                .args([
                    "-std=gnu11",
                    "-ffreestanding",
                    "-nostdinc",
                    "-Werror",
                    "-fsyntax-only",
                ])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/wasm-scalar-declarations.c"
                ))
                .output()
                .expect("existing Clang is required for declaration oracle");
            assert!(
                output.status.success(),
                "{target:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
