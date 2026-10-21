//! A function: arenas of blocks, instructions and values, plus block order.
//!
//! All mutation goes through methods so invariants (result values, aliases)
//! stay in one place.
//!
//! Removing an instruction or block takes it out of the layout but leaves
//! its arena entry (handles stay valid, and nothing is renumbered). Values
//! it defined still exist; using one is an error the verifier reports.
//! Blocks store their instructions as an array (in one pool shared by all
//! blocks), so inserting or removing in the middle of a block is linear in
//! its size; fine for now, and the methods hide the representation if it
//! needs to change.

use crate::entity::{KeyIndex, ListPool, ListRange};
use crate::inst::{BlockCall, Clause, InstData, Symbol};
use crate::layout::Layout;
use crate::{
    Arena, Block, DataRef, EntityRef, FuncRef, Inst, SigRef, SourceFile, StackSlot, Type, Value,
};
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CallConv {
    /// The target's C calling convention.
    C,
    /// Keel's private direct-call convention. Version 1 reuses target C
    /// placements/preservation, but narrow values need no C ABI extension.
    /// Its machine contract may evolve together with every contained caller.
    Fast,
}

/// How a narrow integer is widened when it crosses a call. C passes and
/// returns `char` and `short` extended to 32 bits by their signedness on
/// Apple arm64, which the type alone does not say.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArgExt {
    None,
    Sext,
    Uext,
}

/// What a `ptr` parameter stands for, when it is a C struct passed or
/// returned by value. The IR only ever handles the struct's address; the
/// calling convention copies the bytes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Purpose {
    /// An ordinary value.
    Normal,
    /// The caller passes the address of a struct; the callee receives the
    /// address of its own copy, which it may change. The bytes are read at
    /// the call instruction. A frontend requiring an earlier value snapshot
    /// must copy them before intervening memory effects during evaluation.
    ByVal(Layout),
    /// The caller passes the address of memory for the struct result, which
    /// the callee fills in. Only as the first parameter of a function that
    /// returns nothing.
    Sret(Layout),
}

/// A parameter or return value of a signature: its type and how the ABI
/// treats it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AbiParam {
    pub ty: Type,
    /// Only for `i8` and `i16`.
    pub ext: ArgExt,
    pub purpose: Purpose,
}

impl AbiParam {
    pub fn new(ty: Type) -> Self {
        Self {
            ty,
            ext: ArgExt::None,
            purpose: Purpose::Normal,
        }
    }

    /// The layout of the struct a `byval` or `sret` parameter points to.
    #[inline]
    pub fn layout(&self) -> Option<&Layout> {
        match &self.purpose {
            Purpose::Normal => None,
            Purpose::ByVal(layout) | Purpose::Sret(layout) => Some(layout),
        }
    }
}

impl fmt::Display for AbiParam {
    /// `i8 sext`, `ptr byval {i32, f64}`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ty)?;
        match self.ext {
            ArgExt::None => {}
            ArgExt::Sext => f.write_str(" sext")?,
            ArgExt::Uext => f.write_str(" uext")?,
        }
        match &self.purpose {
            Purpose::Normal => Ok(()),
            Purpose::ByVal(layout) => write!(f, " byval {layout}"),
            Purpose::Sret(layout) => write!(f, " sret {layout}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Signature {
    pub params: Vec<AbiParam>,
    pub returns: Vec<AbiParam>,
    pub cc: CallConv,
    /// Takes more arguments after `params`, like C's `...`. A call passes
    /// `params` and then any number of `i32`, `i64`, `ptr` or `f64`: the
    /// types C's default argument promotions leave.
    pub variadic: bool,
    /// May return more than once, like `setjmp`: after a `longjmp`, the
    /// call returns again with the registers it saved. Written
    /// `returns_twice`.
    pub returns_twice: bool,
}

impl fmt::Display for Signature {
    /// `(i32, ptr) -> i64 cc(c)`, or `(ptr, ...) -> i32 cc(c)`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        write_list(f, &self.params)?;
        if self.variadic {
            f.write_str(if self.params.is_empty() {
                "..."
            } else {
                ", ..."
            })?;
        }
        f.write_str(")")?;
        match self.returns.as_slice() {
            [] => {}
            [ty] => write!(f, " -> {ty}")?,
            tys => {
                f.write_str(" -> (")?;
                write_list(f, tys)?;
                f.write_str(")")?;
            }
        }
        match self.cc {
            CallConv::C => f.write_str(" cc(c)")?,
            CallConv::Fast => f.write_str(" cc(fast)")?,
        }
        if self.returns_twice {
            f.write_str(" returns_twice")?;
        }
        Ok(())
    }
}

impl Signature {
    /// A C-convention signature with plain parameters and returns.
    pub fn new(params: &[Type], returns: &[Type]) -> Self {
        Self {
            params: params.iter().map(|&ty| AbiParam::new(ty)).collect(),
            returns: returns.iter().map(|&ty| AbiParam::new(ty)).collect(),
            cc: CallConv::C,
            variadic: false,
            returns_twice: false,
        }
    }

    pub fn param_types(&self) -> Vec<Type> {
        self.params.iter().map(|p| p.ty).collect()
    }

    pub fn return_types(&self) -> Vec<Type> {
        self.returns.iter().map(|p| p.ty).collect()
    }
}

/// Function promises and optimization policy, deliberately separate from ABI types.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct FunctionAttrs {
    /// Producer promises that direct references cannot resolve to a different
    /// definition, even though the symbol may remain externally visible.
    /// This does not change linkage, visibility, ABI, or linker options.
    pub noninterposable: bool,
    pub inlinehint: bool,
    pub alwaysinline: bool,
    pub noinline: bool,
    pub noreturn: bool,
    pub nounwind: bool,
}

impl FunctionAttrs {
    pub fn merge(&mut self, other: Self) {
        self.noninterposable |= other.noninterposable;
        self.inlinehint |= other.inlinehint;
        self.alwaysinline |= other.alwaysinline;
        self.noinline |= other.noinline;
        self.noreturn |= other.noreturn;
        self.nounwind |= other.nounwind;
    }

    pub fn validate(self, returns_twice: bool) -> Result<(), &'static str> {
        if self.alwaysinline && self.noinline {
            return Err("alwaysinline conflicts with noinline");
        }
        if self.noreturn && returns_twice {
            return Err("noreturn conflicts with returns_twice");
        }
        Ok(())
    }
}

impl fmt::Display for FunctionAttrs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (set, name) in [
            (self.noninterposable, "noninterposable"),
            (self.inlinehint, "inlinehint"),
            (self.alwaysinline, "alwaysinline"),
            (self.noinline, "noinline"),
            (self.noreturn, "noreturn"),
            (self.nounwind, "nounwind"),
        ] {
            if set {
                write!(f, " {name}")?;
            }
        }
        Ok(())
    }
}

/// A function known by name and signature: a module's `decl`, or an entry in
/// a function's table of callees.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuncDecl {
    pub attrs: FunctionAttrs,
    pub name: String,
    pub sig: Signature,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValueDef {
    /// The `n`th result of an instruction.
    Result(Inst, u32),
    /// The `n`th parameter of a block.
    Param(Block, u32),
    /// Replaced by another value; see [`Function::replace_all_uses`].
    Alias(Value),
}

#[derive(Clone, Debug)]
pub struct ValueData {
    pub ty: Type,
    pub def: ValueDef,
}

/// A stack slot: `size` bytes aligned to `align` (a power of two, at most 16).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StackSlotData {
    pub size: u32,
    pub align: u32,
}

