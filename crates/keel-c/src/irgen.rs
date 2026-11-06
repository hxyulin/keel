//! Keel IR from the typed tree.
//!
//! - **Locals.** A scalar local whose address is never taken (and that is
//!   not `volatile`) is an SSA variable of the frontend's builder, which
//!   places block parameters where control flow merges. Every other local
//!   (arrays, structs, `&x`) gets a stack slot. That is what `mem2reg`
//!   does in LLVM, decided up front because the parser has seen every use.
//!   With debug info all named locals are in stack slots, where the
//!   debugger finds them.
//! - **Aggregates** are handled by address: a struct-typed expression
//!   evaluates to a pointer to its bytes, and assignment copies them.
//! - **Only what is used is emitted.** Exported functions and objects are
//!   always emitted; `static` ones, `inline` ones and declarations from
//!   headers only when something emitted refers to them. Headers declare
//!   hundreds of functions, and define `inline` ones a program may never
//!   call.

use crate::ast::*;
use crate::debug::DebugTypes;
use crate::eval::{Base, Const, eval};
use crate::int_const::{self, IntConst, IntTy, Policy};
use crate::parse::Parser;
use crate::types::*;
use crate::{Error, Loc};
use keel_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use keel_ir::fxhash::FxHashMap as HashMap;
use keel_ir::{
    AbiParam, ArgExt, BinaryOp, Block, ConvertOp, DataInit, DataItem, DataObject, DebugGlobal,
    DebugScope, DebugType, DebugVar, EntityRef, FloatCC, FuncDecl, Function, InstData, IntCC,
    Intrinsic, Language, MemoryFlags, Module, Ordering, OverflowOp, Purpose, RmwOp, Signature,
    SourceFile, SourceLoc, StackSlot, Symbol, Type, Value, ValueDef,
};

type R<T> = Result<T, Error>;

mod abi;
mod builtin;
mod call;
mod data;
mod dispatch;
mod expr;
mod fold;
mod place;
mod stmt;
pub(crate) mod wasm_scalar;
use self::{dispatch::*, fold::*};

/// A source parameter either has one IR operand or no ABI payload.
#[derive(Clone, Copy, Debug)]
enum ParamMapping {
    Omitted,
    Operand(usize),
}

#[derive(Clone, Copy, Debug)]
enum ResultMapping {
    Void,
    Empty,
    Direct,
    Indirect,
}

/// Keeps source parameter positions separate from ABI operand positions.
struct CAbiMapping {
    signature: Signature,
    params: Vec<ParamMapping>,
    result: ResultMapping,
}

struct FunctionContext<'p> {
    global_alignments: Option<HashMap<&'p str, u32>>,
    volatile_members: Vec<std::cell::Cell<Option<bool>>>,
    ssa: FunctionBuilderContext,
}

pub struct Gen<'p> {
    p: crate::unit::Unit<'p>,
    /// The parser's function definitions, each dropped once emitted.
    funcs: Vec<Option<FuncDef<'p>>>,
    files: &'p [&'p str],
    module: Module,
    debug: bool,
    library_builtins: crate::LibraryBuiltins,
    library_builtin_symbols: Vec<bool>,
    /// Module source files, by the lexer's file index.
    srcfiles: HashMap<u32, SourceFile>,
    used: Vec<bool>,
    worklist: Vec<usize>,
    strings_used: Vec<bool>,
    /// Signatures of functions declared without a prototype, from their
    /// first call.
    unprototyped: HashMap<usize, Signature>,
    /// Data made up for local initializers: bytes to symbol.
    anon: HashMap<Vec<u8>, String>,
    dtypes: DebugTypes,
    /// Alignment facts for objects emitted into this module.
    generated_alignments: HashMap<String, u32>,
    /// Worker-owned caches and reusable SSA tables; never shared between jobs.
    context: FunctionContext<'p>,
}

