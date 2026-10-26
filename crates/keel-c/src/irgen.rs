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

pub(crate) mod wasm_scalar;

pub struct Gen<'p> {
    p: &'p Parser<'p>,
    /// The parser's function definitions, each dropped once emitted.
    funcs: Vec<Option<FuncDef<'p>>>,
    files: &'p [&'p str],
    module: Module,
    debug: bool,
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
    /// Borrow immutable declaration spellings. Build once, on the first
    /// alignment query; aliases retain the first declaration's alignment.
    global_alignments: Option<HashMap<&'p str, u32>>,
    /// Non-declaration objects emitted while functions are built. The
    /// first module object wins, matching Module::data_object.
    generated_alignments: HashMap<String, u32>,
    /// Whether each struct has a volatile member at any depth, once asked.
    /// Struct definitions no longer change after parsing.
    volatile_members: Vec<std::cell::Cell<Option<bool>>>,
    /// The SSA builder's tables, handed from one function to the next.
    ssa: FunctionBuilderContext,
}

impl<'p> Gen<'p> {
    pub fn new(
        p: &'p Parser<'p>,
        funcs: Vec<FuncDef<'p>>,
        files: &'p [&'p str],
        debug: bool,
    ) -> Self {
        Self {
            p,
            funcs: funcs.into_iter().map(Some).collect(),
            files,
            module: Module::default(),
            debug,
            srcfiles: HashMap::default(),
            used: vec![false; p.globals.len()],
            worklist: Vec::new(),
            strings_used: vec![false; p.strings.len()],
            unprototyped: HashMap::default(),
            anon: HashMap::default(),
            dtypes: DebugTypes::default(),
            global_alignments: None,
            generated_alignments: HashMap::default(),
            volatile_members: vec![std::cell::Cell::new(None); p.structs.len()],
            ssa: FunctionBuilderContext::new(),
        }
    }

    /// Whether an object of type `ty` is or contains anything volatile.
    /// Pointer pointee qualifiers do not qualify the pointer object itself.
    fn object_volatile(&self, ty: &Ty, sugar: &Sugar) -> bool {
        sugar.is_volatile(&self.p.typedefs)
            || match ty {
                Ty::Array(a) => self.object_volatile(&a.elem, &sugar.pointee(&self.p.typedefs)),
                Ty::Struct(id) => {
                    if let Some(volatile) = self.volatile_members[*id].get() {
                        return volatile;
                    }
                    let volatile = self.p.structs[*id]
                        .members
                        .iter()
                        .any(|m| self.object_volatile(&m.ty, &m.sugar));
                    self.volatile_members[*id].set(Some(volatile));
                    volatile
                }
                _ => false,
            }
    }

    pub fn run(mut self) -> R<Module> {
        self.module.assembly = self.p.assembly.clone();
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
        for global in &self.p.globals {
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
                && (global.align > 4096 || !global.align.max(1).is_power_of_two())
            {
                return Err(Error::new(
                    Loc::default(),
                    "custom/retained data alignment must be a power of two up to 4096",
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
                    if f.ret == Ty::Int(IntKind::Int) && f.prototyped && !f.variadic
                    && matches!(f.params, [Ty::Ptr(Ty::Func(cb))]
                        if cb.ret == Ty::Void && cb.prototyped && !cb.variadic && cb.params.is_empty()));
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
            if let Some(loc) = global.weak {
                if global.internal
                    || global.thread_local
                    || (!global.defined && global.visibility.is_some_and(|v| v.hidden()))
                {
                    return Err(Error::new(
                        loc,
                        "weak requires external linkage, non-TLS storage and default visibility for undefined references",
                    ));
                }
            }
            if global.retain.is_some() && !global.defined {
                return Err(Error::new(
                    global.retain.unwrap_or_default(),
                    "retain requires a definition in this translation unit",
                ));
            }
            if let Some(loc) = global.lifecycle {
                if !matches!(global.ty, Ty::Func(f) if f.ret == Ty::Void && f.prototyped && f.params.is_empty() && !f.variadic)
                    || global.incompatible_return
                    || !global.defined
                    || global.func.is_none()
                    || global.returns_twice
                    || global.function_attrs.noreturn
                {
                    return Err(Error::new(
                        loc,
                        "constructor/destructor requires a defined, normally returning void(void) function",
                    ));
                }
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
                            ..global.function_attrs
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
                    Some((f.name.clone(), f.sig.clone(), f.attrs))
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
        let globals = self.global_alignments.get_or_insert_with(|| {
            let mut alignments =
                HashMap::with_capacity_and_hasher(self.p.globals.len(), Default::default());
            for global in &self.p.globals {
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
        self.dtypes.get(&mut self.module, self.p, ty, sugar)
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
        for ty in std::iter::once(&fty.ret).chain(fty.params) {
            self.check_value_type(ty, loc)?;
        }
        Ok(())
    }

    /// How a value of C type `ty` crosses a call; the name of the type if
    /// keel cannot pass it.
    fn abi_param(&self, ty: &Ty) -> Result<AbiParam, &'static str> {
        if let Some(name) = self.types().unsupported_value(ty) {
            return Err(name);
        }
        if self.p.c_layout != CDataLayout::NATIVE_LP64 {
            // Private preflight admits only plain i32/i64 signatures.
            return Ok(AbiParam::new(ty.ir_with_layout(self.p.c_layout)));
        }
        Ok(match ty {
            Ty::Struct(_) | Ty::Complex(_) => AbiParam {
                ty: Type::Ptr,
                ext: ArgExt::None,
                purpose: Purpose::ByVal(self.aggregate_layout(ty)?),
            },
            Ty::Int(k) if k.size_with_layout(self.p.c_layout) < 4 => AbiParam {
                ty: ty.ir_with_layout(self.p.c_layout),
                ext: if k.signed() {
                    ArgExt::Sext
                } else {
                    ArgExt::Uext
                },
                purpose: Purpose::Normal,
            },
            _ => AbiParam::new(ty.ir_with_layout(self.p.c_layout)),
        })
    }

    fn aggregate_layout(&self, ty: &Ty) -> Result<keel_ir::Layout, &'static str> {
        if let Some(name) = self.types().unsupported_value(ty) {
            return Err(name);
        }
        if self.types().contains_vector_union(ty) {
            return Err("unions containing vectors passed by value");
        }
        if self.p.target == keel_ir::Target::X86_64LinuxGnu && self.types().contains_vector(ty) {
            return Err(
                "vector fields in by-value x86-64 aggregates require unsupported SSEUP classification",
            );
        }
        if self.types().align(ty) > 16 {
            return Err("aggregates aligned above 16 bytes passed by value");
        }
        if self.p.target == keel_ir::Target::X86_64LinuxGnu
            && self.types().has_unaligned_fields(ty, 0)
        {
            return Err("aggregates with unaligned fields passed by value on x86-64");
        }
        if self.types().contains_complex(ty) {
            return self.types().complex_abi_layout(ty).ok_or(
                "complex-containing by-value aggregate with union, bit-field or non-natural layout",
            );
        }
        Ok(if self.p.target == keel_ir::Target::Aarch64LinuxGnu {
            self.types().aapcs_layout(ty)
        } else {
            self.types().abi_layout(ty)
        })
    }

    /// The keel signature of a C function type. A struct result becomes a
    /// leading `sret` parameter. `g` is the function, if known by name.
    fn signature(&self, fty: &FuncTy, g: usize, extra: &[Ty]) -> R<Signature> {
        let mut sig = Signature::new(&[], &[]);
        sig.params.reserve(fty.params.len() + extra.len() + 1);
        if fty.ret.is_pair_or_struct() {
            sig.params.push(AbiParam {
                ty: Type::Ptr,
                ext: ArgExt::None,
                purpose: Purpose::Sret(
                    self.aggregate_layout(&fty.ret)
                        .map_err(|t| self.unsupported(g, t))?,
                ),
            });
        } else if fty.ret != Ty::Void {
            sig.returns.push(
                self.abi_param(&fty.ret)
                    .map_err(|t| self.unsupported(g, t))?,
            );
        }
        for ty in fty.params.iter().chain(extra) {
            sig.params
                .push(self.abi_param(ty).map_err(|t| self.unsupported(g, t))?);
        }
        sig.variadic = fty.variadic;
        sig.returns_twice = g != usize::MAX && self.p.globals[g].returns_twice;
        Ok(sig)
    }

    /// The error for a call to or definition of function `g` (if known)
    /// that passes a value of type `name`.
    fn unsupported(&self, g: usize, name: &str) -> Error {
        let what = match self.p.globals.get(g) {
            Some(global) => format!("`{}` passes a `{name}`", global.name),
            None => format!("a call passes a `{name}`"),
        };
        Error::new(Loc::default(), format!("{what}, which is not supported"))
    }

    /// Preserve proven address alignment on every emitted access, including
    /// byte copies and initializer chunks. No pointee-type inference: packed
    /// members and pointers cast from byte buffers may be unaligned.
    fn preserve_access_alignment(&mut self, func: &mut Function, seeds: HashMap<Value, u32>) {
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
                    flags.align = flags.align.max(align)
                }
                _ => unreachable!(),
            }
        }
    }

    fn function(&mut self, def: &FuncDef<'p>) -> R<Function> {
        let global = &self.p.globals[def.global];
        let sig = self.signature(&def.ty, def.global, &[])?;
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
                && local.sugar.is_restrict(&self.p.typedefs)
            {
                func.restrict_params
                    .push((index + usize::from(def.ty.ret.is_pair_or_struct())) as u32);
            }
        }
        func.attrs = keel_ir::FunctionAttrs {
            weak: global.weak.is_some(),
            ..global.function_attrs
        };
        func.local = global.internal;
        func.hidden = global.hidden();
        let mut b = FunctionBuilder::with_context(&mut func, std::mem::take(&mut self.ssa));
        let entry = b.create_block();
        b.seal_block(entry);
        b.switch_to_block(entry);
        let params: Vec<Value> = sig
            .params
            .iter()
            .map(|p| b.append_block_param(entry, p.ty))
            .collect();
        let dispatch_table = DispatchTable::for_function(self.p, def);
        let mut fg = FnGen {
            g: self,
            b,
            def,
            locals: Vec::with_capacity(def.locals.len()),
            alignments: HashMap::default(),
            sret: None,
            breaks: Vec::new(),
            continues: Vec::new(),
            labels: HashMap::default(),
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
                let reserve = size
                    .checked_add(if align > 16 { align - 1 } else { 0 })
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| Error::new(def.loc, "local object exceeds the IR stack-slot size limit (including alignment padding)"))?;
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
        let mut params = params.into_iter();
        if def.ty.ret.is_pair_or_struct() {
            fg.sret = params.next();
        }
        for ((&id, value), abi_ty) in def.params.iter().zip(params).zip(def.ty.params) {
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
        fg.emit_dispatch();
        fg.b.seal_all_blocks();
        let alignments = std::mem::take(&mut fg.alignments);
        fg.g.ssa = fg.b.finalize_with_context();
        self.preserve_access_alignment(&mut func, alignments);
        Ok(func)
    }

    /// A defined object: its bytes, with relocations for addresses.
    fn data(&mut self, g: usize, record_references: bool) -> R<DataObject> {
        let global = &self.p.globals[g];
        let types = self.types();
        let size = types.size(&global.ty) as usize;
        let align = if self.p.c_layout.pointer_bytes() == 4
            && matches!(global.ty, Ty::Array(_))
            && size >= 16
        {
            global.align.max(16)
        } else {
            global.align.max(1)
        } as u32;
        let pointer_bytes = self.p.c_layout.pointer_bytes() as usize;
        let mut bytes = vec![0u8; size];
        let mut relocs: Vec<(usize, String, i64)> = Vec::new();
        let items = global.init.as_ref().map_or(&[][..], |i| &i.items[..]);
        for item in items {
            match item {
                InitItem::Bytes { offset, bytes: b } => {
                    let start = *offset as usize;
                    let n = b.len().min(size - start);
                    bytes[start..start + n].copy_from_slice(&b[..n]);
                }
                InitItem::Expr { offset, expr, bits } => {
                    let offset = *offset as usize;
                    // All declarations have merged: only final strong binding
                    // proves a global address nonnull in a static initializer.
                    let value = if self.p.c_layout == CDataLayout::NATIVE_LP64 {
                        crate::eval::eval_with(expr, &|g| self.p.globals[g].weak.is_none())
                    } else {
                        crate::eval::eval_with_binding_and_layout(
                            expr,
                            &|g| self.p.globals[g].weak.is_none(),
                            self.p.c_layout,
                        )
                    };
                    let Some(value) = value else {
                        return Err(Error::new(expr.loc, "initializer is not a constant"));
                    };
                    let width = self.types().size(&expr.ty) as usize;
                    match value {
                        Const::Int(v) => match bits {
                            Some(bf) => {
                                // Write the field itself, including a possible ninth byte for
                                // an unaligned 64-bit field. Never touch its declared unit's tail.
                                for bit in 0..bf.width {
                                    let target = bf.shift + bit;
                                    let byte = &mut bytes[offset + (target / 8) as usize];
                                    let mask = 1 << (target % 8);
                                    *byte = (*byte & !mask) | (((v as u128 >> bit) as u8 & 1) * mask);
                                }
                            }
                            None => {
                                let v = if expr.ty.is_signed() {
                                    v as i128 as u128
                                } else {
                                    v as u64 as u128
                                };
                                write_le(&mut bytes[offset..offset + width], v);
                            }
                        },
                        Const::Complex(r, i) => {
                            let Ty::Complex(elem) = expr.ty else {
                                unreachable!()
                            };
                            let n = width / 2;
                            for (index, f) in [r, i].into_iter().enumerate() {
                                let bits = if *elem == Ty::Float {
                                    u128::from((f as f32).to_bits())
                                } else {
                                    u128::from(f.to_bits())
                                };
                                let start = offset + index * n;
                                write_le(&mut bytes[start..start + n], bits);
                            }
                        }
                        Const::Float(f) => {
                            let v = if expr.ty == Ty::Float {
                                u128::from((f as f32).to_bits())
                            } else {
                                u128::from(f.to_bits())
                            };
                            write_le(&mut bytes[offset..offset + width], v);
                        }
                        Const::Addr(base, addend) => {
                            if width != pointer_bytes
                                || (pointer_bytes == 4 && !matches!(expr.ty, Ty::Ptr(_)))
                            {
                                return Err(Error::new(expr.loc, "address in a narrow integer"));
                            }
                            let symbol = match base {
                                Base::Global(target) => {
                                    if record_references {
                                        self.use_global(target);
                                    }
                                    self.p.globals[target].symbol.to_owned()
                                }
                                Base::Str(s) => {
                                    if record_references {
                                        self.string_symbol(s)
                                    } else {
                                        "placement.validation.string".into()
                                    }
                                }
                            };
                            relocs.push((offset, symbol, addend));
                        }
                    }
                }
            }
        }
        relocs.sort_by_key(|r| r.0);
        let mut items = Vec::new();
        let mut pos = 0;
        for (offset, symbol, addend) in relocs {
            if !offset.is_multiple_of(pointer_bytes)
                || offset < pos
                || offset
                    .checked_add(pointer_bytes)
                    .is_none_or(|end| end > size)
            {
                return Err(Error::new(
                    Loc::default(),
                    "unaligned pointer relocations are not supported",
                ));
            }
            push_bytes(&mut items, &bytes[pos..offset]);
            let Ok(addend) = u32::try_from(addend) else {
                return Err(Error::new(
                    Loc::default(),
                    format!("`{}`: negative offset from `{symbol}`", global.name),
                ));
            };
            items.push(DataItem::Ptr { symbol, addend });
            pos = offset + pointer_bytes;
        }
        push_bytes(&mut items, &bytes[pos..]);
        if items.is_empty() {
            items.push(DataItem::Zero(1));
        }
        Ok(DataObject {
            name: global.symbol.to_owned(),
            thread_local: global.thread_local,
            local: global.internal,
            weak: global.weak.is_some(),
            weak_odr: false,
            hidden: global.hidden(),
            retain: global.retain.is_some(),
            section: global.section,
            init: Some(DataInit {
                constant: global.is_const && !global.thread_local,
                align,
                items,
            }),
        })
    }
}

fn write_le(bytes: &mut [u8], v: u128) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (v >> (8 * i)) as u8;
    }
}