/// Where an instruction came from: a line and column in one of the module's
/// source files, counted from 1. 0 means unknown, as in DWARF. And the
/// lexical scope it is in, if not just the function's.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SourceLoc {
    pub file: SourceFile,
    pub line: u32,
    pub col: u32,
    pub scope: Option<crate::DebugScope>,
}

impl fmt::Display for SourceLoc {
    /// `f0:12:5`, or `f0:12:5 ds1` in a scope.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)?;
        if let Some(scope) = self.scope {
            write!(f, " {scope}")?;
        }
        Ok(())
    }
}

/// A block's lists, in [`Function::block_values`] and
/// [`Function::block_insts`].
#[derive(Copy, Clone, Default)]
struct BlockData {
    params: ListRange<Value>,
    insts: ListRange<Inst>,
    /// Whether the block is in [`Function::layout`].
    in_layout: bool,
}

/// Kept small: there is one per instruction ever created.
#[derive(Clone)]
struct InstNode {
    data: InstData,
    results: ResultRange,
    /// The containing block's index, or [`NO_BLOCK`] once removed.
    block: u32,
    /// One more than the location's index in [`Function::locs`]; 0 for none.
    loc: u32,
}

const NO_BLOCK: u32 = u32::MAX;

/// Append-only pool offsets, checked on creation so that they fit `u32`.
#[derive(Copy, Clone)]
struct ResultRange {
    start: u32,
    len: u32,
}

impl ResultRange {
    fn new(start: usize, len: usize) -> Self {
        let end = start.checked_add(len).expect("result pool range overflow");
        u32::try_from(end).expect("result pool range overflow");
        Self {
            start: start as u32,
            len: len as u32,
        }
    }

    #[inline]
    fn as_range(self) -> std::ops::Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

impl InstNode {
    #[inline]
    fn block(&self) -> Option<Block> {
        (self.block != NO_BLOCK).then(|| Block::new(self.block as usize))
    }
}

/// Where a new or moved instruction goes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InsertPoint {
    /// After the last instruction of the block.
    End(Block),
    /// Just before an instruction already in the layout.
    Before(Inst),
}

#[derive(Clone)]
pub struct Function {
    /// Definition-only C restrict access contracts, indexed into entry parameters.
    /// For an object accessed through a restricted parameter and modified during
    /// this invocation, all accesses must be based on that parameter. Read-only
    /// aliasing is permitted. This implies neither unequal pointer values nor
    /// nonnull, alignment, dereferenceability, or immutable memory. Indices must
    /// be unique, normal-purpose pointer parameters. C pointee eligibility is a
    /// frontend obligation: opaque IR pointers cannot express it. Cloning the
    /// whole function preserves scope; inlining must drop callee contracts.
    pub restrict_params: Vec<u32>,
    pub attrs: FunctionAttrs,
    pub name: String,
    pub sig: Signature,
    blocks: Arena<Block, BlockData>,
    /// Every block's parameters and instructions.
    block_values: ListPool<Value>,
    block_insts: ListPool<Inst>,
    insts: Arena<Inst, InstNode>,
    values: Arena<Value, ValueData>,
    result_pool: Vec<Value>,
    /// Instruction locations, shared by runs of instructions with the same
    /// one; see [`InstNode::loc`].
    locs: Vec<SourceLoc>,
    /// Scratch for [`Function::insert_inst`]; empty between calls.
    result_tys: Vec<Type>,
    stack_slots: Arena<StackSlot, StackSlotData>,
    /// Functions this one calls. Keeping their signatures here makes a
    /// function self-contained: result types and verification of calls need
    /// no module.
    callees: Arena<FuncRef, FuncDecl>,
    /// Data objects whose address this function takes, by name. Their
    /// contents belong to the module.
    data_symbols: Arena<DataRef, String>,
    /// Signatures of indirect calls, without duplicates.
    signatures: Arena<SigRef, Signature>,
    /// Lookups for the `import_*` methods, so that a function referring to
    /// many symbols does not scan its tables once per reference.
    callee_index: KeyIndex,
    data_index: KeyIndex,
    sig_index: KeyIndex,
    /// Block order. The first block is the entry.
    layout: Vec<Block>,
    /// Number of instructions in the layout's blocks.
    layout_len: usize,
    /// The function that interprets this function's landing pads while
    /// an exception unwinds, such as C++'s `__gxx_personality_v0`. Needed
    /// for `invoke`.
    pub personality: Option<String>,
    /// Visible only within its module, like C's `static`: not exported to
    /// the linker, so other modules may use the name for their own.
    pub local: bool,
    /// ODR definition coalesced by the linker across translation units.
    pub weak_odr: bool,
    /// The source-level return type, `dreturn dt0`, for debuggers; none
    /// for `void` or if unknown.
    pub debug_return: Option<crate::DebugType>,
    /// Where it is declared, `dloc f0:12:5`: the prologue's location.
    /// Without it the prologue takes the first instruction's line.
    pub debug_loc: Option<SourceLoc>,
    /// Lexical scopes, by [`crate::DebugScope`]: each one's parent, or
    /// `None` for one directly in the function. A parent comes first.
    pub debug_scopes: Vec<Option<crate::DebugScope>>,
    /// Source-level variables, for debuggers; see [`crate::debug`].
    pub debug_vars: Vec<crate::DebugVar>,
}

// Preserve the public logical Debug representation, including detached nodes.
impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Nodes<'a>(&'a Function);
        struct Node<'a>(&'a Function, Inst);
        impl fmt::Debug for Node<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let (func, inst) = (self.0, self.1);
                f.debug_struct("InstNode")
                    .field("data", func.inst_data(inst))
                    .field("results", &func.inst_results(inst))
                    .field("block", &func.insts[inst].block())
                    .field("loc", &func.inst_loc(inst))
                    .finish()
            }
        }
        impl fmt::Debug for Nodes<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.insts.iter().map(|(i, _)| (i, Node(self.0, i))))
                    .finish()
            }
        }
        struct Blocks<'a>(&'a Function);
        struct BlockLists<'a>(&'a Function, Block);
        impl fmt::Debug for BlockLists<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("BlockData")
                    .field("params", &self.0.block_params(self.1))
                    .field("insts", &self.0.block_insts(self.1))
                    .finish()
            }
        }
        impl fmt::Debug for Blocks<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.blocks.keys().map(|b| (b, BlockLists(self.0, b))))
                    .finish()
            }
        }
        f.debug_struct("Function")
            .field("restrict_params", &self.restrict_params)
            .field("attrs", &self.attrs)
            .field("name", &self.name)
            .field("sig", &self.sig)
            .field("blocks", &Blocks(self))
            .field("insts", &Nodes(self))
            .field("values", &self.values)
            .field("stack_slots", &self.stack_slots)
            .field("callees", &self.callees)
            .field("data_symbols", &self.data_symbols)
            .field("signatures", &self.signatures)
            .field("layout", &self.layout)
            .field("personality", &self.personality)
            .field("local", &self.local)
            .field("weak_odr", &self.weak_odr)
            .field("debug_return", &self.debug_return)
            .field("debug_loc", &self.debug_loc)
            .field("debug_scopes", &self.debug_scopes)
            .field("debug_vars", &self.debug_vars)
            .finish()
    }
}

