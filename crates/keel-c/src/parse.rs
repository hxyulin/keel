//! The parser: declarations and statements. Expressions are in
//! `expr.rs`, initializers in `init.rs`.
//!
//! C cannot be parsed without knowing which names are types (`T * x;` is a
//! declaration if `T` is a typedef, a multiplication otherwise), so the
//! parser keeps the scopes, and since it already knows every name's type
//! it type-checks as it goes, producing the typed tree in `ast.rs`.
//!
//! Beyond ISO C it accepts what Apple's headers use: `__attribute__((..))`
//! (`aligned`, `packed`, `returns_twice` and `noreturn` have semantics), `__asm`
//! labels that rename a symbol, `__inline`, `__restrict`, nullability
//! qualifiers, `__extension__`, `__int128` and a few `__builtin_`s.

use crate::ast::*;
use crate::lex::{Tok, Token};
use crate::strings::Strings;
use crate::types::*;
use crate::{Error, Loc};
use keel_ir::Target;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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
    /// By index in the typedef table.
    Typedef(usize),
    Enum(i64),
}

#[derive(Default)]
struct Scope<'a> {
    /// Names first bound in this scope, restored in reverse order on exit.
    names: Vec<&'a str>,
    /// Struct, union and enum tags; enums are just `int`, so only their
    /// existence is recorded.
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

struct DeclSpec {
    ty: Ty,
    sugar: Sugar,
    infer: bool,
    storage: Storage,
    is_const: bool,
    is_volatile: bool,
    inline: bool,
    thread_local: bool,
    attrs: Attrs,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub struct Attrs {
    pub aligned: Option<u64>,
    pub alignas: Option<u64>,
    pub packed: bool,
    pub returns_twice: bool,
    pub noreturn: bool,
    pub function_attrs: keel_ir::FunctionAttrs,
}

impl Attrs {
    fn merge(&mut self, other: Self) {
        if let Some(a) = other.aligned {
            self.aligned = Some(self.aligned.unwrap_or(1).max(a));
        }
        if let Some(a) = other.alignas {
            self.alignas = Some(self.alignas.unwrap_or(0).max(a));
        }
        self.packed |= other.packed;
        self.returns_twice |= other.returns_twice;
        self.noreturn |= other.noreturn;
        self.function_attrs.merge(other.function_attrs);
    }
    fn alignment(self) -> Option<u64> {
        self.aligned
            .into_iter()
            .chain(self.alignas.filter(|a| *a != 0))
            .max()
    }
}

/// A parsed declarator: the name (if not abstract) and the full type.
pub struct Declarator<'a> {
    pub name: Option<(&'a str, Loc)>,
    pub ty: Ty,
    pub sugar: Sugar,
    /// `const` or `volatile` on the declared object itself.
    pub is_const: bool,
    pub is_volatile: bool,
    pub attrs: Attrs,
}

pub(crate) struct SwitchCtx {
    pub ty: Ty,
    pub cases: Vec<(i64, usize)>,
    pub default: Option<usize>,
    pub labels: usize,
}

/// Per-function state.
pub(crate) struct FuncState<'a> {
    pub locals: Vec<Local<'a>>,
    /// Block scopes, by their parent, and the one being parsed.
    pub scopes: Vec<Option<usize>>,
    pub scope: Option<usize>,
    pub label_addrs: Vec<&'a str>,
    pub ret: Ty,
    pub name: &'a str,
    pub switches: Vec<SwitchCtx>,
}