impl<'p> Gen<'p> {
    pub fn new(
        p: &'p Parser<'p>,
        funcs: Vec<FuncDef<'p>>,
        files: &'p [&'p str],
        debug: bool,
    ) -> Self {
        Self {
            p: crate::unit::Unit::new(p),
            funcs: funcs.into_iter().map(Some).collect(),
            files,
            module: Module::default(),
            debug,
            library_builtins: if p.target == keel_ir::Target::Wasm32UnknownUnknown {
                crate::LibraryBuiltins::disabled()
            } else {
                crate::LibraryBuiltins::hosted()
            },
            library_builtin_symbols: p
                .globals
                .iter()
                .enumerate()
                .map(|(index, global)| {
                    crate::LibraryBuiltin::named(global.name).is_some()
                        && !p
                            .globals
                            .iter()
                            .enumerate()
                            .any(|(other, alias)| other != index && alias.symbol == global.symbol)
                })
                .collect(),
            srcfiles: HashMap::default(),
            used: vec![false; p.globals.len()],
            worklist: Vec::new(),
            strings_used: vec![false; p.strings.len()],
            unprototyped: HashMap::default(),
            anon: HashMap::default(),
            dtypes: DebugTypes::default(),
            generated_alignments: HashMap::default(),
            context: FunctionContext {
                global_alignments: None,
                volatile_members: vec![std::cell::Cell::new(None); p.structs.len()],
                ssa: FunctionBuilderContext::new(),
            },
        }
    }

    pub fn with_library_builtins(mut self, builtins: crate::LibraryBuiltins) -> Self {
        self.library_builtins = builtins;
        self
    }

    /// Whether an object of type `ty` is or contains anything volatile.
    /// Pointer pointee qualifiers do not qualify the pointer object itself.
    fn object_volatile(&self, ty: &Ty, sugar: &Sugar) -> bool {
        sugar.is_volatile(self.p.typedefs)
            || match ty {
                Ty::Array(a) => self.object_volatile(&a.elem, &sugar.pointee(self.p.typedefs)),
                Ty::Struct(id) => {
                    if let Some(volatile) = self.context.volatile_members[*id].get() {
                        return volatile;
                    }
                    let volatile = self.p.structs[*id]
                        .members
                        .iter()
                        .any(|m| self.object_volatile(&m.ty, &m.sugar));
                    self.context.volatile_members[*id].set(Some(volatile));
                    volatile
                }
                _ => false,
            }
    }

    pub fn run(mut self) -> R<Module> {
        self.module.assembly = self.p.assembly.to_vec();
        if self.p.c_layout != CDataLayout::NATIVE_LP64 {
            self.check_private_scalar_profile()?;
        }
        // Validate even discarded static/inline bodies against callback and
        // direct-call contracts added by later declarations in the unit.
        for def in self.funcs.iter().flatten() {
            self.check_value_signature(def.ty, def.loc)?;
            // Run before pruning: discarded definitions cannot hide unsafe ABI
            // calls, varargs, aggregate reads/copies or unsupported locals.
            for local in def.locals.iter().filter(|l| !l.compound_literal) {
                self.check_value_type(&local.ty, def.loc)?;
            }
            let mut error = None;
            for stmt in def.body {
                visit_stmt_exprs(stmt, &mut |e, use_| {
                    if let Err(e) = self.check_value_expr(e, use_) {
                        error = Some(e);
                        None
                    } else {
                        Some(())
                    }
                });
                if let Some(error) = error.take() {
                    return Err(error);
                }
            }
            if def.locals.iter().any(|l| l.cleanup.is_some()) {
                for &(g, loc) in &def.direct_calls {
                    if crate::parse::nonlocal_exit(&self.p.globals[g]) {
                        return Err(Error::new(
                            loc,
                            "nonlocal exit/returns-twice call in a function containing cleanup locals is not supported yet",
                        ));
                    }
                }
            }
            for (g, loc) in def.locals.iter().filter_map(|l| l.cleanup) {
                let callback = &self.p.globals[g];
                if callback.incompatible_return {
                    return Err(Error::new(
                        loc,
                        "cleanup function has incompatible return-type redeclarations",
                    ));
                }
                if crate::parse::nonlocal_exit(callback) || callback.function_attrs.noreturn {
                    return Err(Error::new(loc, "cleanup callback must return normally"));
                }
            }
        }
        for global in self.p.globals {
            if global.defined && !global.compound_literal && !matches!(global.ty, Ty::Func(_)) {
                self.check_value_type(&global.ty, Loc::default())?;
                if let Some(init) = &global.init {
                    let mut error = None;
                    visit_init_exprs(init, &mut |e, use_| {
                        if let Err(e) = self.check_value_expr(e, use_) {
                            error = Some(e);
                            None
                        } else {
                            Some(())
                        }
                    });
                    if let Some(error) = error {
                        return Err(error);
                    }
                }
            }
        }
        let mut placements = std::collections::BTreeMap::new();
        for (g, global) in self.p.globals.iter().enumerate() {
            if (global.section.is_some() || global.retain.is_some())
                && !matches!(global.ty, Ty::Func(_))
                && (global.align > u64::from(keel_ir::MAX_DATA_ALIGN)
                    || !global.align.max(1).is_power_of_two())
            {
                return Err(Error::new(
                    Loc::default(),
                    format!(
                        "custom/retained data alignment must be a power of two up to {}",
                        keel_ir::MAX_DATA_ALIGN
                    ),
                ));
            }
            if let Some(section) = global.section {
                use keel_ir::section::Placement;
                // An imported object's initializer and final section flags belong
                // to its defining translation unit. Check names without assigning
                // storage or allocating a section in this module.
                if !global.defined && !matches!(global.ty, Ty::Func(_)) {
                    let candidates = if global.thread_local {
                        &[Placement::TlsData, Placement::TlsZeroFill][..]
                    } else {
                        &[
                            Placement::Data,
                            Placement::ZeroFill,
                            Placement::ReadOnly,
                            Placement::Relro,
                        ][..]
                    };
                    if candidates
                        .iter()
                        .all(|&placement| section.validate(self.p.target, placement).is_err())
                    {
                        return Err(Error::new(
                            Loc::default(),
                            "section name is invalid for an external object",
                        ));
                    }
                    continue;
                }
                let placement = if matches!(global.ty, Ty::Func(_)) {
                    Placement::Code
                } else if global.defined {
                    Placement::for_object(
                        global.thread_local,
                        self.data(g, false)?.init.as_ref().unwrap(),
                    )
                } else if global.is_const {
                    Placement::ReadOnly
                } else {
                    Placement::Data
                };
                section
                    .validate(self.p.target, placement)
                    .map_err(|e| Error::new(Loc::default(), e))?;
                if !global.defined {
                    continue;
                }
                let key = section.as_str().to_owned();
                let props = (
                    section.initialized_storage(placement),
                    self.p.target != keel_ir::Target::Aarch64AppleDarwin && global.retain.is_some(),
                );
                if placements
                    .insert(key, props)
                    .is_some_and(|old| old != props)
                {
                    return Err(Error::new(
                        Loc::default(),
                        "incompatible storage or retention in shared custom section",
                    ));
                }
            }
        }
        let darwin_destructor = (self.p.target == keel_ir::Target::Aarch64AppleDarwin)
            .then(|| self.p.globals.iter().find(|g| g.function_attrs.destructor))
            .flatten();
        for (g, global) in self.p.globals.iter().enumerate() {
            // Check the C contract before pruning and before pointer types are
            // erased to IR `ptr`. Include asm aliases of the runtime symbol.
            if let Some(callback) = darwin_destructor
                && global.symbol == "atexit"
            {
                let ordinary_type = matches!(global.ty, Ty::Func(f)
                    if f.ret == Ty::Int(IntKind::Int) && f.has_prototype() && !f.variadic
                    && matches!(f.params, [Ty::Ptr(Ty::Func(cb))]
                        if cb.ret == Ty::Void && cb.has_prototype() && !cb.variadic && cb.params.is_empty()));
                if !ordinary_type
                    || global.defined
                    || global.internal
                    || global.weak.is_some()
                    || global.incompatible_return
                    || global.returns_twice
                    || global.function_attrs.noreturn
                {
                    return Err(Error::new(
                        callback.lifecycle.unwrap_or_default(),
                        "Darwin destructor registration requires the ordinary libc atexit interface",
                    ));
                }
                self.use_global(g);
            }
            if let Some(loc) = global.weak
                && (global.internal
                    || global.thread_local
                    || (!global.defined && global.visibility.is_some_and(|v| v.hidden())))
            {
                return Err(Error::new(
                    loc,
                    "weak requires external linkage, non-TLS storage and default visibility for undefined references",
                ));
            }
            if global.retain.is_some() && !global.defined {
                return Err(Error::new(
                    global.retain.unwrap_or_default(),
                    "retain requires a definition in this translation unit",
                ));
            }
            if let Some(loc) = global.lifecycle
                && (!matches!(global.ty, Ty::Func(f) if f.ret == Ty::Void && f.has_prototype() && f.params.is_empty() && !f.variadic)
                    || global.incompatible_return
                    || !global.defined
                    || global.func.is_none()
                    || global.returns_twice
                    || global.function_attrs.noreturn)
            {
                return Err(Error::new(
                    loc,
                    "constructor/destructor requires a defined, normally returning void(void) function",
                ));
            }
            if global.defined
                && (!global.internal
                    || global.used
                    || global.retain.is_some()
                    || global.lifecycle.is_some())
            {
                self.use_global(g);
            }
        }
        // Emit in source order, then whatever those refer to.
        self.worklist.reverse();
        while let Some(g) = self.worklist.pop() {
            let global = &self.p.globals[g];
            match (&global.ty, global.func) {
                (Ty::Func(_), Some(f)) => {
                    // Each is emitted once; its tree is not needed after.
                    let def = self.funcs[f].take().expect("a function emitted twice");
                    if let Some(error) = def.x86_feature_error {
                        return Err(error);
                    }
                    let func = self.function(&def)?;
                    self.module.functions.push(func);
                }
                (Ty::Func(fty), None) => {
                    let sig = match self.unprototyped.get(&g) {
                        Some(sig) => sig.clone(),
                        None => self.signature(fty, g, &[])?,
                    };
                    self.module.decls.push(FuncDecl {
                        name: global.symbol.to_owned(),
                        attrs: keel_ir::FunctionAttrs {
                            weak: global.weak.is_some(),
                            // Placement annotates definitions, not call contracts.
                            section: None,
                            ..global.function_attrs.clone()
                        },
                        sig,
                    });
                }
                _ if global.defined => {
                    let data = self.data(g, true)?;
                    self.module.data.push(data);
                    // Made-up names (compound literals) are not identifiers.
                    if self.debug && !global.name.contains('.') {
                        let ty = self
                            .debug_type(&global.ty, &global.sugar)
                            .expect("a void object");
                        self.module.debug_globals.push(DebugGlobal {
                            name: global.name.to_owned(),
                            ty,
                            symbol: global.symbol.to_owned(),
                        });
                    }
                }
                _ => self.module.data.push(DataObject {
                    name: global.symbol.to_owned(),
                    thread_local: global.thread_local,
                    local: false,
                    weak: global.weak.is_some(),
                    weak_odr: false,
                    hidden: false,
                    retain: false,
                    section: None,
                    init: None,
                }),
            }
        }
        // Registration order follows definitions, not the first (possibly
        // forward) declaration or the worklist's dependency traversal. IR and
        // object writers use function layout order for lifecycle arrays.
        if self
            .module
            .functions
            .iter()
            .any(|f| f.attrs.constructor || f.attrs.destructor)
        {
            let definition_order: HashMap<_, _> = self
                .p
                .globals
                .iter()
                .filter_map(|g| g.func.map(|index| (g.symbol, index)))
                .collect();
            self.module
                .functions
                .sort_by_key(|f| definition_order[f.name.as_str()]);
        }
        // An address-only import can precede the first call, even in another
        // function. Complete those unspecified signatures after all bodies have
        // supplied their promoted call types. This also completes aggregate
        // results, whose address-only signature already has a hidden sret
        // parameter. Actual calls were checked against this one signature.
        for (&g, sig) in &self.unprototyped {
            let name = self.p.globals[g].symbol;
            for f in &mut self.module.functions {
                let refs: Vec<_> = f
                    .callees()
                    .filter_map(|(id, d)| (d.name == name).then_some(id))
                    .collect();
                for id in refs {
                    f.callee_mut(id).sig = sig.clone();
                }
            }
            for d in &mut self.module.decls {
                if d.name == name {
                    d.sig = sig.clone();
                }
            }
        }
        if self.p.target == keel_ir::Target::Aarch64AppleDarwin {
            // Darwin no longer uses __mod_term_func for ordinary C destructors.
            // Executable-only subset: register with libSystem atexit at startup.
            let callbacks: Vec<_> = self
                .module
                .functions
                .iter_mut()
                .filter_map(|f| {
                    if !f.attrs.destructor {
                        return None;
                    }
                    f.attrs.destructor = false;
                    Some((f.name.clone(), f.sig.clone(), f.attrs.clone()))
                })
                .collect();
            if !callbacks.is_empty() {
                let sig = Signature::new(&[Type::Ptr], &[Type::I32]);
                if self.module.has_symbol("atexit") {
                    if !self
                        .module
                        .decls
                        .iter()
                        .any(|d| d.name == "atexit" && d.sig == sig)
                    {
                        return Err(Error::new(
                            Loc::default(),
                            "Darwin destructor registration requires the ordinary libc atexit interface",
                        ));
                    }
                } else {
                    self.module.decls.push(FuncDecl {
                        name: "atexit".into(),
                        sig: sig.clone(),
                        attrs: Default::default(),
                    });
                }
                for (i, (name, callback_sig, attrs)) in callbacks.into_iter().enumerate() {
                    let mut wrapper_name = format!(".keel.atexit.{i}");
                    // Even a pruned declaration can be named by opaque asm.
                    // Never capture a user's external reference with a thunk.
                    while self.module.has_symbol(&wrapper_name)
                        || self.p.globals.iter().any(|g| g.symbol == wrapper_name)
                    {
                        wrapper_name.push('.');
                    }
                    let mut wrapper = Function::new(&wrapper_name, Signature::new(&[], &[]));
                    wrapper.local = true;
                    wrapper.attrs.constructor = true;
                    let callback = wrapper.import_function(&name, callback_sig);
                    wrapper.callee_mut(callback).attrs = attrs;
                    let register = wrapper.import_function("atexit", sig.clone());
                    let block = wrapper.create_block();
                    let end = keel_ir::InsertPoint::End(block);
                    let addr = wrapper.ins(end).symbol_addr(Symbol::Func(callback));
                    wrapper.ins(end).call(register, &[addr]);
                    wrapper.ins(end).return_(&[]);
                    self.module.functions.push(wrapper);
                }
            }
        }
        if self.debug {
            self.module.debug_language = Some(Language::C11);
        }
        // Wasm preflight above rejects every section/linkage/lifecycle attribute,
        // including unused definitions. This validator describes native objects.
        if self.p.target != keel_ir::Target::Wasm32UnknownUnknown {
            keel_ir::section::validate_module(&self.module, self.p.target)
                .map_err(|e| Error::new(Loc::default(), e))?;
        }
        self.module.sync_function_attrs();
        Ok(self.module)
    }

    fn use_global(&mut self, g: usize) {
        if !self.used[g] {
            self.used[g] = true;
            self.worklist.push(g);
        }
    }

    fn symbol_alignment(&mut self, name: &str) -> u32 {
        let globals = self.context.global_alignments.get_or_insert_with(|| {
            let mut alignments =
                HashMap::with_capacity_and_hasher(self.p.globals.len(), Default::default());
            for global in self.p.globals {
                alignments
                    .entry(global.symbol)
                    .or_insert(global.align as u32);
            }
            alignments
        });
        globals
            .get(name)
            .copied()
            .or_else(|| self.generated_alignments.get(name).copied())
            .unwrap_or(1)
    }

    /// All non-declaration module data goes through here so an alignment
    /// query sees literals and initializer bytes emitted later in a function.
    fn push_generated_data(&mut self, data: DataObject) {
        let align = data.init.as_ref().map_or(1, |init| init.align);
        self.generated_alignments
            .entry(data.name.clone())
            .or_insert(align);
        self.module.data.push(data);
    }

    fn string_symbol(&mut self, i: usize) -> String {
        let name = format!(".str.{i}");
        if !self.strings_used[i] {
            self.strings_used[i] = true;
            let s = &self.p.strings[i];
            self.push_generated_data(DataObject {
                name: name.clone(),
                thread_local: false,
                local: true,
                weak: false,
                weak_odr: false,
                hidden: false,
                retain: false,
                section: None,
                init: Some(DataInit {
                    constant: true,
                    align: if s.wide { 4 } else { 1 },
                    items: vec![DataItem::Bytes(s.bytes.to_vec())],
                }),
            });
        }
        name
    }

    /// A constant object holding `bytes`, for copying into locals.
    fn anon_data(&mut self, bytes: &[u8]) -> String {
        if let Some(name) = self.anon.get(bytes) {
            return name.clone();
        }
        let name = format!(".init.{}", self.anon.len());
        self.push_generated_data(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak: false,
            weak_odr: false,
            hidden: false,
            retain: false,
            section: None,
            init: Some(DataInit {
                constant: true,
                align: 8,
                items: vec![DataItem::Bytes(bytes.to_vec())],
            }),
        });
        self.anon.insert(bytes.to_vec(), name.clone());
        name
    }

    fn srcloc(&mut self, loc: Loc) -> Option<SourceLoc> {
        if !self.debug {
            return None;
        }
        let file = match self.srcfiles.get(&loc.file) {
            Some(f) => *f,
            None => {
                let f = self.module.add_file(self.files[loc.file as usize]);
                self.srcfiles.insert(loc.file, f);
                f
            }
        };
        Some(SourceLoc {
            file,
            line: loc.line,
            col: loc.col,
            scope: None,
        })
    }

    fn types(&self) -> Types<'_> {
        self.p.types()
    }

    fn debug_type(&mut self, ty: &Ty, sugar: &Sugar) -> Option<DebugType> {
        self.dtypes.get(&mut self.module, &self.p, ty, sugar)
    }

    fn check_value_expr(&self, e: &Expr<'_>, use_: ExprUse) -> R<()> {
        // Literal storage recorded during parsing may belong to an unevaluated
        // query. Reaching its expression here establishes actual materialization,
        // even when only its address (or a supported member) is requested.
        if let ExprKind::CompoundLit(..) = e.kind {
            self.check_value_type(&e.ty, e.loc)?;
        }
        if let ExprKind::Global(g) = e.kind
            && self.p.globals[g].compound_literal
        {
            self.check_value_type(&e.ty, e.loc)?;
            if let Some(init) = &self.p.globals[g].init {
                let mut result = Ok(());
                visit_init_exprs(init, &mut |e, use_| {
                    result = self.check_value_expr(e, use_);
                    result.as_ref().ok().copied()
                });
                result?;
            }
        }
        // Array expressions decay to pointers. Taking an object's address or
        // computing a member address does not read the containing value.
        if use_ != ExprUse::Address && !matches!(e.ty, Ty::Array(_)) {
            self.check_value_type(&e.ty, e.loc)?;
        }
        if let ExprKind::Global(_) = e.kind
            && let Ty::Func(fty) = e.ty
        {
            self.check_value_signature(fty, e.loc)?;
        }
        if let ExprKind::Call(callee, _) = e.kind
            && let Some(fty) = callee.ty.pointee().and_then(Ty::func)
        {
            self.check_value_signature(fty, e.loc)?;
        }
        Ok(())
    }

    fn check_value_type(&self, ty: &Ty, loc: Loc) -> R<()> {
        if let Some(name) = self.types().unsupported_value(ty) {
            return Err(Error::new(
                loc,
                format!("values containing `{name}` are not supported"),
            ));
        }
        Ok(())
    }

    fn check_value_signature(&self, fty: &FuncTy, loc: Loc) -> R<()> {
        if fty.microsoft_x64 {
            return Err(Error::new(
                loc,
                "evaluated ms_abi function calls, addresses and definitions require unsupported Microsoft x64 ABI lowering",
            ));
        }
        for ty in std::iter::once(&fty.ret).chain(fty.params) {
            self.check_value_type(ty, loc)?;
        }
        Ok(())
    }

    /// Preserve proven address alignment on every emitted access, including
    /// byte copies and initializer chunks. No pointee-type inference: packed
    /// members and pointers cast from byte buffers may be unaligned.
    /// A fault on a non-volatile C access is undefined behavior, so every
    /// such access is marked `trap_ub`.
    fn finish_access_flags(&mut self, func: &mut Function, seeds: HashMap<Value, u32>) {
        // StackSlot, DataRef and Value are append-only dense arena indices.
        // Memoized results (including seeds) are kept by value index.
        let mut known = vec![None; func.num_values()];
        for (value, align) in seeds {
            known[value.index()] = Some(align);
        }
        let slots: Vec<_> = func.stack_slots().map(|(_, d)| d.align).collect();
        let symbols: Vec<_> = func
            .data_symbols()
            .map(|(_, name)| self.symbol_alignment(name))
            .collect();
        fn alignment(
            func: &Function,
            value: Value,
            known: &mut [Option<u32>],
            slots: &[u32],
            symbols: &[u32],
            depth: usize,
        ) -> u32 {
            let value = func.resolve(value);
            if let Some(a) = known[value.index()] {
                return a;
            }
            // Block parameters (including cyclic loop phis) and excessively
            // deep expressions conservatively carry no alignment promise.
            if depth == 64 {
                return 1;
            }
            let ValueDef::Result(inst, _) = func.value_def(value) else {
                return 1;
            };
            let result = match func.inst_data(inst) {
                InstData::StackAddr { slot } => slots[slot.index()],
                InstData::SymbolAddr {
                    symbol: Symbol::Data(data),
                } => symbols[data.index()],
                InstData::Iconst { imm, .. } => 1u32 << (*imm as u64).trailing_zeros().min(31),
                InstData::Binary {
                    op: op @ (BinaryOp::Ptradd | BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::Imul),
                    args,
                } => {
                    let a = alignment(func, args[0], known, slots, symbols, depth + 1);
                    let b = alignment(func, args[1], known, slots, symbols, depth + 1);
                    if *op == BinaryOp::Imul {
                        u64::from(a).saturating_mul(u64::from(b)).min(1 << 31) as u32
                    } else {
                        a.min(b)
                    }
                }
                _ => 1,
            };
            known[value.index()] = Some(result);
            result
        }
        let accesses: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| match func.inst_data(inst) {
                InstData::Load { addr, .. } | InstData::Store { addr, .. } => Some((inst, *addr)),
                _ => None,
            })
            .collect();
        for (inst, addr) in accesses {
            let align = alignment(func, addr, &mut known, &slots, &symbols, 0);
            match func.inst_data_mut(inst) {
                InstData::Load { flags, .. } | InstData::Store { flags, .. } => {
                    flags.align = flags.align.max(align);
                    flags.trap_ub = !flags.volatile;
                }
                _ => unreachable!(),
            }
        }
    }

    fn function(&mut self, def: &FuncDef<'p>) -> R<Function> {
        let global = &self.p.globals[def.global];
        let mapping = self.abi_mapping(def.ty, def.global, &[])?;
        let sig = &mapping.signature;
        let mut func = Function::new(global.symbol.to_owned(), sig.clone());
        // Most functions take 110 to 180 bytes of tree per instruction (a
        // little more per value), and 500 to 900 per block. Reserving as if
        // each took more than nearly any does skips most of the reallocation
        // without leaving functions with room they never use.
        let tree = def.nodes.used_bytes();
        func.reserve(tree / 192, tree / 224, tree / 1024);
        // Only the definition's own parameter objects supply restrict facts.
        // Declarations and ABI signatures deliberately carry none.
        for (index, &id) in def.params.iter().enumerate() {
            let local = &def.locals[id];
            if matches!(&local.ty, Ty::Ptr(t) if !matches!(t, Ty::Func(_) | Ty::Void))
                && local.sugar.is_restrict(self.p.typedefs)
                && let ParamMapping::Operand(operand) = mapping.params[index]
            {
                func.restrict_params.push(operand as u32);
            }
        }
        func.attrs = keel_ir::FunctionAttrs {
            weak: global.weak.is_some(),
            ..global.function_attrs.clone()
        };
        func.local = global.internal;
        func.hidden = global.hidden();
        let mut b = FunctionBuilder::with_context(&mut func, std::mem::take(&mut self.context.ssa));
        let entry = b.create_block();
        b.seal_block(entry);
        b.switch_to_block(entry);
        let params: Vec<Value> = sig
            .params
            .iter()
            .map(|p| b.append_block_param(entry, p.ty))
            .collect();
        let dispatch_table = DispatchTable::for_function(&self.p, def);
        let mut fg = FnGen {
            g: self,
            b,
            def,
            locals: Vec::with_capacity(def.locals.len()),
            alignments: HashMap::default(),
            sret: None,
            result: mapping.result,
            breaks: Vec::new(),
            continues: Vec::new(),
            labels: HashMap::default(),
            asm_goto_targets: Vec::new(),
            callees: HashMap::default(),
            dispatch: None,
            dispatch_table,
            cases: Vec::new(),
            terminated: false,
            scopes: Vec::new(),
            loc: None,
            scope: None,
            loop_depth: 0,
            labels_placed: false,
            exit_actions: Vec::new(),
            has_cleanup: def.locals.iter().any(|l| l.cleanup.is_some()),
            cleanup_jumps: crate::cleanup_jumps::CleanupJumps::analyze(def)?,
        };
        for local in &def.locals {
            let storage = if local.ty.runtime_sized() {
                Storage::Var(fg.b.declare_var(Type::Ptr))
            } else if !fg.g.debug
                && !local.in_memory
                && local.ty.is_scalar()
                && !local.ty.is_complex()
            {
                Storage::Var(fg.b.declare_var(local.ty.ir_with_layout(fg.g.p.c_layout)))
            } else {
                let types = fg.g.types();
                let size = types.size(&local.ty).max(1);
                let align = if fg.g.p.c_layout.pointer_bytes() == 4
                    && matches!(local.ty, Ty::Array(_))
                    && size >= 16
                {
                    local.align.max(16)
                } else {
                    local.align
                };
                let reserve = stack_storage_size(size, if align > 16 { align - 1 } else { 0 })
                    .map_err(|message| Error::new(def.loc, message))?;
                let slot = fg.b.create_stack_slot(reserve, align.min(16) as u32);
                if align <= 16 {
                    Storage::Slot(slot)
                } else {
                    let base = fg.b.ins().stack_addr(slot);
                    let bits = fg.b.ins().convert(ConvertOp::PtrToInt, Type::I64, base);
                    let padding = fg.b.ins().iconst(Type::I64, (align - 1) as i64);
                    let upper = fg.b.ins().iadd(bits, padding);
                    let mask = fg.b.ins().iconst(Type::I64, -(align as i64));
                    let rounded = fg.b.ins().and(upper, mask);
                    let addr = fg.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, rounded);
                    fg.alignments.insert(addr, align as u32);
                    let debug_slot = fg.g.debug.then(|| {
                        let pointer = fg.b.create_stack_slot(8, 8);
                        let at = fg.b.ins().stack_addr(pointer);
                        fg.b.ins().store(addr, at);
                        pointer
                    });
                    Storage::Aligned { addr, debug_slot }
                }
            };
            fg.locals.push(storage);
        }
        if fg.g.debug {
            fg.b.function_mut().debug_loc = fg.g.srcloc(def.loc);
            let ret = match &fg.g.p.globals[def.global].sugar {
                Sugar::Func(ret) => **ret,
                _ => Sugar::Plain,
            };
            fg.b.function_mut().debug_return = fg.g.debug_type(&def.ty.ret, &ret);
            // Only scopes with variables need to be told apart.
            let named = |local: &&Local| !local.name.contains('.');
            let mut has_vars = vec![false; def.scopes.len()];
            for scope in def.locals.iter().filter(named).filter_map(|l| l.scope) {
                has_vars[scope] = true;
            }
            for (scope, parent) in def.scopes.iter().enumerate() {
                let parent = parent.and_then(|p| fg.scopes[p]);
                let own = has_vars[scope].then(|| {
                    fg.b.function_mut().debug_scopes.push(parent);
                    DebugScope::new(fg.b.function().debug_scopes.len() - 1)
                });
                fg.scopes.push(own.or(parent));
            }
            for (id, local) in def.locals.iter().enumerate() {
                let location = match fg.locals[id] {
                    Storage::Slot(slot) => Some((slot, false)),
                    Storage::Aligned {
                        debug_slot: Some(slot),
                        ..
                    } => Some((slot, true)),
                    _ => None,
                };
                if let Some((slot, indirect)) = location
                    && named(&local)
                {
                    let ty =
                        fg.g.debug_type(&local.ty, &local.sugar)
                            .expect("a void local");
                    fg.b.function_mut().debug_vars.push(DebugVar {
                        name: local.name.to_owned(),
                        ty,
                        slot,
                        indirect,
                        param: def.params.contains(&id),
                        scope: local.scope.and_then(|s| fg.scopes[s]),
                    });
                }
            }
        }
        if matches!(mapping.result, ResultMapping::Indirect) {
            fg.sret = Some(params[0]);
        }
        for ((&id, parameter), abi_ty) in def.params.iter().zip(&mapping.params).zip(def.ty.params)
        {
            let ParamMapping::Operand(index) = *parameter else {
                // The local slot is the C parameter object; no incoming bytes exist.
                continue;
            };
            let value = params[index];
            // Old-style definitions receive promoted values but own objects of
            // the declared type. Convert before SSA binding AND width-sensitive
            // stores (including debug/address-taken/volatile parameter slots).
            let value = fg.convert(value, abi_ty, &def.locals[id].ty, def.loc)?;
            match fg.locals[id] {
                Storage::Var(var) => fg.b.def_var(var, value),
                // The callee owns its copy of a struct passed by value.
                // The debugger needs it at a fixed place in the frame, so
                // with debug info it is copied there.
                Storage::Slot(slot) if def.locals[id].ty.is_pair_or_struct() => {
                    if fg.g.debug {
                        let addr = fg.b.ins().stack_addr(slot);
                        fg.copy(addr, value, &def.locals[id].ty);
                    } else {
                        fg.locals[id] = Storage::Addr(value);
                    }
                }
                Storage::Slot(slot) => {
                    let addr = fg.b.ins().stack_addr(slot);
                    let flags = fg.object_flags(&def.locals[id].ty, &def.locals[id].sugar);
                    fg.b.ins().store_with_flags(value, addr, flags);
                }
                Storage::Aligned { addr, .. } => {
                    if def.locals[id].ty.is_pair_or_struct() {
                        fg.copy(addr, value, &def.locals[id].ty);
                    } else {
                        let flags = fg.object_flags(&def.locals[id].ty, &def.locals[id].sugar);
                        fg.b.ins().store_with_flags(value, addr, flags);
                    }
                }
                Storage::Addr(_) => unreachable!(),
            }
        }
        // Storing the parameters has no location, so it is part of the
        // prologue: a breakpoint on the function stops after it, where the
        // debugger can read them.
        fg.set_loc(def.loc);
        for stmt in def.body {
            fg.stmt(stmt)?;
        }
        if !fg.terminated {
            fg.return_default()?;
        }
        for &(label, loc) in &fg.asm_goto_targets {
            if fg.b.function().block_insts(fg.labels[label]).is_empty() {
                return Err(Error::new(
                    loc,
                    format!("asm goto references undefined label `{label}`"),
                ));
            }
        }
        fg.emit_dispatch();
        fg.b.seal_all_blocks();
        let alignments = std::mem::take(&mut fg.alignments);
        fg.g.context.ssa = fg.b.finalize_with_context();
        self.finish_access_flags(&mut func, alignments);
        Ok(func)
    }
}