impl Function {
    pub fn new(name: impl Into<String>, sig: Signature) -> Self {
        Self {
            restrict_params: Vec::new(),
            attrs: FunctionAttrs::default(),
            name: name.into(),
            sig,
            blocks: Arena::new(),
            block_values: ListPool::default(),
            block_insts: ListPool::default(),
            insts: Arena::new(),
            values: Arena::new(),
            result_pool: Vec::new(),
            locs: Vec::new(),
            result_tys: Vec::new(),
            stack_slots: Arena::new(),
            callees: Arena::new(),
            data_symbols: Arena::new(),
            signatures: Arena::new(),
            callee_index: KeyIndex::default(),
            data_index: KeyIndex::default(),
            sig_index: KeyIndex::default(),
            layout: Vec::new(),
            layout_len: 0,
            personality: None,
            local: false,
            weak_odr: false,
            debug_return: None,
            debug_loc: None,
            debug_scopes: Vec::new(),
            debug_vars: Vec::new(),
        }
    }

    pub fn create_stack_slot(&mut self, size: u32, align: u32) -> StackSlot {
        self.stack_slots.push(StackSlotData { size, align })
    }

    #[inline]
    pub fn stack_slots(&self) -> impl Iterator<Item = (StackSlot, StackSlotData)> + '_ {
        self.stack_slots.iter().map(|(ss, data)| (ss, *data))
    }

    #[inline]
    pub fn num_stack_slots(&self) -> usize {
        self.stack_slots.len()
    }

    /// Returns a reference for calling `name`, reusing an existing one.
    pub fn import_function(&mut self, name: &str, sig: Signature) -> FuncRef {
        // A callee renamed through `callee_mut` would be in the wrong slot.
        let callees = &self.callees;
        self.callee_index
            .check_touched(|i| KeyIndex::hash_of(callees[FuncRef::new(i)].name.as_str()));
        let hash = match find_entry(&self.callees, &mut self.callee_index, name, |d| {
            d.name.as_str()
        }) {
            Ok(func_ref) => return func_ref,
            Err(hash) => hash,
        };
        let func_ref = self.callees.push(FuncDecl {
            attrs: FunctionAttrs::default(),
            name: name.to_string(),
            sig,
        });
        self.callee_index.push(hash);
        func_ref
    }

    #[inline]
    pub fn callee_mut(&mut self, func_ref: FuncRef) -> &mut FuncDecl {
        self.callee_index
            .touch(func_ref.index(), self.callees.len());
        &mut self.callees[func_ref]
    }

    #[inline]
    pub fn callee(&self, func_ref: FuncRef) -> &FuncDecl {
        &self.callees[func_ref]
    }

    #[inline]
    pub fn callees(&self) -> impl Iterator<Item = (FuncRef, &FuncDecl)> {
        self.callees.iter()
    }

    /// Returns a reference to data object `name`, reusing an existing one.
    pub fn import_data(&mut self, name: &str) -> DataRef {
        let hash = match find_entry(&self.data_symbols, &mut self.data_index, name, |n| {
            n.as_str()
        }) {
            Ok(data_ref) => return data_ref,
            Err(hash) => hash,
        };
        let data_ref = self.data_symbols.push(name.to_string());
        self.data_index.push(hash);
        data_ref
    }

    #[inline]
    pub fn data_symbols(&self) -> impl Iterator<Item = (DataRef, &str)> {
        self.data_symbols.iter().map(|(r, name)| (r, name.as_str()))
    }

    /// Returns a reference to `sig` for indirect calls, reusing an existing
    /// one.
    pub fn import_signature(&mut self, sig: Signature) -> SigRef {
        let hash = match find_entry(&self.signatures, &mut self.sig_index, &sig, |s| s) {
            Ok(sig_ref) => return sig_ref,
            Err(hash) => hash,
        };
        let sig_ref = self.signatures.push(sig);
        self.sig_index.push(hash);
        sig_ref
    }

    #[inline]
    pub fn signature(&self, sig_ref: SigRef) -> &Signature {
        &self.signatures[sig_ref]
    }

    #[inline]
    pub fn signatures(&self) -> impl Iterator<Item = (SigRef, &Signature)> {
        self.signatures.iter()
    }

    /// The linker name of `symbol`.
    #[inline]
    pub fn symbol_name(&self, symbol: Symbol) -> &str {
        match symbol {
            Symbol::Func(func_ref) => &self.callees[func_ref].name,
            Symbol::Data(data_ref) => &self.data_symbols[data_ref],
        }
    }

    /// Creates a block at the end of the layout.
    pub fn create_block(&mut self) -> Block {
        let block = self.blocks.push(BlockData {
            in_layout: true,
            ..BlockData::default()
        });
        self.layout.push(block);
        block
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Value {
        let params = &mut self.blocks[block].params;
        let n = params.len() as u32;
        let value = self.values.push(ValueData {
            ty,
            def: ValueDef::Param(block, n),
        });
        self.block_values.push(params, value);
        value
    }

    /// Appends an instruction to `block` and creates its result values.
    pub fn append_inst(&mut self, block: Block, data: InstData) -> Inst {
        self.insert_inst(InsertPoint::End(block), data)
    }

    /// Result signature implied by the opcode and operands. Shared by
    /// construction and verification; visiting avoids per-instruction
    /// allocations in the verifier's hot path.
    pub(crate) fn for_each_result_type(&self, data: &InstData, mut visit: impl FnMut(Type)) {
        match data {
            InstData::VectorSplat { ty, .. }
            | InstData::Iconst { ty, .. }
            | InstData::Fconst { ty, .. }
            | InstData::Convert { ty, .. }
            | InstData::Load { ty, .. }
            | InstData::AtomicLoad { ty, .. }
            | InstData::VaArg { ty, .. } => visit(*ty),
            InstData::AtomicRmw { args, .. } => visit(self.value_type(args[1])),
            InstData::AtomicCas { args, .. } | InstData::Select { args } => {
                visit(self.value_type(args[1]))
            }
            InstData::StackAddr { .. } | InstData::SymbolAddr { .. } => visit(Type::Ptr),
            InstData::VectorExtract { arg, .. } => visit(
                self.value_type(*arg)
                    .vector()
                    .map_or(Type::I8, |v| v.lane_type()),
            ),
            InstData::VectorIcmp { args, .. } | InstData::VectorFcmp { args, .. } => visit(
                self.value_type(args[0])
                    .vector()
                    .map_or(Type::I8, |v| Type::Vector(v.mask())),
            ),
            InstData::VectorBinary { args, .. }
            | InstData::VectorInsert { args, .. }
            | InstData::VectorShuffle { args, .. } => visit(self.value_type(args[0])),
            InstData::VectorSelect { args } => visit(self.value_type(args[1])),
            InstData::VectorUnary { arg, .. } | InstData::Unary { arg, .. } => {
                visit(self.value_type(*arg))
            }
            InstData::Binary { args, .. } => visit(self.value_type(args[0])),
            InstData::Fma { args } => visit(self.value_type(args[0])),
            InstData::Overflow { args, .. } => {
                visit(self.value_type(args[0]));
                visit(Type::I8);
            }
            InstData::Icmp { .. } | InstData::Fcmp { .. } => visit(Type::I8),
            InstData::Call { func, .. } | InstData::Invoke { func, .. } => {
                for param in &self.callees[*func].sig.returns {
                    visit(param.ty);
                }
            }
            InstData::CallIndirect { sig, .. } => {
                for param in &self.signatures[*sig].returns {
                    visit(param.ty);
                }
            }
            InstData::LandingPad { .. } => {
                visit(Type::Ptr);
                visit(Type::I32);
            }
            InstData::EhTypeid { .. } => visit(Type::I32),
            InstData::Intrinsic { id, .. } => {
                for &ty in id.info().returns {
                    visit(ty);
                }
            }
            InstData::InlineAsm { asm, .. } => {
                for &(_, ty) in &asm.outputs {
                    visit(ty);
                }
            }
            InstData::Store { .. }
            | InstData::AtomicStore { .. }
            | InstData::Fence { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. }
            | InstData::Return { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::Trap => {}
        }
    }

    /// Inserts an instruction at `at` and creates its result values, typed
    /// from its operands.
    pub fn insert_inst(&mut self, at: InsertPoint, data: InstData) -> Inst {
        // A panicking inference loses only the buffer.
        let mut result_tys = std::mem::take(&mut self.result_tys);
        let inst = self.insert_inst_with_scratch(at, data, &mut result_tys);
        result_tys.clear();
        self.result_tys = result_tys;
        inst
    }

    /// Inserts using caller-owned reusable result-type storage. Type inference
    /// finishes before any instruction or value is created. Contents from an
    /// earlier insertion (including a panicking inference) are discarded first.
    pub fn insert_inst_with_scratch(
        &mut self,
        at: InsertPoint,
        data: InstData,
        result_tys: &mut Vec<Type>,
    ) -> Inst {
        result_tys.clear();
        self.for_each_result_type(&data, |ty| result_tys.push(ty));
        let inst = self.create_inst(data, result_tys);
        self.place(inst, at);
        inst
    }

    /// Like [`Function::append_inst`] with the result types supplied, for the
    /// parser, whose operands may not exist yet.
    pub(crate) fn append_inst_with_results(
        &mut self,
        block: Block,
        data: InstData,
        result_tys: &[Type],
    ) -> Inst {
        let inst = self.create_inst(data, result_tys);
        self.place(inst, InsertPoint::End(block));
        inst
    }

    /// Creates an instruction and its results, outside the layout.
    fn create_inst(&mut self, data: InstData, result_tys: &[Type]) -> Inst {
        let results = ResultRange::new(self.result_pool.len(), result_tys.len());
        // Check all handle/index arithmetic before mutating the function.
        if !result_tys.is_empty() {
            let last = self
                .values
                .len()
                .checked_add(result_tys.len() - 1)
                .expect("value index overflow");
            u32::try_from(last).expect("value index overflow");
            u32::try_from(result_tys.len() - 1).expect("result index overflow");
        }
        self.result_pool.reserve(result_tys.len());
        let inst = self.insts.push(InstNode {
            data,
            results,
            block: NO_BLOCK,
            loc: 0,
        });
        for (n, &ty) in result_tys.iter().enumerate() {
            let value = self.values.push(ValueData {
                ty,
                def: ValueDef::Result(inst, u32::try_from(n).expect("result index overflow")),
            });
            self.result_pool.push(value);
        }
        inst
    }

    /// Puts an instruction that is not in the layout at `at`.
    fn place(&mut self, inst: Inst, at: InsertPoint) {
        debug_assert!(self.is_removed(inst));
        let (block, index) = match at {
            InsertPoint::End(block) => {
                let data = &mut self.blocks[block];
                self.block_insts.push(&mut data.insts, inst);
                self.layout_len += usize::from(data.in_layout);
                self.insts[inst].block = block.index() as u32;
                self.compact_insts_if_wasteful();
                return;
            }
            InsertPoint::Before(before) => {
                let block = self.inst_block(before);
                (block, self.position(before))
            }
        };
        let data = &mut self.blocks[block];
        self.block_insts.insert(&mut data.insts, index, inst);
        self.layout_len += usize::from(data.in_layout);
        self.insts[inst].block = block.index() as u32;
        self.compact_insts_if_wasteful();
    }

    /// Passes that insert and remove many instructions leave rooms in the
    /// pool that no block uses; drop them once they are most of it.
    #[inline]
    fn compact_insts_if_wasteful(&mut self) {
        if self.block_insts.buffer_len() > 2 * self.layout_len + 256 {
            // SAFETY: block data holds every instruction list.
            unsafe {
                self.block_insts.compact(
                    self.layout_len,
                    self.blocks.values_mut().map(|b| &mut b.insts),
                )
            };
        }
    }

    /// Index of `inst` within its block.
    fn position(&self, inst: Inst) -> usize {
        let block = self.inst_block(inst);
        self.block_insts(block)
            .iter()
            .position(|&i| i == inst)
            .expect("an instruction is in its block")
    }

    /// Takes `inst` out of the layout. Its results must be unused (or
    /// replaced with [`Function::replace_all_uses`]) by then.
    pub fn remove_inst(&mut self, inst: Inst) {
        let block = self.inst_block(inst);
        let index = self.position(inst);
        let data = &mut self.blocks[block];
        self.block_insts.remove(&mut data.insts, index);
        self.layout_len -= usize::from(data.in_layout);
        self.insts[inst].block = NO_BLOCK;
    }

    /// Moves `inst` to `at`, keeping its results.
    pub fn move_inst(&mut self, inst: Inst, at: InsertPoint) {
        self.remove_inst(inst);
        self.place(inst, at);
    }

    /// Whether `inst` has been removed from the layout.
    #[inline]
    pub fn is_removed(&self, inst: Inst) -> bool {
        self.insts[inst].block == NO_BLOCK
    }

    /// Mutable access to an instruction, for rewriting operands or
    /// replacing it with another instruction **with the same result
    /// types** (e.g. folding an `iadd` to an `iconst`).
    /// Rewriting data must preserve the result count and types. To change
    /// the result signature, insert a new instruction, replace its uses and
    /// remove the old one. The verifier checks this contract after passes.
    #[inline]
    pub fn inst_data_mut(&mut self, inst: Inst) -> &mut InstData {
        &mut self.insts[inst].data
    }

    /// The source location `inst` was compiled from, if known.
    #[inline]
    pub fn inst_loc(&self, inst: Inst) -> Option<SourceLoc> {
        match self.insts[inst].loc {
            0 => None,
            n => Some(self.locs[n as usize - 1]),
        }
    }

    /// Every location an instruction has been given, including removed
    /// ones, possibly repeated.
    pub(crate) fn locs(&self) -> &[SourceLoc] {
        &self.locs
    }

    pub fn set_inst_loc(&mut self, inst: Inst, loc: Option<SourceLoc>) {
        let index = match loc {
            None => 0,
            Some(loc) => {
                // Instructions mostly come in runs from one source line.
                if self.locs.last() != Some(&loc) {
                    self.locs.push(loc);
                    u32::try_from(self.locs.len()).expect("location index overflow");
                }
                self.locs.len() as u32
            }
        };
        self.insts[inst].loc = index;
    }

    /// Takes `block` and its instructions out of the layout. Nothing may
    /// branch to it any more.
    pub fn remove_block(&mut self, block: Block) {
        assert_ne!(
            self.entry_block(),
            Some(block),
            "cannot remove the entry block"
        );
        let data = &mut self.blocks[block];
        for &inst in self.block_insts.get(data.insts) {
            self.insts[inst].block = NO_BLOCK;
        }
        let removed = data.insts.len();
        self.block_insts.free(&mut data.insts);
        if std::mem::replace(&mut data.in_layout, false) {
            self.layout_len -= removed;
            // A block is in the layout once; the blocks after it move up
            // in one copy.
            let index = self.layout.iter().position(|&b| b == block);
            self.layout.remove(index.expect("a block in the layout"));
        }
    }

    /// Whether `block` is in the layout.
    #[inline]
    pub fn is_block_inserted(&self, block: Block) -> bool {
        self.blocks[block].in_layout
    }

    /// Removes parameter `index` of `block` and returns its value, which
    /// must be unused (or replaced). Branches to `block` still pass the
    /// argument; the caller removes it from each of them.
    pub fn remove_block_param(&mut self, block: Block, index: usize) -> Value {
        let value = self
            .block_values
            .remove(&mut self.blocks[block].params, index);
        let params = self.block_values.get(self.blocks[block].params);
        for (n, &param) in params.iter().enumerate().skip(index) {
            if matches!(self.values[param].def, ValueDef::Param(..)) {
                self.values[param].def = ValueDef::Param(block, n as u32);
            }
        }
        value
    }

    /// Makes every use of `old` read `new` instead. `old`'s defining
    /// instruction is left in place; dead code removal deletes it.
    pub fn replace_all_uses(&mut self, old: Value, new: Value) {
        assert_ne!(self.resolve(new), old, "alias cycle");
        assert_eq!(self.value_type(old), self.value_type(new), "type mismatch");
        self.values[old].def = ValueDef::Alias(new);
    }

    /// Follows aliases to the value that is actually defined.
    #[inline]
    pub fn resolve(&self, mut value: Value) -> Value {
        while let ValueDef::Alias(next) = self.values[value].def {
            value = next;
        }
        value
    }

    /// Number of values ever created; value indices are below this.
    #[inline]
    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    /// Number of instructions ever created; instruction indices are below
    /// this.
    #[inline]
    pub fn num_insts(&self) -> usize {
        self.insts.len()
    }

    /// Number of blocks ever created; block indices are below this.
    #[inline]
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    #[inline]
    pub fn entry_block(&self) -> Option<Block> {
        self.layout.first().copied()
    }

    #[inline]
    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.layout.iter().copied()
    }

    #[inline]
    pub fn block_params(&self, block: Block) -> &[Value] {
        self.block_values.get(self.blocks[block].params)
    }

    #[inline]
    pub fn block_insts(&self, block: Block) -> &[Inst] {
        self.block_insts.get(self.blocks[block].insts)
    }

    #[inline]
    pub fn inst_data(&self, inst: Inst) -> &InstData {
        &self.insts[inst].data
    }

    #[inline]
    pub fn inst_results(&self, inst: Inst) -> &[Value] {
        let range = self.insts[inst].results.as_range();
        debug_assert!(range.end <= self.result_pool.len());
        // SAFETY: `create_inst` gives a node its range only once nothing
        // can stop it filling that range in, and the pool never shrinks.
        unsafe { self.result_pool.get_unchecked(range) }
    }

    /// The block containing `inst`. Panics if it was removed.
    #[inline]
    pub fn inst_block(&self, inst: Inst) -> Block {
        self.insts[inst].block().expect("instruction was removed")
    }

    /// The block containing `inst`, unless it was removed.
    #[inline]
    pub(crate) fn inst_block_if_placed(&self, inst: Inst) -> Option<Block> {
        self.insts[inst].block()
    }

    /// Every instruction in the layout, block by block.
    #[inline]
    pub fn layout_insts(&self) -> impl Iterator<Item = (Block, Inst)> + '_ {
        LayoutInsts {
            func: self,
            blocks: self.layout.iter(),
            block: Block::new(0),
            insts: [].iter(),
            remaining: self.layout_len,
        }
    }

    /// Whether `value` is still defined: by an instruction in the layout,
    /// or as a parameter its block still has. Aliases resolve first.
    pub fn is_value_defined(&self, value: Value) -> bool {
        match self.value_def(self.resolve(value)) {
            ValueDef::Result(inst, _) => !self.is_removed(inst),
            ValueDef::Param(block, n) => {
                self.is_block_inserted(block)
                    && self.block_params(block).get(n as usize) == Some(&self.resolve(value))
            }
            ValueDef::Alias(_) => unreachable!("resolved"),
        }
    }

    /// The single result of `inst`. Panics if it has none.
    #[inline]
    pub fn first_result(&self, inst: Inst) -> Value {
        self.inst_results(inst)[0]
    }

    #[inline]
    pub fn value_type(&self, value: Value) -> Type {
        self.values[value].ty
    }

    #[inline]
    pub fn value_def(&self, value: Value) -> ValueDef {
        self.values[value].def
    }
}

