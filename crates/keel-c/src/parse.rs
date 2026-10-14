//! The parser: declarations and statements. Expressions are in
//! `expr.rs`, initializers in `init.rs`.
//!
//! C cannot be parsed without knowing which names are types (`T * x;` is a
//! declaration if `T` is a typedef, a multiplication otherwise), so the
//! parser keeps the scopes, and since it already knows every name's type
//! it type-checks as it goes, producing the typed tree in `ast.rs`.
//!
//! Beyond ISO C it accepts what Apple's headers use: `__attribute__((..))`
//! (only `aligned`, `packed` and `returns_twice` mean anything), `__asm`
//! labels that rename a symbol, `__inline`, `__restrict`, nullability
//! qualifiers, `__extension__`, `__int128` and a few `__builtin_`s.

use crate::ast::*;
use crate::lex::{Tok, Token};
use crate::types::*;
use crate::{Error, Loc};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub type R<T> = Result<T, Error>;

/// A string literal: its bytes with the terminator, and whether it is wide.
#[derive(Clone, Debug)]
pub struct StrLit {
    pub bytes: Vec<u8>,
    pub wide: bool,
}

#[derive(Clone)]
enum Sym {
    Local(LocalId),
    Global(usize),
    /// By index in the typedef table.
    Typedef(usize),
    Enum(i64),
}

#[derive(Default)]
struct Scope {
    names: HashMap<String, Sym>,
    /// Struct, union and enum tags; enums are just `int`, so only their
    /// existence is recorded.
    tags: HashMap<String, Option<usize>>,
    /// Enum tags, by index in the enum table, for debug info.
    enums: HashMap<String, usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Storage {
    None,
    Typedef,
    Extern,
    Static,
}

struct DeclSpec {
    ty: Ty,
    sugar: Sugar,
    storage: Storage,
    is_const: bool,
    is_volatile: bool,
    inline: bool,
    thread_local: bool,
    attrs: Attrs,
}

#[derive(Default, Clone, Copy)]
pub struct Attrs {
    pub aligned: Option<u64>,
    pub packed: bool,
    pub returns_twice: bool,
}

/// A parsed declarator: the name (if not abstract) and the full type.
pub struct Declarator {
    pub name: Option<(String, Loc)>,
    pub ty: Ty,
    pub sugar: Sugar,
    /// `const` or `volatile` on the declared object itself.
    pub is_const: bool,
    pub is_volatile: bool,
}

pub(crate) struct SwitchCtx {
    pub ty: Ty,
    pub cases: Vec<(i64, usize)>,
    pub default: Option<usize>,
    pub labels: usize,
}

/// Per-function state.
pub(crate) struct FuncState {
    pub locals: Vec<Local>,
    /// Block scopes, by their parent, and the one being parsed.
    pub scopes: Vec<Option<usize>>,
    pub scope: Option<usize>,
    pub label_addrs: Vec<String>,
    pub ret: Ty,
    pub name: String,
    pub switches: Vec<SwitchCtx>,
}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    scopes: Vec<Scope>,
    pub structs: Vec<StructDef>,
    pub typedefs: Vec<TypedefDef>,
    pub enums: Vec<EnumDef>,
    pub globals: Vec<Global>,
    pub funcs: Vec<FuncDef>,
    pub strings: Vec<StrLit>,
    pub(crate) fs: Option<FuncState>,
    /// Functions declared at least once without `inline` (or with
    /// `extern`), which therefore get an external definition.
    external_def: HashSet<usize>,
    /// Parameters of the last parameter list parsed: those of a function
    /// definition's declarator, when its body follows.
    last_params: Vec<(Option<String>, Ty, Sugar, Loc)>,
    /// Counts names made up for static locals and compound literals.
    unique: usize,
}

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
    "__builtin_va_list",
    "typeof",
    "__typeof__",
    "__typeof",
    "_Complex",
    "_Atomic",
];