#[derive(Clone, Copy)]
enum Storage {
    Var(Variable),
    Slot(StackSlot),
    /// A struct parameter: the address of the callee's copy.
    Addr(Value),
    /// An object aligned within a larger fixed slot. In debug builds a
    /// second slot holds its address for a DW_OP_deref location.
    Aligned {
        addr: Value,
        debug_slot: Option<StackSlot>,
    },
}

/// Where an assignment goes.
enum Place {
    Var(Variable),
    Mem(Value, Type, MemoryFlags),
    Bits {
        flags: MemoryFlags,
        addr: Value,
        unit: Type,
        field: BitField,
        signed: bool,
    },
    Aggregate {
        addr: Value,
        size: u32,
        align: u32,
        flags: MemoryFlags,
    },
}

// ---- constants known while the tree is lowered ----
//
// Macros leave operations on literals (`(t) | ((v) << 4)`, `-1`) that are
// not in a constant-expression context, so the parser keeps them. Folding
// them here, before any operand is emitted, avoids creating the operand
// constants at all. Only literals and operations on them are folded, so no
// evaluation is skipped; operations that trap at run time are left alone.

#[derive(Clone, Copy)]
enum ExitAction {
    Restore(Value),
    Cleanup(LocalId),
}

struct FnGen<'g, 'p, 'f> {
    g: &'g mut Gen<'p>,
    b: FunctionBuilder<'f>,
    def: &'f FuncDef<'p>,
    locals: Vec<Storage>,
    /// Facts attached to distinct SSA definitions, never to a mutable C variable.
    alignments: HashMap<Value, u32>,
    sret: Option<Value>,
    result: ResultMapping,
    breaks: Vec<(Block, usize)>,
    continues: Vec<(Block, usize)>,
    /// Registered as declarations execute; emit in reverse order on each exit.
    /// Branch exits do not consume the list used to compile sibling paths.
    exit_actions: Vec<ExitAction>,
    has_cleanup: bool,
    cleanup_jumps: crate::cleanup_jumps::CleanupJumps<'p>,
    labels: HashMap<&'p str, Block>,
    asm_goto_targets: Vec<(&'p str, Loc)>,
    /// Functions already imported, by global: their signatures are fixed
    /// by a prototype or definition.
    callees: HashMap<usize, keel_ir::FuncRef>,
    /// All computed gotos share one switch, with the label number as its
    /// first parameter. Leave it unsealed until every dispatch edge exists.
    dispatch: Option<Block>,
    /// Optional immutable-table fusion; every indirect goto uses this table.
    dispatch_table: Option<DispatchTable>,
    /// The case blocks of the switches being built.
    cases: Vec<Vec<Block>>,
    /// The current block ends in a terminator; code that follows goes in a
    /// new, unreachable block.
    terminated: bool,
    /// The debug scope of each block scope: its own if it declares
    /// variables, else its parent's. Empty without debug info.
    scopes: Vec<Option<DebugScope>>,
    /// The location being given to instructions, and their scope.
    loc: Option<SourceLoc>,
    scope: Option<DebugScope>,
    /// How many loops the statement being built is in.
    loop_depth: u32,
    /// Whether a label has been placed: a `goto` after it may run what
    /// follows the label again.
    labels_placed: bool,
}

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    fn new_block(&mut self) -> Block {
        self.b.create_block()
    }

    fn switch_to(&mut self, block: Block) {
        self.b.switch_to_block(block);
        self.terminated = false;
    }

    fn jump_to(&mut self, block: Block) {
        if !self.terminated {
            self.b.ins().jump(block, &[]);
            self.terminated = true;
        }
    }

    /// Code after a `return` or `break` goes in a block no branch reaches.
    fn ensure_block(&mut self) {
        if self.terminated {
            let block = self.new_block();
            self.b.seal_block(block);
            self.switch_to(block);
        }
    }

    fn set_loc(&mut self, loc: Loc) {
        self.loc = self.g.srcloc(loc);
        self.set_scope(self.scope);
    }

    /// Puts the instructions that follow in lexical scope `scope`.
    fn set_scope(&mut self, scope: Option<DebugScope>) {
        self.scope = scope;
        if let Some(loc) = &mut self.loc {
            loc.scope = scope;
        }
        self.b.set_srcloc(self.loc);
    }

    fn size(&self, ty: &Ty) -> u64 {
        self.g.types().size(ty)
    }

    /// The address of temporary `slot`, whose lifetime starts here. With
    /// debug information every slot keeps its own home, so no markers.
    pub(crate) fn start_temporary(&mut self, slot: StackSlot) -> Value {
        let address = self.b.ins().stack_addr(slot);
        if !self.g.debug {
            self.b.ins().intrinsic(Intrinsic::LifetimeStart, &[address]);
        }
        address
    }

    /// Ends the lifetime that `start_temporary` began at `address`.
    pub(crate) fn end_temporary(&mut self, address: Value) {
        if !self.g.debug && !self.terminated {
            self.b.ins().intrinsic(Intrinsic::LifetimeEnd, &[address]);
        }
    }

    fn temporary_slot(&mut self, ty: &Ty, loc: Loc) -> R<StackSlot> {
        let size = self
            .g
            .types()
            .checked_size(ty)
            .and_then(|size| stack_storage_size(size, 0))
            .map_err(|message| Error::new(loc, message))?;
        let align = u32::try_from(self.g.types().align(ty))
            .map_err(|_| Error::new(loc, "temporary alignment exceeds the IR limit"))?;
        Ok(self.b.create_stack_slot(size, align))
    }

    fn label_block(&mut self, name: &'p str) -> Block {
        if let Some(&b) = self.labels.get(name) {
            return b;
        }
        let b = self.new_block();
        self.labels.insert(name, b);
        b
    }

    fn zero(&mut self, ty: Type) -> Value {
        match ty {
            Type::Vector(shape) => {
                let zero = self.zero(shape.lane_type());
                self.b.ins().vsplat(Type::Vector(shape), zero)
            }
            Type::F32 => self.b.ins().f32const(0.0),
            Type::F64 => self.b.ins().f64const(0.0),
            Type::Ptr => {
                let z = self.b.ins().iconst(Type::I64, 0);
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, z)
            }
            ty => self.b.ins().iconst(ty, 0),
        }
    }

    /// Returns at the end of a function: nothing, or zero (which `main`
    /// must return).
    fn return_default(&mut self) -> R<()> {
        self.emit_exit_actions(0)?;
        let ret = &self.def.ty.ret;
        if *ret == Ty::Void || ret.is_pair_or_struct() {
            self.b.ins().return_(&[]);
        } else {
            let z = self.zero(ret.ir_with_layout(self.g.p.c_layout));
            self.b.ins().return_(&[z]);
        }
        self.terminated = true;
        Ok(())
    }

    /// Compute a size from saved dimension counts; pointer size is static.
    fn runtime_size(&mut self, ty: &Ty<'p>) -> R<Value> {
        if let Ty::Array(a) = ty {
            let count = if let Some(id) = a.vla {
                self.val(&Expr {
                    kind: ExprKind::Local(id),
                    ty: ULONG,
                    sugar: Sugar::Plain,
                    loc: self.def.loc,
                })?
            } else {
                self.b.ins().iconst(Type::I64, a.len.unwrap_or(0) as i64)
            };
            let stride = self.runtime_size(&a.elem)?;
            Ok(self.b.ins().imul(count, stride))
        } else {
            let size = self.size(ty);
            Ok(self.b.ins().iconst(Type::I64, size as i64))
        }
    }

    fn has_vm_types(&self) -> bool {
        self.def.has_vm
    }

    fn has_cleanup(&self) -> bool {
        self.has_cleanup
    }

    fn emit_exit_actions(&mut self, depth: usize) -> R<()> {
        if self.terminated {
            return Ok(());
        }
        for index in (depth..self.exit_actions.len()).rev() {
            match self.exit_actions[index] {
                ExitAction::Restore(token) => {
                    self.b.ins().intrinsic(Intrinsic::StackRestore, &[token]);
                }
                ExitAction::Cleanup(id) => {
                    let local = &self.def.locals[id];
                    let (g, loc) = local.cleanup.unwrap();
                    let Ty::Func(fty) = self.g.p.globals[g].ty else {
                        unreachable!()
                    };
                    let object = Expr {
                        kind: ExprKind::Local(id),
                        ty: local.ty,
                        sugar: local.sugar,
                        loc,
                    };
                    self.set_loc(loc);
                    let addr = self.addr(&object)?;
                    let mut args = Vec::new();
                    // Cleanup discards the result, but nonempty aggregates still
                    // require the normal hidden result address.
                    let mapping = self.g.abi_mapping(fty, g, &[])?;
                    let mut result = None;
                    if matches!(mapping.result, ResultMapping::Indirect) {
                        let slot = self.temporary_slot(&fty.ret, loc)?;
                        let address = self.start_temporary(slot);
                        result = Some(address);
                        args.push(address);
                    }
                    args.push(addr);
                    self.g.use_global(g);
                    let callee = self.import_global(g, fty, None)?;
                    self.b.ins().call(callee, &args);
                    if let Some(address) = result {
                        self.end_temporary(address);
                    }
                }
            }
        }
        Ok(())
    }

    fn leave_scope(&mut self, depth: usize) -> R<()> {
        self.emit_exit_actions(depth)?;
        self.exit_actions.truncate(depth);
        Ok(())
    }

    fn register_cleanup(&mut self, id: LocalId) {
        if self.def.locals[id].cleanup.is_some() {
            self.exit_actions.push(ExitAction::Cleanup(id));
        }
    }

    // ---- statements ----

    /// Copies an object of type `ty` from `src` to `dst`: small ones with
    /// loads and stores, big ones with `memmove` (self-assignment is valid).
    fn copy(&mut self, dst: Value, src: Value, ty: &Ty) {
        let size = self.size(ty);
        self.copy_bytes_with_flags(
            dst,
            src,
            size,
            MemoryFlags::default(),
            MemoryFlags::default(),
        );
    }

    fn copy_bytes_with_flags(
        &mut self,
        dst: Value,
        src: Value,
        size: u64,
        dst_flags: MemoryFlags,
        src_flags: MemoryFlags,
    ) {
        if size > 64 && !dst_flags.volatile && !src_flags.volatile {
            let n = self.b.ins().iconst(Type::I64, size as i64);
            self.b.ins().intrinsic(Intrinsic::Memmove, &[dst, src, n]);
            return;
        }
        // aarch64 allows unaligned loads and stores, so use wide ones.
        let mut off = 0;
        for (chunk, ty) in [
            (8, Type::I64),
            (4, Type::I32),
            (2, Type::I16),
            (1, Type::I8),
        ] {
            while size - off >= chunk {
                let s = self.offset(src, off);
                let d = self.offset(dst, off);
                let v = self.b.ins().load_with_flags(ty, s, src_flags);
                self.b.ins().store_with_flags(v, d, dst_flags);
                off += chunk;
            }
        }
    }

    fn zero_bytes_with_flags(&mut self, dst: Value, size: u64, flags: MemoryFlags) {
        if size > 64 && !flags.volatile {
            let z = self.b.ins().iconst(Type::I8, 0);
            let n = self.b.ins().iconst(Type::I64, size as i64);
            self.b.ins().intrinsic(Intrinsic::Memset, &[dst, z, n]);
            return;
        }
        let mut off = 0;
        for (chunk, ty) in [
            (8, Type::I64),
            (4, Type::I32),
            (2, Type::I16),
            (1, Type::I8),
        ] {
            while size - off >= chunk {
                let d = self.offset(dst, off);
                let z = self.b.ins().iconst(ty, 0);
                self.b.ins().store_with_flags(z, d, flags);
                off += chunk;
            }
        }
    }

    /// Runs a local's initializer.
    fn init_local(&mut self, id: LocalId, init: &Init<'p>) -> R<()> {
        let ty = self.def.locals[id].ty;
        let flags = self.object_flags(&ty, &self.def.locals[id].sugar);
        if let Storage::Var(var) = self.locals[id] {
            let v = match init.items.first() {
                Some(InitItem::Expr { expr, .. }) => self.val(expr)?,
                _ => self.zero(ty.ir_with_layout(self.g.p.c_layout)),
            };
            self.b.def_var(var, v);
            return Ok(());
        }
        let base = match self.locals[id] {
            Storage::Slot(slot) => self.b.ins().stack_addr(slot),
            Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
            Storage::Var(_) => unreachable!(),
        };
        let whole =
            matches!(init.items, [InitItem::Expr { offset: 0, expr, bits: None }] if expr.ty == ty);
        if !whole {
            let size = self.size(&ty);
            self.zero_bytes_with_flags(base, size, flags);
        }
        for item in init.items {
            match item {
                InitItem::Expr { offset, expr, bits } => {
                    let v = self.val(expr)?;
                    let addr = self.offset(base, *offset);
                    let place = match bits {
                        Some(field) => Place::Bits {
                            flags,
                            addr,
                            unit: expr.ty.ir_with_layout(self.g.p.c_layout),
                            field: *field,
                            signed: false,
                        },
                        None if Self::is_aggregate(&expr.ty) => {
                            self.aggregate_place(addr, &expr.ty, flags, expr.loc)?
                        }
                        None => Place::Mem(addr, expr.ty.ir_with_layout(self.g.p.c_layout), flags),
                    };
                    self.store_place(&place, v);
                }
                InitItem::Bytes { offset, bytes } => {
                    let size = (self.size(&ty) - offset).min(bytes.len() as u64);
                    let name = self.g.anon_data(&bytes[..size as usize]);
                    let data = self.b.import_data(&name);
                    let src = self.b.ins().symbol_addr(Symbol::Data(data));
                    let dst = self.offset(base, *offset);
                    self.copy_bytes_with_flags(dst, src, size, flags, MemoryFlags::default());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use crate::strings::Strings;

    fn parser(arena: &bumpalo::Bump) -> Parser<'_> {
        let mut p = Parser::new(
            "int first; int second; int collision;",
            "align.c",
            crate::Options::default().target,
            Strings::new(arena),
        );
        p.translation_unit().unwrap();
        p.globals[0].symbol = "shared";
        p.globals[0].align = 8;
        p.globals[1].symbol = "shared";
        p.globals[1].align = 64;
        p.globals[2].symbol = ".str.0";
        p.globals[2].align = 16;
        p.strings.push(crate::parse::StrLit {
            bytes: b"a\0",
            wide: false,
        });
        p.strings.push(crate::parse::StrLit {
            bytes: b"b\0\0\0\0\0\0\0",
            wide: true,
        });
        p.strings.push(crate::parse::StrLit {
            bytes: b"c\0",
            wide: false,
        });
        p
    }

    fn old_alignment(g: &Gen<'_>, name: &str) -> u32 {
        g.p.globals
            .iter()
            .find(|global| global.symbol == name)
            .map(|global| global.align as u32)
            .or_else(|| {
                g.module
                    .data_object(name)
                    .and_then(|data| data.init.as_ref())
                    .map(|init| init.align)
            })
            .unwrap_or(1)
    }

    fn data(name: &str, align: Option<u32>) -> DataObject {
        DataObject {
            section: None,
            retain: false,
            name: name.into(),
            local: true,
            weak: false,
            weak_odr: false,
            hidden: false,
            thread_local: false,
            init: align.map(|align| DataInit {
                align,
                constant: true,
                items: vec![],
            }),
        }
    }

    #[test]
    fn symbol_index_preserves_first_matches_and_late_generated_data() {
        let arena = bumpalo::Bump::new();
        let p = parser(&arena);
        let mut g = Gen::new(&p, Vec::new(), &[], false);
        assert!(g.context.global_alignments.is_none());
        assert_eq!(g.symbol_alignment("unknown"), 1);
        assert_eq!(g.symbol_alignment("shared"), 8);
        // These objects are emitted after the declaration index is built.
        for i in 0..3 {
            g.string_symbol(i);
        }
        let init = g.anon_data(b"bytes");
        assert_eq!(g.anon_data(b"bytes"), init);
        g.push_generated_data(data("duplicate", Some(32)));
        g.push_generated_data(data("duplicate", Some(64)));
        g.push_generated_data(data("no_initializer", None));
        g.push_generated_data(data("no_initializer", Some(64)));
        for name in [
            "shared",
            ".str.0",
            ".str.1",
            ".str.2",
            &init,
            "duplicate",
            "no_initializer",
            "unknown",
        ] {
            assert_eq!(g.symbol_alignment(name), old_alignment(&g, name), "{name}");
        }
        assert_eq!(g.symbol_alignment(".str.0"), 16);
        assert_eq!(g.symbol_alignment(".str.1"), 4);
        assert_eq!(g.symbol_alignment("duplicate"), 32);
        assert_eq!(g.symbol_alignment("no_initializer"), 1);
    }

    #[test]
    fn dense_tables_keep_alignment_and_conservative_offsets() {
        let arena = bumpalo::Bump::new();
        let p = parser(&arena);
        let mut g = Gen::new(&p, Vec::new(), &[], false);
        let init = g.anon_data(b"bytes");
        let mut f = Function::new("f", Signature::new(&[], &[Type::I8]));
        let entry = f.create_block();
        f.create_stack_slot(32, 8);
        let slot = f.create_stack_slot(64, 16);
        for i in 0..12 {
            f.import_data(&format!("unused{i}"));
        }
        let shared = f.import_data("shared");
        let generated = f.import_data(&init);
        let unknown = f.import_data("unknown");
        let stack = f.ins(keel_ir::InsertPoint::End(entry)).stack_addr(slot);
        let shared = f
            .ins(keel_ir::InsertPoint::End(entry))
            .symbol_addr(Symbol::Data(shared));
        let generated = f
            .ins(keel_ir::InsertPoint::End(entry))
            .symbol_addr(Symbol::Data(generated));
        let unknown = f
            .ins(keel_ir::InsertPoint::End(entry))
            .symbol_addr(Symbol::Data(unknown));
        let offset = f.ins(keel_ir::InsertPoint::End(entry)).iconst(Type::I64, 2);
        let unaligned = f
            .ins(keel_ir::InsertPoint::End(entry))
            .ptradd(shared, offset);
        let mut result = None;
        for addr in [stack, shared, generated, unknown, unaligned] {
            let value = f.ins(keel_ir::InsertPoint::End(entry)).load(Type::I8, addr);
            result.get_or_insert(value);
        }
        f.ins(keel_ir::InsertPoint::End(entry))
            .return_(&[result.unwrap()]);
        g.finish_access_flags(&mut f, HashMap::default());
        keel_ir::verify(&f).unwrap();
        let alignments: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, inst)| match f.inst_data(inst) {
                InstData::Load { flags, .. } => Some((flags.align, flags.trap_ub)),
                _ => None,
            })
            .collect();
        assert_eq!(
            alignments,
            [16, 8, 8, 1, 2].map(|align| (align, true)),
            "C accesses carry their alignment and an undefined fault"
        );
    }

    /// The previous map-memoized walk, kept verbatim as the reference for
    /// the dense value-indexed memo (including its depth-limited caching).
    fn old_alignments(
        func: &Function,
        mut known: HashMap<Value, u32>,
        slots: &[u32],
        symbols: &[u32],
    ) -> Vec<u32> {
        fn alignment(
            func: &Function,
            value: Value,
            known: &mut HashMap<Value, u32>,
            slots: &[u32],
            symbols: &[u32],
            depth: usize,
        ) -> u32 {
            let value = func.resolve(value);
            if let Some(&a) = known.get(&value) {
                return a;
            }
            if depth == 64 {
                return 1;
            }
            let ValueDef::Result(inst, _) = func.value_def(value) else {
                return 1;
            };
            let result = match func.inst_data(inst) {
                InstData::StackAddr { slot } => slots[slot.index()],
                InstData::SymbolAddr {
                    symbol: Symbol::Data(data),
                } => symbols[data.index()],
                InstData::Iconst { imm, .. } => 1u32 << (*imm as u64).trailing_zeros().min(31),
                InstData::Binary {
                    op: op @ (BinaryOp::Ptradd | BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::Imul),
                    args,
                } => {
                    let a = alignment(func, args[0], known, slots, symbols, depth + 1);
                    let b = alignment(func, args[1], known, slots, symbols, depth + 1);
                    if *op == BinaryOp::Imul {
                        u64::from(a).saturating_mul(u64::from(b)).min(1 << 31) as u32
                    } else {
                        a.min(b)
                    }
                }
                _ => 1,
            };
            known.insert(value, result);
            result
        }
        let accesses: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| match func.inst_data(inst) {
                InstData::Load { addr, flags, .. } | InstData::Store { addr, flags, .. } => {
                    Some((*addr, flags.align))
                }
                _ => None,
            })
            .collect();
        accesses
            .into_iter()
            .map(|(addr, align)| align.max(alignment(func, addr, &mut known, slots, symbols, 0)))
            .collect()
    }

    fn access_alignments(f: &Function) -> Vec<u32> {
        f.layout_insts()
            .filter_map(|(_, inst)| match f.inst_data(inst) {
                InstData::Load { flags, .. } | InstData::Store { flags, .. } => Some(flags.align),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dense_memo_matches_map_memo_with_seeds_aliases_and_depth_limit() {
        use keel_ir::InsertPoint::End;
        let arena = bumpalo::Bump::new();
        let p = parser(&arena);
        let mut g = Gen::new(&p, Vec::new(), &[], false);
        let mut f = Function::new("f", Signature::new(&[Type::Ptr], &[]));
        let entry = f.create_block();
        let param = f.append_block_param(entry, Type::Ptr);
        let next = f.create_block();
        let merged = f.append_block_param(next, Type::Ptr);
        let slot = f.create_stack_slot(256, 16);
        let shared = f.import_data("shared");
        let base = f.ins(End(entry)).stack_addr(slot);
        let data = f.ins(End(entry)).symbol_addr(Symbol::Data(shared));
        let four = f.ins(End(entry)).iconst(Type::I64, 4);
        let six = f.ins(End(entry)).iconst(Type::I64, 6);
        // A seed on an otherwise unaligned value, as __builtin_assume_aligned.
        let zero = f.ins(End(entry)).iconst(Type::I64, 0);
        let assumed = f.ins(End(entry)).ptradd(param, zero);
        // A chain deeper than the walk's limit. Visiting its tail first caches
        // depth-limited answers that later, shallower queries then reuse.
        let mut chain = vec![base];
        for i in 0..80 {
            let step = if i % 2 == 0 { four } else { six };
            let last = *chain.last().unwrap();
            chain.push(f.ins(End(entry)).ptradd(last, step));
        }
        let scaled = f.ins(End(entry)).imul(four, six);
        let mixed = f.ins(End(entry)).ptradd(data, scaled);
        f.ins(End(entry)).jump(next, &[assumed]);
        let mut addresses = vec![chain[80], chain[70], chain[10], chain[1], base];
        addresses.extend([data, mixed, assumed, merged, param]);
        for &addr in &addresses {
            let v = f.ins(End(next)).load(Type::I8, addr);
            f.ins(End(next)).store(v, addr);
        }
        // An alias, as a removed trivial SSA parameter leaves behind.
        let alias = f.append_block_param(next, Type::Ptr);
        f.replace_all_uses(alias, chain[3]);
        f.ins(End(next)).load(Type::I32, alias);
        f.ins(End(next)).return_(&[]);
        let mut seeds = HashMap::default();
        seeds.insert(assumed, 32);
        let slots: Vec<_> = f.stack_slots().map(|(_, d)| d.align).collect();
        let symbols: Vec<_> = f
            .data_symbols()
            .map(|(_, name)| g.symbol_alignment(name))
            .collect();
        let expected = old_alignments(&f, seeds.clone(), &slots, &symbols);
        g.finish_access_flags(&mut f, seeds);
        let got = access_alignments(&f);
        assert_eq!(got, expected);
        // Sanity: the cases above really differ from one another.
        // Each address has a load and a store; the alias load is last.
        assert_eq!(&got[..4], [1, 1, 1, 1], "depth-limited answers cached");
        assert_eq!(got[4], 2, "a shallow chain value: stack + 4 + 6 + ...");
        assert_eq!(got[6], 4, "stack + 4");
        assert_eq!(got[8], 16, "the stack slot base");
        assert_eq!(got[14], 32, "seeded value");
        assert_eq!(got[16], 1, "block parameter");
        assert_eq!(*got.last().unwrap(), 2, "alias resolves to its target");
    }
}