/// [`Function::layout_insts`]: a flattened walk that knows its length, so
/// collecting it allocates once.
struct LayoutInsts<'a> {
    func: &'a Function,
    blocks: std::slice::Iter<'a, Block>,
    block: Block,
    insts: std::slice::Iter<'a, Inst>,
    remaining: usize,
}

impl Iterator for LayoutInsts<'_> {
    type Item = (Block, Inst);

    #[inline]
    fn next(&mut self) -> Option<(Block, Inst)> {
        loop {
            if let Some(&inst) = self.insts.next() {
                self.remaining -= 1;
                return Some((self.block, inst));
            }
            let Some(&block) = self.blocks.next() else {
                debug_assert_eq!(self.remaining, 0, "layout length is stale");
                return None;
            };
            self.block = block;
            // SAFETY: the layout only holds blocks `create_block` made.
            let lists = unsafe { self.func.blocks.get_unchecked(block) };
            self.insts = self.func.block_insts.get(lists.insts).iter();
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

/// The first entry of `arena` whose key is `key`, or else the hash to index
/// a new entry under (0 while the arena is scanned). Builds `index` once the
/// arena is big enough.
#[inline]
fn find_entry<K: EntityRef, V, Q: std::hash::Hash + PartialEq + ?Sized>(
    arena: &Arena<K, V>,
    index: &mut KeyIndex,
    key: &Q,
    key_of: impl Fn(&V) -> &Q,
) -> Result<K, u32> {
    if arena.len() < KeyIndex::FROM {
        return match arena.iter().find(|(_, v)| key_of(v) == key) {
            Some((k, _)) => Ok(k),
            None => Err(0),
        };
    }
    if index.len() != arena.len() {
        index.rebuild(
            arena.len(),
            arena.iter().map(|(_, v)| KeyIndex::hash_of(key_of(v))),
        );
    }
    let hash = KeyIndex::hash_of(key);
    match index.find(hash, |i| key_of(&arena[K::new(i)]) == key) {
        Some(i) => Ok(K::new(i)),
        None => Err(hash),
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "func @{}{}{}", self.name, self.sig, self.attrs)?;
        if !self.restrict_params.is_empty() {
            f.write_str(" restrict_params(")?;
            write_list(f, self.restrict_params.iter())?;
            f.write_str(")")?;
        }
        if self.local {
            f.write_str(" local")?;
        }
        if self.weak_odr {
            f.write_str(" weak_odr")?;
        }
        if let Some(personality) = &self.personality {
            write!(f, " personality @{personality}")?;
        }
        f.write_str(" {\n")?;
        for (ss, data) in self.stack_slots() {
            writeln!(
                f,
                "    {ss} = stack_slot {}, align {}",
                data.size, data.align
            )?;
        }
        if let Some(ty) = self.debug_return {
            writeln!(f, "    dreturn {ty}")?;
        }
        if let Some(loc) = self.debug_loc {
            writeln!(f, "    dloc {loc}")?;
        }
        for (i, parent) in self.debug_scopes.iter().enumerate() {
            write!(
                f,
                "    dscope {}",
                <crate::DebugScope as crate::EntityRef>::new(i)
            )?;
            if let Some(parent) = parent {
                write!(f, " in {parent}")?;
            }
            f.write_str("\n")?;
        }
        for var in &self.debug_vars {
            writeln!(f, "    {var}")?;
        }

        for block in self.blocks() {
            write!(f, "{block}")?;
            let params = self.block_params(block);
            if !params.is_empty() {
                f.write_str("(")?;
                for (i, &param) in params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{param}: {}", self.value_type(param))?;
                }
                f.write_str(")")?;
            }
            f.write_str(":\n")?;
            for &inst in self.block_insts(block) {
                f.write_str("    ")?;
                self.write_inst(f, inst)?;
                if let Some(loc) = self.inst_loc(inst) {
                    write!(f, " !{loc}")?;
                }
                f.write_str("\n")?;
            }
        }
        f.write_str("}\n")
    }
}

