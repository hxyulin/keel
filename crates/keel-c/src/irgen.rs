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
use keel_frontend::{FunctionBuilder, Variable};
use keel_ir::{
    AbiParam, ArgExt, BinaryOp, Block, ConvertOp, DataInit, DataItem, DataObject, DebugGlobal,
    DebugScope, DebugType, DebugVar, EntityRef, FloatCC, FuncDecl, Function, IntCC, Intrinsic,
    Language, Module, Ordering, Purpose, RmwOp, Signature, SourceFile, SourceLoc, StackSlot,
    Symbol, Type, Value,
};
use std::collections::HashMap;

type R<T> = Result<T, Error>;

pub struct Gen<'p> {
    p: &'p Parser,
    files: &'p [String],
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
}

impl<'p> Gen<'p> {
    pub fn new(p: &'p Parser, files: &'p [String], debug: bool) -> Self {
        Self {
            p,
            files,
            module: Module::default(),
            debug,
            srcfiles: HashMap::new(),
            used: vec![false; p.globals.len()],
            worklist: Vec::new(),
            strings_used: vec![false; p.strings.len()],
            unprototyped: HashMap::new(),
            anon: HashMap::new(),
            dtypes: DebugTypes::default(),
        }
    }

    pub fn run(mut self) -> R<Module> {
        for (g, global) in self.p.globals.iter().enumerate() {
            if global.defined && !global.internal {
                self.use_global(g);
            }
        }
        // Emit in source order, then whatever those refer to.
        self.worklist.reverse();
        while let Some(g) = self.worklist.pop() {
            let global = &self.p.globals[g];
            match (&global.ty, global.func) {
                (Ty::Func(_), Some(f)) => {
                    let func = self.function(&self.p.funcs[f])?;
                    self.module.functions.push(func);
                }
                (Ty::Func(fty), None) => {
                    let sig = match self.unprototyped.get(&g) {
                        Some(sig) => sig.clone(),
                        None => self.signature(fty, g, &[])?,
                    };
                    self.module.decls.push(FuncDecl {
                        name: global.symbol.clone(),
                        sig,
                    });
                }
                _ if global.defined => {
                    let data = self.data(g)?;
                    self.module.data.push(data);
                    // Made-up names (compound literals) are not identifiers.
                    if self.debug && !global.name.contains('.') {
                        let ty = self
                            .debug_type(&global.ty, &global.sugar)
                            .expect("a void object");
                        self.module.debug_globals.push(DebugGlobal {
                            name: global.name.clone(),
                            ty,
                            symbol: global.symbol.clone(),
                        });
                    }
                }
                _ => self.module.data.push(DataObject {
                    name: global.symbol.clone(),
                    thread_local: global.thread_local,
                    local: false,
                    init: None,
                }),
            }
        }
        if self.debug {
            self.module.debug_language = Some(Language::C11);
        }
        Ok(self.module)
    }

    fn use_global(&mut self, g: usize) {
        if !self.used[g] {
            self.used[g] = true;
            self.worklist.push(g);
        }
    }