pub struct Parser<'a> {
    toks: Vec<Token<'a>>,
    pub(crate) text: Strings<'a>,
    pos: usize,
    scopes: Vec<Scope<'a>>,
    /// One spelling lookup, followed by its innermost binding. The short
    /// stack also supports file-scope declarations introduced inside blocks.
    names: HashMap<&'a str, SmallVec<[(usize, Sym); 2]>>,
    type_nodes: TypeNodes,
    plain_sugar: Rc<Sugar>,
    pub structs: Vec<StructDef<'a>>,
    pub typedefs: Vec<TypedefDef<'a>>,
    pub enums: Vec<EnumDef<'a>>,
    pub globals: Vec<Global<'a>>,
    pub funcs: Vec<FuncDef<'a>>,
    pub strings: Vec<StrLit<'a>>,
    literal_ids: [HashMap<&'a [u8], usize>; 2],
    pub(crate) fs: Option<FuncState<'a>>,
    /// Functions declared at file scope at least once without `inline` (or with
    /// `extern`), for which inline-only emission does not apply.
    /// Explicit static linkage, recorded in `Global::internal`, still wins.
    external_def: HashSet<usize>,
    /// Parameters of the last parameter list parsed: those of a function
    /// definition's declarator, when its body follows.
    last_params: Vec<(Option<&'a str>, Ty, Sugar, Loc)>,
    /// Counts names made up for static locals and compound literals.
    unique: usize,
    pub(crate) target: Target,
    /// SysV's `struct __va_list_tag`, once a `va_list` needs it.
    va_list_tag: Option<usize>,
}

fn is_type_keyword(s: &str) -> bool {
    matches!(
        s,
        "void"
            | "_Bool"
            | "char"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "signed"
            | "unsigned"
            | "__signed"
            | "__signed__"
            | "struct"
            | "union"
            | "enum"
            | "typedef"
            | "extern"
            | "static"
            | "auto"
            | "register"
            | "const"
            | "volatile"
            | "restrict"
            | "__restrict"
            | "__restrict__"
            | "__const"
            | "__volatile"
            | "__volatile__"
            | "inline"
            | "__inline"
            | "__inline__"
            | "_Noreturn"
            | "_Thread_local"
            | "__thread"
            | "_Alignas"
            | "__attribute__"
            | "__attribute"
            | "_Nullable"
            | "_Nonnull"
            | "_Null_unspecified"
            | "__nullable"
            | "__nonnull"
            | "__int128"
            | "__int128_t"
            | "__uint128_t"
            | "_Float16"
            | "__float128"
            | "__builtin_va_list"
            | "typeof"
            | "__typeof__"
            | "__typeof"
            | "__auto_type"
            | "_Complex"
            | "_Atomic"
    )
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
    "typeof",
    "__typeof__",
    "__typeof",
    "__auto_type",
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

impl<'a> Parser<'a> {
    pub fn new(toks: Vec<Token<'a>>, target: Target, text: Strings<'a>) -> Self {
        Self {
            toks,
            text,
            pos: 0,
            scopes: vec![Scope::default()],
            names: HashMap::new(),
            type_nodes: TypeNodes::default(),
            plain_sugar: Rc::new(Sugar::Plain),
            structs: Vec::new(),
            typedefs: Vec::new(),
            enums: Vec::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            strings: Vec::new(),
            literal_ids: [HashMap::new(), HashMap::new()],
            fs: None,
            external_def: HashSet::new(),
            last_params: Vec::new(),
            unique: 0,
            target,
            va_list_tag: None,
        }
    }

    /// `__builtin_va_list`. Apple's is a pointer to the next argument.
    /// SysV's is `struct __va_list_tag[1]`: the offsets of the next
    /// general and vector register in the callee's register save area,
    /// the next stack argument, and the save area. Being an array, it
    /// decays to a pointer when passed, and a `va_list` parameter is one.
    fn va_list(&mut self) -> Ty {
        if self.target == Target::Aarch64AppleDarwin {
            return self.ptr_type(crate::types::CHAR);
        }
        let id = *self.va_list_tag.get_or_insert_with(|| {
            let member = |name: &'static str, ty: Ty, offset| Member {
                align: Types(&self.structs).align(&ty),
                name: Some(name.into()),
                ty,
                sugar: Sugar::Plain,
                offset,
                bits: None,
            };
            let void_ptr = Ty::Ptr(Rc::new(Ty::Void));
            self.structs.push(StructDef {
                tag: Some("__va_list_tag".into()),
                is_union: false,
                members: vec![
                    member("gp_offset", crate::types::UINT, 0),
                    member("fp_offset", crate::types::UINT, 4),
                    member("overflow_arg_area", void_ptr.clone(), 8),
                    member("reg_save_area", void_ptr, 16),
                ],
                size: 24,
                align: 8,
                complete: true,
            });
            self.structs.len() - 1
        });
        Ty::Array(self.type_ref(Ty::Struct(id)), Some(1))
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
            Tok::Punct(q) => q == p,
            Tok::Ident(s) => s == p,
            _ => false,
        }
    }

    pub(crate) fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.pos = (self.pos + 1).min(self.toks.len() - 1);
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
            let bindings = self.names.get_mut(name).unwrap();
            debug_assert_eq!(bindings.last().unwrap().0, depth);
            bindings.pop();
            if bindings.is_empty() {
                self.names.remove(name);
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

    fn lookup(&self, name: &str) -> Option<&Sym> {
        self.names
            .get(name)
            .and_then(|bindings| bindings.last())
            .map(|(_, sym)| sym)
    }

    fn bind(&mut self, name: &'a str, sym: Sym) {
        self.bind_at(self.scopes.len() - 1, name, sym);
    }

    fn bind_at(&mut self, depth: usize, name: &'a str, sym: Sym) {
        let bindings = self.names.entry(name).or_default();
        match bindings.binary_search_by_key(&depth, |&(depth, _)| depth) {
            Ok(i) => bindings[i].1 = sym,
            Err(i) => {
                bindings.insert(i, (depth, sym));
                self.scopes[depth].names.push(name);
            }
        }
    }

    pub(crate) fn type_ref(&mut self, ty: Ty) -> Rc<Ty> {
        self.type_nodes.node(ty)
    }

    pub(crate) fn ptr_type(&mut self, ty: Ty) -> Ty {
        Ty::Ptr(self.type_ref(ty))
    }

    pub(crate) fn sugar_ref(&self, sugar: Sugar) -> Rc<Sugar> {
        match sugar {
            Sugar::Plain => self.plain_sugar.clone(),
            other => Rc::new(other),
        }
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

    pub(crate) fn type_alignment(&self, ty: &Ty, sugar: &Sugar) -> u64 {
        sugar
            .alignment(&self.typedefs)
            .unwrap_or_else(|| self.align_of(ty))
    }

    fn object_alignment(&self, ty: &Ty, sugar: &Sugar, attrs: Attrs, loc: Loc) -> R<u64> {
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

    pub(crate) fn typedef_type(&self, name: &str) -> Option<Ty> {
        match self.lookup(name) {
            Some(Sym::Typedef(i)) => Some(self.typedefs[*i].ty.clone()),
            _ => None,
        }
    }

    /// Whether the next token starts a type name or declaration.
    pub(crate) fn is_typename(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => is_type_keyword(s) || self.typedef_type(s).is_some(),
            _ => false,
        }
    }

    /// The value of an identifier in an expression.
    pub(crate) fn lookup_value(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        match self.lookup(name).cloned() {
            Some(Sym::Local(id)) => {
                let ty = self.fs.as_ref().unwrap().locals[id].ty.clone();
                Ok(Expr {
                    kind: ExprKind::Local(id),
                    sugar: self.fs.as_ref().unwrap().locals[id].sugar.clone(),
                    ty,
                    loc,
                })
            }
            Some(Sym::Global(g)) => Ok(Expr {
                kind: ExprKind::Global(g),
                sugar: self.globals[g].sugar.clone(),
                ty: self.globals[g].ty.clone(),
                loc,
            }),
            Some(Sym::Enum(v)) => Ok(Expr {
                kind: ExprKind::Int(v),
                sugar: Sugar::Plain,
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
        name: &'a str,
        ty: Ty,
        sugar: Sugar,
        volatile: bool,
    ) -> LocalId {
        let in_memory = volatile || sugar.is_volatile(&self.typedefs) || !ty.is_scalar();
        let align = self.type_alignment(&ty, &sugar);
        let fs = self.fs.as_mut().unwrap();
        fs.locals.push(Local {
            name: name,
            ty,
            sugar,
            align,
            scope: fs.scope,
            in_memory,
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
        ty: Ty,
        init: Init<'a>,
        is_const: bool,
    ) -> usize {
        let align = self.align_of(&ty);
        self.globals.push(Global {
            symbol: name,
            name,
            ty,
            sugar: Sugar::Plain,
            align,
            internal: true,
            defined: true,
            init: Some(init),
            is_const,
            thread_local: false,
            returns_twice: false,
            noreturn: false,
            function_attrs: keel_ir::FunctionAttrs::default(),
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
        while self.peek() != Tok::Eof {
            if self.eat(";") {
                continue;
            }
            self.declaration(true)?;
        }
        // Until now `internal` records explicit static linkage only. C99
        // inline-only emission depends on all file-scope declarations in this
        // unit, including declarations that follow the definition.
        for f in &self.funcs {
            if !self.external_def.contains(&f.global) {
                self.globals[f.global].internal = true;
            }
        }
        Ok(())
    }

    /// A declaration, at file scope or in a block. Returns the statements
    /// initializing the locals it declares.
    pub(crate) fn declaration(&mut self, file_scope: bool) -> R<Vec<Stmt<'a>>> {
        let mut out = Vec::new();
        if self.static_assert()? {
            return Ok(out);
        }
        let spec = self.decl_spec()?;
        if spec.infer && spec.storage == Storage::Typedef {
            return Err(Error::new(
                self.loc(),
                "`__auto_type` cannot declare a typedef",
            ));
        }
        if !spec.infer && self.eat(";") {
            return Ok(out);
        }
        loop {
            let loc = self.loc();
            let mut d = if spec.infer {
                let name = self.ident()?;
                Declarator {
                    name: Some((name, loc)),
                    ty: spec.ty.clone(),
                    sugar: spec.sugar.clone(),
                    is_const: false,
                    is_volatile: false,
                    attrs: Attrs::default(),
                }
            } else {
                self.declarator_sugar(spec.ty.clone(), spec.sugar.clone())?
            };
            let mut attrs = spec.attrs;
            attrs.merge(d.attrs);
            let label = self.asm_label_and_attrs(&mut attrs)?;
            d.attrs = attrs;
            let Some((name, name_loc)) = d.name.clone() else {
                return Err(Error::new(loc, "declaration needs a name"));
            };
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
                    self.atomic_type(expr.ty.clone(), name_loc)?
                } else {
                    expr.ty.clone()
                };
                // Inference drops top-level object qualifiers but retains
                // qualified pointees. Explicit qualifiers remain outermost.
                let inferred = expr.sugar.unqualified(&self.typedefs).clone();
                d.sugar = if spec.is_volatile {
                    Sugar::Volatile(self.sugar_ref(inferred))
                } else {
                    inferred
                };
                if spec.is_const {
                    d.sugar = Sugar::Const(self.sugar_ref(d.sugar));
                }
                Some(Init {
                    items: vec![InitItem::Expr {
                        offset: 0,
                        expr,
                        bits: None,
                    }],
                })
            } else {
                None
            };
            // The specifiers' qualifiers are on the object only if the
            // declarator adds nothing but arrays: `const char *a[2]` is an
            // array of mutable pointers.
            let mut elem = &d.ty;
            while let Ty::Array(inner, _) = elem {
                elem = inner;
            }
            let base = spec.infer || *elem == spec.ty;
            if let Some(what) = elem.unusable()
                && !matches!(spec.storage, Storage::Typedef | Storage::Extern)
            {
                return Err(Error::new(
                    name_loc,
                    format!("`{name}`: objects of type `{what}` are not supported"),
                ));
            }
            let is_const = d.is_const || (spec.is_const && base);
            let is_volatile = d.is_volatile || (spec.is_volatile && base);
            if spec.storage == Storage::Typedef {
                if attrs.alignas.is_some() {
                    return Err(Error::new(name_loc, "`_Alignas` cannot apply to a typedef"));
                }
                self.typedefs.push(TypedefDef {
                    name: name,
                    ty: d.ty,
                    sugar: d.sugar,
                    align: attrs.aligned,
                });
                self.bind(name, Sym::Typedef(self.typedefs.len() - 1));
            } else if let Ty::Func(fty) = &d.ty {
                if attrs.alignment().is_some() || attrs.alignas.is_some() {
                    return Err(Error::new(
                        name_loc,
                        "function alignment attributes are not supported",
                    ));
                }
                let g = self
                    .declare_function(&name, label, fty.clone(), &spec, attrs, file_scope)
                    .map_err(|message| Error::new(name_loc, message))?;
                self.globals[g].sugar = d.sugar.clone();
                if file_scope && self.is("{") {
                    self.function_body(g, fty.clone(), name_loc)?;
                    return Ok(out);
                }
            } else if !file_scope && spec.storage == Storage::None {
                // In scope from the end of its declarator: `T *p =
                // malloc(sizeof *p)` refers to itself.
                let id = self.new_local(name, d.ty.clone(), d.sugar.clone(), is_volatile);
                let align = self.object_alignment(&d.ty, &d.sugar, attrs, name_loc)?;
                self.fs.as_mut().unwrap().locals[id].align = align;
                let mut ty = d.ty;
                let mut init = inferred_init;
                if init.is_none() && self.eat("=") {
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
        d: Declarator<'a>,
        label: Option<&'a str>,
        is_const: bool,
        file_scope: bool,
        inferred_init: Option<Init<'a>>,
    ) -> R<()> {
        let name = d.name.as_ref().unwrap().0;
        let align = self.object_alignment(&d.ty, &d.sugar, d.attrs, d.name.as_ref().unwrap().1)?;
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
                name: name,
                symbol,
                ty: d.ty.clone(),
                sugar: d.sugar.clone(),
                align,
                internal: true,
                defined: true,
                init: None,
                is_const,
                thread_local: spec.thread_local,
                returns_twice: false,
                noreturn: false,
                function_attrs: keel_ir::FunctionAttrs::default(),
                func: None,
            });
            let g = self.globals.len() - 1;
            self.bind(name, Sym::Global(g));
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
        self.globals[g].align = self.globals[g].align.max(align);
        if inferred_init.is_some() || self.eat("=") {
            let loc = self.loc();
            if !file_scope && !local_static {
                return Err(Error::new(
                    loc,
                    "a block-scope `extern` cannot be initialized",
                ));
            }
            let ty = self.globals[g].ty.clone();
            let (init, ty) = match inferred_init {
                Some(init) => (init, ty),
                None => self.initializer(&ty)?,
            };
            let global = &mut self.globals[g];
            global.ty = ty;
            global.init = Some(init);
            global.defined = true;
        }
        Ok(())
    }

    /// Finds or adds the file-scope global `name`, merging this
    /// declaration's type into it (a later one may complete an array).
    fn global(&mut self, name: &str, label: Option<&'a str>, ty: Ty) -> usize {
        let existing = self
            .names
            .get(name)
            .and_then(|bindings| bindings.first())
            .and_then(|(depth, s)| match s {
                Sym::Global(g) if *depth == 0 => Some(*g),
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
                    name: self.text.intern(name),
                    symbol: self.text.intern(name),
                    ty,
                    sugar: Sugar::Plain,
                    align: 1,
                    internal: false,
                    defined: false,
                    init: None,
                    is_const: false,
                    thread_local: false,
                    returns_twice: false,
                    noreturn: false,
                    function_attrs: keel_ir::FunctionAttrs::default(),
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
        g
    }

    fn declare_function(
        &mut self,
        name: &str,
        label: Option<&'a str>,
        fty: Rc<FuncTy>,
        spec: &DeclSpec,
        attrs: Attrs,
        file_scope: bool,
    ) -> Result<usize, &'static str> {
        let g = self.global(name, label, Ty::Func(fty));
        if spec.storage == Storage::Static {
            self.globals[g].internal = true;
        }
        if file_scope && (!spec.inline || spec.storage == Storage::Extern) {
            self.external_def.insert(g);
        }
        let returns_twice = attrs.returns_twice
            || matches!(
                name,
                "setjmp" | "_setjmp" | "sigsetjmp" | "__sigsetjmp" | "savectx" | "vfork"
            );
        self.globals[g].returns_twice |= returns_twice;
        self.globals[g].noreturn |= attrs.noreturn;
        let mut policies = attrs.function_attrs;
        policies.inlinehint |= spec.inline;
        policies.noreturn |= attrs.noreturn;
        self.globals[g].function_attrs.merge(policies);
        self.globals[g]
            .function_attrs
            .validate(self.globals[g].returns_twice)?;
        Ok(g)
    }

    fn function_body(&mut self, g: usize, fty: Rc<FuncTy>, loc: Loc) -> R<()> {
        if self.globals[g].func.is_some() {
            return Err(Error::new(
                loc,
                format!("`{}` is defined twice", self.globals[g].name),
            ));
        }
        let params = std::mem::take(&mut self.last_params);
        let name = self.globals[g].name;
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
    fn asm_label_and_attrs(&mut self, attrs: &mut Attrs) -> R<Option<&'a str>> {
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
                    Target::X86_64LinuxGnu => &text,
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
                let name = match self.advance() {
                    Tok::Ident(s) => s,
                    _ => return Err(self.unexpected("an attribute")),
                };
                let name = name.trim_matches('_');
                match name {
                    "aligned" if self.eat("(") => {
                        let n = self.alignment_constant(false)?;
                        attrs.aligned = Some(attrs.aligned.unwrap_or(1).max(n));
                        self.expect(")")?;
                    }
                    "aligned" => attrs.aligned = Some(16),
                    _ => {
                        match name {
                            "packed" => attrs.packed = true,
                            "returns_twice" => attrs.returns_twice = true,
                            "noreturn" => attrs.noreturn = true,
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

    // ---- types ----

    fn alignment_constant(&mut self, allow_zero: bool) -> R<u64> {
        let loc = self.loc();
        let e = self.conditional()?;
        let Some(crate::eval::Const::Int(n)) = crate::eval::eval_constant(&e, false) else {
            return Err(Error::new(loc, "alignment must be an integer constant"));
        };
        if !e.ty.is_integer()
            || n < 0
            || (n == 0 && !allow_zero)
            || (n != 0 && !(n as u64).is_power_of_two())
        {
            return Err(Error::new(loc, "alignment must be a power of two"));
        }
        if n > 4096 {
            return Err(Error::new(loc, "alignment above 4096 is not supported"));
        }
        Ok(n as u64)
    }

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
        const AUTO: u32 = 1 << 21;

        let loc = self.loc();
        let mut spec = DeclSpec {
            ty: Ty::Int(IntKind::Int),
            sugar: Sugar::Plain,
            infer: false,
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
                self.advance();
                spec.storage = s;
                continue;
            }
            match word {
                "const" | "__const" => spec.is_const = true,
                "volatile" | "__volatile" | "__volatile__" => spec.is_volatile = true,
                "inline" | "__inline" | "__inline__" => spec.inline = true,
                "_Thread_local" | "__thread" => spec.thread_local = true,
                "_Noreturn" => spec.attrs.noreturn = true,
                w if IGNORED_QUALIFIERS.contains(&w) => {}
                "_Atomic" if !matches!(self.peek_at(1), Tok::Punct("(")) => is_atomic = true,
                "_Complex" => is_complex = true,
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
                            spec.sugar = sugar;
                            self.expect(")")?;
                            Some(ty)
                        }
                        "__builtin_va_list" => {
                            self.advance();
                            Some(self.va_list())
                        }
                        "__float128" => {
                            self.advance();
                            Some(Ty::Opaque {
                                name: "__float128",
                                size: 16,
                            })
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
            // On x86-64 Linux it is the x87 80-bit format, which keel does
            // not have: its values are refused rather than miscompiled.
            c if c == LONG + DOUBLE => match self.target {
                Target::Aarch64AppleDarwin => Ty::Double,
                Target::X86_64LinuxGnu => Ty::Opaque {
                    name: "long double",
                    size: 16,
                },
            },
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
            let size = 2 * Types(&self.structs).size(&spec.ty);
            spec.ty = Ty::Opaque {
                name: "_Complex",
                size,
            };
        }
        if is_atomic {
            spec.ty = self.atomic_type(spec.ty, loc)?;
        }
        if spec.is_volatile {
            spec.sugar = Sugar::Volatile(self.sugar_ref(spec.sugar));
        }
        if spec.is_const {
            spec.sugar = Sugar::Const(self.sugar_ref(spec.sugar));
        }
        Ok(spec)
    }

    /// A type name, as in casts and `sizeof`: specifiers and an abstract
    /// declarator.
    pub(crate) fn type_name(&mut self) -> R<Ty> {
        Ok(self.type_name_sugar()?.0)
    }

    pub(crate) fn type_name_sugar(&mut self) -> R<(Ty, Sugar)> {
        let spec = self.decl_spec()?;
        if spec.infer {
            return Err(Error::new(
                self.loc(),
                "`__auto_type` is only allowed in variable declarations",
            ));
        }
        let d = self.declarator_sugar(spec.ty, spec.sugar)?;
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
        Ok((d.ty, d.sugar))
    }

    /// `_Atomic(ty)`, which keel supports for integers and pointers.
    fn atomic_type(&mut self, ty: Ty, loc: Loc) -> R<Ty> {
        match ty {
            Ty::Atomic(_) => Ok(ty),
            ty if ty.is_integer() || ty.is_ptr() => Ok(Ty::Atomic(self.type_ref(ty))),
            _ => Err(Error::new(
                loc,
                "`_Atomic` is only supported on integers and pointers",
            )),
        }
    }

    /// Pointer qualifiers after a `*`. Returns (const, volatile, atomic).
    fn qualifiers(&mut self, attrs: &mut Attrs) -> R<(bool, bool, bool)> {
        let (mut c, mut v, mut a) = (false, false, false);
        loop {
            match self.peek() {
                Tok::Ident(w) if w == "_Atomic" => a = true,
                Tok::Ident(w) if w == "const" || w == "__const" => c = true,
                Tok::Ident(w) if w == "volatile" || w == "__volatile__" || w == "__volatile" => {
                    v = true
                }
                Tok::Ident(w) if IGNORED_QUALIFIERS.contains(&w) => {}
                Tok::Ident(w) if w == "__attribute__" || w == "__attribute" => {
                    self.attributes(attrs)?;
                    continue;
                }
                _ => return Ok((c, v, a)),
            }
            self.advance();
        }
    }

    /// A declarator applied to the specifiers' type and its sugar.
    fn declarator_sugar(&mut self, ty: Ty, sugar: Sugar) -> R<Declarator<'a>> {
        self.declarator_layer(ty, sugar, None)
    }

    fn declarator_layer(
        &mut self,
        mut ty: Ty,
        mut sugar: Sugar,
        inherited_owner: Option<Rc<FuncTy>>,
    ) -> R<Declarator<'a>> {
        // Only functions constructed by enclosing declarator layers supply
        // inherited ownership. A typedef's hidden function is not that layer.
        let ambiguous_base = inherited_owner.is_none()
            && !matches!(ty, Ty::Func(_))
            && Self::contract_owner(&ty).is_some();
        let (mut is_const, mut is_volatile) = (false, false);
        let mut attrs = Attrs::default();
        // `^` is a block pointer (Apple's closures, in headers when
        // `__BLOCKS__` is defined); as a value it is just a pointer.
        while self.eat("*") || self.eat("^") {
            let loc = self.loc();
            ty = self.ptr_type(ty);
            sugar = sugar.wrap(Sugar::Ptr);
            let is_atomic;
            (is_const, is_volatile, is_atomic) = self.qualifiers(&mut attrs)?;
            if is_atomic {
                ty = self.atomic_type(ty, loc)?;
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
            self.skip_parens()?;
            let (ty, sugar) = self.type_suffix(ty, sugar)?;
            // The current layer's attributes retain their incoming owner.
            // A newly constructed suffix becomes the incoming owner for the
            // nested layer, which may carry separate named-function promises.
            let nested_owner = Self::named_contract_owner(&ty)
                .cloned()
                .or_else(|| inherited_owner.clone());
            let owner = inherited_owner.or_else(|| Self::named_contract_owner(&ty).cloned());
            let end = self.pos;
            self.pos = start;
            let mut d = self.declarator_layer(ty, sugar, nested_owner)?;
            // A purely grouping layer constructs no function of its own;
            // its attributes follow the nested declarator's owner instead.
            let owner = owner.or_else(|| Self::named_contract_owner(&d.ty).cloned());
            Self::merge_declarator_attrs(&mut d.attrs, attrs, owner.as_ref(), &d.ty);
            self.expect(")")?;
            self.pos = end;
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
        let (ty, sugar) = self.type_suffix(ty, sugar)?;
        let owner = inherited_owner.or_else(|| Self::named_contract_owner(&ty).cloned());
        let mut named_attrs = Attrs::default();
        Self::merge_declarator_attrs(&mut named_attrs, attrs, owner.as_ref(), &ty);
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

    /// A declarator layer inherits the function constructed by its enclosing
    /// layer, or otherwise owns the function constructed by its own suffix (or nested
    /// declarator when this is a purely grouping layer). Parentheses apply
    /// that suffix before parsing the nested layer. Thus `* attr h(void)` owns
    /// h when returning a scalar pointer, but owns the returned function in
    /// `(* attr h(void))(void)`. Rc identity (not ABI equality) distinguishes
    /// those functions through arbitrary parenthesized composition.
    fn named_contract_owner(ty: &Ty) -> Option<&Rc<FuncTy>> {
        match ty {
            Ty::Func(f) => Some(f),
            _ => None,
        }
    }

    fn contract_owner(ty: &Ty) -> Option<&Rc<FuncTy>> {
        match ty {
            Ty::Func(f) => Some(f),
            Ty::Ptr(t) | Ty::Array(t, _) | Ty::Atomic(t) => Self::contract_owner(t),
            _ => None,
        }
    }

    fn merge_declarator_attrs(
        named: &mut Attrs,
        mut layer: Attrs,
        owner: Option<&Rc<FuncTy>>,
        declared: &Ty,
    ) {
        // Only contracts owned by the actual declared function escape the
        // declarator. Pointer contracts remain unrepresented; layout attributes
        // still merge exactly as before, independently of contract ownership.
        let is_named =
            matches!((owner, declared), (Some(owner), Ty::Func(f)) if Rc::ptr_eq(owner, f));
        if !is_named {
            layer.returns_twice = false;
            layer.noreturn = false;
            layer.function_attrs = keel_ir::FunctionAttrs::default();
        }
        named.merge(layer);
    }

    /// After `(` in a declarator: a nested declarator, or parameters?
    fn nested_declarator_follows(&self) -> bool {
        match self.peek_at(1) {
            Tok::Punct("*") | Tok::Punct("(") | Tok::Punct("[") | Tok::Punct("^") => true,
            Tok::Ident(s) => {
                !is_type_keyword(s) && self.typedef_type(s).is_none() || s == "__attribute__"
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
            let mut attrs = Attrs::default();
            while self.eat("static") || self.qualifiers(&mut attrs)? != (false, false, false) {}
            if attrs.alignment().is_some() || attrs.alignas.is_some() {
                return Err(Error::new(
                    self.loc(),
                    "alignment attributes in array declarators are not supported",
                ));
            }
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
            if !self
                .size_of(&elem)
                .is_multiple_of(self.type_alignment(&elem, &sugar))
            {
                return Err(Error::new(
                    self.loc(),
                    "array element size is not a multiple of its alignment",
                ));
            }
            return Ok((
                Ty::Array(self.type_ref(elem), len),
                sugar.wrap(Sugar::Array),
            ));
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
            if spec.infer {
                return Err(Error::new(loc, "`__auto_type` cannot declare a parameter"));
            }
            let d = self.declarator_sugar(spec.ty, spec.sugar)?;
            let mut attrs = spec.attrs;
            attrs.merge(d.attrs);
            self.attributes(&mut attrs)?;
            if attrs.alignment().is_some() || attrs.alignas.is_some() {
                return Err(Error::new(
                    loc,
                    "alignment attributes on parameters are not supported",
                ));
            }
            // Arrays and functions are passed as pointers.
            let sugar = match (&d.ty, d.sugar) {
                (Ty::Func(_), s) => s.wrap(Sugar::Ptr),
                (Ty::Array(_, _), s) => Sugar::Ptr(self.sugar_ref(s.pointee(&self.typedefs))),
                (_, s) => s,
            };
            let ty = match d.ty {
                Ty::Array(elem, _) => Ty::Ptr(elem),
                Ty::Func(_) => self.ptr_type(d.ty),
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
                let s = s;
                self.advance();
                Some(s)
            }
            _ => None,
        };
        let new_struct = |p: &mut Parser<'a>, tag: Option<&'a str>| {
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
            let id = new_struct(self, Some(tag));
            self.scopes.last_mut().unwrap().tags.insert(tag, Some(id));
            return Ok(Ty::Struct(id));
        }
        let pack = self.toks[self.pos].pack;
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
                if matches!(spec.ty, Ty::Struct(_)) {
                    self.object_alignment(&spec.ty, &spec.sugar, spec.attrs, floc)?;
                    fields.push((
                        None,
                        spec.ty,
                        spec.sugar,
                        None,
                        spec.attrs.alignment(),
                        spec.attrs.packed,
                    ));
                }
                continue;
            }
            loop {
                let mut member_attrs = spec.attrs;
                let (name, ty, sugar) = if self.is(":") {
                    (None, spec.ty.clone(), spec.sugar.clone())
                } else {
                    let d = self.declarator_sugar(spec.ty.clone(), spec.sugar.clone())?;
                    member_attrs.merge(d.attrs);
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
                self.attributes(&mut member_attrs)?;
                self.object_alignment(&ty, &sugar, member_attrs, floc)?;
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
                ));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
        }
        self.attributes(&mut attrs)?;
        for field in &mut fields {
            field.5 |= attrs.packed;
        }
        let (members, mut size, mut align) =
            layout_struct(&self.types(), &self.typedefs, is_union, fields, pack)
                .map_err(|message| Error::new(loc, message))?;
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
            let s = s;
            self.advance();
            self.scopes.last_mut().unwrap().tags.insert(s, None);
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
            values.push((name, next));
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
    pub(crate) fn block_items(&mut self) -> R<Vec<Stmt<'a>>> {
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

    fn statement(&mut self) -> R<Stmt<'a>> {
        let loc = self.loc();
        if let Tok::Ident(word) = self.peek() {
            match word {
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
    fn labeled_statement(&mut self) -> R<Stmt<'a>> {
        if self.is("}") {
            return Ok(Stmt::Block(Vec::new()));
        }
        if self.is_declaration_start() {
            return Ok(Stmt::Block(self.declaration(false)?));
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
    (is_type_keyword(s) && !s.starts_with("__builtin") && !s.ends_with("_t"))
        || matches!(
            s,
            "if" | "else"
                | "while"
                | "do"
                | "for"
                | "switch"
                | "case"
                | "default"
                | "break"
                | "continue"
                | "return"
                | "goto"
                | "sizeof"
                | "_Alignof"
                | "__alignof__"
                | "_Static_assert"
                | "_Generic"
                | "asm"
                | "__asm"
                | "__asm__"
        )
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
            vec![],
            crate::Options::default().target,
            Strings::new(&arena),
        );
        let words = ["a", "b", "c", "d", "e", "f", "g"];
        let mut model = vec![HashMap::new()];
        let mut state = 7u32;
        for step in 0..3_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let name = words[(state as usize >> 8) % words.len()];
            match (state >> 16) % 8 {
                0 if model.len() < 8 => {
                    p.push_scope();
                    model.push(HashMap::new());
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
                        Sym::Enum(step as i64)
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
            vec![],
            crate::Options::default().target,
            Strings::new(&arena),
        );
        p.push_scope();
        p.bind("object", Sym::Local(3));
        p.push_scope();
        let global = p.global("object", None, INT);
        assert_eq!(p.lookup("object"), Some(&Sym::Global(global)));
        p.pop_scope();
        assert_eq!(p.lookup("object"), Some(&Sym::Local(3)));
        p.pop_scope();
        assert_eq!(p.lookup("object"), Some(&Sym::Global(global)));
        assert_eq!(p.global("object", None, INT), global);
        assert_eq!(p.globals.len(), 1);
        for depth in 0..6 {
            p.push_scope();
            p.bind("object", Sym::Enum(depth));
        }
        for depth in (0..6).rev() {
            assert_eq!(p.lookup("object"), Some(&Sym::Enum(depth)));
            p.pop_scope();
        }
        assert_eq!(p.lookup("object"), Some(&Sym::Global(global)));
        assert!(Rc::ptr_eq(
            &p.sugar_ref(Sugar::Plain),
            &p.sugar_ref(Sugar::Plain)
        ));
    }
}