impl Function {
    /// Formats one instruction the way the printer does, e.g. `v3 = iadd v1, v2`.
    pub fn display_inst(&self, inst: Inst) -> impl fmt::Display + '_ {
        struct DisplayInst<'a>(&'a Function, Inst);
        impl fmt::Display for DisplayInst<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_inst(f, self.1)
            }
        }
        DisplayInst(self, inst)
    }

    /// Appends exactly what [`Self::display_inst`] prints to `out`, without
    /// formatting machinery; codegen writes one per assembly comment.
    pub fn write_inst_to<W: fmt::Write + ?Sized>(&self, out: &mut W, inst: Inst) -> fmt::Result {
        self.write_inst(out, inst)
    }

    fn write_inst<W: fmt::Write + ?Sized>(&self, f: &mut W, inst: Inst) -> fmt::Result {
        // Codegen prints every instruction as an assembly comment. Write the
        // pieces directly rather than nesting `write!`, which re-enters
        // `fmt::write` for every operand; mnemonics come from `write_name`.
        let results = self.inst_results(inst);
        if !results.is_empty() {
            write_values(f, results.iter().copied())?;
            f.write_str(" = ")?;
        }
        // Operands are printed resolved, so aliases never show up in text.
        let v = |value: Value| self.resolve(value);
        match self.inst_data(inst) {
            InstData::Iconst { ty, imm } => {
                f.write_str("iconst.")?;
                write_name(f, Name::Type(*ty))?;
                f.write_str(" ")?;
                write_decimal(f, "", imm.is_negative(), imm.unsigned_abs())
            }
            InstData::Fconst { ty, bits } => {
                write!(f, "fconst.{ty} {}", crate::inst::float_text(*ty, *bits))
            }
            InstData::StackAddr { slot } => {
                f.write_str("stack_addr ")?;
                write_decimal(f, "ss", false, crate::EntityRef::index(*slot) as u64)
            }
            InstData::Call { func, args: a } => {
                f.write_str("call @")?;
                f.write_str(&self.callees[*func].name)?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                f.write_str(")")
            }
            InstData::SymbolAddr { symbol } => {
                write!(f, "symbol_addr @{}", self.symbol_name(*symbol))
            }
            InstData::Invoke {
                func,
                args: a,
                dests,
            } => {
                f.write_str("invoke @")?;
                f.write_str(&self.callees[*func].name)?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                f.write_str("), ")?;
                self.write_block_call(f, &dests[0])?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[1])
            }
            InstData::LandingPad { clauses } => {
                f.write_str("landingpad")?;
                for (i, clause) in clauses.iter().enumerate() {
                    f.write_str(if i == 0 { " " } else { ", " })?;
                    match clause {
                        Clause::Catch(Some(typeinfo)) => {
                            write!(f, "catch @{}", self.data_symbols[*typeinfo])?
                        }
                        Clause::Catch(None) => f.write_str("catch null")?,
                        Clause::Cleanup => f.write_str("cleanup")?,
                    }
                }
                Ok(())
            }
            InstData::EhTypeid { typeinfo } => {
                write!(f, "eh_typeid @{}", self.data_symbols[*typeinfo])
            }
            InstData::ReturnCall { func, args: a } => {
                f.write_str("return_call @")?;
                f.write_str(&self.callees[*func].name)?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                f.write_str(")")
            }
            InstData::ReturnCallIndirect {
                sig,
                callee,
                args: a,
            } => {
                f.write_str("return_call_indirect ")?;
                write_value(f, v(*callee))?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                write!(f, ") : {}", self.signatures[*sig])
            }
            InstData::CallIndirect {
                sig,
                callee,
                args: a,
            } => {
                f.write_str("call_indirect ")?;
                write_value(f, v(*callee))?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                write!(f, ") : {}", self.signatures[*sig])
            }
            InstData::Intrinsic { id, args: a } => {
                write!(f, "intrinsic {id}(")?;
                self.write_args(f, a)?;
                f.write_str(")")
            }
            InstData::InlineAsm { asm, args: a } => {
                f.write_str("asm ")?;
                write!(f, "{}", Quoted(asm.template.as_bytes()))?;
                f.write_str(", ")?;
                write!(f, "{}", Quoted(asm.constraints().as_bytes()))?;
                f.write_str("(")?;
                self.write_args(f, a)?;
                f.write_str(")")?;
                match &asm.outputs[..] {
                    [] => Ok(()),
                    [(_, ty)] => write!(f, " -> {ty}"),
                    outputs => {
                        f.write_str(" -> (")?;
                        write_list(f, outputs.iter().map(|(_, ty)| ty))?;
                        f.write_str(")")
                    }
                }
            }
            InstData::Trap => f.write_str("trap"),
            InstData::VectorSplat { ty, arg } => {
                f.write_str("vsplat.")?;
                write_name(f, Name::Type(*ty))?;
                f.write_str(" ")?;
                write_value(f, v(*arg))
            }
            InstData::VectorUnary { op, arg } => {
                f.write_str("v")?;
                write_name(f, Name::Unary(*op))?;
                f.write_str(" ")?;
                write_value(f, v(*arg))
            }
            InstData::VectorBinary { op, args: a } => {
                write_name(f, Name::Vector(*op))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::VectorExtract { arg, lane } => {
                f.write_str("vextract ")?;
                write_value(f, v(*arg))?;
                f.write_str(", ")?;
                write_decimal(f, "", false, u64::from(*lane))
            }
            InstData::VectorInsert { args: a, lane } => {
                f.write_str("vinsert ")?;
                self.write_args(f, a)?;
                f.write_str(", ")?;
                write_decimal(f, "", false, u64::from(*lane))
            }
            InstData::VectorShuffle { args: a, lanes } => {
                f.write_str("vshuffle ")?;
                self.write_args(f, a)?;
                f.write_str(", [")?;
                write_list(f, lanes.iter())?;
                f.write_str("]")
            }
            InstData::VectorIcmp { cond, args: a } => {
                f.write_str("vicmp.")?;
                write_name(f, Name::IntCC(*cond))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::VectorFcmp { cond, args: a } => {
                f.write_str("vfcmp.")?;
                write_name(f, Name::FloatCC(*cond))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::VectorSelect { args: a } => {
                f.write_str("vselect ")?;
                self.write_args(f, a)
            }
            InstData::Unary { op, arg } => {
                write_name(f, Name::Unary(*op))?;
                f.write_str(" ")?;
                write_value(f, v(*arg))
            }
            InstData::Binary { op, args: a } => {
                write_name(f, Name::Binary(*op))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::Overflow { op, args: a } => {
                write_name(f, Name::Overflow(*op))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::Fma { args: a } => {
                f.write_str("fma ")?;
                self.write_args(f, a)
            }
            InstData::Select { args: a } => {
                f.write_str("select ")?;
                self.write_args(f, a)
            }
            InstData::Convert { op, ty, arg } => {
                write_name(f, Name::Convert(*op))?;
                f.write_str(".")?;
                write_name(f, Name::Type(*ty))?;
                f.write_str(" ")?;
                write_value(f, v(*arg))
            }
            InstData::Icmp { cond, args: a } => {
                f.write_str("icmp.")?;
                write_name(f, Name::IntCC(*cond))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::Fcmp { cond, args: a } => {
                f.write_str("fcmp.")?;
                write_name(f, Name::FloatCC(*cond))?;
                f.write_str(" ")?;
                self.write_args(f, a)
            }
            InstData::Load { ty, addr, flags } => {
                f.write_str("load.")?;
                write_name(f, Name::Type(*ty))?;
                f.write_str(" ")?;
                write_flags(f, flags)?;
                write_value(f, v(*addr))
            }
            InstData::VaArg { ty, list } => {
                f.write_str("va_arg.")?;
                write_name(f, Name::Type(*ty))?;
                f.write_str(" ")?;
                write_value(f, v(*list))
            }
            InstData::Store { value, addr, flags } => {
                f.write_str("store ")?;
                write_flags(f, flags)?;
                write_value(f, v(*value))?;
                f.write_str(", ")?;
                write_value(f, v(*addr))
            }
            InstData::AtomicLoad { ty, ordering, addr } => {
                write!(f, "atomic_load.{ty} {ordering} {}", v(*addr))
            }
            InstData::AtomicStore {
                ordering,
                value,
                addr,
            } => write!(f, "atomic_store {ordering} {}, {}", v(*value), v(*addr)),
            InstData::AtomicRmw {
                op,
                ordering,
                args: a,
            } => {
                write!(f, "atomic_rmw.{op} {ordering} {}, {}", v(a[0]), v(a[1]))
            }
            InstData::AtomicCas { ordering, args: a } => write!(
                f,
                "atomic_cas {ordering} {}, {}, {}",
                v(a[0]),
                v(a[1]),
                v(a[2])
            ),
            InstData::Fence { ordering } => write!(f, "fence {ordering}"),
            InstData::Jump { dest } => {
                f.write_str("jump ")?;
                self.write_block_call(f, dest)
            }
            InstData::Brif { cond, dests } => {
                f.write_str("brif ")?;
                write_value(f, v(*cond))?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[0])?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[1])
            }
            InstData::Switch { arg, dests, cases } => {
                f.write_str("switch ")?;
                write_value(f, v(*arg))?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[0])?;
                f.write_str(", [")?;
                for (i, (case, dest)) in cases.iter().zip(&dests[1..]).enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{case}: ")?;
                    self.write_block_call(f, dest)?;
                }
                f.write_str("]")
            }
            InstData::Return { args: a } => {
                f.write_str("return")?;
                if !a.is_empty() {
                    f.write_str(" ")?;
                    self.write_args(f, a)?;
                }
                Ok(())
            }
        }
    }

    fn write_args<W: fmt::Write + ?Sized>(&self, f: &mut W, args: &[Value]) -> fmt::Result {
        write_values(f, args.iter().map(|&a| self.resolve(a)))
    }

    fn write_block_call<W: fmt::Write + ?Sized>(&self, f: &mut W, call: &BlockCall) -> fmt::Result {
        write_decimal(
            f,
            "block",
            false,
            crate::EntityRef::index(call.block) as u64,
        )?;
        if !call.args.is_empty() {
            f.write_str("(")?;
            self.write_args(f, &call.args)?;
            f.write_str(")")?;
        }
        Ok(())
    }
}