/// Qualifiers that change nothing for code generation.
const IGNORED_QUALIFIERS: &[&str] = &[
    "restrict",
    "__restrict",
    "__restrict__",
    "_Nullable",
    "_Nonnull",
    "_Null_unspecified",
    "__nullable",
    "__nonnull",
];

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self {
            toks,
            pos: 0,
            scopes: vec![Scope::default()],
            structs: Vec::new(),
            typedefs: Vec::new(),
            enums: Vec::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            strings: Vec::new(),
            fs: None,
            external_def: HashSet::new(),
            last_params: Vec::new(),
            unique: 0,
        }
    }

    // ---- tokens ----

    pub(crate) fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    pub(crate) fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }

    pub(crate) fn loc(&self) -> Loc {
        self.toks[self.pos].loc
    }

    pub(crate) fn advance(&mut self) -> Tok {
        let tok = self.toks[self.pos].tok.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    pub(crate) fn pos_save(&self) -> usize {
        self.pos
    }

    pub(crate) fn pos_restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub(crate) fn is(&self, p: &str) -> bool {
        match self.peek() {
            Tok::Punct(q) => *q == p,
            Tok::Ident(s) => s == p,
            _ => false,
        }
    }

    pub(crate) fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, p: &str) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("`{p}`")))
        }
    }

    pub(crate) fn unexpected(&self, wanted: &str) -> Error {
        let found = match self.peek() {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Punct(p) => format!("`{p}`"),
            Tok::Int { value, .. } => format!("`{value}`"),
            Tok::Float { value, .. } => format!("`{value}`"),
            Tok::Char(_) => "a character constant".into(),
            Tok::BadNumber(text) => format!("invalid number `{text}`"),
            Tok::Str { .. } => "a string".into(),
            Tok::Eof => "the end of the file".into(),
        };
        Error::new(self.loc(), format!("expected {wanted}, found {found}"))
    }

    pub(crate) fn ident(&mut self) -> R<String> {
        match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let s = s.clone();
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
        self.scopes.pop();
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

    fn lookup(&self, name: &str) -> Option<&Sym> {
        self.scopes.iter().rev().find_map(|s| s.names.get(name))
    }

    fn bind(&mut self, name: String, sym: Sym) {
        self.scopes.last_mut().unwrap().names.insert(name, sym);
    }

    pub(crate) fn types(&self) -> Types<'_> {
        Types(&self.structs)
    }

    pub(crate) fn size_of(&self, ty: &Ty) -> u64 {
        self.types().size(ty)
    }

    pub(crate) fn align_of(&self, ty: &Ty) -> u64 {
        self.types().align(ty)
    }

    pub(crate) fn typedef_type(&self, name: &str) -> Option<Ty> {
        match self.lookup(name) {
            Some(Sym::Typedef(i)) => Some(self.typedefs[*i].ty.clone()),
            _ => None,
        }
    }

    /// Whether the next token starts a type name or declaration.
    pub(crate) fn is_typename(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => TYPE_KEYWORDS.contains(&s.as_str()) || self.typedef_type(s).is_some(),
            _ => false,
        }
    }

    /// The value of an identifier in an expression.
    pub(crate) fn lookup_value(&mut self, name: &str, loc: Loc) -> R<Expr> {
        match self.lookup(name).cloned() {
            Some(Sym::Local(id)) => {
                let ty = self.fs.as_ref().unwrap().locals[id].ty.clone();
                Ok(Expr {
                    kind: ExprKind::Local(id),
                    ty,
                    loc,
                })
            }
            Some(Sym::Global(g)) => Ok(Expr {
                kind: ExprKind::Global(g),
                ty: self.globals[g].ty.clone(),
                loc,
            }),
            Some(Sym::Enum(v)) => Ok(Expr {
                kind: ExprKind::Int(v),
                ty: INT,
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
        name: String,
        ty: Ty,
        sugar: Sugar,
        volatile: bool,
    ) -> LocalId {
        let in_memory = volatile || !ty.is_scalar();
        let fs = self.fs.as_mut().unwrap();
        fs.locals.push(Local {
            name: name.clone(),
            ty,
            sugar,
            scope: fs.scope,
            in_memory,
        });
        let id = fs.locals.len() - 1;
        self.bind(name, Sym::Local(id));
        id
    }

    /// A made-up global name, unique in this file.
    pub(crate) fn unique_name(&mut self, base: &str) -> String {
        self.unique += 1;
        format!("{base}.{}", self.unique)
    }

    /// Adds an unnamed object with static storage, like a file-scope
    /// compound literal.
    pub(crate) fn add_static(&mut self, name: String, ty: Ty, init: Init, is_const: bool) -> usize {
        self.globals.push(Global {
            symbol: name.clone(),
            name,
            ty,
            sugar: Sugar::Plain,
            internal: true,
            defined: true,
            init: Some(init),
            is_const,
            thread_local: false,
            returns_twice: false,
            func: None,
        });
        self.globals.len() - 1
    }

    pub(crate) fn add_string(&mut self, bytes: Vec<u8>, wide: bool) -> usize {
        let lit = StrLit { bytes, wide };
        if let Some(i) = self
            .strings
            .iter()
            .position(|s| s.bytes == lit.bytes && s.wide == wide)
        {
            return i;
        }
        self.strings.push(lit);
        self.strings.len() - 1
    }

    // ---- translation unit ----

    pub fn translation_unit(&mut self) -> R<()> {
        while *self.peek() != Tok::Eof {
            if self.eat(";") {
                continue;
            }
            self.declaration(true)?;
        }
        Ok(())
    }

    /// A declaration, at file scope or in a block. Returns the statements
    /// initializing the locals it declares.
    pub(crate) fn declaration(&mut self, file_scope: bool) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        if self.static_assert()? {
            return Ok(out);
        }
        let spec = self.decl_spec()?;
        if self.eat(";") {
            return Ok(out);
        }
        loop {
            let loc = self.loc();
            let d = self.declarator_sugar(spec.ty.clone(), spec.sugar.clone())?;
            let mut attrs = spec.attrs;
            let label = self.asm_label_and_attrs(&mut attrs)?;
            let Some((name, name_loc)) = d.name.clone() else {
                return Err(Error::new(loc, "declaration needs a name"));
            };
            // The specifiers' qualifiers are on the object only if the
            // declarator adds nothing but arrays: `const char *a[2]` is an
            // array of mutable pointers.
            let mut elem = &d.ty;
            while let Ty::Array(inner, _) = elem {
                elem = inner;
            }
            let base = *elem == spec.ty;
            let is_const = d.is_const || (spec.is_const && base);
            let is_volatile = d.is_volatile || (spec.is_volatile && base);
            if spec.storage == Storage::Typedef {
                self.typedefs.push(TypedefDef {
                    name: name.clone(),
                    ty: d.ty,
                    sugar: d.sugar,
                });
                self.bind(name, Sym::Typedef(self.typedefs.len() - 1));
            } else if let Ty::Func(fty) = &d.ty {
                let g = self.declare_function(&name, label, fty.clone(), &spec, attrs, file_scope);
                self.globals[g].sugar = d.sugar.clone();
                if file_scope && self.is("{") {
                    self.function_body(g, fty.clone(), name_loc)?;
                    return Ok(out);
                }
            } else if !file_scope && spec.storage == Storage::None {
                // In scope from the end of its declarator: `T *p =
                // malloc(sizeof *p)` refers to itself.
                let id = self.new_local(name.clone(), d.ty.clone(), d.sugar.clone(), is_volatile);
                let mut ty = d.ty;
                let mut init = None;
                if self.eat("=") {
                    let (i, complete) = self.initializer(&ty)?;
                    ty = complete;
                    init = Some(i);
                }
                let incomplete = match &ty {
                    Ty::Array(_, None) | Ty::Void => true,
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
                local.in_memory |= !ty.is_scalar();
                local.ty = ty;
                out.push(match init {
                    Some(init) => Stmt::Init(id, init, loc),
                    None => Stmt::Declare(id),
                });
            } else {
                self.object_declaration(&spec, d, name, label, is_const, file_scope)?;
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")?;
        Ok(out)
    }

    fn static_assert(&mut self) -> R<bool> {
        if !self.eat("_Static_assert") {
            return Ok(false);
        }
        let loc = self.loc();
        self.expect("(")?;
        let value = self.const_int()?;
        if self.eat(",") {
            self.advance();
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
        spec: &DeclSpec,
        d: Declarator,
        name: String,
        label: Option<String>,
        is_const: bool,
        file_scope: bool,
    ) -> R<()> {
        let local_static = !file_scope && spec.storage == Storage::Static;
        let g = if local_static {
            let func = &self.fs.as_ref().unwrap().name;
            let symbol = format!("{func}.{name}");
            let symbol = if self.globals.iter().any(|g| g.symbol == symbol) {
                self.unique_name(&symbol)
            } else {
                symbol
            };
            self.globals.push(Global {
                name: name.clone(),
                symbol,
                ty: d.ty.clone(),
                sugar: d.sugar.clone(),
                internal: true,
                defined: true,
                init: None,
                is_const,
                thread_local: spec.thread_local,
                returns_twice: false,
                func: None,
            });
            let g = self.globals.len() - 1;
            self.bind(name.clone(), Sym::Global(g));
            g
        } else {
            let g = self.global(&name, label, d.ty.clone());
            let global = &mut self.globals[g];
            global.sugar = d.sugar.clone();
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
        if self.eat("=") {
            let loc = self.loc();
            if !file_scope && !local_static {
                return Err(Error::new(
                    loc,
                    "a block-scope `extern` cannot be initialized",
                ));
            }
            let ty = self.globals[g].ty.clone();
            let (init, ty) = self.initializer(&ty)?;
            let global = &mut self.globals[g];
            global.ty = ty;
            global.init = Some(init);
            global.defined = true;
        }
        Ok(())
    }

    /// Finds or adds the file-scope global `name`, merging this
    /// declaration's type into it (a later one may complete an array).
    fn global(&mut self, name: &str, label: Option<String>, ty: Ty) -> usize {
        let existing = self.scopes[0].names.get(name).and_then(|s| match s {
            Sym::Global(g) => Some(*g),
            _ => None,
        });
        let g = match existing {
            Some(g) => {
                let global = &mut self.globals[g];
                let more_complete = match (&global.ty, &ty) {
                    (Ty::Array(_, None), Ty::Array(_, Some(_))) => true,
                    (Ty::Func(old), Ty::Func(new)) => !old.prototyped && new.prototyped,
                    _ => false,
                };
                if more_complete {
                    global.ty = ty;
                }
                g
            }
            None => {
                self.globals.push(Global {
                    name: name.to_string(),
                    symbol: name.to_string(),
                    ty,
                    sugar: Sugar::Plain,
                    internal: false,
                    defined: false,
                    init: None,
                    is_const: false,
                    thread_local: false,
                    returns_twice: false,
                    func: None,
                });
                let g = self.globals.len() - 1;
                self.scopes[0]
                    .names
                    .insert(name.to_string(), Sym::Global(g));
                g
            }
        };
        if let Some(label) = label {
            self.globals[g].symbol = label;
        }
        // A block-scope declaration is visible in its block only.
        if self.scopes.len() > 1 {
            self.bind(name.to_string(), Sym::Global(g));
        }
        g
    }

    fn declare_function(
        &mut self,
        name: &str,
        label: Option<String>,
        fty: Rc<FuncTy>,
        spec: &DeclSpec,
        attrs: Attrs,
        file_scope: bool,
    ) -> usize {
        let g = self.global(name, label, Ty::Func(fty));
        if spec.storage == Storage::Static {
            self.globals[g].internal = true;
        }
        if !spec.inline || spec.storage == Storage::Extern || !file_scope {
            self.external_def.insert(g);
        }
        let returns_twice = attrs.returns_twice
            || matches!(
                name,
                "setjmp" | "_setjmp" | "sigsetjmp" | "__sigsetjmp" | "savectx" | "vfork"
            );
        self.globals[g].returns_twice |= returns_twice;
        g
    }

    fn function_body(&mut self, g: usize, fty: Rc<FuncTy>, loc: Loc) -> R<()> {
        if self.globals[g].func.is_some() {
            return Err(Error::new(
                loc,
                format!("`{}` is defined twice", self.globals[g].name),
            ));
        }
        // A C99 inline definition, never declared otherwise, is only an
        // inline definition; another file must provide the external one.
        // Making it internal gives this file its own copy instead.
        if !self.external_def.contains(&g) {
            self.globals[g].internal = true;
        }
        let params = std::mem::take(&mut self.last_params);
        let name = self.globals[g].name.clone();
        self.fs = Some(FuncState {
            locals: Vec::new(),
            scopes: Vec::new(),
            scope: None,
            label_addrs: Vec::new(),
            ret: fty.ret.clone(),
            name,
            switches: Vec::new(),
        });
        self.push_scope();
        let mut ids = Vec::new();
        for (name, ty, sugar, ploc) in params {
            let Some(name) = name else {
                return Err(Error::new(ploc, "parameter needs a name"));
            };
            ids.push(self.new_local(name, ty, sugar, false));
        }
        self.expect("{")?;
        let body = self.block_items()?;
        self.pop_scope();
        let fs = self.fs.take().unwrap();
        self.funcs.push(FuncDef {
            global: g,
            ty: fty,
            params: ids,
            locals: fs.locals,
            scopes: fs.scopes,
            label_addrs: fs.label_addrs,
            body,
            loc,
        });
        let global = &mut self.globals[g];
        global.func = Some(self.funcs.len() - 1);
        global.defined = true;
        Ok(())
    }

    /// `__asm("label")` and attributes after a declarator.
    fn asm_label_and_attrs(&mut self, attrs: &mut Attrs) -> R<Option<String>> {
        let mut label = None;
        loop {
            if self.is("__asm") || self.is("__asm__") || self.is("asm") {
                self.advance();
                self.expect("(")?;
                let mut text = Vec::new();
                while let Tok::Str { bytes, .. } = self.peek() {
                    text.extend_from_slice(bytes);
                    self.advance();
                }
                self.expect(")")?;
                let text = String::from_utf8_lossy(&text).into_owned();
                // Keel adds the `_` every Darwin symbol starts with.
                let Some(name) = text.strip_prefix('_') else {
                    return Err(Error::new(
                        self.loc(),
                        format!("asm label `{text}` does not start with `_`"),
                    ));
                };
                label = Some(name.to_string());
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
                let name = match self.advance() {
                    Tok::Ident(s) => s,
                    _ => return Err(self.unexpected("an attribute")),
                };
                let name = name.trim_matches('_');
                match name {
                    "aligned" if self.eat("(") => {
                        let n = self.const_int()? as u64;
                        attrs.aligned = Some(attrs.aligned.unwrap_or(1).max(n));
                        self.expect(")")?;
                    }
                    "aligned" => attrs.aligned = Some(16),
                    _ => {
                        match name {
                            "packed" => attrs.packed = true,
                            "returns_twice" => attrs.returns_twice = true,
                            _ => {}
                        }
                        if self.eat("(") {
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

    // ---- types ----

    fn decl_spec(&mut self) -> R<DeclSpec> {
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

        let loc = self.loc();
        let mut spec = DeclSpec {
            ty: Ty::Int(IntKind::Int),
            sugar: Sugar::Plain,
            storage: Storage::None,
            is_const: false,
            is_volatile: false,
            inline: false,
            thread_local: false,
            attrs: Attrs::default(),
        };
        let mut counter = 0u32;
        let mut other: Option<Ty> = None;
        let mut is_atomic = false;
        while let Tok::Ident(word) = self.peek().clone() {
            let storage = match word.as_str() {
                "typedef" => Some(Storage::Typedef),
                "extern" => Some(Storage::Extern),
                "static" => Some(Storage::Static),
                "auto" | "register" => Some(Storage::None),
                _ => None,
            };
            if let Some(s) = storage {
                self.advance();
                spec.storage = s;
                continue;
            }
            match word.as_str() {
                "const" | "__const" => spec.is_const = true,
                "volatile" | "__volatile" | "__volatile__" => spec.is_volatile = true,
                "inline" | "__inline" | "__inline__" => spec.inline = true,
                "_Thread_local" | "__thread" => spec.thread_local = true,
                "_Noreturn" => {}
                w if IGNORED_QUALIFIERS.contains(&w) => {}
                "_Atomic" if !matches!(self.peek_at(1), Tok::Punct("(")) => is_atomic = true,
                "_Complex" => {
                    return Err(Error::new(self.loc(), format!("`{word}` is not supported")));
                }
                "__attribute__" | "__attribute" => {
                    let mut attrs = spec.attrs;
                    self.attributes(&mut attrs)?;
                    spec.attrs = attrs;
                    continue;
                }
                "_Alignas" => {
                    self.advance();
                    self.expect("(")?;
                    let n = if self.is_typename() {
                        let ty = self.type_name()?;
                        self.align_of(&ty)
                    } else {
                        self.const_int()? as u64
                    };
                    self.expect(")")?;
                    spec.attrs.aligned = Some(spec.attrs.aligned.unwrap_or(1).max(n));
                    continue;
                }
                _ => {
                    // A type specifier. A typedef name only counts if no
                    // other type specifier came before it (`T x` vs a
                    // redeclaration `int T`).
                    let ty = match word.as_str() {
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
                            let ty = self.type_name()?;
                            self.expect(")")?;
                            Some(self.atomic_type(ty, loc)?)
                        }
                        "typeof" | "__typeof__" | "__typeof" => {
                            self.advance();
                            self.expect("(")?;
                            let ty = if self.is_typename() {
                                self.type_name()?
                            } else {
                                self.expr()?.ty
                            };
                            self.expect(")")?;
                            Some(ty)
                        }
                        "__builtin_va_list" => {
                            self.advance();
                            Some(Ty::ptr_to(crate::types::CHAR))
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
                                Some(self.typedefs[i].ty.clone())
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
                    let bit = match word.as_str() {
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
        spec.ty = match counter {
            OTHER => other.unwrap(),
            0 => {
                if spec.storage == Storage::None && !spec.is_const && !spec.inline {
                    return Err(self.unexpected("a type"));
                }
                Ty::Int(Int)
            }
            VOID => Ty::Void,
            BOOL => Ty::Int(Bool),
            CHAR => Ty::Int(Char),
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
            c if c == LONG + DOUBLE => Ty::Double,
            HALF => Ty::Half,
            _ => return Err(Error::new(loc, "invalid combination of type specifiers")),
        };
        if is_atomic {
            spec.ty = self.atomic_type(spec.ty, loc)?;
        }
        if spec.is_volatile {
            spec.sugar = Sugar::Volatile(Rc::new(spec.sugar));
        }
        if spec.is_const {
            spec.sugar = Sugar::Const(Rc::new(spec.sugar));
        }
        Ok(spec)
    }

    /// A type name, as in casts and `sizeof`: specifiers and an abstract
    /// declarator.
    pub(crate) fn type_name(&mut self) -> R<Ty> {
        let spec = self.decl_spec()?;
        Ok(self.declarator(spec.ty)?.ty)
    }

    /// `_Atomic(ty)`, which keel supports for integers and pointers.
    fn atomic_type(&self, ty: Ty, loc: Loc) -> R<Ty> {
        match ty {
            Ty::Atomic(_) => Ok(ty),
            ty if ty.is_integer() || ty.is_ptr() => Ok(Ty::Atomic(Rc::new(ty))),
            _ => Err(Error::new(
                loc,
                "`_Atomic` is only supported on integers and pointers",
            )),
        }
    }

    /// Pointer qualifiers after a `*`. Returns (const, volatile, atomic).
    fn qualifiers(&mut self) -> R<(bool, bool, bool)> {
        let (mut c, mut v, mut a) = (false, false, false);
        loop {
            match self.peek() {
                Tok::Ident(w) if w == "_Atomic" => a = true,
                Tok::Ident(w) if w == "const" || w == "__const" => c = true,
                Tok::Ident(w) if w == "volatile" || w == "__volatile__" || w == "__volatile" => {
                    v = true
                }
                Tok::Ident(w) if IGNORED_QUALIFIERS.contains(&w.as_str()) => {}
                Tok::Ident(w) if w == "__attribute__" || w == "__attribute" => {
                    self.attributes(&mut Attrs::default())?;
                    continue;
                }
                _ => return Ok((c, v, a)),
            }
            self.advance();
        }
    }

    /// A declarator, possibly abstract, applied to the specifiers' type.
    pub(crate) fn declarator(&mut self, ty: Ty) -> R<Declarator> {
        self.declarator_sugar(ty, Sugar::Plain)
    }

    /// A declarator applied to the specifiers' type and its sugar.
    fn declarator_sugar(&mut self, mut ty: Ty, mut sugar: Sugar) -> R<Declarator> {
        let (mut is_const, mut is_volatile) = (false, false);
        // `^` is a block pointer (Apple's closures, in headers when
        // `__BLOCKS__` is defined); as a value it is just a pointer.
        while self.eat("*") || self.eat("^") {
            let loc = self.loc();
            ty = Ty::ptr_to(ty);
            sugar = sugar.wrap(Sugar::Ptr);
            let is_atomic;
            (is_const, is_volatile, is_atomic) = self.qualifiers()?;
            if is_atomic {
                ty = self.atomic_type(ty, loc)?;
            }
            if is_volatile {
                sugar = Sugar::Volatile(Rc::new(sugar));
            }
            if is_const {
                sugar = Sugar::Const(Rc::new(sugar));
            }
        }
        self.attributes(&mut Attrs::default())?;
        if self.is("(") && self.nested_declarator_follows() {
            // `(*name)[3]`: the suffix after the parentheses applies first,
            // so skip the inner declarator, read the suffix, and come back.
            self.advance();
            let start = self.pos;
            self.skip_parens()?;
            let (ty, sugar) = self.type_suffix(ty, sugar)?;
            let end = self.pos;
            self.pos = start;
            let d = self.declarator_sugar(ty, sugar)?;
            self.expect(")")?;
            self.pos = end;
            return Ok(d);
        }
        let name = match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let loc = self.loc();
                let s = s.clone();
                self.advance();
                Some((s, loc))
            }
            _ => None,
        };
        let (ty, sugar) = self.type_suffix(ty, sugar)?;
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
        })
    }

    /// After `(` in a declarator: a nested declarator, or parameters?
    fn nested_declarator_follows(&self) -> bool {
        match self.peek_at(1) {
            Tok::Punct("*") | Tok::Punct("(") | Tok::Punct("[") | Tok::Punct("^") => true,
            Tok::Ident(s) => {
                !TYPE_KEYWORDS.contains(&s.as_str()) && self.typedef_type(s).is_none()
                    || s == "__attribute__"
            }
            _ => false,
        }
    }

    fn type_suffix(&mut self, ty: Ty, sugar: Sugar) -> R<(Ty, Sugar)> {
        if self.eat("(") {
            let fty = self.params(ty)?;
            return Ok((Ty::Func(Rc::new(fty)), sugar.wrap(Sugar::Func)));
        }
        if self.eat("[") {
            while self.eat("static") || self.qualifiers()? != (false, false, false) {}
            let loc = self.loc();
            let len = if self.is("]") {
                None
            } else if self.is("*") && matches!(self.peek_at(1), Tok::Punct("]")) {
                return Err(Error::new(loc, "variable-length arrays are not supported"));
            } else {
                let e = self.assign()?;
                match crate::eval::eval_int(&e) {
                    Some(n) if n >= 0 => Some(n as u64),
                    Some(_) => return Err(Error::new(loc, "array size is negative")),
                    None => {
                        return Err(Error::new(loc, "variable-length arrays are not supported"));
                    }
                }
            };
            self.expect("]")?;
            let (elem, sugar) = self.type_suffix(ty, sugar)?;
            return Ok((Ty::Array(Rc::new(elem), len), sugar.wrap(Sugar::Array)));
        }
        Ok((ty, sugar))
    }

    /// A parameter list, after the `(`.
    fn params(&mut self, ret: Ty) -> R<FuncTy> {
        let mut fty = FuncTy {
            ret,
            params: Vec::new(),
            variadic: false,
            prototyped: true,
        };
        let mut names = Vec::new();
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
        loop {
            if self.eat("...") {
                fty.variadic = true;
                break;
            }
            let loc = self.loc();
            if !self.is_typename() {
                return Err(Error::new(
                    loc,
                    "old-style parameter lists are not supported",
                ));
            }
            let spec = self.decl_spec()?;
            let d = self.declarator_sugar(spec.ty, spec.sugar)?;
            self.attributes(&mut Attrs::default())?;
            // Arrays and functions are passed as pointers.
            let sugar = match (&d.ty, d.sugar) {
                (Ty::Func(_), s) => s.wrap(Sugar::Ptr),
                (_, Sugar::Array(elem)) => Sugar::Ptr(elem),
                (_, s) => s,
            };
            let ty = match d.ty {
                Ty::Array(elem, _) => Ty::Ptr(elem),
                Ty::Func(_) => Ty::ptr_to(d.ty),
                ty => ty,
            };
            fty.params.push(ty.clone());
            names.push((d.name.map(|(n, _)| n), ty, sugar, loc));
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        self.last_params = names;
        Ok(fty)
    }

    fn struct_spec(&mut self, is_union: bool) -> R<Ty> {
        let mut attrs = Attrs::default();
        self.attributes(&mut attrs)?;
        let loc = self.loc();
        let tag = match self.peek() {
            Tok::Ident(s) if !is_keyword(s) => {
                let s = s.clone();
                self.advance();
                Some(s)
            }
            _ => None,
        };
        let new_struct = |p: &mut Parser, tag: Option<String>| {
            p.structs.push(StructDef {
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
                    Some(id) => Ok(Ty::Struct(*id)),
                    None => Err(Error::new(loc, format!("`{tag}` is an enum"))),
                };
            }
            let id = new_struct(self, Some(tag.clone()));
            self.scopes.last_mut().unwrap().tags.insert(tag, Some(id));
            return Ok(Ty::Struct(id));
        }
        let pack = self.toks[self.pos].pack;
        self.advance();
        let id = match &tag {
            Some(t) => match self.scopes.last().unwrap().tags.get(t) {
                Some(Some(id)) if !self.structs[*id].complete => *id,
                _ => {
                    let id = new_struct(self, Some(t.clone()));
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .tags
                        .insert(t.clone(), Some(id));
                    id
                }
            },
            None => new_struct(self, None),
        };
        let mut fields = Vec::new();
        while !self.eat("}") {
            if self.static_assert()? {
                continue;
            }
            let floc = self.loc();
            let spec = self.decl_spec()?;
            if self.eat(";") {
                // An anonymous struct or union member.
                if matches!(spec.ty, Ty::Struct(_)) {
                    fields.push((None, spec.ty, Sugar::Plain, None));
                }
                continue;
            }
            loop {
                let (name, ty, sugar) = if self.is(":") {
                    (None, spec.ty.clone(), spec.sugar.clone())
                } else {
                    let d = self.declarator_sugar(spec.ty.clone(), spec.sugar.clone())?;
                    (d.name.map(|(n, _)| n), d.ty, d.sugar)
                };
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
                let mut member_attrs = Attrs::default();
                self.attributes(&mut member_attrs)?;
                if let Some(a) = member_attrs.aligned {
                    // Only alignment on struct members is supported through
                    // the struct's own alignment.
                    attrs.aligned = Some(attrs.aligned.unwrap_or(1).max(a));
                }
                fields.push((name, ty, sugar, width));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.attributes(&mut attrs)?;
        let (members, mut size, mut align) = layout_struct(
            &self.types(),
            is_union,
            fields,
            if attrs.packed { Some(1) } else { pack },
        );
        if let Some(a) = attrs.aligned {
            align = align.max(a);
            size = size.next_multiple_of(align);
        }
        let def = &mut self.structs[id];
        def.members = members;
        def.size = size;
        def.align = align;
        def.complete = true;
        Ok(Ty::Struct(id))
    }

    /// An enum specifier: its values' type, and the enum for debug info.
    fn enum_spec(&mut self) -> R<(Ty, Sugar)> {
        self.attributes(&mut Attrs::default())?;
        let mut tag = None;
        if let Tok::Ident(s) = self.peek()
            && !is_keyword(s)
        {
            let s = s.clone();
            self.advance();
            self.scopes.last_mut().unwrap().tags.insert(s.clone(), None);
            tag = Some(s);
        }
        // C23's fixed underlying type, `enum : uint32_t { .. }`, which
        // Apple's headers use; otherwise the type is `int`.
        let ty = if self.eat(":") {
            self.type_name()?
        } else {
            INT
        };
        if !self.eat("{") {
            let known = tag
                .as_ref()
                .and_then(|t| self.scopes.iter().rev().find_map(|s| s.enums.get(t)));
            return Ok((ty, known.map_or(Sugar::Plain, |&i| Sugar::Enum(i))));
        }
        let mut values = Vec::new();
        let mut next: i64 = 0;
        while !self.eat("}") {
            let name = self.ident()?;
            self.attributes(&mut Attrs::default())?;
            if self.eat("=") {
                next = self.const_int()?;
            }
            values.push((name.clone(), next));
            self.bind(name, Sym::Enum(next));
            next = next.wrapping_add(1);
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
        }
        self.attributes(&mut Attrs::default())?;
        self.enums.push(EnumDef {
            tag: tag.clone(),
            ty: ty.clone(),
            values,
        });
        let i = self.enums.len() - 1;
        if let Some(tag) = tag {
            self.scopes.last_mut().unwrap().enums.insert(tag, i);
        }
        Ok((ty, Sugar::Enum(i)))
    }

    /// An integer constant expression.
    pub(crate) fn const_int(&mut self) -> R<i64> {
        let loc = self.loc();
        let e = self.conditional()?;
        crate::eval::eval_int(&e).ok_or_else(|| Error::new(loc, "expected a constant expression"))
    }

    // ---- statements ----

    /// The items of a `{}` block, after the `{`, in a new scope.
    pub(crate) fn block_items(&mut self) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        while !self.eat("}") {
            if self.is_declaration_start() {
                out.extend(self.declaration(false)?);
            } else {
                out.push(self.statement()?);
            }
        }
        Ok(out)
    }

    fn is_declaration_start(&self) -> bool {
        if self.is("_Static_assert") {
            return true;
        }
        // `T:` is a label, even if `T` is a typedef name.
        self.is_typename() && !matches!(self.peek_at(1), Tok::Punct(":"))
    }

    fn statement(&mut self) -> R<Stmt> {
        let loc = self.loc();
        if let Tok::Ident(word) = self.peek().clone() {
            match word.as_str() {
                "if" => {
                    self.advance();
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    let then = self.statement()?;
                    let els = if self.eat("else") {
                        Some(Box::new(self.statement()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(cond, Box::new(then), els));
                }
                "while" => {
                    self.advance();
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    let body = self.statement()?;
                    return Ok(Stmt::While(cond, Box::new(body)));
                }
                "do" => {
                    self.advance();
                    let body = self.statement()?;
                    self.expect("while")?;
                    self.expect("(")?;
                    let cond = self.condition()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    return Ok(Stmt::DoWhile(Box::new(body), cond));
                }
                "for" => {
                    self.advance();
                    self.expect("(")?;
                    let scope = self.push_block();
                    let init = if self.eat(";") {
                        None
                    } else if self.is_declaration_start() {
                        Some(Box::new(Stmt::Block(self.declaration(false)?)))
                    } else {
                        let e = self.expr()?;
                        self.expect(";")?;
                        Some(Box::new(Stmt::Expr(e)))
                    };
                    let cond = if self.is(";") {
                        None
                    } else {
                        Some(self.condition()?)
                    };
                    self.expect(";")?;
                    let step = if self.is(")") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(")")?;
                    let body = self.statement()?;
                    self.pop_block();
                    let for_ = Stmt::For {
                        init,
                        cond,
                        step,
                        body: Box::new(body),
                    };
                    return Ok(Stmt::Scoped(scope, vec![for_]));
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
                        ty: cond.ty.clone(),
                        cases: Vec::new(),
                        default: None,
                        labels: 0,
                    });
                    let body = self.statement()?;
                    let sw = self.fs.as_mut().unwrap().switches.pop().unwrap();
                    return Ok(Stmt::Switch {
                        cond,
                        body: Box::new(body),
                        cases: sw.cases,
                        default: sw.default,
                        labels: sw.labels,
                    });
                }
                "case" | "default" => {
                    self.advance();
                    // GNU's `case 1 ... 5:` is a case for each value.
                    let values = if word == "case" {
                        let v = self.const_int()?;
                        let last = if self.eat("...") {
                            self.const_int()?
                        } else {
                            v
                        };
                        if last < v || last - v >= 1 << 16 {
                            return Err(Error::new(loc, "case range is empty or too big"));
                        }
                        Some(v..=last)
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
                        Some(values) => {
                            for v in values {
                                let v = crate::eval::normalize(v, &sw.ty);
                                if sw.cases.iter().any(|&(c, _)| c == v) {
                                    return Err(Error::new(loc, format!("duplicate case {v}")));
                                }
                                sw.cases.push((v, label));
                            }
                        }
                        None => {
                            if sw.default.is_some() {
                                return Err(Error::new(loc, "two `default` labels"));
                            }
                            sw.default = Some(label);
                        }
                    }
                    let stmt = self.labeled_statement()?;
                    return Ok(Stmt::Block(vec![Stmt::Case(label), stmt]));
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
                    let ret = self.fs.as_ref().unwrap().ret.clone();
                    if ret == Ty::Void {
                        // `return f();` in a void function, f returning void.
                        return Ok(Stmt::Block(vec![Stmt::Expr(e), Stmt::Return(None, loc)]));
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
                    return Ok(Stmt::Goto(label));
                }
                "asm" | "__asm" | "__asm__" => {
                    return Err(Error::new(loc, "inline assembly is not supported yet"));
                }
                _ if matches!(self.peek_at(1), Tok::Punct(":")) && !is_keyword(&word) => {
                    self.advance();
                    self.advance();
                    self.attributes(&mut Attrs::default())?;
                    let stmt = self.labeled_statement()?;
                    return Ok(Stmt::Block(vec![Stmt::Label(word), stmt]));
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
            return Ok(Stmt::Block(Vec::new()));
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(Stmt::Expr(e))
    }

    /// The statement after a label. C23 allows a declaration there, and
    /// so do compilers in practice; so does this.
    fn labeled_statement(&mut self) -> R<Stmt> {
        if self.is("}") {
            return Ok(Stmt::Block(Vec::new()));
        }
        if self.is_declaration_start() {
            return Ok(Stmt::Block(self.declaration(false)?));
        }
        self.statement()
    }

    /// A controlling expression: any scalar.
    fn condition(&mut self) -> R<Expr> {
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
impl Parser {
    /// A type a member cannot have. A struct or union of size 0 is
    /// complete (GNU allows empty ones, and zero-length arrays); an array
    /// with no length is a flexible array member if its element is.
    fn incomplete(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Void | Ty::Func(_) => true,
            Ty::Struct(id) => !self.structs[*id].complete,
            Ty::Array(elem, _) => self.incomplete(elem),
            _ => false,
        }
    }
}

/// Words that cannot be identifiers.
pub(crate) fn is_keyword(s: &str) -> bool {
    const OTHER: &[&str] = &[
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
    (TYPE_KEYWORDS.contains(&s) && !s.starts_with("__builtin") && !s.ends_with("_t"))
        || OTHER.contains(&s)
}