/// Data items for `bytes`: runs of zeros as `zero n`, the rest as strings.
fn push_bytes(items: &mut Vec<DataItem>, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        let zeros = bytes[i..].iter().take_while(|&&b| b == 0).count();
        if zeros > 0 && (zeros >= 16 || i + zeros == bytes.len()) {
            items.push(DataItem::Zero(zeros as u32));
            i += zeros;
            continue;
        }
        let mut end = i;
        while end < bytes.len() {
            let run = bytes[end..].iter().take_while(|&&b| b == 0).count();
            if run >= 16 || end + run == bytes.len() && run > 0 {
                break;
            }
            end += run.max(1);
        }
        items.push(DataItem::Bytes(bytes[i..end].to_vec()));
        i = end;
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

/// How deep `fold_int` looks, bounding the work for any tree. Flag masks
/// spelled `A | B | C | ...` are long left-leaning chains.
const FOLD_DEPTH: u32 = 32;

/// An integer of at most 64 bits: the type whose values `fold_int` gives.
fn small_int(ty: &Ty, layout: CDataLayout) -> bool {
    ty.int_kind()
        .is_some_and(|k| k.size_with_layout(layout) <= 8)
}

/// The value of integer expression `e` if it is made of literals alone,
/// normalized to its type as the parser normalizes literals.
fn fold_int(e: &Expr<'_>, depth: u32, layout: CDataLayout) -> Option<i64> {
    if !small_int(&e.ty, layout) {
        return None;
    }
    let depth = depth.checked_sub(1)?;
    let v = match &e.kind {
        ExprKind::Int(v) => *v,
        ExprKind::Cast(a) if small_int(&a.ty, layout) => fold_int(a, depth, layout)?,
        ExprKind::Unary(op, a) if small_int(&a.ty, layout) => {
            let x = fold_int(a, depth, layout)?;
            match op {
                UnOp::Neg => x.wrapping_neg(),
                UnOp::BitNot => !x,
                UnOp::Not => i64::from(x == 0),
            }
        }
        ExprKind::Binary(op, a, b) if small_int(&a.ty, layout) && small_int(&b.ty, layout) => {
            let x = fold_int(a, depth, layout)?;
            let y = fold_int(b, depth, layout)?;
            let signed = a.ty.is_signed();
            let bits = a.ty.int_kind()?.size_with_layout(layout) as i64 * 8;
            let (ux, uy) = (x as u64, y as u64);
            let min = i64::MIN >> (64 - bits);
            match op {
                BinOp::Add => x.wrapping_add(y),
                BinOp::Sub => x.wrapping_sub(y),
                BinOp::Mul => x.wrapping_mul(y),
                // Division by zero and the signed overflow trap at run time.
                BinOp::Div | BinOp::Rem if y == 0 || (signed && x == min && y == -1) => {
                    return None;
                }
                BinOp::Div if signed => x / y,
                BinOp::Div => (ux / uy) as i64,
                BinOp::Rem if signed => x % y,
                BinOp::Rem => (ux % uy) as i64,
                // Out-of-range amounts are left to the target's semantics.
                BinOp::Shl | BinOp::Shr if !(0..bits).contains(&y) => return None,
                BinOp::Shl => x << y,
                BinOp::Shr if signed => x >> y,
                BinOp::Shr => (ux >> y) as i64,
                BinOp::And => x & y,
                BinOp::Or => x | y,
                BinOp::Xor => x ^ y,
                BinOp::Eq => i64::from(x == y),
                BinOp::Ne => i64::from(x != y),
                BinOp::Lt if signed => i64::from(x < y),
                BinOp::Le if signed => i64::from(x <= y),
                BinOp::Gt if signed => i64::from(x > y),
                BinOp::Ge if signed => i64::from(x >= y),
                BinOp::Lt => i64::from(ux < uy),
                BinOp::Le => i64::from(ux <= uy),
                BinOp::Gt => i64::from(ux > uy),
                BinOp::Ge => i64::from(ux >= uy),
            }
        }
        // `sizeof(size_t) < sizeof(long) ? A : B`: the other arm is not
        // evaluated, whatever it is.
        ExprKind::Cond(c, a, b) if small_int(&c.ty, layout) => {
            let arm = if fold_int(c, depth, layout)? != 0 {
                a
            } else {
                b
            };
            if arm.ty != e.ty {
                return None;
            }
            fold_int(arm, depth, layout)?
        }
        _ => return None,
    };
    Some(crate::eval::normalize_with_layout(v, &e.ty, layout))
}

/// Whether `x op k` is `x` for integer `x` of type `ty`.
fn right_identity(op: BinOp, k: i64, ty: &Ty, layout: CDataLayout) -> bool {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => k == 0,
        BinOp::Mul | BinOp::Div => k == 1,
        BinOp::And => k == crate::eval::normalize_with_layout(-1, ty, layout),
        _ => false,
    }
}

/// Whether `k op x` is `x`.
fn left_identity(op: BinOp, k: i64, ty: &Ty, layout: CDataLayout) -> bool {
    match op {
        BinOp::Add | BinOp::Or | BinOp::Xor => k == 0,
        BinOp::Mul => k == 1,
        BinOp::And => k == crate::eval::normalize_with_layout(-1, ty, layout),
        _ => false,
    }
}

/// A literal, or an operation on literals, whose evaluation does nothing
/// and cannot fail.
fn inert(e: &Expr<'_>, layout: CDataLayout) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Cast(a) => convertible(&a.ty, &e.ty) && inert(a, layout),
        _ => fold_int(e, FOLD_DEPTH, layout).is_some(),
    }
}

/// Whether `FnGen::convert` (or a cast to `void`) accepts this conversion.
fn convertible(from: &Ty, to: &Ty) -> bool {
    let wide = |t: &Ty| t.int_kind().is_some_and(|k| k.size() == 16);
    *to == Ty::Void
        || from.is_scalar()
            && to.is_scalar()
            && from.unusable().is_none()
            && to.unusable().is_none()
            && !((from.is_float() || from.is_complex()) && (to.is_ptr() || wide(to)))
            && !((to.is_float() || to.is_complex()) && (from.is_ptr() || wide(from)))
}

/// An expression that is non-zero exactly when `e` is, and evaluates the
/// same way: conversions to `_Bool`, between pointers and to an integer at
/// least as wide, and `__builtin_expect` with inert expectations.
fn same_truth<'e, 'a>(e: &'e Expr<'a>, layout: CDataLayout) -> Option<&'e Expr<'a>> {
    match &e.kind {
        ExprKind::Cast(a) => {
            let to_bool = e.ty == Ty::Int(IntKind::Bool)
                && (a.ty.is_integer() || a.ty.is_ptr() || matches!(a.ty, Ty::Float | Ty::Double));
            let keeps = to_bool
                || e.ty.is_ptr() && a.ty.is_ptr()
                || matches!((e.ty.int_kind(), a.ty.int_kind()),
                    (Some(to), Some(from)) if to.size_with_layout(layout) >= from.size_with_layout(layout));
            keeps.then_some(&**a)
        }
        ExprKind::Builtin(Builtin::Hint, args) if args[1..].iter().all(|e| inert(e, layout)) => {
            Some(&args[0])
        }
        _ => None,
    }
}

/// `e` with `same_truth` applied as far as it goes.
fn strip_truth<'e, 'a>(mut e: &'e Expr<'a>, layout: CDataLayout) -> &'e Expr<'a> {
    while let Some(inner) = same_truth(e, layout) {
        e = inner;
    }
    e
}

/// An integer comparison `a op b` under truth-preserving wrappers, so that
/// `(a op b) != 0` is `a op b`. Float comparisons are excluded: with NaN,
/// negating one is not the opposite comparison.
fn int_comparison<'e, 'a>(
    e: &'e Expr<'a>,
    layout: CDataLayout,
) -> Option<(BinOp, &'e Expr<'a>, &'e Expr<'a>)> {
    match &strip_truth(e, layout).kind {
        ExprKind::Binary(op, a, b) if op.is_comparison() && !a.ty.is_float() => Some((*op, a, b)),
        _ => None,
    }
}