/// `prefix`, an optional minus sign and the decimal digits of `n` in a single
/// `write_str`, exactly as `Display` prints entities and integers.
fn write_decimal<W: fmt::Write + ?Sized>(
    f: &mut W,
    prefix: &str,
    negative: bool,
    mut n: u64,
) -> fmt::Result {
    if !negative && n < 1000 {
        if !prefix.is_empty() {
            f.write_str(prefix)?;
        }
        return f.write_str(small_decimal(n as usize));
    }
    let mut buf = [0u8; 32];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if negative {
        i -= 1;
        buf[i] = b'-';
    }
    i -= prefix.len();
    buf[i..i + prefix.len()].copy_from_slice(prefix.as_bytes());
    f.write_str(std::str::from_utf8(&buf[i..]).expect("ASCII digits"))
}

/// Mnemonics and types printed by instructions.
#[derive(Clone, Copy)]
enum Name {
    Type(Type),
    Binary(crate::BinaryOp),
    Unary(crate::UnaryOp),
    Convert(crate::ConvertOp),
    Overflow(crate::OverflowOp),
    IntCC(crate::IntCC),
    FloatCC(crate::FloatCC),
    Vector(crate::VectorBinaryOp),
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Type(x) => x.fmt(f),
            Name::Binary(x) => x.fmt(f),
            Name::Unary(x) => x.fmt(f),
            Name::Convert(x) => x.fmt(f),
            Name::Overflow(x) => x.fmt(f),
            Name::IntCC(x) => x.fmt(f),
            Name::FloatCC(x) => x.fmt(f),
            Name::Vector(x) => x.fmt(f),
        }
    }
}