    fn string_symbol(&mut self, i: usize) -> String {
        let name = format!(".str.{i}");
        if !self.strings_used[i] {
            self.strings_used[i] = true;
            let s = &self.p.strings[i];
            self.module.data.push(DataObject {
                name: name.clone(),
                thread_local: false,
                local: true,
                init: Some(DataInit {
                    constant: true,
                    align: if s.wide { 4 } else { 1 },
                    items: vec![DataItem::Bytes(s.bytes.clone())],
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
        self.module.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
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
                let f = self.module.add_file(self.files[loc.file as usize].clone());
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

    /// How a value of C type `ty` crosses a call; the name of the type if
    /// keel cannot pass it.
    fn abi_param(&self, ty: &Ty) -> Result<AbiParam, &'static str> {
        if let Some(name) = ty.unusable() {
            return Err(name);
        }
        Ok(match ty {
            Ty::Struct(_) => AbiParam {
                ty: Type::Ptr,
                ext: ArgExt::None,
                purpose: Purpose::ByVal(self.aggregate_layout(ty)?),
            },
            Ty::Int(k) if k.size() < 4 => AbiParam {
                ty: ty.ir(),
                ext: if k.signed() {
                    ArgExt::Sext
                } else {
                    ArgExt::Uext
                },
                purpose: Purpose::Normal,
            },
            _ => AbiParam::new(ty.ir()),
        })
    }

    fn aggregate_layout(&self, ty: &Ty) -> Result<keel_ir::Layout, &'static str> {
        if self.types().align(ty) > 16 {
            return Err("aggregates aligned above 16 bytes passed by value");
        }
        if self.p.target == keel_ir::Target::X86_64LinuxGnu
            && self.types().has_unaligned_fields(ty, 0)
        {
            return Err("aggregates with unaligned fields passed by value on x86-64");
        }
        Ok(self.types().abi_layout(ty))
    }

    /// The keel signature of a C function type. A struct result becomes a
    /// leading `sret` parameter. `g` is the function, if known by name.
    fn signature(&self, fty: &FuncTy, g: usize, extra: &[Ty]) -> R<Signature> {
        let mut sig = Signature::new(&[], &[]);
        if let Ty::Struct(_) = fty.ret {
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

    fn function(&mut self, def: &'p FuncDef) -> R<Function> {
        let global = &self.p.globals[def.global];
        let sig = self.signature(&def.ty, def.global, &[])?;
        let mut func = Function::new(global.symbol.clone(), sig.clone());
        func.local = global.internal;
        let mut b = FunctionBuilder::new(&mut func);
        let entry = b.create_block();
        b.seal_block(entry);
        b.switch_to_block(entry);
        let params: Vec<Value> = sig
            .params
            .iter()
            .map(|p| b.func.append_block_param(entry, p.ty))
            .collect();
        let mut fg = FnGen {
            g: self,
            b,
            def,
            locals: Vec::new(),
            sret: None,
            breaks: Vec::new(),
            continues: Vec::new(),
            labels: HashMap::new(),
            dispatch: None,
            cases: Vec::new(),
            terminated: false,
            scopes: Vec::new(),
            loc: None,
            scope: None,
        };
        for local in &def.locals {
            let storage = if !fg.g.debug && !local.in_memory && local.ty.is_scalar() {
                Storage::Var(fg.b.declare_var(local.ty.ir()))
            } else {
                let types = fg.g.types();
                let size = types.size(&local.ty).max(1);
                let align = local.align.max(types.align(&local.ty));
                let reserve = size
                    .checked_add(if align > 16 { align - 1 } else { 0 })
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| Error::new(def.loc, "aligned stack object is too large"))?;
                let slot = fg.b.func.create_stack_slot(reserve, align.min(16) as u32);
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
                    let debug_slot = fg.g.debug.then(|| {
                        let pointer = fg.b.func.create_stack_slot(8, 8);
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
            fg.b.func.debug_loc = fg.g.srcloc(def.loc);
            let ret = match &fg.g.p.globals[def.global].sugar {
                Sugar::Func(ret) => ret.as_ref().clone(),
                _ => Sugar::Plain,
            };
            fg.b.func.debug_return = fg.g.debug_type(&def.ty.ret, &ret);
            // Only scopes with variables need to be told apart.
            let named = |local: &&Local| !local.name.contains('.');
            let mut has_vars = vec![false; def.scopes.len()];
            for scope in def.locals.iter().filter(named).filter_map(|l| l.scope) {
                has_vars[scope] = true;
            }
            for (scope, parent) in def.scopes.iter().enumerate() {
                let parent = parent.and_then(|p| fg.scopes[p]);
                let own = has_vars[scope].then(|| {
                    fg.b.func.debug_scopes.push(parent);
                    DebugScope::new(fg.b.func.debug_scopes.len() - 1)
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
                    fg.b.func.debug_vars.push(DebugVar {
                        name: local.name.clone(),
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
        if def.ty.ret.is_struct() {
            fg.sret = params.next();
        }
        for (&id, value) in def.params.iter().zip(params) {
            match fg.locals[id] {
                Storage::Var(var) => fg.b.def_var(var, value),
                // The callee owns its copy of a struct passed by value.
                // The debugger needs it at a fixed place in the frame, so
                // with debug info it is copied there.
                Storage::Slot(slot) if def.locals[id].ty.is_struct() => {
                    if fg.g.debug {
                        let addr = fg.b.ins().stack_addr(slot);
                        fg.copy(addr, value, &def.locals[id].ty);
                    } else {
                        fg.locals[id] = Storage::Addr(value);
                    }
                }
                Storage::Slot(slot) => {
                    let addr = fg.b.ins().stack_addr(slot);
                    fg.b.ins().store(value, addr);
                }
                Storage::Aligned { addr, .. } => {
                    if def.locals[id].ty.is_struct() {
                        fg.copy(addr, value, &def.locals[id].ty);
                    } else {
                        fg.b.ins().store(value, addr);
                    }
                }
                Storage::Addr(_) => unreachable!(),
            }
        }
        // Storing the parameters has no location, so it is part of the
        // prologue: a breakpoint on the function stops after it, where the
        // debugger can read them.
        fg.set_loc(def.loc);
        for stmt in &def.body {
            fg.stmt(stmt)?;
        }
        if !fg.terminated {
            fg.return_default();
        }
        fg.emit_dispatch();
        fg.b.seal_all_blocks();
        fg.b.finalize();
        Ok(func)
    }

    /// A defined object: its bytes, with relocations for addresses.
    fn data(&mut self, g: usize) -> R<DataObject> {
        let global = &self.p.globals[g];
        let types = self.types();
        let size = types.size(&global.ty) as usize;
        let align = global.align.max(types.align(&global.ty)).max(1) as u32;
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
                    let Some(value) = eval(expr) else {
                        return Err(Error::new(expr.loc, "initializer is not a constant"));
                    };
                    let width = self.types().size(&expr.ty) as usize;
                    match value {
                        Const::Int(v) => match bits {
                            Some(bf) => {
                                let mask = (1u128 << bf.width) - 1;
                                let mut unit = read_le(&bytes[offset..offset + width]);
                                unit &= !(mask << bf.shift);
                                unit |= (v as u128 & mask) << bf.shift;
                                write_le(&mut bytes[offset..offset + width], unit);
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
                        Const::Float(f) => {
                            let v = if expr.ty == Ty::Float {
                                u128::from((f as f32).to_bits())
                            } else {
                                u128::from(f.to_bits())
                            };
                            write_le(&mut bytes[offset..offset + width], v);
                        }
                        Const::Addr(base, addend) => {
                            if width != 8 {
                                return Err(Error::new(expr.loc, "address in a narrow integer"));
                            }
                            let symbol = match base {
                                Base::Global(target) => {
                                    self.use_global(target);
                                    self.p.globals[target].symbol.clone()
                                }
                                Base::Str(s) => self.string_symbol(s),
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
            if !offset.is_multiple_of(8) {
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
            pos = offset + 8;
        }
        push_bytes(&mut items, &bytes[pos..]);
        if items.is_empty() {
            items.push(DataItem::Zero(1));
        }
        Ok(DataObject {
            name: global.symbol.clone(),
            thread_local: global.thread_local,
            local: global.internal,
            init: Some(DataInit {
                constant: global.is_const && !global.thread_local,
                align,
                items,
            }),
        })
    }
}

fn read_le(bytes: &[u8]) -> u128 {
    bytes
        .iter()
        .rev()
        .fold(0u128, |acc, &b| (acc << 8) | u128::from(b))
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
    Mem(Value, Type),
    Bits {
        addr: Value,
        unit: Type,
        field: BitField,
        signed: bool,
    },
    Aggregate(Value),
}

struct FnGen<'g, 'p, 'f> {
    g: &'g mut Gen<'p>,
    b: FunctionBuilder<'f>,
    def: &'p FuncDef,
    locals: Vec<Storage>,
    sret: Option<Value>,
    breaks: Vec<Block>,
    continues: Vec<Block>,
    labels: HashMap<String, Block>,
    /// All computed gotos share one switch, with the label number as its
    /// first parameter. Leave it unsealed until every dispatch edge exists.
    dispatch: Option<Block>,
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
}

impl FnGen<'_, '_, '_> {
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

    fn label_block(&mut self, name: &str) -> Block {
        if let Some(&b) = self.labels.get(name) {
            return b;
        }
        let b = self.new_block();
        self.labels.insert(name.to_string(), b);
        b
    }

    fn zero(&mut self, ty: Type) -> Value {
        match ty {
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
    fn return_default(&mut self) {
        let ret = &self.def.ty.ret;
        if *ret == Ty::Void || ret.is_struct() {
            self.b.ins().return_(&[]);
        } else {
            let z = self.zero(ret.ir());
            self.b.ins().return_(&[z]);
        }
        self.terminated = true;
    }

    // ---- statements ----

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Expr(e) => {
                self.ensure_block();
                self.set_loc(e.loc);
                self.emit(e)?;
            }
            Stmt::Init(id, init, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                self.init_local(*id, init)?;
            }
            // Without this, a variable read on a path that did not assign it
            // in this iteration would read the last iteration's value: a
            // block parameter live around the whole loop. Zero starts a new
            // value here instead.
            Stmt::Declare(id) => {
                if let Storage::Var(var) = self.locals[*id] {
                    self.ensure_block();
                    let zero = self.zero(self.def.locals[*id].ty.ir());
                    self.b.def_var(var, zero);
                }
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.stmt(s)?;
                }
            }
            Stmt::Scoped(scope, stmts) => {
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts {
                    self.stmt(s)?;
                }
                self.set_scope(outer);
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
                self.breaks.push(exit);
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
            Stmt::Case(label) => {
                let block = self.cases.last().unwrap()[*label];
                self.jump_to(block);
                self.switch_to(block);
            }
            Stmt::Break => {
                self.ensure_block();
                let target = *self.breaks.last().unwrap();
                self.jump_to(target);
            }
            Stmt::Continue => {
                self.ensure_block();
                let target = *self.continues.last().unwrap();
                self.jump_to(target);
            }
            Stmt::Return(value, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                match value {
                    None => self.return_default(),
                    Some(e) => {
                        if let Some(sret) = self.sret {
                            let src = self.val(e)?;
                            let ty = e.ty.clone();
                            self.copy(sret, src, &ty);
                            self.b.ins().return_(&[]);
                        } else {
                            let v = self.val(e)?;
                            self.b.ins().return_(&[v]);
                        }
                        self.terminated = true;
                    }
                }
            }
            Stmt::Goto(label) => {
                self.ensure_block();
                let block = self.label_block(label);
                self.jump_to(block);
            }
            Stmt::GotoIndirect(target) => {
                self.ensure_block();
                self.set_loc(target.loc);
                let p = self.val(target)?;
                let n = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, p);
                let dispatch = match self.dispatch {
                    Some(block) => block,
                    None => {
                        let block = self.new_block();
                        self.b.func.append_block_param(block, Type::I64);
                        self.dispatch = Some(block);
                        block
                    }
                };
                self.b.ins().jump(dispatch, &[n]);
                self.terminated = true;
            }
            Stmt::Label(label) => {
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
        let n = self.b.func.block_params(dispatch)[0];
        let call = |block| keel_ir::BlockCall {
            block,
            args: Vec::new(),
        };
        let table = self
            .def
            .label_addrs
            .iter()
            .enumerate()
            .map(|(i, label)| (i as i64 + 1, call(self.label_block(label))))
            .collect();
        // A value other than an address-taken label is undefined.
        let nowhere = self.new_block();
        self.b.ins().switch(n, call(nowhere), table);
        self.b.seal_block(nowhere);
        self.switch_to(nowhere);
        self.b.ins().trap();
        self.terminated = true;
    }

    fn loop_body(&mut self, body: &Stmt, block: Block, exit: Block, cont: Block) -> R<()> {
        self.switch_to(block);
        self.breaks.push(exit);
        self.continues.push(cont);
        self.stmt(body)?;
        self.breaks.pop();
        self.continues.pop();
        Ok(())
    }

    /// Branches to `t` if `cond` is true (non-zero), else to `f`, without
    /// materializing `&&`, `||` and `!` as values.
    fn branch(&mut self, cond: &Expr, t: Block, f: Block) -> R<()> {
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
    fn truth(&mut self, e: &Expr) -> R<Value> {
        if let ExprKind::Binary(op, a, b) = &e.kind
            && op.is_comparison()
        {
            return self.compare(*op, a, b);
        }
        let v = self.val(e)?;
        Ok(match &e.ty {
            Ty::Int(k) if k.size() <= 8 => v,
            ty => self.nonzero(v, ty),
        })
    }

    /// `v != 0` as an `i8`.
    fn nonzero(&mut self, v: Value, ty: &Ty) -> Value {
        let z = self.zero(ty.ir());
        if ty.is_float() {
            self.b.ins().fcmp(FloatCC::Ne, v, z)
        } else {
            self.b.ins().icmp(IntCC::Ne, v, z)
        }
    }

    // ---- expressions ----

    /// The value of a non-void expression; for aggregates, their address.
    fn val(&mut self, e: &Expr) -> R<Value> {
        match self.emit(e)? {
            Some(v) => Ok(v),
            None => Err(Error::new(e.loc, "a void expression has no value")),
        }
    }

    fn is_aggregate(ty: &Ty) -> bool {
        matches!(ty, Ty::Struct(_) | Ty::Array(..) | Ty::Func(_))
    }

    fn emit(&mut self, e: &Expr) -> R<Option<Value>> {
        let v = match &e.kind {
            ExprKind::LabelAddr(n) => {
                let n = self.b.ins().iconst(Type::I64, *n as i64);
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            ExprKind::Int(v) => match &e.ty {
                Ty::Ptr(_) => {
                    let n = self.b.ins().iconst(Type::I64, *v);
                    self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
                }
                ty => self.b.ins().iconst(ty.ir(), *v),
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
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Var(var) => self.b.use_var(var),
                _ => {
                    let place = self.place(e)?.unwrap();
                    self.load_place(&place)
                }
            },
            ExprKind::Unary(op, a) => {
                let v = self.val(a)?;
                match op {
                    UnOp::Neg if a.ty.is_float() => self.b.ins().fneg(v),
                    UnOp::Neg => self.b.ins().ineg(v),
                    UnOp::BitNot => self.b.ins().bnot(v),
                    UnOp::Not => {
                        let z = self.zero(a.ty.ir());
                        let c = if a.ty.is_float() {
                            self.b.ins().fcmp(FloatCC::Eq, v, z)
                        } else {
                            self.b.ins().icmp(IntCC::Eq, v, z)
                        };
                        self.b.ins().uext(Type::I32, c)
                    }
                }
            }
            ExprKind::Binary(op, a, b) if op.is_comparison() => {
                let c = self.compare(*op, a, b)?;
                self.b.ins().uext(Type::I32, c)
            }
            ExprKind::Binary(op, a, b) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                self.arith(*op, &e.ty, x, y, e.loc)?
            }
            ExprKind::PtrAdd(p, n, size) => {
                let base = self.val(p)?;
                let offset = self.scaled(n, *size)?;
                self.b.ins().ptradd(base, offset)
            }
            ExprKind::PtrDiff(a, b, size) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                let x = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, x);
                let y = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, y);
                let d = self.b.ins().isub(x, y);
                // The difference is an exact multiple of the size.
                if *size <= 1 {
                    d
                } else if size.is_power_of_two() {
                    let s = self.b.ins().iconst(Type::I64, size.trailing_zeros() as i64);
                    self.b.ins().sshr(d, s)
                } else {
                    let s = self.b.ins().iconst(Type::I64, *size as i64);
                    self.b.ins().sdiv(d, s)
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
                self.store_place(&place, v, &lhs.ty);
                match place {
                    Place::Aggregate(addr) => addr,
                    _ => v,
                }
            }
            ExprKind::OpAssign { op, ty, lhs, rhs } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let old = self.convert(old, &lhs.ty, ty, e.loc)?;
                let r = self.val(rhs)?;
                let new = self.arith(*op, ty, old, r, e.loc)?;
                let new = self.convert(new, ty, &lhs.ty, e.loc)?;
                self.store_place(&place, new, &lhs.ty);
                new
            }
            ExprKind::PtrOpAssign { lhs, rhs, size } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let offset = self.scaled(rhs, *size)?;
                let new = self.b.ins().ptradd(old, offset);
                self.store_place(&place, new, &lhs.ty);
                new
            }
            ExprKind::IncDec { lhs, inc, prefix } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let new = match &lhs.ty {
                    Ty::Ptr(_) => {
                        let size = self.size(lhs.ty.pointee().unwrap()).max(1) as i64;
                        let d = self
                            .b
                            .ins()
                            .iconst(Type::I64, if *inc { size } else { -size });
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
                        let one = self.b.ins().iconst(ty.ir(), 1);
                        if *inc {
                            self.b.ins().iadd(old, one)
                        } else {
                            self.b.ins().isub(old, one)
                        }
                    }
                };
                self.store_place(&place, new, &lhs.ty);
                if *prefix { new } else { old }
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
                    e.ty.ir()
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
                self.emit(a)?;
                return self.emit(b);
            }
            ExprKind::StmtExpr {
                scope,
                stmts,
                value,
            } => {
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts {
                    self.stmt(s)?;
                }
                let v = match value {
                    Some(v) => {
                        self.ensure_block();
                        self.emit(v)?
                    }
                    None => None,
                };
                self.set_scope(outer);
                return Ok(v);
            }
            ExprKind::Call(f, args) => return self.call(f, args, e),
            ExprKind::Cast(inner) => {
                let v = self.emit(inner)?;
                if e.ty == Ty::Void {
                    return Ok(None);
                }
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
                    Ty::Int(k) if k.size() < 4 => {
                        let v = self.b.ins().va_arg(Type::I32, addr);
                        self.b.ins().trunc(e.ty.ir(), v)
                    }
                    Ty::Int(k) if k.size() == 16 => {
                        return Err(Error::new(e.loc, "`va_arg` of `__int128`"));
                    }
                    Ty::Float => {
                        let v = self.b.ins().va_arg(Type::F64, addr);
                        self.b.ins().convert(ConvertOp::Fptrunc, Type::F32, v)
                    }
                    ty => self.b.ins().va_arg(ty.ir(), addr),
                }
            }
            ExprKind::Builtin(op, args) => return self.builtin(*op, args, e),
        };
        Ok(Some(v))
    }

    /// `n * size` as an `i64`, for pointer arithmetic.
    fn scaled(&mut self, n: &Expr, size: u64) -> R<Value> {
        if let ExprKind::Int(k) = n.kind {
            return Ok(self.b.ins().iconst(Type::I64, k.wrapping_mul(size as i64)));
        }
        let v = self.val(n)?;
        if size == 1 {
            return Ok(v);
        }
        let s = self.b.ins().iconst(Type::I64, size as i64);
        Ok(self.b.ins().imul(v, s))
    }

    fn compare(&mut self, op: BinOp, a: &Expr, b: &Expr) -> R<Value> {
        let (x, y) = (self.val(a)?, self.val(b)?);
        let ty = &a.ty;
        if ty.is_float() {
            let cc = match op {
                BinOp::Eq => FloatCC::Eq,
                BinOp::Ne => FloatCC::Ne,
                BinOp::Lt => FloatCC::Lt,
                BinOp::Le => FloatCC::Le,
                BinOp::Gt => FloatCC::Gt,
                _ => FloatCC::Ge,
            };
            return Ok(self.b.ins().fcmp(cc, x, y));
        }
        let signed = ty.is_signed();
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
        Ok(self.b.ins().icmp(cc, x, y))
    }

    fn arith(&mut self, op: BinOp, ty: &Ty, x: Value, y: Value, loc: Loc) -> R<Value> {
        let ins = self.b.ins();
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
        let ins = self.b.ins();
        let (fi, ti) = (from.ir(), to.ir());
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
                if k.size() == 16 {
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
                if k.size() == 16 {
                    return Err(Error::new(loc, "float to `__int128` is not supported"));
                }
                let op = if k.signed() {
                    ConvertOp::Fptosi
                } else {
                    ConvertOp::Fptoui
                };
                let wide = if k.size() == 8 { Type::I64 } else { Type::I32 };
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
                let n = match k.size() {
                    8 => v,
                    16 => ins.trunc(Type::I64, v),
                    _ if k.signed() => ins.sext(Type::I64, v),
                    _ => ins.uext(Type::I64, v),
                };
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            _ => return Err(Error::new(loc, "unsupported conversion")),
        })
    }

    // ---- places ----

    /// Where an lvalue lives. `None` for a void-typed dereference.
    fn place(&mut self, e: &Expr) -> R<Option<Place>> {
        if e.ty == Ty::Void {
            return Ok(None);
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
            let base = self.addr(base)?;
            let addr = self.offset(base, *offset);
            return Ok(Some(Place::Bits {
                addr,
                unit: e.ty.ir(),
                field: *field,
                signed: e.ty.is_signed(),
            }));
        }
        let addr = self.addr(e)?;
        Ok(Some(if Self::is_aggregate(&e.ty) {
            Place::Aggregate(addr)
        } else {
            if let Some(name) = e.ty.unusable() {
                return Err(Error::new(e.loc, format!("`{name}` is not supported")));
            }
            Place::Mem(addr, e.ty.ir())
        }))
    }

    fn load_place(&mut self, place: &Place) -> Value {
        match *place {
            Place::Var(var) => self.b.use_var(var),
            Place::Mem(addr, ty) => self.b.ins().load(ty, addr),
            Place::Aggregate(addr) => addr,
            Place::Bits {
                addr,
                unit,
                field,
                signed,
            } => {
                let bits = i64::from(unit.bytes().unwrap() * 8);
                let word = self.b.ins().load(unit, addr);
                let left = bits - i64::from(field.shift) - i64::from(field.width);
                let l = self.b.ins().iconst(unit, left);
                let up = self.b.ins().shl(word, l);
                let r = self.b.ins().iconst(unit, bits - i64::from(field.width));
                if signed {
                    self.b.ins().sshr(up, r)
                } else {
                    self.b.ins().ushr(up, r)
                }
            }
        }
    }

    fn store_place(&mut self, place: &Place, v: Value, ty: &Ty) {
        match *place {
            Place::Var(var) => self.b.def_var(var, v),
            Place::Mem(addr, _) => {
                self.b.ins().store(v, addr);
            }
            Place::Aggregate(addr) => self.copy(addr, v, ty),
            Place::Bits {
                addr, unit, field, ..
            } => {
                let mask = ((1u128 << field.width) - 1) << field.shift;
                let word = self.b.ins().load(unit, addr);
                let keep = self.b.ins().iconst(unit, !mask as i64);
                let kept = self.b.ins().and(word, keep);
                let shift = self.b.ins().iconst(unit, i64::from(field.shift));
                let moved = self.b.ins().shl(v, shift);
                let m = self.b.ins().iconst(unit, mask as i64);
                let new = self.b.ins().and(moved, m);
                let merged = self.b.ins().or(kept, new);
                self.b.ins().store(merged, addr);
            }
        }
    }

    fn offset(&mut self, base: Value, offset: u64) -> Value {
        if offset == 0 {
            return base;
        }
        let n = self.b.ins().iconst(Type::I64, offset as i64);
        self.b.ins().ptradd(base, n)
    }

    /// The address of an lvalue (or of an aggregate rvalue's bytes).
    fn addr(&mut self, e: &Expr) -> R<Value> {
        Ok(match &e.kind {
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Slot(slot) => self.b.ins().stack_addr(slot),
                Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
                Storage::Var(_) => {
                    return Err(Error::new(e.loc, "internal error: address of a register"));
                }
            },
            ExprKind::Global(g) => {
                self.g.use_global(*g);
                let global = &self.g.p.globals[*g];
                let symbol = if let Ty::Func(fty) = &global.ty {
                    let sig = self.callee_signature(*g, fty, &[])?;
                    Symbol::Func(self.b.func.import_function(&global.symbol, sig))
                } else {
                    Symbol::Data(self.b.func.import_data(&global.symbol))
                };
                self.b.ins().symbol_addr(symbol)
            }
            ExprKind::Str(i) => {
                let name = self.g.string_symbol(*i);
                let data = self.b.func.import_data(&name);
                self.b.ins().symbol_addr(Symbol::Data(data))
            }
            ExprKind::Deref(p) => self.val(p)?,
            ExprKind::Member { base, offset, .. } => {
                let base = self.addr(base)?;
                self.offset(base, *offset)
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
    fn callee_signature(&mut self, g: usize, fty: &FuncTy, args: &[Ty]) -> R<Signature> {
        if fty.prototyped || self.g.p.globals[g].func.is_some() {
            return self.g.signature(fty, g, &[]);
        }
        if let Some(sig) = self.g.unprototyped.get(&g) {
            return Ok(sig.clone());
        }
        let sig = self.g.signature(fty, g, args)?;
        if !args.is_empty() {
            self.g.unprototyped.insert(g, sig.clone());
        }
        Ok(sig)
    }

    fn call(&mut self, f: &Expr, args: &[Expr], e: &Expr) -> R<Option<Value>> {
        let fty = f.ty.pointee().and_then(Ty::func).unwrap().clone();
        self.set_loc(e.loc);
        let mut vals = Vec::new();
        let ret_slot = if fty.ret.is_struct() {
            let (size, align) = (self.size(&fty.ret), self.g.types().align(&fty.ret));
            let slot = self
                .b
                .func
                .create_stack_slot(size.max(1) as u32, align as u32);
            vals.push(self.b.ins().stack_addr(slot));
            Some(slot)
        } else {
            None
        };
        for arg in args {
            if !fty.prototyped && arg.ty.is_struct() {
                return Err(Error::new(arg.loc, "struct argument without a prototype"));
            }
            if vals.len() >= fty.params.len() + usize::from(ret_slot.is_some())
                && (arg.ty.is_struct() || arg.ty == Ty::Int(IntKind::Int128))
            {
                return Err(Error::new(
                    arg.loc,
                    "this variadic argument type is not supported",
                ));
            }
            vals.push(self.val(arg)?);
        }
        let extra: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
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
                self.g.use_global(g);
                let sig = self.callee_signature(g, &fty, &extra)?;
                let name = self.g.p.globals[g].symbol.clone();
                let fref = self.b.func.import_function(&name, sig);
                self.b.ins().call(fref, &vals)
            }
            None => {
                let callee = self.val(f)?;
                let extra = if fty.prototyped { &[][..] } else { &extra[..] };
                let sig = self.g.signature(&fty, usize::MAX, extra)?;
                let sig = self.b.func.import_signature(sig);
                self.b.ins().call_indirect(sig, callee, &vals)
            }
        };
        if let Some(slot) = ret_slot {
            return Ok(Some(self.b.ins().stack_addr(slot)));
        }
        Ok(self.b.func.inst_results(inst).first().copied())
    }

    fn builtin(&mut self, op: Builtin, args: &[Expr], e: &Expr) -> R<Option<Value>> {
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.val(a)?);
        }
        let ins = self.b.ins();
        Ok(Some(match op {
            Builtin::Bswap => ins.bswap(vals[0]),
            Builtin::Fabs => ins.fabs(vals[0]),
            Builtin::Overflow(op) => {
                let (value, overflow) = ins.overflow(op, vals[0], vals[1]);
                self.b.ins().store(value, vals[2]);
                overflow
            }
            Builtin::Hint => vals[0],
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
                if self.b.func.value_type(r) == Type::I64 {
                    self.b.ins().trunc(Type::I32, r)
                } else {
                    r
                }
            }
            Builtin::Alloca => {
                let inst = ins.intrinsic(Intrinsic::StackAlloc, &vals);
                self.b.func.inst_results(inst)[0]
            }
            Builtin::FrameAddress => {
                let inst = ins.intrinsic(Intrinsic::FrameAddress, &[]);
                self.b.func.inst_results(inst)[0]
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
                let order = crate::eval::eval_int(args.last().unwrap()).unwrap_or(5);
                let load = matches!(op, Builtin::AtomicLoad);
                let store = matches!(op, Builtin::AtomicStore);
                let ordering = match order {
                    0 => Ordering::Relaxed,
                    1 | 2 if store => Ordering::Relaxed,
                    1 | 2 => Ordering::Acquire,
                    3 if load => Ordering::Relaxed,
                    3 => Ordering::Release,
                    4 if load => Ordering::Acquire,
                    4 if store => Ordering::Release,
                    4 => Ordering::AcqRel,
                    _ => Ordering::SeqCst,
                };
                match op {
                    Builtin::AtomicLoad => ins.atomic_load(e.ty.ir(), ordering, vals[0]),
                    Builtin::AtomicStore => {
                        ins.atomic_store(ordering, vals[1], vals[0]);
                        if e.ty == Ty::Void {
                            return Ok(None);
                        }
                        vals[1]
                    }
                    Builtin::AtomicRmw { op, new } => {
                        let old = ins.atomic_rmw(op, ordering, vals[0], vals[1]);
                        if !new {
                            return Ok(Some(old));
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
                    Builtin::AtomicCas => ins.atomic_cas(ordering, vals[0], vals[1], vals[2]),
                    Builtin::AtomicCasBool => {
                        let old = ins.atomic_cas(ordering, vals[0], vals[1], vals[2]);
                        self.b.ins().icmp(IntCC::Eq, old, vals[1])
                    }
                    Builtin::AtomicCompareExchange => {
                        let ty = args[2].ty.ir();
                        let expected = ins.load(ty, vals[1]);
                        let old = self
                            .b
                            .ins()
                            .atomic_cas(ordering, vals[0], expected, vals[2]);
                        // Unconditionally: on success it is the same value.
                        self.b.ins().store(old, vals[1]);
                        self.b.ins().icmp(IntCC::Eq, old, expected)
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

    // ---- memory ----

    /// Copies an object of type `ty` from `src` to `dst`: small ones with
    /// loads and stores, big ones with `memcpy`.
    fn copy(&mut self, dst: Value, src: Value, ty: &Ty) {
        let size = self.size(ty);
        self.copy_bytes(dst, src, size);
    }

    fn copy_bytes(&mut self, dst: Value, src: Value, size: u64) {
        if size > 64 {
            let n = self.b.ins().iconst(Type::I64, size as i64);
            self.b.ins().intrinsic(Intrinsic::Memcpy, &[dst, src, n]);
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
                let v = self.b.ins().load(ty, s);
                self.b.ins().store(v, d);
                off += chunk;
            }
        }
    }

    fn zero_bytes(&mut self, dst: Value, size: u64) {
        if size > 64 {
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
                self.b.ins().store(z, d);
                off += chunk;
            }
        }
    }

    /// Runs a local's initializer.
    fn init_local(&mut self, id: LocalId, init: &Init) -> R<()> {
        let ty = self.def.locals[id].ty.clone();
        if let Storage::Var(var) = self.locals[id] {
            let v = match init.items.first() {
                Some(InitItem::Expr { expr, .. }) => self.val(expr)?,
                _ => self.zero(ty.ir()),
            };
            self.b.def_var(var, v);
            return Ok(());
        }
        let base = match self.locals[id] {
            Storage::Slot(slot) => self.b.ins().stack_addr(slot),
            Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
            Storage::Var(_) => unreachable!(),
        };
        let whole = matches!(init.items.as_slice(), [InitItem::Expr { offset: 0, expr, bits: None }] if expr.ty == ty);
        if !whole {
            let size = self.size(&ty);
            self.zero_bytes(base, size);
        }
        for item in &init.items {
            match item {
                InitItem::Expr { offset, expr, bits } => {
                    let v = self.val(expr)?;
                    let addr = self.offset(base, *offset);
                    let place = match bits {
                        Some(field) => Place::Bits {
                            addr,
                            unit: expr.ty.ir(),
                            field: *field,
                            signed: false,
                        },
                        None if Self::is_aggregate(&expr.ty) => Place::Aggregate(addr),
                        None => Place::Mem(addr, expr.ty.ir()),
                    };
                    self.store_place(&place, v, &expr.ty);
                }
                InitItem::Bytes { offset, bytes } => {
                    let size = (self.size(&ty) - offset).min(bytes.len() as u64);
                    let name = self.g.anon_data(&bytes[..size as usize]);
                    let data = self.b.func.import_data(&name);
                    let src = self.b.ins().symbol_addr(Symbol::Data(data));
                    let dst = self.offset(base, *offset);
                    self.copy_bytes(dst, src, size);
                }
            }
        }
        Ok(())
    }
}
