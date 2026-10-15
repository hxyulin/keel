//! A function: arenas of blocks, instructions and values, plus block order.
//!
//! All mutation goes through methods so invariants (result values, aliases)
//! stay in one place.
//!
//! Removing an instruction or block takes it out of the layout but leaves
//! its arena entry (handles stay valid, and nothing is renumbered). Values
//! it defined still exist; using one is an error the verifier reports.
//! Blocks store their instructions in a `Vec`, so inserting or removing in
//! the middle of a block is linear in its size; fine for now, and the
//! methods hide the representation if it needs to change.

use crate::inst::{BlockCall, Clause, InstData, Symbol};
use crate::layout::Layout;
use crate::{Arena, Block, DataRef, FuncRef, Inst, SigRef, SourceFile, StackSlot, Type, Value};
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CallConv {
    /// The target's C calling convention.
    C,
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
#[derive(Clone, PartialEq, Eq, Debug)]
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
#[derive(Clone, PartialEq, Eq, Debug)]
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

#[derive(Clone, PartialEq, Eq, Debug)]
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
    pub inlinehint: bool,
    pub alwaysinline: bool,
    pub noinline: bool,
    pub noreturn: bool,
    pub nounwind: bool,
}

impl FunctionAttrs {
    pub fn merge(&mut self, other: Self) {
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

#[derive(Clone, Debug, Default)]
struct BlockData {
    params: Vec<Value>,
    insts: Vec<Inst>,
}

#[derive(Clone, Debug)]
struct InstNode {
    data: InstData,
    results: Vec<Value>,
    /// `None` once removed.
    block: Option<Block>,
    loc: Option<SourceLoc>,
}

/// Where a new or moved instruction goes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InsertPoint {
    /// After the last instruction of the block.
    End(Block),
    /// Just before an instruction already in the layout.
    Before(Inst),
}

#[derive(Clone, Debug)]
pub struct Function {
    pub attrs: FunctionAttrs,
    pub name: String,
    pub sig: Signature,
    blocks: Arena<Block, BlockData>,
    insts: Arena<Inst, InstNode>,
    values: Arena<Value, ValueData>,
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
    /// Block order. The first block is the entry.
    layout: Vec<Block>,
    /// The function that interprets this function's landing pads while
    /// an exception unwinds, such as C++'s `__gxx_personality_v0`. Needed
    /// for `invoke`.
    pub personality: Option<String>,
    /// Visible only within its module, like C's `static`: not exported to
    /// the linker, so other modules may use the name for their own.
    pub local: bool,
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

impl Function {
    pub fn new(name: impl Into<String>, sig: Signature) -> Self {
        Self {
            attrs: FunctionAttrs::default(),
            name: name.into(),
            sig,
            blocks: Arena::new(),
            insts: Arena::new(),
            values: Arena::new(),
            stack_slots: Arena::new(),
            callees: Arena::new(),
            data_symbols: Arena::new(),
            signatures: Arena::new(),
            layout: Vec::new(),
            personality: None,
            local: false,
            debug_return: None,
            debug_loc: None,
            debug_scopes: Vec::new(),
            debug_vars: Vec::new(),
        }
    }

    pub fn create_stack_slot(&mut self, size: u32, align: u32) -> StackSlot {
        self.stack_slots.push(StackSlotData { size, align })
    }

    pub fn stack_slots(&self) -> impl Iterator<Item = (StackSlot, StackSlotData)> + '_ {
        self.stack_slots.iter().map(|(ss, data)| (ss, *data))
    }

    pub fn num_stack_slots(&self) -> usize {
        self.stack_slots.len()
    }

    /// Returns a reference for calling `name`, reusing an existing one.
    pub fn import_function(&mut self, name: &str, sig: Signature) -> FuncRef {
        if let Some((func_ref, _)) = self.callees.iter().find(|(_, d)| d.name == name) {
            return func_ref;
        }
        self.callees.push(FuncDecl {
            attrs: FunctionAttrs::default(),
            name: name.to_string(),
            sig,
        })
    }

    pub fn callee_mut(&mut self, func_ref: FuncRef) -> &mut FuncDecl {
        &mut self.callees[func_ref]
    }

    pub fn callee(&self, func_ref: FuncRef) -> &FuncDecl {
        &self.callees[func_ref]
    }

    pub fn callees(&self) -> impl Iterator<Item = (FuncRef, &FuncDecl)> {
        self.callees.iter()
    }

    /// Returns a reference to data object `name`, reusing an existing one.
    pub fn import_data(&mut self, name: &str) -> DataRef {
        if let Some((data_ref, _)) = self.data_symbols.iter().find(|(_, n)| *n == name) {
            return data_ref;
        }
        self.data_symbols.push(name.to_string())
    }