impl Name {
    /// A dense cache slot: each kind owns 64 consecutive slots.
    fn slot(self) -> usize {
        let (kind, index) = match self {
            Name::Type(ty) => (
                0,
                match ty {
                    Type::I8 => 0,
                    Type::I16 => 1,
                    Type::I32 => 2,
                    Type::I64 => 3,
                    Type::I128 => 4,
                    Type::F32 => 5,
                    Type::F64 => 6,
                    Type::Ptr => 7,
                    Type::Vector(v) => 8 + v as usize,
                },
            ),
            Name::Binary(x) => (1, x as usize),
            Name::Unary(x) => (2, x as usize),
            Name::Convert(x) => (3, x as usize),
            Name::Overflow(x) => (4, x as usize),
            Name::IntCC(x) => (5, x as usize),
            Name::FloatCC(x) => (6, x as usize),
            Name::Vector(x) => (7, x as usize),
        };
        assert!(index < 64, "too many variants for the name cache");
        kind * 64 + index
    }
}

/// `name`'s `Display` text, remembered per thread so that printing an
/// instruction does not enter `fmt::write` for its mnemonic.
fn write_name<W: fmt::Write + ?Sized>(f: &mut W, name: Name) -> fmt::Result {
    thread_local! {
        static NAMES: std::cell::RefCell<Vec<Option<Box<str>>>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    let slot = name.slot();
    NAMES.with(|names| {
        let mut names = names.borrow_mut();
        if names.len() <= slot {
            names.resize(8 * 64, None);
        }
        let text = names[slot].get_or_insert_with(|| name.to_string().into_boxed_str());
        f.write_str(text)
    })
}

/// A quoted string, as the parser reads it.
struct Quoted<'a>(&'a [u8]);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::data::write_quoted(f, self.0)
    }
}

/// Default flags print nothing; others keep their own `Display`.
fn write_flags<W: fmt::Write + ?Sized>(f: &mut W, flags: &crate::MemoryFlags) -> fmt::Result {
    if *flags == crate::MemoryFlags::default() {
        return Ok(());
    }
    write!(f, "{flags}")
}