/// Whether `e` contains no statement (so no label) and no compound literal,
/// looking at most `budget` nodes deep.
fn no_statements(e: &Expr<'_>, budget: u32) -> bool {
    let Some(budget) = budget.checked_sub(1) else {
        return false;
    };
    let all = |es: &[&Expr<'_>]| es.iter().all(|e| no_statements(e, budget));
    match &e.kind {
        ExprKind::Unary(_, a)
        | ExprKind::Cast(a)
        | ExprKind::Deref(a)
        | ExprKind::Addr(a)
        | ExprKind::VaStart(a)
        | ExprKind::VaArg(a)
        | ExprKind::IncDec { lhs: a, .. }
        | ExprKind::Member { base: a, .. } => all(&[a]),
        ExprKind::VectorIndex(a, b)
        | ExprKind::Binary(_, a, b)
        | ExprKind::PtrAdd(a, b, _)
        | ExprKind::PtrDiff(a, b, _)
        | ExprKind::LogAnd(a, b)
        | ExprKind::LogOr(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::Comma(a, b)
        | ExprKind::OpAssign { lhs: a, rhs: b, .. }
        | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => all(&[a, b]),
        ExprKind::Cond(a, b, c) => all(&[a, b, c]),
        ExprKind::Call(callee, args) => {
            no_statements(callee, budget) && args.iter().all(|a| no_statements(a, budget))
        }
        ExprKind::Builtin(_, args) => args.iter().all(|a| no_statements(a, budget)),
        ExprKind::CompoundLit(..) | ExprKind::StmtExpr { .. } => false,
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::LabelAddr(_)
        | ExprKind::Local(_)
        | ExprKind::Global(_) => true,
    }
}

/// The comparison true exactly when `op` is false.
fn negated(op: BinOp) -> BinOp {
    match op {
        BinOp::Eq => BinOp::Ne,
        BinOp::Ne => BinOp::Eq,
        BinOp::Lt => BinOp::Ge,
        BinOp::Ge => BinOp::Lt,
        BinOp::Le => BinOp::Gt,
        BinOp::Gt => BinOp::Le,
        _ => unreachable!("not a comparison"),
    }
}

/// `x != 0` or `x == 0` with integer `x` and a literal zero: whether it is
/// `!=`, and `x`.
fn zero_test<'e, 'a>(e: &'e Expr<'a>) -> Option<(bool, &'e Expr<'a>)> {
    match &e.kind {
        ExprKind::Binary(op, x, z) => Some((zero_test_op(*op, x, z)?, x)),
        _ => None,
    }
}

/// `zero_test` of `x op z`.
fn zero_test_op(op: BinOp, x: &Expr<'_>, z: &Expr<'_>) -> Option<bool> {
    (matches!(op, BinOp::Ne | BinOp::Eq) && x.ty.is_integer() && matches!(z.kind, ExprKind::Int(0)))
        .then_some(op == BinOp::Ne)
}

/// A bounded fusion of `goto *constant_table[index]` into the existing
/// shared switch. Numeric label values and ordinary table uses never change.
struct DispatchTable {
    global: usize,
    labels: Vec<usize>,
}

/// Deliberately accept only the parser's ordinary pointer-array subscript.
/// In particular, do not strip casts, commute pointer arithmetic, or infer
/// equivalence after modular multiplication by the element size.
fn dispatch_index<'e, 'a>(target: &'e Expr<'a>) -> Option<(usize, &'e Expr<'a>)> {
    let ExprKind::Deref(address) = &target.kind else {
        return None;
    };
    let ExprKind::PtrAdd(base, index, 8) = &address.kind else {
        return None;
    };
    let ExprKind::Addr(array) = &base.kind else {
        return None;
    };
    let ExprKind::Global(global) = array.kind else {
        return None;
    };
    if !matches!(target.ty, Ty::Ptr(_)) || !matches!(index.ty, Ty::Int(IntKind::Long)) {
        return None;
    }
    Some((global, index))
}

// Global::is_const describes emitted data, and currently loses the pointer
// qualifier for arrays. Prove source immutability from the retained type shape
// instead; never mistake const pointees for const pointer elements.
fn constant_elements(sugar: &Sugar, defs: &[TypedefDef<'_>]) -> bool {
    match sugar {
        Sugar::Const(_) => true,
        Sugar::Array(inner) | Sugar::Restrict(inner) | Sugar::Volatile(inner) => {
            constant_elements(inner, defs)
        }
        Sugar::Typedef(i) => constant_elements(&defs[*i].sugar, defs),
        _ => false,
    }
}

impl DispatchTable {
    fn for_function(p: &Parser<'_>, def: &FuncDef<'_>) -> Option<Self> {
        if def.label_addrs.is_empty() {
            return None;
        }
        let mut selected = None;
        for stmt in def.body {
            visit_stmt_exprs(stmt, &mut |target, use_| {
                if use_ != ExprUse::IndirectGoto {
                    return Some(());
                }
                let (global, _) = dispatch_index(target)?;
                if target.sugar.is_volatile(&p.typedefs) {
                    return None;
                }
                if selected.is_some_and(|previous| previous != global) {
                    return None;
                }
                selected = Some(global);
                Some(())
            })?;
        }
        let global = selected?;
        let object = &p.globals[global];
        if !object.internal
            || !object.defined
            || !constant_elements(&object.sugar, &p.typedefs)
            || object.thread_local
            || object.sugar.is_volatile(&p.typedefs)
        {
            return None;
        }
        let Ty::Array(&ArrayTy {
            elem: element,
            len: Some(len),
            ..
        }) = object.ty
        else {
            return None;
        };
        if !matches!(element, Ty::Ptr(_))
            || len == 0
            || len > 4096
            || len > 2 * def.label_addrs.len() as u64
        {
            return None;
        }
        let mut labels = vec![0; len as usize];
        for item in object.init.as_ref()?.items {
            let InitItem::Expr {
                offset,
                expr,
                bits: None,
            } = item
            else {
                return None;
            };
            if offset % 8 != 0 || offset / 8 >= len || !matches!(expr.ty, Ty::Ptr(_)) {
                return None;
            }
            let Const::Int(label) = eval(expr)? else {
                return None;
            };
            let label = usize::try_from(label).ok()?;
            if label > def.label_addrs.len() {
                return None;
            }
            // Match data(): zero-fill first, then apply flattened writes in
            // order, including repeated and designated initializers.
            labels[*offset as usize / 8] = label;
        }
        Some(Self { global, labels })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ExprUse {
    Value,
    Address,
    IndirectGoto,
}

/// Inspect evaluated expressions, including GNU statement expressions. Keep
/// address computations distinct from value reads and identify indirect gotos
/// for dispatch-table recognition. Returning None stops the traversal.
fn visit_stmt_exprs(
    stmt: &Stmt<'_>,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    match stmt {
        Stmt::Expr(e) | Stmt::Return(Some(e), _) => visit_exprs(e, f)?,
        Stmt::GotoIndirect(e) => {
            visit_exprs_as(e, ExprUse::IndirectGoto, f)?;
        }
        Stmt::Init(_, init, _) => visit_init_exprs(init, f)?,
        Stmt::Asm(asm) => {
            for (e, _) in asm.outputs {
                visit_exprs(e, f)?;
            }
            for e in asm.inputs {
                visit_exprs(e, f)?;
            }
        }
        Stmt::Block(stmts) | Stmt::Scoped(_, stmts) => {
            for stmt in stmts.iter() {
                visit_stmt_exprs(stmt, f)?;
            }
        }
        Stmt::If(e, yes, no) => {
            visit_exprs(e, f)?;
            visit_stmt_exprs(yes, f)?;
            if let Some(no) = no {
                visit_stmt_exprs(no, f)?;
            }
        }
        Stmt::While(e, body) | Stmt::DoWhile(body, e) => {
            visit_exprs(e, f)?;
            visit_stmt_exprs(body, f)?;
        }
        Stmt::Switch { cond, body, .. } => {
            visit_exprs(cond, f)?;
            visit_stmt_exprs(body, f)?;
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(init) = init {
                visit_stmt_exprs(init, f)?;
            }
            if let Some(cond) = cond {
                visit_exprs(cond, f)?;
            }
            if let Some(step) = step {
                visit_exprs(step, f)?;
            }
            visit_stmt_exprs(body, f)?;
        }
        Stmt::Declare(_)
        | Stmt::Case(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Return(None, _)
        | Stmt::Goto(_)
        | Stmt::Label(_) => {}
    }
    Some(())
}

fn visit_init_exprs(
    init: &Init<'_>,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    for item in init.items {
        if let InitItem::Expr { expr, .. } = item {
            visit_exprs(expr, f)?;
        }
    }
    Some(())
}

fn visit_exprs(e: &Expr<'_>, f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>) -> Option<()> {
    visit_exprs_as(e, ExprUse::Value, f)
}

fn visit_exprs_as(
    e: &Expr<'_>,
    use_: ExprUse,
    f: &mut impl FnMut(&Expr<'_>, ExprUse) -> Option<()>,
) -> Option<()> {
    f(e, use_)?;
    match &e.kind {
        ExprKind::Unary(_, a)
        | ExprKind::Cast(a)
        | ExprKind::Deref(a)
        | ExprKind::VaStart(a)
        | ExprKind::VaArg(a)
        | ExprKind::IncDec { lhs: a, .. } => visit_exprs(a, f)?,
        ExprKind::Addr(a) | ExprKind::Member { base: a, .. } => {
            visit_exprs_as(a, ExprUse::Address, f)?;
        }
        ExprKind::VectorIndex(a, b)
        | ExprKind::Binary(_, a, b)
        | ExprKind::PtrAdd(a, b, _)
        | ExprKind::PtrDiff(a, b, _)
        | ExprKind::LogAnd(a, b)
        | ExprKind::LogOr(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::Comma(a, b)
        | ExprKind::OpAssign { lhs: a, rhs: b, .. }
        | ExprKind::PtrOpAssign { lhs: a, rhs: b, .. } => {
            visit_exprs(a, f)?;
            visit_exprs(b, f)?;
        }
        ExprKind::Cond(a, b, c) => {
            visit_exprs(a, f)?;
            visit_exprs(b, f)?;
            visit_exprs(c, f)?;
        }
        ExprKind::Call(callee, args) => {
            visit_exprs(callee, f)?;
            for arg in args.iter() {
                visit_exprs(arg, f)?;
            }
        }
        ExprKind::Builtin(_, args) => {
            for arg in args.iter() {
                visit_exprs(arg, f)?;
            }
        }
        ExprKind::CompoundLit(_, init) => visit_init_exprs(init, f)?,
        ExprKind::StmtExpr { stmts, value, .. } => {
            for stmt in stmts.iter() {
                visit_stmt_exprs(stmt, f)?;
            }
            if let Some(value) = value {
                visit_exprs(value, f)?;
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::LabelAddr(_)
        | ExprKind::Local(_)
        | ExprKind::Global(_) => {}
    }
    Some(())
}

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
    breaks: Vec<(Block, usize)>,
    continues: Vec<(Block, usize)>,
    /// Registered as declarations execute; emit in reverse order on each exit.
    /// Branch exits do not consume the list used to compile sibling paths.
    exit_actions: Vec<ExitAction>,
    has_cleanup: bool,
    labels: HashMap<&'p str, Block>,
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
                    // The normal ABI uses a hidden result address for pairs
                    // and structs, even though cleanup discards that result.
                    if fty.ret.is_pair_or_struct() {
                        let size = self.size(&fty.ret);
                        let align = self.g.types().align(&fty.ret);
                        let slot = self.b.create_stack_slot(size.max(1) as u32, align as u32);
                        args.push(self.b.ins().stack_addr(slot));
                    }
                    args.push(addr);
                    self.g.use_global(g);
                    let callee = self.import_global(g, fty, None)?;
                    self.b.ins().call(callee, &args);
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

    fn stmt(&mut self, s: &Stmt<'p>) -> R<()> {
        // A loop's condition and step run again too, statement
        // expressions in them included.
        let looping = matches!(s, Stmt::While(..) | Stmt::DoWhile(..) | Stmt::For { .. });
        self.loop_depth += u32::from(looping);
        let result = self.stmt_kind(s);
        self.loop_depth -= u32::from(looping);
        result
    }

    fn stmt_kind(&mut self, s: &Stmt<'p>) -> R<()> {
        match s {
            Stmt::Asm(asm) => {
                self.ensure_block();
                self.set_loc(asm.loc);
                let mut places = Vec::new();
                let mut readwrite = Vec::new();
                let mut memory_args = Vec::new();
                for ((e, rw), &memory) in asm.outputs.iter().zip(asm.output_memory) {
                    if memory {
                        memory_args.push(self.val(e)?);
                        continue;
                    }
                    // Capture once, before inputs: postincrement and calls in
                    // an output address must not run again on writeback.
                    let place = self.place(e)?.expect("checked asm output");
                    if *rw {
                        readwrite.push(self.load_place(&place));
                    }
                    places.push(place);
                }
                let mut args = memory_args;
                for e in asm.inputs {
                    args.push(self.val(e)?);
                }
                args.extend(readwrite);
                let types: Vec<_> = asm
                    .outputs
                    .iter()
                    .zip(asm.output_memory)
                    .filter(|(_, memory)| !**memory)
                    .map(|((e, _), _)| e.ty.ir_with_layout(self.g.p.c_layout))
                    .collect();
                let contract = keel_ir::InlineAsm::new(asm.template, asm.constraints, &types)
                    .map_err(|e| Error::new(asm.loc, e))?;
                let inst = self.b.ins().inline_asm(contract, &args);
                let values = self.b.function().inst_results(inst).to_vec();
                for (place, value) in places.iter().zip(values) {
                    self.store_place(place, value);
                }
            }
            Stmt::Expr(e) => {
                self.ensure_block();
                self.set_loc(e.loc);
                self.discard(e)?;
            }
            Stmt::Init(id, init, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                self.init_local(*id, init)?;
                self.register_cleanup(*id);
            }
            // Without this, a variable read on a path that did not assign it
            // in this iteration would read the last iteration's value: a
            // block parameter live around the whole loop. Zero starts a new
            // value here instead. A declaration that runs at most once, in
            // no loop and with no label before it, needs no zero: a read
            // that no assignment reaches already reads as zero.
            Stmt::Declare(id) => {
                let ty = self.def.locals[*id].ty;
                if ty.runtime_sized() {
                    self.ensure_block();
                    let bytes = self.runtime_size(&ty)?;
                    let save = self.b.ins().intrinsic(Intrinsic::StackSave, &[]);
                    let token = self.b.function().inst_results(save)[0];
                    self.exit_actions.push(ExitAction::Restore(token));
                    let inst = self.b.ins().intrinsic(Intrinsic::StackAlloc, &[bytes]);
                    let addr = self.b.function().inst_results(inst)[0];
                    let Storage::Var(var) = self.locals[*id] else {
                        unreachable!()
                    };
                    self.b.def_var(var, addr);
                    self.alignments
                        .insert(addr, self.def.locals[*id].align as u32);
                    self.register_cleanup(*id);
                    return Ok(());
                }
                if let Storage::Var(var) = self.locals[*id]
                    && (self.loop_depth > 0 || self.labels_placed)
                {
                    self.ensure_block();
                    let zero = self.zero(self.def.locals[*id].ty.ir_with_layout(self.g.p.c_layout));
                    self.b.def_var(var, zero);
                }
                self.register_cleanup(*id);
            }
            Stmt::Block(stmts) => {
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
            }
            Stmt::Scoped(scope, stmts) => {
                let depth = self.exit_actions.len();
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
                self.set_scope(outer);
                self.leave_scope(depth)?;
            }
            Stmt::If(cond, then, els) => {
                self.ensure_block();
                self.set_loc(cond.loc);
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch(cond, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                self.switch_to(t);
                self.stmt(then)?;
                self.jump_to(end);
                self.switch_to(f);
                match els {
                    Some(els) => self.stmt(els)?,
                    // The jump past the `then` branch is the condition's,
                    // not the last line of that branch's.
                    None => self.set_loc(cond.loc),
                }
                self.jump_to(end);
                self.b.seal_block(end);
                self.switch_to(end);
            }
            Stmt::While(cond, body) => {
                self.ensure_block();
                let (header, body_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.jump_to(header);
                self.switch_to(header);
                self.set_loc(cond.loc);
                self.branch(cond, body_block, exit)?;
                self.b.seal_block(body_block);
                self.loop_body(body, body_block, exit, header)?;
                self.jump_to(header);
                self.b.seal_block(header);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::DoWhile(body, cond) => {
                self.ensure_block();
                let (body_block, cond_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.jump_to(body_block);
                self.loop_body(body, body_block, exit, cond_block)?;
                self.jump_to(cond_block);
                self.b.seal_block(cond_block);
                self.switch_to(cond_block);
                self.set_loc(cond.loc);
                self.branch(cond, body_block, exit)?;
                self.b.seal_block(body_block);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(init) = init {
                    self.stmt(init)?;
                }
                self.ensure_block();
                let (header, body_block, step_block, exit) = (
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                );
                self.jump_to(header);
                self.switch_to(header);
                match cond {
                    Some(cond) => {
                        self.set_loc(cond.loc);
                        self.branch(cond, body_block, exit)?;
                    }
                    None => self.jump_to(body_block),
                }
                self.b.seal_block(body_block);
                self.loop_body(body, body_block, exit, step_block)?;
                self.jump_to(step_block);
                self.b.seal_block(step_block);
                self.switch_to(step_block);
                if let Some(step) = step {
                    self.set_loc(step.loc);
                    self.emit(step)?;
                }
                self.jump_to(header);
                self.b.seal_block(header);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::Switch {
                cond,
                body,
                cases,
                default,
                labels,
            } => {
                if self.has_cleanup() {
                    return Err(Error::new(
                        cond.loc,
                        "switch in a function containing cleanup locals is not supported yet",
                    ));
                }
                if self.has_vm_types() {
                    return Err(Error::new(
                        cond.loc,
                        "switch in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                self.set_loc(cond.loc);
                let value = self.val(cond)?;
                let blocks: Vec<Block> = (0..*labels).map(|_| self.new_block()).collect();
                let exit = self.new_block();
                let default_block = default.map_or(exit, |d| blocks[d]);
                let table = cases
                    .iter()
                    .map(|&(v, label)| {
                        let dest = keel_ir::BlockCall {
                            block: blocks[label],
                            args: Vec::new(),
                        };
                        (v, dest)
                    })
                    .collect();
                let default_call = keel_ir::BlockCall {
                    block: default_block,
                    args: Vec::new(),
                };
                self.b.ins().switch(value, default_call, table);
                self.terminated = true;
                self.cases.push(blocks.clone());
                self.breaks.push((exit, self.exit_actions.len()));
                self.stmt(body)?;
                self.jump_to(exit);
                self.breaks.pop();
                self.cases.pop();
                for block in blocks {
                    self.b.seal_block(block);
                }
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            // A case block's branches are the switch's, built already, and
            // the fall-through into it: sealing it now resolves its reads
            // directly instead of through parameters filled in later.
            Stmt::Case(label) => {
                let block = self.cases.last().unwrap()[*label];
                self.jump_to(block);
                self.b.seal_block(block);
                self.switch_to(block);
            }
            Stmt::Break => {
                self.ensure_block();
                let (target, depth) = *self.breaks.last().unwrap();
                self.emit_exit_actions(depth)?;
                self.jump_to(target);
            }
            Stmt::Continue => {
                self.ensure_block();
                let (target, depth) = *self.continues.last().unwrap();
                self.emit_exit_actions(depth)?;
                self.jump_to(target);
            }
            Stmt::Return(value, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                match value {
                    None => self.return_default()?,
                    Some(e) => {
                        if let Some(sret) = self.sret {
                            let src = self.val(e)?;
                            let ty = e.ty;
                            self.copy(sret, src, &ty);
                            self.emit_exit_actions(0)?;
                            self.b.ins().return_(&[]);
                        } else {
                            let v = self.val(e)?;
                            self.emit_exit_actions(0)?;
                            self.b.ins().return_(&[v]);
                        }
                        self.terminated = true;
                    }
                }
            }
            Stmt::Goto(label) => {
                if self.has_cleanup() {
                    return Err(Error::new(
                        self.def.loc,
                        "goto in a function containing cleanup locals is not supported yet",
                    ));
                }
                if self.has_vm_types() {
                    return Err(Error::new(
                        self.def.loc,
                        "goto in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                let block = self.label_block(label);
                self.jump_to(block);
            }
            Stmt::GotoIndirect(target) => {
                if self.has_cleanup() {
                    return Err(Error::new(
                        target.loc,
                        "computed goto in a function containing cleanup locals is not supported yet",
                    ));
                }
                if self.has_vm_types() {
                    return Err(Error::new(
                        target.loc,
                        "computed goto in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                self.set_loc(target.loc);
                let n = if let Some(table) = &self.dispatch_table {
                    let (global, index) = dispatch_index(target).expect("preflighted dispatch");
                    debug_assert_eq!(global, table.global);
                    // Keep the numeric table available, including in debug info.
                    self.g.use_global(global);
                    // PtrAdd's index is already a long. Its evaluation (including
                    // assignments and pc++) stays at the original goto location.
                    self.val(index)?
                } else {
                    let p = self.val(target)?;
                    self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, p)
                };
                let dispatch = match self.dispatch {
                    Some(block) => block,
                    None => {
                        let block = self.new_block();
                        self.b.append_block_param(block, Type::I64);
                        self.dispatch = Some(block);
                        block
                    }
                };
                self.b.ins().jump(dispatch, &[n]);
                self.terminated = true;
            }
            Stmt::Label(label) => {
                self.labels_placed = true;
                let block = self.label_block(label);
                self.jump_to(block);
                self.switch_to(block);
            }
        }
        Ok(())
    }

    fn emit_dispatch(&mut self) {
        let Some(dispatch) = self.dispatch else {
            return;
        };
        self.switch_to(dispatch);
        self.b.set_srcloc(None);
        let n = self.b.function().block_params(dispatch)[0];
        let call = |block| keel_ir::BlockCall {
            block,
            args: Vec::new(),
        };
        // Keep one shared merge and one switch, whether its key is a numeric
        // label token or an index into a proven immutable table.
        let labels: Vec<(i64, &'p str)> = match &self.dispatch_table {
            Some(table) => table
                .labels
                .iter()
                .enumerate()
                .filter(|(_, label)| **label != 0)
                .map(|(i, &label)| (i as i64, self.def.label_addrs[label - 1]))
                .collect(),
            None => self
                .def
                .label_addrs
                .iter()
                .enumerate()
                .map(|(i, &label)| (i as i64 + 1, label))
                .collect(),
        };
        // Zero entries and invalid tokens still trap. An out-of-bounds table
        // access has no defined C behavior; the fused default also traps.
        let nowhere = self.new_block();
        let table = labels
            .into_iter()
            .map(|(key, label)| (key, call(self.label_block(label))))
            .collect();
        self.b.ins().switch(n, call(nowhere), table);
        self.b.seal_block(nowhere);
        self.switch_to(nowhere);
        self.b.ins().trap();
        self.terminated = true;
    }

    fn loop_body(&mut self, body: &Stmt<'p>, block: Block, exit: Block, cont: Block) -> R<()> {
        self.switch_to(block);
        self.breaks.push((exit, self.exit_actions.len()));
        self.continues.push((cont, self.exit_actions.len()));
        self.stmt(body)?;
        self.breaks.pop();
        self.continues.pop();
        Ok(())
    }

    /// Branches to `t` if `cond` is true (non-zero), else to `f`, without
    /// materializing `&&`, `||` and `!` as values.
    fn branch(&mut self, cond: &Expr<'p>, t: Block, f: Block) -> R<()> {
        match &cond.kind {
            ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
                let mid = self.new_block();
                if matches!(cond.kind, ExprKind::LogAnd(..)) {
                    self.branch(a, mid, f)?;
                } else {
                    self.branch(a, t, mid)?;
                }
                self.b.seal_block(mid);
                self.switch_to(mid);
                self.branch(b, t, f)
            }
            ExprKind::Unary(UnOp::Not, a) => self.branch(a, f, t),
            _ if let Some(inner) = same_truth(cond, self.g.p.c_layout) => self.branch(inner, t, f),
            _ if let Some((ne, x)) = zero_test(cond) => {
                if ne {
                    self.branch(x, t, f)
                } else {
                    self.branch(x, f, t)
                }
            }
            // A literal condition (`if (sizeof(long) == 8)`): one constant.
            // Both edges stay: code built into the other block would
            // otherwise be an unreached predecessor of the join, passing
            // it values of its own.
            _ if let Some(v) = fold_int(cond, FOLD_DEPTH, self.g.p.c_layout) => {
                let c = self.b.ins().iconst(Type::I8, i64::from(v != 0));
                self.b.ins().brif(c, t, &[], f, &[]);
                self.terminated = true;
                Ok(())
            }
            _ => {
                let c = self.truth(cond)?;
                self.b.ins().brif(c, t, &[], f, &[]);
                self.terminated = true;
                Ok(())
            }
        }
    }

    /// A value that is non-zero exactly when `e` is true. Comparisons give
    /// their `i8` directly; integers are their own truth value.
    fn truth(&mut self, e: &Expr<'p>) -> R<Value> {
        if let ExprKind::Binary(op, a, b) = &e.kind
            && op.is_comparison()
        {
            return self.compare(*op, a, b);
        }
        let v = self.val(e)?;
        Ok(match &e.ty {
            Ty::Int(k) if k.size_with_layout(self.g.p.c_layout) <= 8 => v,
            ty => self.nonzero(v, ty),
        })
    }

    /// `v != 0` as an `i8`.
    fn nonzero(&mut self, v: Value, ty: &Ty) -> Value {
        if let Ty::Complex(elem) = ty {
            let (r, i) = self.complex_parts(v, elem);
            let r = self.nonzero(r, elem);
            let i = self.nonzero(i, elem);
            return self.b.ins().or(r, i);
        }
        let z = self.zero(ty.ir_with_layout(self.g.p.c_layout));
        if ty.is_float() {
            self.b.ins().fcmp(FloatCC::Ne, v, z)
        } else {
            self.b.ins().icmp(IntCC::Ne, v, z)
        }
    }

    // ---- expressions ----

    /// The value of a non-void expression; for aggregates, their address.
    fn val(&mut self, e: &Expr<'p>) -> R<Value> {
        match self.emit(e)? {
            Some(v) if e.ty.is_complex() => {
                // Complex is a C scalar: capture its value now, even when a
                // later operand overwrites the source object.
                let slot = self
                    .b
                    .create_stack_slot(self.size(&e.ty) as u32, self.g.types().align(&e.ty) as u32);
                let addr = self.b.ins().stack_addr(slot);
                self.copy(addr, v, &e.ty);
                Ok(addr)
            }
            Some(v) => Ok(v),
            None => Err(Error::new(e.loc, "a void expression has no value")),
        }
    }

    fn is_aggregate(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Struct(_) | Ty::Complex(_) | Ty::Array(..) | Ty::Func(_)
        )
    }

    /// Evaluates `e` for its effects only. Literals, and reads of local
    /// variables that are not volatile, have none.
    fn discard(&mut self, e: &Expr<'p>) -> R<()> {
        let mut inner = e;
        while let ExprKind::Cast(a) = &inner.kind
            && convertible(&a.ty, &inner.ty)
        {
            inner = a;
        }
        match &inner.kind {
            ExprKind::Comma(a, b) => {
                self.discard(a)?;
                self.discard(b)
            }
            ExprKind::Local(_) if !self.object_flags(&inner.ty, &inner.sugar).volatile => Ok(()),
            _ if inert(inner, self.g.p.c_layout) => Ok(()),
            _ => self.emit(e).map(drop),
        }
    }

    fn emit(&mut self, e: &Expr<'p>) -> R<Option<Value>> {
        // Operations on literals that macros leave outside constant
        // expressions: one constant instead of the operands and the
        // operation.
        if matches!(
            e.kind,
            ExprKind::Unary(..) | ExprKind::Binary(..) | ExprKind::Cast(_)
        ) && let Some(v) = fold_int(e, FOLD_DEPTH, self.g.p.c_layout)
        {
            return Ok(Some(
                self.b
                    .ins()
                    .iconst(e.ty.ir_with_layout(self.g.p.c_layout), v),
            ));
        }
        let v = match &e.kind {
            ExprKind::LabelAddr(n) => {
                let n = self.b.ins().iconst(Type::I64, *n as i64);
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            ExprKind::Int(v) => match &e.ty {
                Ty::Ptr(_) => {
                    let bits = crate::eval::normalize_with_layout(*v, &e.ty, self.g.p.c_layout);
                    let n = self.b.ins().iconst(Type::I64, bits);
                    self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
                }
                ty => self
                    .b
                    .ins()
                    .iconst(ty.ir_with_layout(self.g.p.c_layout), *v),
            },
            ExprKind::Float(f) => match e.ty {
                Ty::Float => self.b.ins().f32const(*f as f32),
                _ => self.b.ins().f64const(*f),
            },
            ExprKind::Str(_)
            | ExprKind::Global(_)
            | ExprKind::Deref(_)
            | ExprKind::Member { .. }
            | ExprKind::CompoundLit(..) => match self.place(e)? {
                Some(place) => self.load_place(&place),
                None => return Ok(None),
            },
            ExprKind::VectorIndex(base, index) => {
                let vector = self.val(base)?;
                if let Some(lane) = fold_int(index, FOLD_DEPTH, self.g.p.c_layout) {
                    self.b.ins().vextract(vector, lane as u8)
                } else {
                    let slot = self.b.create_stack_slot(16, 16);
                    let address = self.b.ins().stack_addr(slot);
                    self.b.ins().store(vector, address);
                    let offset = self.scaled(index, self.size(&e.ty))?;
                    let address = self.b.ins().ptradd(address, offset);
                    self.b
                        .ins()
                        .load(e.ty.ir_with_layout(self.g.p.c_layout), address)
                }
            }
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Var(var) => self.b.use_var(var),
                _ => {
                    let place = self.place(e)?.unwrap();
                    self.load_place(&place)
                }
            },
            // `!(a < b)` is `a >= b` for integers and pointers.
            ExprKind::Unary(UnOp::Not, a)
                if let Some((op, x, y)) = int_comparison(a, self.g.p.c_layout) =>
            {
                let c = self.compare(negated(op), x, y)?;
                self.b.ins().uext(Type::I32, c)
            }
            ExprKind::Unary(op, a) if a.ty.is_complex() => {
                let v = self.val(a)?;
                let Ty::Complex(elem) = a.ty else {
                    unreachable!()
                };
                match op {
                    UnOp::Neg => {
                        let (r, i) = self.complex_parts(v, elem);
                        let r = self.b.ins().fneg(r);
                        let i = self.b.ins().fneg(i);
                        self.complex_pair(r, i, elem)
                    }
                    UnOp::Not => {
                        let c = self.nonzero(v, &a.ty);
                        let one = self.b.ins().iconst(Type::I8, 1);
                        let c = self.b.ins().xor(c, one);
                        self.b.ins().uext(Type::I32, c)
                    }
                    _ => return Err(Error::new(e.loc, "unsupported complex unary operation")),
                }
            }
            ExprKind::Unary(op, a) if a.ty.is_vector() => {
                let v = self.val(a)?;
                let Ty::Vector(elem, _) = a.ty else {
                    unreachable!()
                };
                let op = match op {
                    UnOp::Neg if elem.is_float() => keel_ir::UnaryOp::Fneg,
                    UnOp::Neg => keel_ir::UnaryOp::Ineg,
                    UnOp::BitNot => keel_ir::UnaryOp::Bnot,
                    _ => return Err(Error::new(e.loc, "unsupported vector unary operation")),
                };
                self.b.ins().vector_unary(op, v)
            }
            ExprKind::Unary(op, a) => {
                let v = self.val(a)?;
                match op {
                    UnOp::Neg if a.ty.is_float() => self.b.ins().fneg(v),
                    UnOp::Neg => self.b.ins().ineg(v),
                    UnOp::BitNot => self.b.ins().bnot(v),
                    UnOp::Not => {
                        let z = self.zero(a.ty.ir_with_layout(self.g.p.c_layout));
                        let c = if a.ty.is_float() {
                            self.b.ins().fcmp(FloatCC::Eq, v, z)
                        } else {
                            self.b.ins().icmp(IntCC::Eq, v, z)
                        };
                        self.b.ins().uext(Type::I32, c)
                    }
                }
            }
            ExprKind::Binary(op, a, b) if op.is_comparison() && a.ty.is_vector() => {
                self.compare(*op, a, b)?
            }
            ExprKind::Binary(op, a, b) if op.is_comparison() => {
                let c = self.compare(*op, a, b)?;
                self.b.ins().uext(Type::I32, c)
            }
            // `n * sizeof(char)`, `x << 0`, `0 | x` from macros: no
            // operation, and no evaluation skipped but the literal's.
            ExprKind::Binary(op, a, b)
                if small_int(&e.ty, self.g.p.c_layout)
                    && fold_int(b, FOLD_DEPTH, self.g.p.c_layout)
                        .is_some_and(|k| right_identity(*op, k, &e.ty, self.g.p.c_layout)) =>
            {
                self.val(a)?
            }
            ExprKind::Binary(op, a, b)
                if small_int(&e.ty, self.g.p.c_layout)
                    && fold_int(a, FOLD_DEPTH, self.g.p.c_layout)
                        .is_some_and(|k| left_identity(*op, k, &e.ty, self.g.p.c_layout)) =>
            {
                self.val(b)?
            }
            ExprKind::Binary(op, a, b) if e.ty.is_complex() => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                self.complex_arith(*op, &a.ty, &b.ty, x, y, e.loc)?
            }
            ExprKind::Binary(op, a, b) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                self.arith(*op, &e.ty, x, y, e.loc)?
            }
            ExprKind::PtrAdd(p, n, size) => {
                if fold_int(n, FOLD_DEPTH, self.g.p.c_layout).is_some() {
                    let (base, off) = self.ptr_parts(e)?;
                    self.offset_by(base, off)
                } else {
                    let (base, off) = self.ptr_parts(p)?;
                    let base = self.offset_by(base, off);
                    let offset = self.scaled(n, *size)?;
                    self.b.ins().ptradd(base, offset)
                }
            }
            ExprKind::PtrDiff(a, b, size) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                let x = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, x);
                let y = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, y);
                let d = self.b.ins().isub(x, y);
                // The difference is an exact multiple of the size.
                let difference = if *size <= 1 {
                    d
                } else if size.is_power_of_two() {
                    let s = self.b.ins().iconst(Type::I64, size.trailing_zeros() as i64);
                    self.b.ins().sshr(d, s)
                } else {
                    let s = self.b.ins().iconst(Type::I64, *size as i64);
                    self.b.ins().sdiv(d, s)
                };
                if self.g.p.c_layout.pointer_bytes() == 4 {
                    self.b.ins().trunc(Type::I32, difference)
                } else {
                    difference
                }
            }
            ExprKind::LogAnd(..) | ExprKind::LogOr(..) => {
                let var = self.b.declare_var(Type::I32);
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch(e, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                for (block, value) in [(t, 1), (f, 0)] {
                    self.switch_to(block);
                    let v = self.b.ins().iconst(Type::I32, value);
                    self.b.def_var(var, v);
                    self.jump_to(end);
                }
                self.b.seal_block(end);
                self.switch_to(end);
                self.b.use_var(var)
            }
            ExprKind::Assign(lhs, rhs) => {
                let place = self.place(lhs)?.unwrap();
                let v = self.val(rhs)?;
                // Normalize bit-fields without rereading volatile storage.
                self.store_place(&place, v)
            }
            ExprKind::OpAssign { op, lhs, rhs } if lhs.ty.is_complex() || rhs.ty.is_complex() => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let calc = crate::expr::usual_with_layout(&lhs.ty, &rhs.ty, self.g.p.c_layout);
                let lt = match calc {
                    Ty::Complex(elem) if !lhs.ty.is_complex() => *elem,
                    _ => calc,
                };
                let old = self.convert(old, &lhs.ty, &lt, e.loc)?;
                // Freeze the old pair before evaluating a side-effecting rhs.
                let old = if let Ty::Complex(elem) = lt {
                    let (r, i) = self.complex_parts(old, elem);
                    self.complex_pair(r, i, elem)
                } else {
                    old
                };
                let r = self.val(rhs)?;
                let new = self.complex_arith(*op, &lt, &rhs.ty, old, r, e.loc)?;
                let new = self.convert(new, &calc, &lhs.ty, e.loc)?;
                self.store_place(&place, new)
            }
            ExprKind::OpAssign { op, lhs, rhs } => {
                let ty = &rhs.ty;
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let old = self.convert(old, &lhs.ty, ty, e.loc)?;
                let r = self.val(rhs)?;
                let new = self.arith(*op, ty, old, r, e.loc)?;
                let new = self.convert(new, ty, &lhs.ty, e.loc)?;
                self.store_place(&place, new)
            }
            ExprKind::PtrOpAssign { lhs, rhs, size } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let offset = self.scaled(rhs, *size)?;
                let new = self.b.ins().ptradd(old, offset);
                self.store_place(&place, new)
            }
            ExprKind::IncDec { lhs, inc, prefix } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let new = match &lhs.ty {
                    Ty::Ptr(_) => {
                        let elem = lhs.ty.pointee().unwrap();
                        let size = if elem.runtime_sized() {
                            self.runtime_size(elem)?
                        } else {
                            let size = self.size(elem).max(1);
                            self.b.ins().iconst(Type::I64, size as i64)
                        };
                        let d = if *inc { size } else { self.b.ins().ineg(size) };
                        self.b.ins().ptradd(old, d)
                    }
                    Ty::Float | Ty::Double => {
                        let one = if lhs.ty == Ty::Float {
                            self.b.ins().f32const(1.0)
                        } else {
                            self.b.ins().f64const(1.0)
                        };
                        if *inc {
                            self.b.ins().fadd(old, one)
                        } else {
                            self.b.ins().fsub(old, one)
                        }
                    }
                    Ty::Int(IntKind::Bool) => {
                        if *inc {
                            self.b.ins().iconst(Type::I8, 1)
                        } else {
                            let one = self.b.ins().iconst(Type::I8, 1);
                            self.b.ins().xor(old, one)
                        }
                    }
                    ty => {
                        let one = self.b.ins().iconst(ty.ir_with_layout(self.g.p.c_layout), 1);
                        if *inc {
                            self.b.ins().iadd(old, one)
                        } else {
                            self.b.ins().isub(old, one)
                        }
                    }
                };
                let stored = self.store_place(&place, new);
                if *prefix { stored } else { old }
            }
            // `sizeof(x) == sizeof(float) ? f(x) : g(x)`, as <math.h>
            // macros dispatch on the type: only the selected arm. The other
            // is never evaluated, and has no statement a jump could reach.
            ExprKind::Cond(c, a, b)
                if let Some(v) = fold_int(c, FOLD_DEPTH, self.g.p.c_layout)
                    && no_statements(if v != 0 { b } else { a }, 64) =>
            {
                let arm = if v != 0 { a } else { b };
                if e.ty == Ty::Void {
                    self.emit(arm)?;
                    return Ok(None);
                }
                self.val(arm)?
            }
            ExprKind::Cond(c, a, b) => {
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch(c, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                if e.ty == Ty::Void {
                    for (block, arm) in [(t, a), (f, b)] {
                        self.switch_to(block);
                        self.emit(arm)?;
                        self.jump_to(end);
                    }
                    self.b.seal_block(end);
                    self.switch_to(end);
                    return Ok(None);
                }
                let ir = if Self::is_aggregate(&e.ty) {
                    Type::Ptr
                } else {
                    e.ty.ir_with_layout(self.g.p.c_layout)
                };
                let var = self.b.declare_var(ir);
                for (block, arm) in [(t, a), (f, b)] {
                    self.switch_to(block);
                    let v = self.val(arm)?;
                    self.b.def_var(var, v);
                    self.jump_to(end);
                }
                self.b.seal_block(end);
                self.switch_to(end);
                self.b.use_var(var)
            }
            ExprKind::Comma(a, b) => {
                self.discard(a)?;
                return self.emit(b);
            }
            ExprKind::StmtExpr {
                scope,
                stmts,
                value,
            } => {
                let depth = self.exit_actions.len();
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
                let v = match value {
                    Some(v) => {
                        self.ensure_block();
                        let value = self.emit(v)?;
                        // Aggregate values are represented by addresses. Freeze
                        // the bytes before callbacks change them or VLA restores
                        // release their storage.
                        if self.exit_actions.len() > depth
                            && Self::is_aggregate(&v.ty)
                            && let Some(value) = value
                        {
                            let layout =
                                self.aggregate_place(value, &v.ty, MemoryFlags::default(), v.loc)?;
                            let Place::Aggregate { size, align, .. } = layout else {
                                unreachable!()
                            };
                            let slot = self.b.create_stack_slot(size.max(1), align.min(16));
                            let copy = self.b.ins().stack_addr(slot);
                            self.copy(copy, value, &v.ty);
                            Some(copy)
                        } else {
                            value
                        }
                    }
                    None => None,
                };
                self.set_scope(outer);
                self.leave_scope(depth)?;
                return Ok(v);
            }
            ExprKind::Call(f, args) => return self.call(f, args, e),
            ExprKind::Cast(inner) => {
                if e.ty == Ty::Void {
                    self.discard(inner)?;
                    return Ok(None);
                }
                // Null pointers and other integer constants made pointers:
                // normalize to the compilation model before the i64 IR conversion.
                if e.ty.is_ptr()
                    && let Some(n) = fold_int(inner, FOLD_DEPTH, self.g.p.c_layout)
                {
                    let bits = crate::eval::normalize_with_layout(n, &e.ty, self.g.p.c_layout);
                    let n = self.b.ins().iconst(Type::I64, bits);
                    let p = self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n);
                    return Ok(Some(p));
                }
                // `x > 0` with a floating `x`: the converted constant,
                // rounded once to nearest as the conversion rounds.
                if matches!(e.ty, Ty::Float | Ty::Double)
                    && let Some(n) = fold_int(inner, FOLD_DEPTH, self.g.p.c_layout)
                {
                    let signed = inner.ty.is_signed();
                    return Ok(Some(match (&e.ty, signed) {
                        (Ty::Float, true) => self.b.ins().f32const(n as f32),
                        (Ty::Float, false) => self.b.ins().f32const(n as u64 as f32),
                        (_, true) => self.b.ins().f64const(n as f64),
                        (_, false) => self.b.ins().f64const(n as u64 as f64),
                    }));
                }
                let v = self.emit(inner)?;
                let v = v.ok_or_else(|| Error::new(e.loc, "a void expression has no value"))?;
                self.convert(v, &inner.ty, &e.ty, e.loc)?
            }
            ExprKind::Addr(inner) => self.addr(inner)?,
            ExprKind::VaStart(ap) => {
                let addr = self.val(ap)?;
                self.b.ins().intrinsic(Intrinsic::VaStart, &[addr]);
                return Ok(None);
            }
            ExprKind::VaArg(ap) => {
                let addr = self.val(ap)?;
                match &e.ty {
                    Ty::Vector(..) => {
                        return Err(Error::new(e.loc, "va_arg of a vector is not supported"));
                    }
                    Ty::Int(k) if k.size_with_layout(self.g.p.c_layout) < 4 => {
                        let v = self.b.ins().va_arg(Type::I32, addr);
                        self.b
                            .ins()
                            .trunc(e.ty.ir_with_layout(self.g.p.c_layout), v)
                    }
                    Ty::Int(k)
                        if k.size_with_layout(self.g.p.c_layout) == 16
                            && self.g.p.target != keel_ir::Target::Aarch64LinuxGnu =>
                    {
                        return Err(Error::new(e.loc, "`va_arg` of `__int128`"));
                    }
                    Ty::Float => {
                        let v = self.b.ins().va_arg(Type::F64, addr);
                        self.b.ins().convert(ConvertOp::Fptrunc, Type::F32, v)
                    }
                    ty => self
                        .b
                        .ins()
                        .va_arg(ty.ir_with_layout(self.g.p.c_layout), addr),
                }
            }
            ExprKind::Builtin(op, args) => return self.builtin(*op, args, e),
        };
        Ok(Some(v))
    }

    /// `n * size` as an `i64`, for pointer arithmetic.
    fn scaled(&mut self, n: &Expr<'p>, size: u64) -> R<Value> {
        if let Some(k) = fold_int(n, FOLD_DEPTH, self.g.p.c_layout) {
            return Ok(self.b.ins().iconst(Type::I64, k.wrapping_mul(size as i64)));
        }
        let v = self.val(n)?;
        let v = if self.b.function().value_type(v) != Type::I64 {
            if n.ty.is_signed() {
                self.b.ins().sext(Type::I64, v)
            } else {
                self.b.ins().uext(Type::I64, v)
            }
        } else {
            v
        };
        if size == 1 {
            return Ok(v);
        }
        let s = self.b.ins().iconst(Type::I64, size as i64);
        Ok(self.b.ins().imul(v, s))
    }

    fn compare(&mut self, op: BinOp, a: &Expr<'p>, b: &Expr<'p>) -> R<Value> {
        // `(x < y) != 0` (as `__builtin_expect` and `!!` spell it) is
        // `x < y`, and `(x < y) == 0` is `x >= y`.
        if let Some(ne) = zero_test_op(op, a, b)
            && let Some((inner, x, y)) = int_comparison(a, self.g.p.c_layout)
        {
            return self.compare(if ne { inner } else { negated(inner) }, x, y);
        }
        let (x, y) = (self.val(a)?, self.val(b)?);
        let ty = &a.ty;
        if let Ty::Complex(elem) = ty {
            let (xr, xi) = self.complex_parts(x, elem);
            let (yr, yi) = self.complex_parts(y, elem);
            let cc = if op == BinOp::Eq {
                FloatCC::Eq
            } else {
                FloatCC::Ne
            };
            let r = self.b.ins().fcmp(cc, xr, yr);
            let i = self.b.ins().fcmp(cc, xi, yi);
            return Ok(if op == BinOp::Eq {
                self.b.ins().and(r, i)
            } else {
                self.b.ins().or(r, i)
            });
        }
        let elem = match ty {
            Ty::Vector(elem, _) => *elem,
            _ => ty,
        };
        if elem.is_float() {
            let cc = match op {
                BinOp::Eq => FloatCC::Eq,
                BinOp::Ne => FloatCC::Ne,
                BinOp::Lt => FloatCC::Lt,
                BinOp::Le => FloatCC::Le,
                BinOp::Gt => FloatCC::Gt,
                _ => FloatCC::Ge,
            };
            return Ok(if ty.is_vector() {
                self.b.ins().vfcmp(cc, x, y)
            } else {
                self.b.ins().fcmp(cc, x, y)
            });
        }
        let signed = elem.is_signed();
        let cc = match op {
            BinOp::Eq => IntCC::Eq,
            BinOp::Ne => IntCC::Ne,
            BinOp::Lt if signed => IntCC::Slt,
            BinOp::Le if signed => IntCC::Sle,
            BinOp::Gt if signed => IntCC::Sgt,
            BinOp::Ge if signed => IntCC::Sge,
            BinOp::Lt => IntCC::Ult,
            BinOp::Le => IntCC::Ule,
            BinOp::Gt => IntCC::Ugt,
            _ => IntCC::Uge,
        };
        Ok(if ty.is_vector() {
            self.b.ins().vicmp(cc, x, y)
        } else {
            self.b.ins().icmp(cc, x, y)
        })
    }

    fn arith(&mut self, op: BinOp, ty: &Ty, x: Value, y: Value, loc: Loc) -> R<Value> {
        let ins = self.b.ins();
        if let Ty::Vector(elem, _) = ty {
            use keel_ir::VectorBinaryOp as V;
            let vector_op = match op {
                BinOp::Add if elem.is_float() => V::Fadd,
                BinOp::Sub if elem.is_float() => V::Fsub,
                BinOp::Mul if elem.is_float() => V::Fmul,
                BinOp::Div if elem.is_float() => V::Fdiv,
                BinOp::Add => V::Iadd,
                BinOp::Sub => V::Isub,
                BinOp::Mul => V::Imul,
                BinOp::And => V::And,
                BinOp::Or => V::Or,
                BinOp::Xor => V::Xor,
                BinOp::Shl => V::Shl,
                BinOp::Shr if elem.is_signed() => V::Sshr,
                BinOp::Shr => V::Ushr,
                _ => return Err(Error::new(loc, "unsupported vector arithmetic")),
            };
            return Ok(ins.vector_binary(vector_op, x, y));
        }
        if ty.is_float() {
            return Ok(match op {
                BinOp::Add => ins.fadd(x, y),
                BinOp::Sub => ins.fsub(x, y),
                BinOp::Mul => ins.fmul(x, y),
                BinOp::Div => ins.fdiv(x, y),
                _ => return Err(Error::new(loc, "invalid float operation")),
            });
        }
        let signed = ty.is_signed();
        Ok(match op {
            BinOp::Add => ins.iadd(x, y),
            BinOp::Sub => ins.isub(x, y),
            BinOp::Mul => ins.imul(x, y),
            BinOp::Div if signed => ins.sdiv(x, y),
            BinOp::Div => ins.udiv(x, y),
            BinOp::Rem if signed => ins.srem(x, y),
            BinOp::Rem => ins.urem(x, y),
            BinOp::Shl => ins.shl(x, y),
            BinOp::Shr if signed => ins.sshr(x, y),
            BinOp::Shr => ins.ushr(x, y),
            BinOp::And => ins.and(x, y),
            BinOp::Or => ins.or(x, y),
            BinOp::Xor => ins.xor(x, y),
            _ => unreachable!("comparison in arith"),
        })
    }

    /// Address-backed pairs share aggregate ABI machinery, but retain a
    /// distinct C type.
    fn complex_pair(&mut self, r: Value, i: Value, elem: &Ty) -> Value {
        let size = self.size(elem) as u32;
        let slot = self.b.create_stack_slot(2 * size, size);
        let addr = self.b.ins().stack_addr(slot);
        self.b.ins().store(r, addr);
        let imag = self.offset(addr, u64::from(size));
        self.b.ins().store(i, imag);
        addr
    }

    fn complex_parts(&mut self, addr: Value, elem: &Ty) -> (Value, Value) {
        let r = self
            .b
            .ins()
            .load(elem.ir_with_layout(self.g.p.c_layout), addr);
        let imag = self.offset(addr, self.size(elem));
        let i = self
            .b
            .ins()
            .load(elem.ir_with_layout(self.g.p.c_layout), imag);
        (r, i)
    }

    fn complex_arith(
        &mut self,
        op: BinOp,
        lt: &Ty,
        rt: &Ty,
        x: Value,
        y: Value,
        loc: Loc,
    ) -> R<Value> {
        let ty = crate::expr::usual_with_layout(lt, rt, self.g.p.c_layout);
        let Ty::Complex(elem) = ty else {
            unreachable!()
        };
        let (xr, xi) = if lt.is_complex() {
            let (r, i) = self.complex_parts(x, elem);
            (r, Some(i))
        } else {
            (x, None)
        };
        let (yr, yi) = if rt.is_complex() {
            let (r, i) = self.complex_parts(y, elem);
            (r, Some(i))
        } else {
            (y, None)
        };
        if matches!(op, BinOp::Mul | BinOp::Div) {
            // Real multiplication and division by a real use scalar operations,
            // avoiding spurious 0*infinity and preserving component zero signs.
            if let (Some(i), None) = (xi, yi) {
                let r = self.arith(op, elem, xr, yr, loc)?;
                let i = self.arith(op, elem, i, yr, loc)?;
                return Ok(self.complex_pair(r, i, elem));
            }
            if op == BinOp::Mul
                && let (None, Some(i)) = (xi, yi)
            {
                let r = self.arith(op, elem, xr, yr, loc)?;
                let i = self.arith(op, elem, xr, i, loc)?;
                return Ok(self.complex_pair(r, i, elem));
            }
            let zero = self.zero(elem.ir_with_layout(self.g.p.c_layout));
            return self.complex_runtime(
                op,
                elem,
                [xr, xi.unwrap_or(zero), yr, yi.unwrap_or(zero)],
                loc,
            );
        }
        let r = self.arith(op, elem, xr, yr, loc)?;
        let i = match (xi, yi) {
            (Some(x), Some(y)) => self.arith(op, elem, x, y, loc)?,
            (Some(x), None) => x,
            (None, Some(y)) if op == BinOp::Sub => self.b.ins().fneg(y),
            (None, Some(y)) => y,
            _ => unreachable!(),
        };
        Ok(self.complex_pair(r, i, elem))
    }

    /// compiler-rt/libgcc ABI: four real arguments and a complex result. The
    /// existing Sret layout lowers to the platform FP return registers, not a
    /// foreign struct ABI. Linking requires the target compiler builtins runtime
    /// (Darwin also uses libSystem; see the complex implementation contract).
    fn complex_runtime(&mut self, op: BinOp, elem: &Ty, args: [Value; 4], loc: Loc) -> R<Value> {
        let name = match (op, elem) {
            (BinOp::Mul, Ty::Float) => "__mulsc3",
            (BinOp::Mul, _) => "__muldc3",
            (BinOp::Div, Ty::Float) => "__divsc3",
            (BinOp::Div, _) => "__divdc3",
            _ => unreachable!(),
        };
        let ty = Ty::Complex(elem);
        // These reserved runtime names must never acquire an incompatible
        // source-selected ABI or weak/internal binding.
        for global in &self.g.p.globals {
            if global.name == name || global.symbol == name {
                return Err(Error::new(
                    loc,
                    "complex runtime symbol is reserved for the compiler builtins ABI",
                ));
            }
        }
        let mut sig = Signature::new(&[], &[]);
        sig.params.push(AbiParam {
            ty: Type::Ptr,
            ext: ArgExt::None,
            purpose: Purpose::Sret(
                self.g
                    .aggregate_layout(&ty)
                    .map_err(|m| Error::new(loc, m))?,
            ),
        });
        sig.params
            .extend((0..4).map(|_| AbiParam::new(elem.ir_with_layout(self.g.p.c_layout))));
        if !self.g.module.decls.iter().any(|d| d.name == name) {
            self.g.module.decls.push(FuncDecl {
                name: name.to_owned(),
                attrs: Default::default(),
                sig: sig.clone(),
            });
        }
        let slot = self
            .b
            .create_stack_slot(self.size(&ty) as u32, self.g.types().align(&ty) as u32);
        let result = self.b.ins().stack_addr(slot);
        let callee = self.b.import_function(name, sig);
        self.b
            .ins()
            .call(callee, &[result, args[0], args[1], args[2], args[3]]);
        Ok(result)
    }

    /// Converts `v` from C type `from` to `to`.
    fn convert(&mut self, v: Value, from: &Ty, to: &Ty, loc: Loc) -> R<Value> {
        if from == to || (from.is_ptr() && to.is_ptr()) || to.is_struct() {
            return Ok(v);
        }
        if let Some(name) = from.unusable().or(to.unusable()) {
            return Err(Error::new(loc, format!("`{name}` is not supported")));
        }
        if *to == Ty::Int(IntKind::Bool) {
            let c = self.nonzero(v, from);
            return Ok(c);
        }
        if let Ty::Complex(elem) = to {
            let (r, i) = if let Ty::Complex(src) = from {
                let (r, i) = self.complex_parts(v, src);
                (
                    self.convert(r, src, elem, loc)?,
                    self.convert(i, src, elem, loc)?,
                )
            } else {
                let r = self.convert(v, from, elem, loc)?;
                (r, self.zero(elem.ir_with_layout(self.g.p.c_layout)))
            };
            return Ok(self.complex_pair(r, i, elem));
        }
        if let Ty::Complex(elem) = from {
            let (r, _) = self.complex_parts(v, elem);
            return self.convert(r, elem, to, loc);
        }
        let ins = self.b.ins();
        let (fi, ti) = (
            from.ir_with_layout(self.g.p.c_layout),
            to.ir_with_layout(self.g.p.c_layout),
        );
        if from.is_vector() || to.is_vector() {
            return Ok(if fi == ti {
                v
            } else {
                ins.convert(ConvertOp::Bitcast, ti, v)
            });
        }
        Ok(match (from, to) {
            (Ty::Int(_), Ty::Int(_)) => {
                let (fs, ts) = (fi.bytes().unwrap(), ti.bytes().unwrap());
                if ts > fs {
                    if from.is_signed() {
                        ins.sext(ti, v)
                    } else {
                        ins.uext(ti, v)
                    }
                } else if ts < fs {
                    ins.trunc(ti, v)
                } else {
                    v
                }
            }
            (Ty::Int(k), _) if to.is_float() => {
                if k.size_with_layout(self.g.p.c_layout) == 16 {
                    return Err(Error::new(loc, "`__int128` to float is not supported"));
                }
                let op = if k.signed() {
                    ConvertOp::Sitofp
                } else {
                    ConvertOp::Uitofp
                };
                ins.convert(op, ti, v)
            }
            (_, Ty::Int(k)) if from.is_float() => {
                if k.size_with_layout(self.g.p.c_layout) == 16 {
                    return Err(Error::new(loc, "float to `__int128` is not supported"));
                }
                let op = if k.signed() {
                    ConvertOp::Fptosi
                } else {
                    ConvertOp::Fptoui
                };
                let wide = if k.size_with_layout(self.g.p.c_layout) == 8 {
                    Type::I64
                } else {
                    Type::I32
                };
                let r = ins.convert(op, wide, v);
                if wide != ti {
                    self.b.ins().trunc(ti, r)
                } else {
                    r
                }
            }
            (Ty::Float, Ty::Double) => ins.convert(ConvertOp::Fpext, ti, v),
            (Ty::Double, Ty::Float) => ins.convert(ConvertOp::Fptrunc, ti, v),
            (Ty::Ptr(_), Ty::Int(_)) => {
                let n = ins.convert(ConvertOp::PtrToInt, Type::I64, v);
                if ti == Type::I64 {
                    n
                } else if ti == Type::I128 {
                    self.b.ins().uext(ti, n)
                } else {
                    self.b.ins().trunc(ti, n)
                }
            }
            (Ty::Int(k), Ty::Ptr(_)) => {
                let n = match k.size_with_layout(self.g.p.c_layout) {
                    8 => v,
                    16 => ins.trunc(Type::I64, v),
                    _ if k.signed() => ins.sext(Type::I64, v),
                    _ => ins.uext(Type::I64, v),
                };
                let n = if self.g.p.c_layout.pointer_bytes() == 4 {
                    let low = self.b.ins().trunc(Type::I32, n);
                    self.b.ins().uext(Type::I64, low)
                } else {
                    n
                };
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            _ => return Err(Error::new(loc, "unsupported conversion")),
        })
    }

    // ---- places ----

    /// Conservatively retain volatile subobjects during aggregate copies.
    fn object_flags(&self, ty: &Ty, sugar: &Sugar) -> MemoryFlags {
        MemoryFlags {
            volatile: self.g.object_volatile(ty, sugar),
            ..MemoryFlags::default()
        }
    }

    fn aggregate_place(&self, addr: Value, ty: &Ty, flags: MemoryFlags, loc: Loc) -> R<Place> {
        let size = u32::try_from(self.size(ty)).map_err(|_| {
            Error::new(loc, "aggregate access exceeds the IR stack-slot size limit")
        })?;
        Ok(Place::Aggregate {
            addr,
            size,
            align: self.g.types().align(ty) as u32,
            flags,
        })
    }

    /// Where an lvalue lives. `None` for a void-typed dereference.
    fn place(&mut self, e: &Expr<'p>) -> R<Option<Place>> {
        if e.ty == Ty::Void {
            return Ok(None);
        }
        if let ExprKind::VectorIndex(base, index) = &e.kind {
            let address = self.addr(base)?;
            let offset = self.scaled(index, self.size(&e.ty))?;
            let address = self.b.ins().ptradd(address, offset);
            return Ok(Some(Place::Mem(
                address,
                e.ty.ir_with_layout(self.g.p.c_layout),
                self.object_flags(&e.ty, &e.sugar),
            )));
        }
        if let ExprKind::Local(id) = e.kind
            && let Storage::Var(var) = self.locals[id]
        {
            return Ok(Some(Place::Var(var)));
        }
        if let ExprKind::Member {
            base,
            offset,
            bits: Some(field),
            ..
        } = &e.kind
        {
            let (base, off) = self.addr_parts(base)?;
            let addr = self.offset_by(base, off.wrapping_add(*offset as i64));
            return Ok(Some(Place::Bits {
                flags: self.object_flags(&e.ty, &e.sugar),
                addr,
                unit: e.ty.ir_with_layout(self.g.p.c_layout),
                field: *field,
                signed: e.ty.is_signed(),
            }));
        }
        let addr = self.addr(e)?;
        let flags = self.object_flags(&e.ty, &e.sugar);
        Ok(Some(if Self::is_aggregate(&e.ty) {
            self.aggregate_place(addr, &e.ty, flags, e.loc)?
        } else {
            if let Some(name) = e.ty.unusable() {
                return Err(Error::new(e.loc, format!("`{name}` is not supported")));
            }
            Place::Mem(addr, e.ty.ir_with_layout(self.g.p.c_layout), flags)
        }))
    }

    fn load_place(&mut self, place: &Place) -> Value {
        match *place {
            Place::Var(var) => self.b.use_var(var),
            Place::Mem(addr, ty, flags) => self.b.ins().load_with_flags(ty, addr, flags),
            Place::Aggregate {
                addr,
                size,
                align,
                flags,
            } => {
                if !flags.volatile {
                    return addr;
                }
                // Materialize a volatile aggregate read now, before an
                // assignment, return or by-value ABI copy loses the lvalue.
                // This private byte snapshot is never exposed as an aligned C object.
                let slot = self.b.create_stack_slot(size, align.min(16));
                let copy = self.b.ins().stack_addr(slot);
                self.copy_bytes_with_flags(
                    copy,
                    addr,
                    u64::from(size),
                    MemoryFlags::default(),
                    flags,
                );
                copy
            }
            Place::Bits {
                flags,
                addr,
                unit,
                field,
                signed,
            } => {
                let bits = i64::from(unit.bytes().unwrap() * 8);
                if field.bytewise {
                    return self.load_packed_bits(addr, unit, field, signed, flags);
                }
                let word = self.b.ins().load_with_flags(unit, addr, flags);
                let left = bits - i64::from(field.shift) - i64::from(field.width);
                // A field in the top bits needs no shift up.
                let up = if left == 0 {
                    word
                } else {
                    let l = self.b.ins().iconst(unit, left);
                    self.b.ins().shl(word, l)
                };
                let r = self.b.ins().iconst(unit, bits - i64::from(field.width));
                if signed {
                    self.b.ins().sshr(up, r)
                } else {
                    self.b.ins().ushr(up, r)
                }
            }
        }
    }

    /// Return the converted value actually stored, including bit-field truncation.
    fn store_place(&mut self, place: &Place, v: Value) -> Value {
        let v = match *place {
            Place::Bits {
                unit,
                field,
                signed,
                ..
            } => {
                let extension = unit.bytes().unwrap() * 8 - field.width;
                if extension == 0 {
                    v
                } else if !signed && field.width < 64 {
                    let bits = (u64::MAX >> (64 - field.width)) as i64;
                    let mask = self.b.ins().iconst(unit, bits);
                    self.b.ins().and(v, mask)
                } else {
                    let shift = self.b.ins().iconst(unit, i64::from(extension));
                    let shifted = self.b.ins().shl(v, shift);
                    if signed {
                        self.b.ins().sshr(shifted, shift)
                    } else {
                        self.b.ins().ushr(shifted, shift)
                    }
                }
            }
            _ => v,
        };
        match *place {
            Place::Var(var) => self.b.def_var(var, v),
            Place::Mem(addr, _, flags) => {
                self.b.ins().store_with_flags(v, addr, flags);
            }
            Place::Aggregate {
                addr, size, flags, ..
            } => {
                self.copy_bytes_with_flags(addr, v, u64::from(size), flags, MemoryFlags::default())
            }
            Place::Bits {
                flags,
                addr,
                unit,
                field,
                ..
            } => {
                if field.bytewise {
                    self.store_packed_bits(addr, unit, field, v, flags);
                    return v;
                }
                let mask = (u128::MAX >> (128 - field.width)) << field.shift;
                let word = self.b.ins().load_with_flags(unit, addr, flags);
                let keep = self.b.ins().iconst(unit, !mask as i64);
                let kept = self.b.ins().and(word, keep);
                // A field in the low bits needs no shift into place.
                let moved = if field.shift == 0 {
                    v
                } else {
                    let shift = self.b.ins().iconst(unit, i64::from(field.shift));
                    self.b.ins().shl(v, shift)
                };
                let m = self.b.ins().iconst(unit, mask as i64);
                let new = self.b.ins().and(moved, m);
                let merged = self.b.ins().or(kept, new);
                self.b.ins().store_with_flags(merged, addr, flags);
            }
        }
        v
    }

    /// Assemble only the field's bytes, keeping all shifts below the value width.
    fn load_packed_bits(
        &mut self,
        addr: Value,
        unit: Type,
        field: BitField,
        signed: bool,
        flags: MemoryFlags,
    ) -> Value {
        let mut value = self.b.ins().iconst(unit, 0);
        let mut consumed = 0;
        while consumed < field.width {
            let position = field.shift + consumed;
            let shift = position % 8;
            let width = (8 - shift).min(field.width - consumed);
            let address = self.offset(addr, u64::from(position / 8));
            let mut byte = self.b.ins().load_with_flags(Type::I8, address, flags);
            if shift != 0 {
                let amount = self.b.ins().iconst(Type::I8, i64::from(shift));
                byte = self.b.ins().ushr(byte, amount);
            }
            if width < 8 {
                let mask = self.b.ins().iconst(Type::I8, (1 << width) - 1);
                byte = self.b.ins().and(byte, mask);
            }
            let mut part = if unit == Type::I8 {
                byte
            } else {
                self.b.ins().convert(ConvertOp::Uext, unit, byte)
            };
            if consumed != 0 {
                let amount = self.b.ins().iconst(unit, i64::from(consumed));
                part = self.b.ins().shl(part, amount);
            }
            value = self.b.ins().or(value, part);
            consumed += width;
        }
        let extension = unit.bytes().unwrap() * 8 - field.width;
        if signed && extension != 0 {
            let amount = self.b.ins().iconst(unit, i64::from(extension));
            let shifted = self.b.ins().shl(value, amount);
            value = self.b.ins().sshr(shifted, amount);
        }
        value
    }

    /// Preserve neighboring bits without reading or writing outside the field's span.
    fn store_packed_bits(
        &mut self,
        addr: Value,
        unit: Type,
        field: BitField,
        value: Value,
        flags: MemoryFlags,
    ) {
        let mut consumed = 0;
        while consumed < field.width {
            let position = field.shift + consumed;
            let shift = position % 8;
            let width = (8 - shift).min(field.width - consumed);
            let address = self.offset(addr, u64::from(position / 8));
            let mut part = value;
            if consumed != 0 {
                let amount = self.b.ins().iconst(unit, i64::from(consumed));
                part = self.b.ins().ushr(part, amount);
            }
            if unit != Type::I8 {
                part = self.b.ins().convert(ConvertOp::Trunc, Type::I8, part);
            }
            if shift != 0 {
                let amount = self.b.ins().iconst(Type::I8, i64::from(shift));
                part = self.b.ins().shl(part, amount);
            }
            if width != 8 {
                let mask = ((1i64 << width) - 1) << shift;
                let bits = self.b.ins().iconst(Type::I8, mask);
                part = self.b.ins().and(part, bits);
                let old = self.b.ins().load_with_flags(Type::I8, address, flags);
                let keep = self.b.ins().iconst(Type::I8, !mask);
                let kept = self.b.ins().and(old, keep);
                part = self.b.ins().or(kept, part);
            }
            self.b.ins().store_with_flags(part, address, flags);
            consumed += width;
        }
    }

    fn offset(&mut self, base: Value, offset: u64) -> Value {
        self.offset_by(base, offset as i64)
    }

    fn offset_by(&mut self, base: Value, offset: i64) -> Value {
        if offset == 0 {
            return base;
        }
        let n = self.b.ins().iconst(Type::I64, offset);
        self.b.ins().ptradd(base, n)
    }

    /// Pointer value `e` as a base and a constant byte offset, so that a
    /// chain of constant steps (`&p[2]`, `(char *)q + 8`, a member of a
    /// member) adds one offset rather than one per step. Every operand
    /// that is not a literal is still evaluated, once and in order.
    fn ptr_parts(&mut self, e: &Expr<'p>) -> R<(Value, i64)> {
        match &e.kind {
            ExprKind::PtrAdd(p, n, size)
                if let Some(k) = fold_int(n, FOLD_DEPTH, self.g.p.c_layout) =>
            {
                let (base, off) = self.ptr_parts(p)?;
                Ok((base, off.wrapping_add(k.wrapping_mul(*size as i64))))
            }
            ExprKind::Cast(p) if e.ty.is_ptr() && p.ty.is_ptr() => self.ptr_parts(p),
            ExprKind::Addr(lvalue) => self.addr_parts(lvalue),
            _ => Ok((self.val(e)?, 0)),
        }
    }

    /// `ptr_parts` of the address of lvalue `e`.
    fn addr_parts(&mut self, e: &Expr<'p>) -> R<(Value, i64)> {
        match &e.kind {
            ExprKind::Member { base, offset, .. } => {
                let (base, off) = self.addr_parts(base)?;
                Ok((base, off.wrapping_add(*offset as i64)))
            }
            ExprKind::Deref(p) => self.ptr_parts(p),
            _ => Ok((self.addr(e)?, 0)),
        }
    }

    /// The address of an lvalue (or of an aggregate rvalue's bytes).
    fn addr(&mut self, e: &Expr<'p>) -> R<Value> {
        Ok(match &e.kind {
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Slot(slot) => self.b.ins().stack_addr(slot),
                Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
                Storage::Var(var) if e.ty.runtime_sized() => self.b.use_var(var),
                Storage::Var(_) => {
                    return Err(Error::new(e.loc, "internal error: address of a register"));
                }
            },
            ExprKind::Global(g) => {
                self.g.use_global(*g);
                let global = &self.g.p.globals[*g];
                let symbol = if let Ty::Func(fty) = &global.ty {
                    Symbol::Func(self.import_global(*g, fty, None)?)
                } else {
                    Symbol::Data(self.b.import_data(&global.symbol))
                };
                self.b.ins().symbol_addr(symbol)
            }
            ExprKind::Str(i) => {
                let name = self.g.string_symbol(*i);
                let data = self.b.import_data(&name);
                self.b.ins().symbol_addr(Symbol::Data(data))
            }
            ExprKind::Deref(_) | ExprKind::Member { .. } => {
                let (base, off) = self.addr_parts(e)?;
                self.offset_by(base, off)
            }
            ExprKind::CompoundLit(id, init) => {
                self.init_local(*id, init)?;
                match self.locals[*id] {
                    Storage::Slot(slot) => self.b.ins().stack_addr(slot),
                    Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
                    Storage::Var(_) => unreachable!(),
                }
            }
            // Struct-valued calls, assignments, conditionals: their value
            // is already an address.
            _ => self.val(e)?,
        })
    }

    /// The signature to call global function `g` with.
    /// Function `g` (of type `fty`), imported into this function under its
    /// symbol. Once imported, later imports find it by name, so a fixed
    /// signature need not be built again.
    fn import_global(
        &mut self,
        g: usize,
        fty: &FuncTy,
        args: Option<&[Ty]>,
    ) -> R<keel_ir::FuncRef> {
        let fixed = fty.prototyped || self.g.p.globals[g].func.is_some();
        if fixed && let Some(&fref) = self.callees.get(&g) {
            return Ok(fref);
        }
        let sig = self.callee_signature(g, fty, args)?;
        let fref = self
            .b
            .import_function(self.g.p.globals[g].symbol, sig.clone());
        // A preceding address-of may have imported an unspecified empty
        // signature. The first unprototyped call supplies its promoted ABI.
        if !fixed {
            self.b.function_mut().callee_mut(fref).sig = sig;
        }
        if fixed {
            self.callees.insert(g, fref);
        }
        Ok(fref)
    }

    fn callee_signature(&mut self, g: usize, fty: &FuncTy, args: Option<&[Ty]>) -> R<Signature> {
        let global = &self.g.p.globals[g];
        let final_ty = global.ty.func().unwrap();
        if global.func.is_some() || final_ty.prototyped {
            return self.g.signature(final_ty, g, &[]);
        }
        if fty.prototyped {
            return self.g.signature(fty, g, &[]);
        }
        if let Some(sig) = self.g.unprototyped.get(&g) {
            return Ok(sig.clone());
        }
        let sig = self.g.signature(fty, g, args.unwrap_or(&[]))?;
        // An actual zero-argument call fixes the signature too. Address-only
        // imports do not, and are completed after all bodies are lowered.
        if args.is_some() {
            self.g.unprototyped.insert(g, sig.clone());
        }
        Ok(sig)
    }

    /// Proof used only to defer ordinary aggregate argument copying. Any
    /// unknown effect, volatile access, call or initialization is a barrier.
    fn may_change_memory(&self, e: &Expr<'p>) -> bool {
        if self.object_flags(&e.ty, &e.sugar).volatile {
            return true;
        }
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::LabelAddr(_)
            | ExprKind::Local(_)
            | ExprKind::Global(_) => false,
            ExprKind::Unary(_, a)
            | ExprKind::Cast(a)
            | ExprKind::Deref(a)
            | ExprKind::Addr(a)
            | ExprKind::Member { base: a, .. } => self.may_change_memory(a),
            ExprKind::VectorIndex(a, b)
            | ExprKind::Binary(_, a, b)
            | ExprKind::PtrAdd(a, b, _)
            | ExprKind::PtrDiff(a, b, _)
            | ExprKind::LogAnd(a, b)
            | ExprKind::LogOr(a, b)
            | ExprKind::Comma(a, b) => self.may_change_memory(a) || self.may_change_memory(b),
            ExprKind::Cond(a, b, c) => {
                self.may_change_memory(a) || self.may_change_memory(b) || self.may_change_memory(c)
            }
            _ => true,
        }
    }

    fn call(&mut self, f: &Expr<'p>, args: &[Expr<'p>], e: &Expr<'p>) -> R<Option<Value>> {
        let fty = f.ty.pointee().and_then(Ty::func).unwrap();
        self.set_loc(e.loc);
        let mut vals = Vec::new();
        let ret_slot = if fty.ret.is_pair_or_struct() {
            let (size, align) = (self.size(&fty.ret), self.g.types().align(&fty.ret));
            let slot = self.b.create_stack_slot(size.max(1) as u32, align as u32);
            vals.push(self.b.ins().stack_addr(slot));
            Some(slot)
        } else {
            None
        };
        // The ABI consumes aggregate addresses at the call instruction.
        // Freeze an argument's bytes now when later evaluation could change
        // them. With no intervening effects, immediate ABI copying suffices.
        let mut snapshots = Vec::new();
        if args.iter().any(|arg| arg.ty.is_pair_or_struct()) {
            snapshots.resize(args.len(), false);
            let mut later_effect = self.may_change_memory(f);
            for (index, arg) in args.iter().enumerate().rev() {
                snapshots[index] = later_effect;
                later_effect |= self.may_change_memory(arg);
            }
        }
        for (index, arg) in args.iter().enumerate() {
            if !fty.prototyped && arg.ty.is_pair_or_struct() {
                return Err(Error::new(arg.loc, "struct argument without a prototype"));
            }
            if vals.len() >= fty.params.len() + usize::from(ret_slot.is_some())
                && (arg.ty.is_pair_or_struct()
                    || arg
                        .ty
                        .int_kind()
                        .is_some_and(|k| k.size_with_layout(self.g.p.c_layout) == 16)
                        && self.g.p.target != keel_ir::Target::Aarch64LinuxGnu)
            {
                return Err(Error::new(
                    arg.loc,
                    "this variadic argument type is not supported",
                ));
            }
            let mut value = self.val(arg)?;
            if arg.ty.is_pair_or_struct() && snapshots[index] {
                let layout =
                    self.aggregate_place(value, &arg.ty, MemoryFlags::default(), arg.loc)?;
                let Place::Aggregate { size, align, .. } = layout else {
                    unreachable!()
                };
                let slot = self.b.create_stack_slot(size.max(1), align);
                let copy = self.b.ins().stack_addr(slot);
                self.copy(copy, value, &arg.ty);
                value = copy;
            }
            vals.push(value);
        }
        // Argument types decide the signature only without a prototype.
        let extra: Vec<Ty> = if fty.prototyped {
            Vec::new()
        } else {
            args.iter().map(|a| a.ty).collect()
        };
        self.set_loc(e.loc);
        let direct = match &f.kind {
            ExprKind::Addr(inner) => match inner.kind {
                ExprKind::Global(g) if matches!(inner.ty, Ty::Func(_)) => Some(g),
                _ => None,
            },
            _ => None,
        };
        let inst = match direct {
            Some(g) => {
                let global = &self.g.p.globals[g];
                if self.has_cleanup() && crate::parse::nonlocal_exit(global) {
                    return Err(Error::new(
                        e.loc,
                        "nonlocal exit/returns-twice call in a function containing cleanup locals is not supported yet",
                    ));
                }
                self.g.use_global(g);
                let fref = self.import_global(g, &fty, Some(&extra))?;
                let function = self.b.function();
                let sig = &function.callee(fref).sig;
                if !fty.prototyped
                    && (sig.params.len() != vals.len()
                        || sig
                            .params
                            .iter()
                            .zip(&vals)
                            .any(|(p, &v)| p.ty != function.value_type(function.resolve(v))))
                {
                    return Err(Error::new(
                        e.loc,
                        "unprototyped call arguments do not match the known function ABI",
                    ));
                }
                self.b.ins().call(fref, &vals)
            }
            None => {
                let callee = self.val(f)?;
                let unprototyped = FuncTy {
                    params: &[],
                    ..*fty
                };
                let sig = self.g.signature(
                    if fty.prototyped { fty } else { &unprototyped },
                    usize::MAX,
                    &extra,
                )?;
                let sig = self.b.import_signature(sig);
                self.b.ins().call_indirect(sig, callee, &vals)
            }
        };
        if direct.is_some_and(|g| self.g.p.globals[g].function_attrs.noreturn) {
            self.b.ins().trap();
            self.terminated = true;
            self.ensure_block();
            // A GNU noreturn function can have a non-void return type.
            // Its nominal result must not escape into this unreachable block.
            return Ok(if fty.ret == Ty::Void {
                None
            } else {
                Some(self.zero(if ret_slot.is_some() {
                    Type::Ptr
                } else {
                    fty.ret.ir_with_layout(self.g.p.c_layout)
                }))
            });
        }
        if let Some(slot) = ret_slot {
            return Ok(Some(self.b.ins().stack_addr(slot)));
        }
        Ok(self.b.function().inst_results(inst).first().copied())
    }

    /// Compute using unsigned magnitudes, retaining a carry beyond 128 bits.
    /// This avoids both signed overflow and an IR integer wider than i128.
    fn generic_overflow(
        &mut self,
        op: OverflowOp,
        args: &[Expr<'p>],
        vals: &[Value],
        loc: Loc,
    ) -> R<Value> {
        let result_ty = args[2].ty.pointee().unwrap();
        let max_size = args[..2]
            .iter()
            .map(|a| a.ty.int_kind().unwrap().size())
            .chain([result_ty.int_kind().unwrap().size()])
            .max()
            .unwrap();
        let wide_ty = Ty::Int(if max_size <= 4 {
            IntKind::ULong
        } else {
            IntKind::UInt128
        });
        let wide = wide_ty.ir_with_layout(self.g.p.c_layout);
        let a = self.convert(vals[0], &args[0].ty, &wide_ty, loc)?;
        let b = self.convert(vals[1], &args[1].ty, &wide_ty, loc)?;
        let zero = self.b.ins().iconst(wide, 0);
        let no = self.b.ins().iconst(Type::I8, 0);
        let yes = self.b.ins().iconst(Type::I8, 1);
        let an = if args[0].ty.is_signed() {
            self.b.ins().icmp(IntCC::Slt, a, zero)
        } else {
            no
        };
        let bn = if args[1].ty.is_signed() {
            self.b.ins().icmp(IntCC::Slt, b, zero)
        } else {
            no
        };
        let na = self.b.ins().ineg(a);
        let nb = self.b.ins().ineg(b);
        let am = self.b.ins().select(an, na, a);
        let bm = self.b.ins().select(bn, nb, b);
        let (magnitude, negative, carry) = if op == OverflowOp::Umul {
            let negative = self.b.ins().xor(an, bn);
            let (product, carry) = if args[..2]
                .iter()
                .any(|a| a.ty.int_kind().unwrap().size() == 16)
            {
                // a0*b0 + (a0*b1 + a1*b0)*2^64. Detect every
                // discarded high limb and carry without computing i256.
                let shift = self.b.ins().iconst(wide, 64);
                let mask = self.b.ins().iconst(wide, -1);
                let mask = self.b.ins().ushr(mask, shift);
                let al = self.b.ins().and(am, mask);
                let bl = self.b.ins().and(bm, mask);
                let ah = self.b.ins().ushr(am, shift);
                let bh = self.b.ins().ushr(bm, shift);
                let ll = self.b.ins().imul(al, bl);
                let lh = self.b.ins().imul(al, bh);
                let hl = self.b.ins().imul(ah, bl);
                let ah_nonzero = self.b.ins().icmp(IntCC::Ne, ah, zero);
                let bh_nonzero = self.b.ins().icmp(IntCC::Ne, bh, zero);
                let hh_nonzero = self.b.ins().and(ah_nonzero, bh_nonzero);
                let lh_high = self.b.ins().ushr(lh, shift);
                let hl_high = self.b.ins().ushr(hl, shift);
                let high = self.b.ins().or(lh_high, hl_high);
                let high_nonzero = self.b.ins().icmp(IntCC::Ne, high, zero);
                let lh_low = self.b.ins().shl(lh, shift);
                let hl_low = self.b.ins().shl(hl, shift);
                let (sum, c1) = self.b.ins().overflow(OverflowOp::Uadd, ll, lh_low);
                let (product, c2) = self.b.ins().overflow(OverflowOp::Uadd, sum, hl_low);
                let c = self.b.ins().or(c1, c2);
                let high = self.b.ins().or(hh_nonzero, high_nonzero);
                (product, self.b.ins().or(c, high))
            } else {
                (self.b.ins().imul(am, bm), no)
            };
            (product, negative, carry)
        } else {
            // Subtraction is addition with the second mathematical sign
            // reversed; negative zero is harmless and normalized below.
            let bn = if op == OverflowOp::Usub {
                self.b.ins().xor(bn, yes)
            } else {
                bn
            };
            let same_sign = self.b.ins().icmp(IntCC::Eq, an, bn);
            let (sum, carry) = self.b.ins().overflow(OverflowOp::Uadd, am, bm);
            let a_larger = self.b.ins().icmp(IntCC::Uge, am, bm);
            let ab = self.b.ins().isub(am, bm);
            let ba = self.b.ins().isub(bm, am);
            let difference = self.b.ins().select(a_larger, ab, ba);
            let difference_sign = self.b.ins().select(a_larger, an, bn);
            let magnitude = self.b.ins().select(same_sign, sum, difference);
            let negative = self.b.ins().select(same_sign, an, difference_sign);
            (magnitude, negative, self.b.ins().and(same_sign, carry))
        };
        let nonzero = self.b.ins().icmp(IntCC::Ne, magnitude, zero);
        // A wrapped zero with carry is still a nonzero mathematical value.
        let nonzero = self.b.ins().or(nonzero, carry);
        let negative = self.b.ins().and(negative, nonzero);
        let kind = result_ty.int_kind().unwrap();
        let bits = kind.size_with_layout(self.g.p.c_layout) * 8;
        let all = self.b.ins().iconst(wide, -1);
        let wide_bits = wide.bytes().unwrap() * 8;
        let limit = if kind.signed() {
            let shift = self
                .b
                .ins()
                .iconst(wide, i64::from(wide_bits) - bits as i64 + 1);
            let positive_max = self.b.ins().ushr(all, shift);
            let one = self.b.ins().uext(wide, negative);
            self.b.ins().iadd(positive_max, one)
        } else if bits == u64::from(wide_bits) {
            all
        } else {
            let shift = self
                .b
                .ins()
                .iconst(wide, i64::from(wide_bits) - bits as i64);
            self.b.ins().ushr(all, shift)
        };
        let too_large = self.b.ins().icmp(IntCC::Ugt, magnitude, limit);
        let overflow = self.b.ins().or(carry, too_large);
        let overflow = if kind.signed() {
            overflow
        } else {
            self.b.ins().or(overflow, negative)
        };
        let negated = self.b.ins().ineg(magnitude);
        let wrapped = self.b.ins().select(negative, negated, magnitude);
        let value = self.convert(wrapped, &wide_ty, result_ty, loc)?;
        let flags = self.object_flags(result_ty, &args[2].sugar.pointee(&self.g.p.typedefs));
        self.b.ins().store_with_flags(value, vals[2], flags);
        Ok(overflow)
    }

    /// Checked memory builtins carry a canonical C prototype even when a
    /// fortified header has not declared the runtime entry point.
    fn checked_memory_builtin(
        &mut self,
        id: Intrinsic,
        args: &[Expr<'p>],
        vals: &[Value],
        e: &Expr<'p>,
    ) -> R<Value> {
        let name = match id {
            Intrinsic::Memcpy => "__memcpy_chk",
            Intrinsic::Memmove => "__memmove_chk",
            Intrinsic::Memset => "__memset_chk",
            _ => unreachable!(),
        };
        let params: Vec<_> = args.iter().map(|arg| arg.ty).collect();
        let mut declared = None;
        // Inspect the completed translation unit, including declarations
        // after the builtin's source position. Never use an incompatible
        // ordinary declaration to choose the builtin's ABI.
        for (g, global) in self.g.p.globals.iter().enumerate() {
            if global.name != name && global.symbol != name {
                continue;
            }
            if !crate::expr::checked_memory_prototype(id, &global.ty) {
                return Err(Error::new(
                    e.loc,
                    "invalid checked memory builtin prototype",
                ));
            }
            if global.returns_twice {
                return Err(Error::new(
                    e.loc,
                    "incompatible checked memory builtin attributes: returns_twice",
                ));
            }
            declared = Some(g);
        }
        // Prove the values actually emitted, after all argument evaluation.
        // The AST evaluator represents integers as i64 and is not a sound
        // proof for expression trees containing __int128 arithmetic. A
        // canonical size_t constant in IR already has its final 64-bit bits.
        fn constant(f: &Function, v: Value, depth: u8) -> Option<u64> {
            let depth = depth.checked_sub(1)?;
            let v = f.resolve(v);
            let ValueDef::Result(i, 0) = f.value_def(v) else {
                return None;
            };
            let mask = |ty: Type| match ty {
                Type::I8 => Some(0xff),
                Type::I16 => Some(0xffff),
                Type::I32 => Some(0xffff_ffff),
                Type::I64 => Some(u64::MAX),
                _ => None,
            };
            if f.inst_results(i) != [v] {
                return None;
            }
            match f.inst_data(i) {
                InstData::Iconst { ty, imm } => Some(*imm as u64 & mask(*ty)?),
                InstData::Convert {
                    op: op @ (ConvertOp::Uext | ConvertOp::Sext | ConvertOp::Trunc),
                    ty,
                    arg,
                } => {
                    let from = f.value_type(f.resolve(*arg));
                    let from_mask = mask(from)?;
                    let mut value = constant(f, *arg, depth)? & from_mask;
                    if *op == ConvertOp::Sext {
                        let shift = 64 - from.bytes()? * 8;
                        value = (((value << shift) as i64) >> shift) as u64;
                    }
                    Some(value & mask(*ty)?)
                }
                _ => None,
            }
        }
        let bound = constant(self.b.function(), vals[3], 32);
        let length = constant(self.b.function(), vals[2], 32);
        if bound
            .is_some_and(|bound| bound == u64::MAX || length.is_some_and(|length| length <= bound))
        {
            let mut ordinary = vals[..3].to_vec();
            if id == Intrinsic::Memset {
                ordinary[1] = self.b.ins().trunc(Type::I8, ordinary[1]);
            }
            self.b.ins().intrinsic(id, &ordinary);
            return Ok(vals[0]);
        }
        let sig = Signature::new(&params.iter().map(Ty::ir).collect::<Vec<_>>(), &[Type::Ptr]);
        let symbol = if let Some(g) = declared {
            self.g.use_global(g);
            self.g.p.globals[g].symbol
        } else {
            if !self.g.module.decls.iter().any(|decl| decl.name == name) {
                self.g.module.decls.push(FuncDecl {
                    name: name.to_owned(),
                    attrs: Default::default(),
                    sig: sig.clone(),
                });
            }
            name
        };
        let callee = self.b.import_function(symbol, sig);
        let call = self.b.ins().call(callee, vals);
        Ok(self.b.function().inst_results(call)[0])
    }

    fn builtin(&mut self, op: Builtin, args: &[Expr<'p>], e: &Expr<'p>) -> R<Option<Value>> {
        // The expectation is usually a literal, with nothing to evaluate.
        if op == Builtin::Hint {
            let v = self.val(&args[0])?;
            for a in &args[1..] {
                self.discard(a)?;
            }
            return Ok(Some(v));
        }
        // C lvalue conversions and compound operators synthesize these builtins
        // without passing through the explicit builtin parser's alignment gate.
        if matches!(
            op,
            Builtin::AtomicLoad
                | Builtin::AtomicStore
                | Builtin::AtomicRmw { .. }
                | Builtin::AtomicFloatRmw { .. }
        ) && let ExprKind::Addr(obj) = args[0].kind
        {
            let align = match obj.kind {
                ExprKind::Local(id) | ExprKind::CompoundLit(id, _) => self.def.locals[id].align,
                ExprKind::Global(id) => self.g.p.globals[id].align,
                ExprKind::Member { align, .. } => align,
                _ => self.g.p.type_alignment(&obj.ty, &obj.sugar),
            };
            if self
                .g
                .types()
                .atomic_size(obj.ty.unatomic(), align)
                .is_none()
            {
                return Err(Error::new(e.loc, "atomic object is not naturally aligned"));
            }
        }
        let mut vals = Vec::new();
        for a in args.iter() {
            vals.push(self.val(a)?);
        }
        if let Builtin::Memory(id) = op
            && args.len() == 4
        {
            return self.checked_memory_builtin(id, args, &vals, e).map(Some);
        }
        if let Builtin::AtomicGeneric { kind, size } = op {
            let ty = match size {
                1 => Type::I8,
                2 => Type::I16,
                4 => Type::I32,
                _ => Type::I64,
            };
            let order_index = if kind == GenericAtomicOp::CompareExchange {
                4
            } else {
                args.len() - 1
            };
            let order = if kind == GenericAtomicOp::CompareExchange
                && crate::eval::eval_int(&args[5]).is_none()
            {
                5
            } else {
                crate::eval::eval_int(&args[order_index]).unwrap_or(5)
            };
            let ordering = atomic_order(
                order,
                kind == GenericAtomicOp::Load,
                kind == GenericAtomicOp::Store,
            );
            let flags = |i: usize| MemoryFlags {
                volatile: self.g.object_volatile(
                    args[i].ty.pointee().unwrap(),
                    &args[i].sugar.pointee(&self.g.p.typedefs),
                ),
                ..MemoryFlags::default()
            };
            let expected_flags = flags(1);
            let desired_flags = flags(if kind == GenericAtomicOp::CompareExchange {
                2
            } else {
                1
            });
            match kind {
                GenericAtomicOp::Load => {
                    let value = self.b.ins().atomic_load(ty, ordering, vals[0]);
                    self.b
                        .ins()
                        .store_with_flags(value, vals[1], expected_flags);
                }
                GenericAtomicOp::Store => {
                    let value = self.b.ins().load_with_flags(ty, vals[1], desired_flags);
                    self.b.ins().atomic_store(ordering, value, vals[0]);
                }
                GenericAtomicOp::Exchange => {
                    let value = self.b.ins().load_with_flags(ty, vals[1], desired_flags);
                    let old = self
                        .b
                        .ins()
                        .atomic_rmw(RmwOp::Xchg, ordering, vals[0], value);
                    let flags = MemoryFlags {
                        volatile: self.g.object_volatile(
                            args[2].ty.pointee().unwrap(),
                            &args[2].sugar.pointee(&self.g.p.typedefs),
                        ),
                        ..MemoryFlags::default()
                    };
                    self.b.ins().store_with_flags(old, vals[2], flags);
                }
                GenericAtomicOp::CompareExchange => {
                    let expected = self.b.ins().load_with_flags(ty, vals[1], expected_flags);
                    let desired = self.b.ins().load_with_flags(ty, vals[2], desired_flags);
                    let old = self
                        .b
                        .ins()
                        .atomic_cas(ordering, vals[0], expected, desired);
                    return Ok(Some(self.atomic_expected(
                        old,
                        expected,
                        vals[1],
                        expected_flags,
                    )));
                }
            }
            return Ok(None);
        }
        if let Builtin::AtomicFloatRmw { op, new } = op {
            let ty = e.ty;
            let bits = if ty == Ty::Float {
                Type::I32
            } else {
                Type::I64
            };
            let first = self.b.ins().atomic_load(bits, Ordering::SeqCst, vals[0]);
            let retry = self.new_block();
            let done = self.new_block();
            let expected = self.b.append_block_param(retry, bits);
            self.b.ins().jump(retry, &[first]);
            self.switch_to(retry);
            let old = self.b.ins().convert(
                ConvertOp::Bitcast,
                ty.ir_with_layout(self.g.p.c_layout),
                expected,
            );
            let x = self.convert(old, &ty, &args[1].ty, e.loc)?;
            let value = self.arith(op, &args[1].ty, x, vals[1], e.loc)?;
            let value = self.convert(value, &args[1].ty, &ty, e.loc)?;
            let desired = self.b.ins().convert(ConvertOp::Bitcast, bits, value);
            let found = self
                .b
                .ins()
                .atomic_cas(Ordering::SeqCst, vals[0], expected, desired);
            let success = self.b.ins().icmp(IntCC::Eq, found, expected);
            self.b.ins().brif(success, done, &[], retry, &[found]);
            self.b.seal_block(retry);
            self.b.seal_block(done);
            self.switch_to(done);
            return Ok(Some(if new { value } else { old }));
        }
        let ins = self.b.ins();
        Ok(Some(match op {
            Builtin::AtomicGeneric { .. } | Builtin::AtomicFloatRmw { .. } => unreachable!(),
            Builtin::Memory(id) => {
                // All arguments were evaluated once above, including at
                // length zero. Copy/fill intrinsics return dst in C; a
                // comparison builtin returns the intrinsic's integer result.
                if id == Intrinsic::Memset {
                    vals[1] = ins.trunc(Type::I8, vals[1]);
                }
                let inst = self.b.ins().intrinsic(id, &vals);
                if id.info().returns.is_empty() {
                    vals[0]
                } else {
                    self.b.function().inst_results(inst)[0]
                }
            }
            Builtin::Pclmulqdq(control) => {
                let mode = ins.iconst(Type::I32, i64::from(control));
                let inst = self
                    .b
                    .ins()
                    .intrinsic(Intrinsic::X86Pclmulqdq128, &[vals[0], vals[1], mode]);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::X86Aes(id)
            | Builtin::X86PackedShift(id)
            | Builtin::X86Saturating(id)
            | Builtin::X86PackedConversion(id) => {
                let inst = ins.intrinsic(id, &vals);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::VectorFloatCompare(cc) => {
                let mask = ins.vfcmp(cc, vals[0], vals[1]);
                self.b.ins().convert(
                    ConvertOp::Bitcast,
                    e.ty.ir_with_layout(self.g.p.c_layout),
                    mask,
                )
            }
            Builtin::VectorSplat => ins.vsplat(e.ty.ir_with_layout(self.g.p.c_layout), vals[0]),
            Builtin::VectorShuffle => ins.vshuffle(
                vals[0],
                vals[1],
                args[2..]
                    .iter()
                    .map(|e| crate::eval::eval_int(e).unwrap() as u8)
                    .collect(),
            ),
            Builtin::Complex => {
                let Ty::Complex(elem) = e.ty else {
                    unreachable!()
                };
                self.complex_pair(vals[0], vals[1], elem)
            }
            Builtin::Bswap => ins.bswap(vals[0]),
            Builtin::Fabs => ins.fabs(vals[0]),
            Builtin::Overflow(op) => {
                let (value, overflow) = ins.overflow(op, vals[0], vals[1]);
                let flags = self.object_flags(
                    args[2].ty.pointee().unwrap(),
                    &args[2].sugar.pointee(&self.g.p.typedefs),
                );
                self.b.ins().store_with_flags(value, vals[2], flags);
                overflow
            }
            Builtin::GenericOverflow(op) => self.generic_overflow(op, args, &vals, e.loc)?,
            Builtin::Hint => vals[0],
            Builtin::AssumeAligned(alignment) => {
                // Keep the contract local to this expression's SSA result.
                // Reusing vals[0] would leak a branch-local assumption to
                // unrelated uses of the original pointer.
                let zero = ins.iconst(Type::I64, 0);
                let result = self.b.ins().ptradd(vals[0], zero);
                let offset = args.get(1).and_then(crate::eval::eval_int);
                let align = match (args.len(), offset) {
                    (1, _) => alignment,
                    (_, Some(offset)) => {
                        alignment.min(1u64 << (offset as u64).trailing_zeros().min(63))
                    }
                    _ => 1,
                };
                self.alignments.insert(result, align.min(1 << 31) as u32);
                result
            }
            Builtin::Signbit => {
                let (int, bits) = match args[0].ty {
                    Ty::Float => (Type::I32, 31),
                    _ => (Type::I64, 63),
                };
                let x = ins.convert(ConvertOp::Bitcast, int, vals[0]);
                let shift = self.b.ins().iconst(int, bits);
                let sign = self.b.ins().binary(BinaryOp::Ushr, x, shift);
                if int == Type::I64 {
                    self.b.ins().trunc(Type::I32, sign)
                } else {
                    sign
                }
            }
            Builtin::Clz | Builtin::Ctz | Builtin::Popcount => {
                let r = match op {
                    Builtin::Clz => ins.clz(vals[0]),
                    Builtin::Ctz => ins.ctz(vals[0]),
                    _ => ins.popcnt(vals[0]),
                };
                if self.b.function().value_type(r) == Type::I64 {
                    self.b.ins().trunc(Type::I32, r)
                } else {
                    r
                }
            }
            Builtin::Alloca => {
                if self.def.has_vm {
                    return Err(Error::new(
                        self.def.loc,
                        "alloca in a function containing VLAs is not supported yet",
                    ));
                }
                let inst = ins.intrinsic(Intrinsic::StackAlloc, &vals);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::FrameAddress => {
                let inst = ins.intrinsic(Intrinsic::FrameAddress, &[]);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::AtomicLoad
            | Builtin::AtomicStore
            | Builtin::AtomicRmw { .. }
            | Builtin::AtomicCas
            | Builtin::AtomicCasBool
            | Builtin::AtomicCompareExchange
            | Builtin::Fence => {
                // `__ATOMIC_RELAXED` 0 ... `__ATOMIC_SEQ_CST` 5; consume
                // (1) is acquire. A load cannot release nor a store
                // acquire, so those orders weaken to what applies.
                let order_arg = if op == Builtin::AtomicCompareExchange {
                    &args[3]
                } else {
                    args.last().unwrap()
                };
                let order = if op == Builtin::AtomicCompareExchange
                    && crate::eval::eval_int(&args[4]).is_none()
                {
                    5
                } else {
                    crate::eval::eval_int(order_arg).unwrap_or(5)
                };
                let load = matches!(op, Builtin::AtomicLoad);
                let store = matches!(op, Builtin::AtomicStore);
                let ordering = atomic_order(order, load, store);
                let value_ty = args[0]
                    .ty
                    .pointee()
                    .map(|t| *t.unatomic())
                    .unwrap_or(Ty::Void);
                let float = value_ty.is_float();
                let repr = if value_ty == Ty::Float {
                    Type::I32
                } else {
                    Type::I64
                };
                // Floating operands enter integer atomic IR by bitcast only.
                if float {
                    let range = match op {
                        Builtin::AtomicStore | Builtin::AtomicRmw { .. } => 1..2,
                        Builtin::AtomicCas | Builtin::AtomicCasBool => 1..3,
                        Builtin::AtomicCompareExchange => 2..3,
                        _ => 0..0,
                    };
                    for i in range {
                        vals[i] = self.b.ins().convert(ConvertOp::Bitcast, repr, vals[i]);
                    }
                }
                let ins = self.b.ins();
                match op {
                    Builtin::AtomicLoad => {
                        let v = ins.atomic_load(
                            if float {
                                repr
                            } else {
                                e.ty.ir_with_layout(self.g.p.c_layout)
                            },
                            ordering,
                            vals[0],
                        );
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                e.ty.ir_with_layout(self.g.p.c_layout),
                                v,
                            )
                        } else {
                            v
                        }
                    }
                    Builtin::AtomicStore => {
                        ins.atomic_store(ordering, vals[1], vals[0]);
                        if e.ty == Ty::Void {
                            return Ok(None);
                        }
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                value_ty.ir_with_layout(self.g.p.c_layout),
                                vals[1],
                            )
                        } else {
                            vals[1]
                        }
                    }
                    Builtin::AtomicRmw { op, new } => {
                        let old = ins.atomic_rmw(op, ordering, vals[0], vals[1]);
                        if !new {
                            return Ok(Some(if float {
                                self.b.ins().convert(
                                    ConvertOp::Bitcast,
                                    value_ty.ir_with_layout(self.g.p.c_layout),
                                    old,
                                )
                            } else {
                                old
                            }));
                        }
                        let bin = match op {
                            RmwOp::Add => BinaryOp::Iadd,
                            RmwOp::Sub => BinaryOp::Isub,
                            RmwOp::And => BinaryOp::And,
                            RmwOp::Or => BinaryOp::Or,
                            RmwOp::Xor => BinaryOp::Xor,
                            _ => unreachable!("the parser only asks for these"),
                        };
                        self.b.ins().binary(bin, old, vals[1])
                    }
                    Builtin::AtomicCas => {
                        let v = ins.atomic_cas(ordering, vals[0], vals[1], vals[2]);
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                value_ty.ir_with_layout(self.g.p.c_layout),
                                v,
                            )
                        } else {
                            v
                        }
                    }
                    Builtin::AtomicCasBool => {
                        let old = ins.atomic_cas(ordering, vals[0], vals[1], vals[2]);
                        self.b.ins().icmp(IntCC::Eq, old, vals[1])
                    }
                    Builtin::AtomicCompareExchange => {
                        let ty = if float {
                            repr
                        } else {
                            args[2].ty.ir_with_layout(self.g.p.c_layout)
                        };
                        let flags = MemoryFlags {
                            volatile: args[1]
                                .sugar
                                .pointee(&self.g.p.typedefs)
                                .is_volatile(&self.g.p.typedefs),
                            ..MemoryFlags::default()
                        };
                        let expected = ins.load_with_flags(ty, vals[1], flags);
                        let old = self
                            .b
                            .ins()
                            .atomic_cas(ordering, vals[0], expected, vals[2]);
                        self.atomic_expected(old, expected, vals[1], flags)
                    }
                    _ => {
                        if order != 0 {
                            ins.fence(ordering);
                        }
                        return Ok(None);
                    }
                }
            }
            Builtin::Debugtrap => {
                ins.intrinsic(Intrinsic::Debugtrap, &[]);
                return Ok(None);
            }
            Builtin::Trap => {
                ins.trap();
                self.terminated = true;
                self.ensure_block();
                return Ok(None);
            }
        }))
    }

    /// The expected buffer is written only on failure (observable for volatile).
    fn atomic_expected(
        &mut self,
        old: Value,
        expected: Value,
        ptr: Value,
        flags: MemoryFlags,
    ) -> Value {
        let success = self.b.ins().icmp(IntCC::Eq, old, expected);
        let fail = self.new_block();
        let done = self.new_block();
        self.b.ins().brif(success, done, &[], fail, &[]);
        self.b.seal_block(fail);
        self.switch_to(fail);
        self.b.ins().store_with_flags(old, ptr, flags);
        self.jump_to(done);
        self.b.seal_block(done);
        self.switch_to(done);
        success
    }

    // ---- memory ----

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
        assert!(g.global_alignments.is_none());
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
        g.preserve_access_alignment(&mut f, HashMap::default());
        keel_ir::verify(&f).unwrap();
        let alignments: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, inst)| match f.inst_data(inst) {
                InstData::Load { flags, .. } => Some(flags.align),
                _ => None,
            })
            .collect();
        assert_eq!(alignments, [16, 8, 8, 1, 2]);
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
        g.preserve_access_alignment(&mut f, seeds);
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

// CAS IR has one ordering: the success order also supplies a legally stronger
// failure order. Runtime orders conservatively select seq_cst.
fn atomic_order(order: i64, load: bool, store: bool) -> Ordering {
    match order {
        0 => Ordering::Relaxed,
        1 | 2 if store => Ordering::Relaxed,
        1 | 2 => Ordering::Acquire,
        3 if load => Ordering::Relaxed,
        3 => Ordering::Release,
        4 if load => Ordering::Acquire,
        4 if store => Ordering::Release,
        4 => Ordering::AcqRel,
        _ => Ordering::SeqCst,
    }
}