    pub fn data_symbols(&self) -> impl Iterator<Item = (DataRef, &str)> {
        self.data_symbols.iter().map(|(r, name)| (r, name.as_str()))
    }

    /// Returns a reference to `sig` for indirect calls, reusing an existing
    /// one.
    pub fn import_signature(&mut self, sig: Signature) -> SigRef {
        if let Some((sig_ref, _)) = self.signatures.iter().find(|(_, s)| **s == sig) {
            return sig_ref;
        }
        self.signatures.push(sig)
    }

    pub fn signature(&self, sig_ref: SigRef) -> &Signature {
        &self.signatures[sig_ref]
    }

    pub fn signatures(&self) -> impl Iterator<Item = (SigRef, &Signature)> {
        self.signatures.iter()
    }

    /// The linker name of `symbol`.
    pub fn symbol_name(&self, symbol: Symbol) -> &str {
        match symbol {
            Symbol::Func(func_ref) => &self.callees[func_ref].name,
            Symbol::Data(data_ref) => &self.data_symbols[data_ref],
        }
    }

    /// Creates a block at the end of the layout.
    pub fn create_block(&mut self) -> Block {
        let block = self.blocks.push(BlockData::default());
        self.layout.push(block);
        block
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Value {
        let n = self.blocks[block].params.len() as u32;
        let value = self.values.push(ValueData {
            ty,
            def: ValueDef::Param(block, n),
        });
        self.blocks[block].params.push(value);
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
        let mut result_tys = Vec::new();
        self.for_each_result_type(&data, |ty| result_tys.push(ty));
        let inst = self.create_inst(data, &result_tys);
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
        let inst = self.insts.push(InstNode {
            data,
            results: Vec::new(),
            block: None,
            loc: None,
        });
        for (n, &ty) in result_tys.iter().enumerate() {
            let value = self.values.push(ValueData {
                ty,
                def: ValueDef::Result(inst, n as u32),
            });
            self.insts[inst].results.push(value);
        }
        inst
    }

    /// Puts an instruction that is not in the layout at `at`.
    fn place(&mut self, inst: Inst, at: InsertPoint) {
        debug_assert!(self.insts[inst].block.is_none());
        let (block, index) = match at {
            InsertPoint::End(block) => (block, self.blocks[block].insts.len()),
            InsertPoint::Before(before) => {
                let block = self.inst_block(before);
                (block, self.position(before))
            }
        };
        self.blocks[block].insts.insert(index, inst);
        self.insts[inst].block = Some(block);
    }

    /// Index of `inst` within its block.
    fn position(&self, inst: Inst) -> usize {
        let block = self.inst_block(inst);
        self.blocks[block]
            .insts
            .iter()
            .position(|&i| i == inst)
            .expect("an instruction is in its block")
    }

    /// Takes `inst` out of the layout. Its results must be unused (or
    /// replaced with [`Function::replace_all_uses`]) by then.
    pub fn remove_inst(&mut self, inst: Inst) {
        let block = self.inst_block(inst);
        let index = self.position(inst);
        self.blocks[block].insts.remove(index);
        self.insts[inst].block = None;
    }

    /// Moves `inst` to `at`, keeping its results.
    pub fn move_inst(&mut self, inst: Inst, at: InsertPoint) {
        self.remove_inst(inst);
        self.place(inst, at);
    }

    /// Whether `inst` has been removed from the layout.
    pub fn is_removed(&self, inst: Inst) -> bool {
        self.insts[inst].block.is_none()
    }

    /// Mutable access to an instruction, for rewriting operands or
    /// replacing it with another instruction **with the same result
    /// types** (e.g. folding an `iadd` to an `iconst`).
    /// Rewriting data must preserve the result count and types. To change
    /// the result signature, insert a new instruction, replace its uses and
    /// remove the old one. The verifier checks this contract after passes.
    pub fn inst_data_mut(&mut self, inst: Inst) -> &mut InstData {
        &mut self.insts[inst].data
    }

    /// The source location `inst` was compiled from, if known.
    pub fn inst_loc(&self, inst: Inst) -> Option<SourceLoc> {
        self.insts[inst].loc
    }

    pub fn set_inst_loc(&mut self, inst: Inst, loc: Option<SourceLoc>) {
        self.insts[inst].loc = loc;
    }

    /// Takes `block` and its instructions out of the layout. Nothing may
    /// branch to it any more.
    pub fn remove_block(&mut self, block: Block) {
        assert_ne!(
            self.entry_block(),
            Some(block),
            "cannot remove the entry block"
        );
        for inst in std::mem::take(&mut self.blocks[block].insts) {
            self.insts[inst].block = None;
        }
        self.layout.retain(|&b| b != block);
    }

    /// Whether `block` is in the layout.
    pub fn is_block_inserted(&self, block: Block) -> bool {
        self.layout.contains(&block)
    }

    /// Removes parameter `index` of `block` and returns its value, which
    /// must be unused (or replaced). Branches to `block` still pass the
    /// argument; the caller removes it from each of them.
    pub fn remove_block_param(&mut self, block: Block, index: usize) -> Value {
        let value = self.blocks[block].params.remove(index);
        for (n, &param) in self.blocks[block].params.iter().enumerate().skip(index) {
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
    pub fn resolve(&self, mut value: Value) -> Value {
        while let ValueDef::Alias(next) = self.values[value].def {
            value = next;
        }
        value
    }

    /// Number of values ever created; value indices are below this.
    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    /// Number of instructions ever created; instruction indices are below
    /// this.
    pub fn num_insts(&self) -> usize {
        self.insts.len()
    }

    /// Number of blocks ever created; block indices are below this.
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn entry_block(&self) -> Option<Block> {
        self.layout.first().copied()
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.layout.iter().copied()
    }

    pub fn block_params(&self, block: Block) -> &[Value] {
        &self.blocks[block].params
    }

    pub fn block_insts(&self, block: Block) -> &[Inst] {
        &self.blocks[block].insts
    }

    pub fn inst_data(&self, inst: Inst) -> &InstData {
        &self.insts[inst].data
    }

    pub fn inst_results(&self, inst: Inst) -> &[Value] {
        &self.insts[inst].results
    }

    /// The block containing `inst`. Panics if it was removed.
    pub fn inst_block(&self, inst: Inst) -> Block {
        self.insts[inst].block.expect("instruction was removed")
    }

    /// Every instruction in the layout, block by block.
    pub fn layout_insts(&self) -> impl Iterator<Item = (Block, Inst)> + '_ {
        self.blocks()
            .flat_map(move |b| self.block_insts(b).iter().map(move |&i| (b, i)))
    }

    /// Whether `value` is still defined: by an instruction in the layout,
    /// or as a parameter its block still has. Aliases resolve first.
    pub fn is_value_defined(&self, value: Value) -> bool {
        match self.value_def(self.resolve(value)) {
            ValueDef::Result(inst, _) => !self.is_removed(inst),
            ValueDef::Param(block, n) => {
                self.is_block_inserted(block)
                    && self.blocks[block].params.get(n as usize) == Some(&self.resolve(value))
            }
            ValueDef::Alias(_) => unreachable!("resolved"),
        }
    }

    /// The single result of `inst`. Panics if it has none.
    pub fn first_result(&self, inst: Inst) -> Value {
        self.insts[inst].results[0]
    }

    pub fn value_type(&self, value: Value) -> Type {
        self.values[value].ty
    }

    pub fn value_def(&self, value: Value) -> ValueDef {
        self.values[value].def
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "func @{}{}{}", self.name, self.sig, self.attrs)?;
        if self.local {
            f.write_str(" local")?;
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

    fn write_inst(&self, f: &mut fmt::Formatter<'_>, inst: Inst) -> fmt::Result {
        let results = self.inst_results(inst);
        if !results.is_empty() {
            write_list(f, results)?;
            f.write_str(" = ")?;
        }
        // Operands are printed resolved, so aliases never show up in text.
        let v = |value: Value| self.resolve(value);
        match self.inst_data(inst) {
            InstData::Iconst { ty, imm } => write!(f, "iconst.{ty} {imm}"),
            InstData::Fconst { ty, bits } => {
                write!(f, "fconst.{ty} {}", crate::inst::float_text(*ty, *bits))
            }
            InstData::StackAddr { slot } => write!(f, "stack_addr {slot}"),
            InstData::Call { func, args } => {
                write!(f, "call @{}(", self.callees[*func].name)?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                f.write_str(")")
            }
            InstData::SymbolAddr { symbol } => {
                write!(f, "symbol_addr @{}", self.symbol_name(*symbol))
            }
            InstData::Invoke { func, args, dests } => {
                write!(f, "invoke @{}(", self.callees[*func].name)?;
                write_list(f, args.iter().map(|&a| v(a)))?;
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
            InstData::ReturnCall { func, args } => {
                write!(f, "return_call @{}(", self.callees[*func].name)?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                f.write_str(")")
            }
            InstData::ReturnCallIndirect { sig, callee, args } => {
                write!(f, "return_call_indirect {}(", v(*callee))?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                write!(f, ") : {}", self.signatures[*sig])
            }
            InstData::CallIndirect { sig, callee, args } => {
                write!(f, "call_indirect {}(", v(*callee))?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                write!(f, ") : {}", self.signatures[*sig])
            }
            InstData::Intrinsic { id, args } => {
                write!(f, "intrinsic {id}(")?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                f.write_str(")")
            }
            InstData::InlineAsm { asm, args } => {
                f.write_str("asm ")?;
                crate::data::write_quoted(f, asm.template.as_bytes())?;
                f.write_str(", ")?;
                crate::data::write_quoted(f, asm.constraints().as_bytes())?;
                f.write_str("(")?;
                write_list(f, args.iter().map(|&a| v(a)))?;
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
            InstData::VectorSplat { ty, arg } => write!(f, "vsplat.{ty} {}", v(*arg)),
            InstData::VectorUnary { op, arg } => write!(f, "v{op} {}", v(*arg)),
            InstData::VectorBinary { op, args } => write!(f, "{op} {}, {}", v(args[0]), v(args[1])),
            InstData::VectorExtract { arg, lane } => write!(f, "vextract {}, {lane}", v(*arg)),
            InstData::VectorInsert { args, lane } => {
                write!(f, "vinsert {}, {}, {lane}", v(args[0]), v(args[1]))
            }
            InstData::VectorShuffle { args, lanes } => {
                write!(f, "vshuffle {}, {}, [", v(args[0]), v(args[1]))?;
                write_list(f, lanes.iter())?;
                f.write_str("]")
            }
            InstData::VectorIcmp { cond, args } => {
                write!(f, "vicmp.{cond} {}, {}", v(args[0]), v(args[1]))
            }
            InstData::VectorFcmp { cond, args } => {
                write!(f, "vfcmp.{cond} {}, {}", v(args[0]), v(args[1]))
            }
            InstData::VectorSelect { args } => {
                write!(f, "vselect {}, {}, {}", v(args[0]), v(args[1]), v(args[2]))
            }
            InstData::Unary { op, arg } => write!(f, "{op} {}", v(*arg)),
            InstData::Binary { op, args } => write!(f, "{op} {}, {}", v(args[0]), v(args[1])),
            InstData::Overflow { op, args } => write!(f, "{op} {}, {}", v(args[0]), v(args[1])),
            InstData::Fma { args } => {
                write!(f, "fma {}, {}, {}", v(args[0]), v(args[1]), v(args[2]))
            }
            InstData::Select { args } => {
                write!(f, "select {}, {}, {}", v(args[0]), v(args[1]), v(args[2]))
            }
            InstData::Convert { op, ty, arg } => write!(f, "{op}.{ty} {}", v(*arg)),
            InstData::Icmp { cond, args } => {
                write!(f, "icmp.{cond} {}, {}", v(args[0]), v(args[1]))
            }
            InstData::Fcmp { cond, args } => {
                write!(f, "fcmp.{cond} {}, {}", v(args[0]), v(args[1]))
            }
            InstData::Load { ty, addr, flags } => write!(f, "load.{ty} {flags}{}", v(*addr)),
            InstData::VaArg { ty, list } => write!(f, "va_arg.{ty} {}", v(*list)),
            InstData::Store { value, addr, flags } => {
                write!(f, "store {flags}{}, {}", v(*value), v(*addr))
            }
            InstData::AtomicLoad { ty, ordering, addr } => {
                write!(f, "atomic_load.{ty} {ordering} {}", v(*addr))
            }
            InstData::AtomicStore {
                ordering,
                value,
                addr,
            } => write!(f, "atomic_store {ordering} {}, {}", v(*value), v(*addr)),
            InstData::AtomicRmw { op, ordering, args } => {
                write!(
                    f,
                    "atomic_rmw.{op} {ordering} {}, {}",
                    v(args[0]),
                    v(args[1])
                )
            }
            InstData::AtomicCas { ordering, args } => write!(
                f,
                "atomic_cas {ordering} {}, {}, {}",
                v(args[0]),
                v(args[1]),
                v(args[2])
            ),
            InstData::Fence { ordering } => write!(f, "fence {ordering}"),
            InstData::Jump { dest } => {
                f.write_str("jump ")?;
                self.write_block_call(f, dest)
            }
            InstData::Brif { cond, dests } => {
                write!(f, "brif {}, ", v(*cond))?;
                self.write_block_call(f, &dests[0])?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[1])
            }
            InstData::Switch { arg, dests, cases } => {
                write!(f, "switch {}, ", v(*arg))?;
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
            InstData::Return { args } => {
                f.write_str("return")?;
                if !args.is_empty() {
                    f.write_str(" ")?;
                    write_list(f, args.iter().map(|&a| v(a)))?;
                }
                Ok(())
            }
        }
    }

    fn write_block_call(&self, f: &mut fmt::Formatter<'_>, call: &BlockCall) -> fmt::Result {
        write!(f, "{}", call.block)?;
        if !call.args.is_empty() {
            f.write_str("(")?;
            write_list(f, call.args.iter().map(|&a| self.resolve(a)))?;
            f.write_str(")")?;
        }
        Ok(())
    }
}

fn write_list<T: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
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