/// The decimal text of 0 through 999, back to back.
const SMALL_DECIMALS: &str = {
    const BYTES: [u8; 2890] = {
        let mut bytes = [0u8; 2890];
        let (mut n, mut i) = (0, 0);
        while n < 1000 {
            if n >= 100 {
                bytes[i] = b'0' + (n / 100) as u8;
                i += 1;
            }
            if n >= 10 {
                bytes[i] = b'0' + (n / 10 % 10) as u8;
                i += 1;
            }
            bytes[i] = b'0' + (n % 10) as u8;
            i += 1;
            n += 1;
        }
        bytes
    };
    match std::str::from_utf8(&BYTES) {
        Ok(text) => text,
        Err(_) => panic!("ASCII digits"),
    }
};

/// The decimal text of `n < 1000`, without validating a digit buffer.
fn small_decimal(n: usize) -> &'static str {
    let (start, len) = match n {
        0..10 => (n, 1),
        10..100 => (10 + 2 * (n - 10), 2),
        _ => (190 + 3 * (n - 100), 3),
    };
    &SMALL_DECIMALS[start..start + len]
}

fn write_value<W: fmt::Write + ?Sized>(f: &mut W, value: Value) -> fmt::Result {
    write_decimal(f, "v", false, crate::EntityRef::index(value) as u64)
}

fn write_values<W: fmt::Write + ?Sized>(
    f: &mut W,
    values: impl IntoIterator<Item = Value>,
) -> fmt::Result {
    for (i, value) in values.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write_value(f, value)?;
    }
    Ok(())
}

fn write_list<W: fmt::Write + ?Sized, T: fmt::Display>(
    f: &mut W,
    items: impl IntoIterator<Item = T>,
) -> fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::IntCC;
    use crate::{EntityRef, verify};

    /// `fn sum(p: *const i32, n: i64) -> i64`: sums `n` sign-extended i32s.
    fn build_sum() -> Function {
        let sig = Signature::new(&[Type::Ptr, Type::I64], &[Type::I64]);
        let mut func = Function::new("sum", sig);
        let entry = func.create_block();
        let header = func.create_block();
        let body = func.create_block();
        let exit = func.create_block();
        let end = InsertPoint::End;

        let p = func.append_block_param(entry, Type::Ptr);
        let n = func.append_block_param(entry, Type::I64);
        let zero = func.ins(end(entry)).iconst(Type::I64, 0);
        func.ins(end(entry)).jump(header, &[zero, zero]);

        let i = func.append_block_param(header, Type::I64);
        let acc = func.append_block_param(header, Type::I64);
        let cond = func.ins(end(header)).icmp(IntCC::Ult, i, n);
        func.ins(end(header)).brif(cond, body, &[], exit, &[acc]);

        let four = func.ins(end(body)).iconst(Type::I64, 4);
        let offset = func.ins(end(body)).imul(i, four);
        let addr = func.ins(end(body)).ptradd(p, offset);
        let elem = func.ins(end(body)).load(Type::I32, addr);
        let wide = func.ins(end(body)).sext(Type::I64, elem);
        let acc2 = func.ins(end(body)).iadd(acc, wide);
        let one = func.ins(end(body)).iconst(Type::I64, 1);
        let i2 = func.ins(end(body)).iadd(i, one);
        func.ins(end(body)).jump(header, &[i2, acc2]);

        let result = func.append_block_param(exit, Type::I64);
        func.ins(end(exit)).return_(&[result]);
        func
    }

    fn def_inst(func: &Function, value: Value) -> Inst {
        match func.value_def(value) {
            ValueDef::Result(inst, _) => inst,
            other => panic!("{value} is {other:?}"),
        }
    }

    #[test]
    fn insert_before_and_move() {
        let mut func = build_sum();
        // Hoist `v12 = iconst.i64 1` into the entry block, before its jump.
        let one = Value::new(12);
        let one_inst = def_inst(&func, one);
        let entry = func.entry_block().unwrap();
        let jump = *func.block_insts(entry).last().unwrap();
        func.move_inst(one_inst, InsertPoint::Before(jump));
        // And put a new constant before it.
        let two = func.ins(InsertPoint::Before(one_inst)).iconst(Type::I64, 2);
        let text = func.to_string();
        assert!(
            text.contains(
                "    v2 = iconst.i64 0\n    v15 = iconst.i64 2\n    v12 = iconst.i64 1\n    jump"
            ),
            "{text}"
        );
        assert_eq!(func.inst_block(def_inst(&func, two)), entry);
        verify(&func).unwrap();
    }

    #[test]
    fn removed_definitions_are_reported() {
        let mut func = build_sum();
        func.remove_inst(def_inst(&func, Value::new(12)));
        let errors = verify(&func).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0]
                .message
                .contains("uses v12, whose definition was removed")
        );
    }

    #[test]
    fn remove_block_param_renumbers() {
        let mut func = build_sum();
        let header = Block::new(1);
        let (i, acc) = (Value::new(3), Value::new(4));
        let removed = func.remove_block_param(header, 0);
        assert_eq!(removed, i);
        assert_eq!(func.block_params(header), &[acc]);
        assert_eq!(func.value_def(acc), ValueDef::Param(header, 0));
        assert!(!func.is_value_defined(i));
        assert!(func.is_value_defined(acc));
    }

    #[test]
    fn removing_an_earlier_parameter_preserves_later_aliases() {
        let mut func = build_sum();
        let header = Block::new(1);
        let acc = Value::new(4);
        let zero = Value::new(2);
        func.replace_all_uses(acc, zero);
        func.remove_block_param(header, 0);
        assert_eq!(func.resolve(acc), zero);
        assert_eq!(func.value_def(acc), ValueDef::Alias(zero));
    }

    #[test]
    fn prints_sum_loop() {
        let expected = "\
func @sum(ptr, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64):
    v2 = iconst.i64 0
    jump block1(v2, v2)
block1(v3: i64, v4: i64):
    v5 = icmp.ult v3, v1
    brif v5, block2, block3(v4)
block2:
    v6 = iconst.i64 4
    v7 = imul v3, v6
    v8 = ptradd v0, v7
    v9 = load.i32 v8
    v10 = sext.i64 v9
    v11 = iadd v4, v10
    v12 = iconst.i64 1
    v13 = iadd v3, v12
    jump block1(v13, v11)
block3(v14: i64):
    return v14
}
";
        assert_eq!(build_sum().to_string(), expected);
    }

    #[test]
    fn replace_all_uses_prints_resolved_operands() {
        let mut func = build_sum();
        let (v4, v11) = (Value::new(4), Value::new(11));
        // Pretend a pass proved `v11` equals the incoming accumulator.
        func.replace_all_uses(v11, v4);
        assert_eq!(func.resolve(v11), v4);
        assert!(func.to_string().contains("jump block1(v13, v4)"));
    }

    #[test]
    fn pinned_and_floating() {
        let func = build_sum();
        let pinned: Vec<String> = func
            .blocks()
            .flat_map(|b| func.block_insts(b).to_vec())
            .filter(|&inst| func.inst_data(inst).is_pinned())
            .map(|inst| inst.to_string())
            .collect();
        // jump, brif, load, jump, return
        assert_eq!(pinned, ["inst1", "inst3", "inst7", "inst12", "inst13"]);
    }
}

#[cfg(test)]
mod result_pool_range_tests {
    use super::ResultRange;

    #[test]
    fn range_overflow_is_checked() {
        assert!(std::panic::catch_unwind(|| ResultRange::new(usize::MAX, 1)).is_err());
        assert!(std::panic::catch_unwind(|| ResultRange::new(u32::MAX as usize, 1)).is_err());
        let max = u32::MAX as usize;
        assert_eq!(ResultRange::new(max, 0).as_range(), max..max);
    }
}
