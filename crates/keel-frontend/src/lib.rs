//! SSA construction for frontends: write code with mutable variables and
//! get block parameters where control flow merges.
//!
//! The algorithm is Braun et al., "Simple and Efficient Construction of
//! Static Single Assignment Form" (2013). It needs no dominance frontiers
//! and works while the CFG is still being built:
//!
//! - **Reading** a variable in a block finds its latest definition there.
//!   If there is none, it asks the predecessors: one predecessor means its
//!   value is simply the predecessor's; several mean a new block parameter,
//!   with each predecessor's value passed as the branch argument.
//! - A block is **sealed** once all its predecessors are known (a loop
//!   header after its back edge is built). Reading in an unsealed block
//!   adds an *incomplete* parameter whose arguments are filled in on
//!   sealing.
//! - A parameter whose arguments are all the same value (or itself) is
//!   **trivial**: it is removed and its uses replaced by that value. A
//!   variable that is merely read in a loop (`n` in `while i < n`) gets a
//!   parameter at the loop header that turns out trivial this way.
//! - A variable read where nothing defined it (C's uninitialized locals, or
//!   code after a `return` that no branch reaches) reads as zero.
//!
//! ```text
//! let mut b = FunctionBuilder::new(&mut func);
//! let i = b.declare_var(Type::I64);
//! b.def_var(i, zero);
//! // ... loop header: b.use_var(i) ...
//! b.seal_block(header);   // after the back edge
//! b.finalize();
//! ```

use keel_ir::{
    Block, ConvertOp, EntityRef, Function, InsertPoint, Inst, InstBuilder, SourceLoc, Type, Value,
};
use smallvec::SmallVec;
use std::collections::HashMap;

keel_ir::entity!(
    /// A mutable variable of the source program.
    Variable,
    "var"
);

/// Sparse definitions within one block. Most blocks use only a handful of
/// variables; keep four inline and spill only the definitions actually used.
/// Wide blocks use a map so reverse-order definitions cannot cause quadratic
/// insertion work. Neither representation allocates a variable-by-block matrix.
enum Definitions {
    Small(SmallVec<[(Variable, Value); 4]>),
    Large(HashMap<Variable, Value>),
}

impl Default for Definitions {
    fn default() -> Self {
        Self::Small(SmallVec::new())
    }
}

impl Definitions {
    fn get(&self, var: Variable) -> Option<Value> {
        match self {
            Self::Small(entries) => entries
                .binary_search_by_key(&var, |&(var, _)| var)
                .ok()
                .map(|i| entries[i].1),
            Self::Large(entries) => entries.get(&var).copied(),
        }
    }

    fn insert(&mut self, var: Variable, value: Value) {
        match self {
            Self::Small(entries) => match entries.binary_search_by_key(&var, |&(var, _)| var) {
                Ok(i) => entries[i].1 = value,
                Err(i) if entries.len() < 32 => entries.insert(i, (var, value)),
                Err(_) => {
                    let mut map = HashMap::with_capacity(64);
                    map.extend(std::mem::take(entries));
                    map.insert(var, value);
                    *self = Self::Large(map);
                }
            },
            Self::Large(entries) => {
                entries.insert(var, value);
            }
        }
    }
}

#[derive(Default)]
struct BlockState {
    /// None for blocks not yet registered with the SSA builder (including
    /// gaps created through raw Function access).
    sealed: Option<bool>,
    defs: Definitions,
    incomplete: Vec<(Variable, Value)>,
    preds: Vec<(Inst, usize)>,
}

pub struct FunctionBuilder<'f> {
    func: &'f mut Function,
    cfg_changed: bool,
    current: Option<Block>,
    /// The location instructions built from now on get.
    srcloc: Option<SourceLoc>,
    var_types: Vec<Type>,
    result_tys: Vec<Type>,
    /// SSA state indexed directly by block, with sparse variable definitions.
    blocks: Vec<BlockState>,
    /// Every parameter the builder added, for the final trivial-parameter
    /// cleanup.
    added_params: Vec<(Block, Value)>,
    /// Incoming edges built so far, indexed by destination block. Sealing
    /// moves a sorted bucket into `preds`, preserving source-layout order.
    incoming: Vec<Vec<(Inst, usize)>>,
    /// The branch terminator already indexed for each source block.
    indexed_terms: Vec<Option<Inst>>,
    /// `ins()` lends out a builder; its insertion is observed on the next
    /// source change or seal, once that mutable borrow has ended.
    pending: Option<Block>,
}

impl<'f> FunctionBuilder<'f> {
    pub fn new(func: &'f mut Function) -> Self {
        Self {
            func,
            cfg_changed: true,
            current: None,
            srcloc: None,
            var_types: Vec::new(),
            result_tys: Vec::new(),
            blocks: Vec::new(),
            added_params: Vec::new(),
            incoming: Vec::new(),
            indexed_terms: Vec::new(),
            pending: None,
        }
    }

    /// Shared access to the function under construction.
    pub fn function(&self) -> &Function {
        self.func
    }

    /// Raw mutable access. Any CFG edits are observed before the next
    /// block is sealed. Already sealed blocks must keep their predecessors.
    /// Prefer the builder's helpers for ordinary construction, so unrelated
    /// changes do not require rebuilding the incoming-edge index.
    pub fn function_mut(&mut self) -> &mut Function {
        self.cfg_changed = true;
        self.func
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Value {
        self.func.append_block_param(block, ty)
    }

    pub fn create_stack_slot(&mut self, size: u32, align: u32) -> keel_ir::StackSlot {
        self.func.create_stack_slot(size, align)
    }

    pub fn import_function(&mut self, name: &str, sig: keel_ir::Signature) -> keel_ir::FuncRef {
        self.func.import_function(name, sig)
    }

    pub fn import_data(&mut self, name: &str) -> keel_ir::DataRef {
        self.func.import_data(name)
    }

    pub fn import_signature(&mut self, sig: keel_ir::Signature) -> keel_ir::SigRef {
        self.func.import_signature(sig)
    }

    pub fn create_block(&mut self) -> Block {
        let block = self.func.create_block();
        self.block_state(block).sealed = Some(false);
        block
    }

    pub fn switch_to_block(&mut self, block: Block) {
        self.current = Some(block);
    }

    pub fn current_block(&self) -> Block {
        self.current
            .expect("no current block; call switch_to_block")
    }

    /// Sets the source location of the instructions built after this.
    pub fn set_srcloc(&mut self, loc: Option<SourceLoc>) {
        self.srcloc = loc;
    }

    /// A builder for one instruction at the end of the current block.
    pub fn ins(&mut self) -> InstBuilder<'_> {
        let block = self.current_block();
        if self.pending != Some(block) {
            self.sync_preds();
            self.pending = Some(block);
        }
        self.func
            .ins_with_scratch(InsertPoint::End(block), &mut self.result_tys)
            .loc(self.srcloc)
    }

    pub fn declare_var(&mut self, ty: Type) -> Variable {
        self.var_types.push(ty);
        Variable::new(self.var_types.len() - 1)
    }

    /// Assigns `value` to `var` in the current block.
    pub fn def_var(&mut self, var: Variable, value: Value) {
        let (want, got) = (self.var_types[var.index()], self.func.value_type(value));
        assert_eq!(want, got, "{var} has type {want}, assigned a {got}");
        let block = self.current_block();
        self.block_state(block).defs.insert(var, value);
    }

    /// The value of `var` at the end of the current block so far.
    pub fn use_var(&mut self, var: Variable) -> Value {
        let block = self.current_block();
        self.read(var, block)
    }

    /// Declares that every branch into `block` has been built, and fills
    /// in the parameters added while it was unsealed.
    pub fn seal_block(&mut self, block: Block) {
        if self.block_state(block).sealed.replace(true) == Some(true) {
            return;
        }
        self.sync_preds();
        if self.incoming.len() <= block.index() {
            self.incoming.resize_with(block.index() + 1, Vec::new);
        }
        let mut preds = std::mem::take(&mut self.incoming[block.index()]);
        // Blocks are created in layout order. Construction may visit them
        // in a different order; keep SSA arguments/values deterministic.
        preds.sort_unstable_by_key(|&(inst, dest)| (self.func.inst_block(inst), dest));
        self.blocks[block.index()].preds = preds;
        for (var, param) in std::mem::take(&mut self.blocks[block.index()].incomplete) {
            self.add_param_args(var, block, param);
        }
    }

    pub fn seal_all_blocks(&mut self) {
        let blocks: Vec<Block> = self.func.blocks().collect();
        for block in blocks {
            self.seal_block(block);
        }
    }

    /// Removes the trivial parameters that only became trivial after
    /// others were removed. Every block must be sealed.
    pub fn finalize(mut self) {
        assert!(
            self.blocks.iter().all(|s| s.sealed != Some(false)),
            "finalize needs every block sealed"
        );
        loop {
            let mut changed = false;
            for (block, param) in std::mem::take(&mut self.added_params) {
                if self.func.block_params(block).contains(&param) {
                    if self.try_remove_trivial(block, param) != param {
                        changed = true;
                    } else {
                        self.added_params.push((block, param));
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn block_state(&mut self, block: Block) -> &mut BlockState {
        if self.blocks.len() <= block.index() {
            self.blocks
                .resize_with(block.index() + 1, BlockState::default);
        }
        &mut self.blocks[block.index()]
    }

    fn read(&mut self, var: Variable, block: Block) -> Value {
        if let Some(value) = self.blocks.get(block.index()).and_then(|s| s.defs.get(var)) {
            return self.func.resolve(value);
        }
        let ty = self.var_types[var.index()];
        let value = if !self.blocks[block.index()]
            .sealed
            .expect("unregistered SSA block")
        {
            let param = self.add_param(block, ty);
            self.blocks[block.index()].incomplete.push((var, param));
            param
        } else {
            match self.blocks[block.index()].preds.as_slice() {
                [] => self.zero(block, ty),
                &[(branch, _)] => {
                    let pred = self.func.inst_block(branch);
                    self.read(var, pred)
                }
                _ => {
                    // Define the parameter first, so a loop that reaches
                    // this block again finds it instead of recursing.
                    let param = self.add_param(block, ty);
                    self.blocks[block.index()].defs.insert(var, param);
                    self.add_param_args(var, block, param)
                }
            }
        };
        self.block_state(block).defs.insert(var, value);
        value
    }

    /// A zero of type `ty` at the top of `block`, for a variable read where
    /// nothing defined it.
    fn zero(&mut self, block: Block, ty: Type) -> Value {
        let at = match self.func.block_insts(block).first() {
            Some(&inst) => InsertPoint::Before(inst),
            None => InsertPoint::End(block),
        };
        match ty {
            Type::Ptr => {
                let zero = self.func.ins(at).iconst(Type::I64, 0);
                self.func
                    .ins(at)
                    .convert(ConvertOp::IntToPtr, Type::Ptr, zero)
            }
            Type::F32 => self.func.ins(at).f32const(0.0),
            Type::F64 => self.func.ins(at).f64const(0.0),
            _ => self.func.ins(at).iconst(ty, 0),
        }
    }

    fn add_param(&mut self, block: Block, ty: Type) -> Value {
        let param = self.func.append_block_param(block, ty);
        self.added_params.push((block, param));
        param
    }

    /// Passes each predecessor's value of `var` to `param`, then removes
    /// `param` if it is trivial. Returns what `param` stands for.
    fn add_param_args(&mut self, var: Variable, block: Block, param: Value) -> Value {
        // Reading can add/remove SSA parameters of other blocks, but sealed
        // predecessor edges stay fixed. `var` already has a definition in
        // this block, so no read reaches this parameter's branch arguments.
        // Read every edge first, and write arguments only for a parameter
        // that stays; most are trivial and then need none.
        let edges = self.blocks[block.index()].preds.len();
        let mut values: SmallVec<[Value; 8]> = SmallVec::with_capacity(edges);
        for edge in 0..edges {
            let (branch, _) = self.blocks[block.index()].preds[edge];
            let pred = self.func.inst_block(branch);
            values.push(self.read(var, pred));
        }
        let index = self.param_index(block, param);
        match self.trivial_value(param, values.iter().copied()) {
            Some(same) => self.remove_trivial(block, index, param, same, false),
            None => {
                for (edge, value) in values.into_iter().enumerate() {
                    let (branch, i) = self.blocks[block.index()].preds[edge];
                    let args = &mut self.func.inst_data_mut(branch).branch_dests_mut()[i].args;
                    debug_assert_eq!(args.len(), index, "arguments are added in parameter order");
                    args.push(value);
                }
                param
            }
        }
    }

    fn param_index(&self, block: Block, param: Value) -> usize {
        self.func
            .block_params(block)
            .iter()
            .position(|&p| p == param)
            .expect("parameter of its block")
    }

    /// `Some(value)` if every argument for `param` is that value (ignoring
    /// `param` itself, passed around a loop); `Some(None)` if there is no
    /// such other value; `None` if `param` merges different values.
    fn trivial_value(
        &self,
        param: Value,
        args: impl Iterator<Item = Value>,
    ) -> Option<Option<Value>> {
        let mut same = None;
        for arg in args {
            let arg = self.func.resolve(arg);
            if arg == param || Some(arg) == same {
                continue;
            }
            if same.is_some() {
                return None;
            }
            same = Some(arg);
        }
        Some(same)
    }

    /// If every argument for `param` is the same value (ignoring `param`
    /// itself, passed around a loop), removes it and returns that value.
    fn try_remove_trivial(&mut self, block: Block, param: Value) -> Value {
        let index = self.param_index(block, param);
        let preds = &self.blocks[block.index()].preds;
        let args = preds
            .iter()
            .map(|&(branch, i)| self.func.inst_data(branch).branch_dests()[i].args[index]);
        match self.trivial_value(param, args) {
            Some(same) => self.remove_trivial(block, index, param, same, true),
            None => param,
        }
    }

    /// Removes trivial `param`, parameter `index` of `block`, and its branch
    /// arguments if they were written. `same` is the value it stands for;
    /// none means no branch into the block defines it.
    fn remove_trivial(
        &mut self,
        block: Block,
        index: usize,
        param: Value,
        same: Option<Value>,
        written: bool,
    ) -> Value {
        let ty = self.func.value_type(param);
        let same = same.unwrap_or_else(|| self.zero(block, ty));
        self.func.remove_block_param(block, index);
        if written {
            for &(branch, i) in &self.blocks[block.index()].preds {
                self.func.inst_data_mut(branch).branch_dests_mut()[i]
                    .args
                    .remove(index);
            }
        }
        self.func.replace_all_uses(param, same);
        same
    }

    /// Observe the instruction builder most recently lent out. Raw mutable
    /// access can change any source, so rebuild once after such access.
    fn sync_preds(&mut self) {
        if self.cfg_changed {
            self.rebuild_preds();
        } else if let Some(block) = self.pending.take() {
            let term = self.branch_terminator(block);
            if self.indexed_terms.len() <= block.index() {
                self.indexed_terms.resize(block.index() + 1, None);
            }
            match (self.indexed_terms[block.index()], term) {
                (old, new) if old == new => {}
                (None, Some(term)) => self.index_term(block, term),
                // Appending after an indexed branch can temporarily hide
                // that branch. Preserve the old last-instruction semantics.
                _ => self.rebuild_preds(),
            }
        }
    }

    fn branch_terminator(&self, block: Block) -> Option<Inst> {
        let &term = self.func.block_insts(block).last()?;
        (!self.func.inst_data(term).branch_dests().is_empty()).then_some(term)
    }

    fn index_term(&mut self, block: Block, term: Inst) {
        if self.indexed_terms.len() <= block.index() {
            self.indexed_terms.resize(block.index() + 1, None);
        }
        self.indexed_terms[block.index()] = Some(term);
        for (i, dest) in self.func.inst_data(term).branch_dests().iter().enumerate() {
            if self.incoming.len() <= dest.block.index() {
                self.incoming.resize_with(dest.block.index() + 1, Vec::new);
            }
            self.incoming[dest.block.index()].push((term, i));
        }
    }

    fn rebuild_preds(&mut self) {
        for edges in &mut self.incoming {
            edges.clear();
        }
        self.indexed_terms.fill(None);
        let blocks: Vec<_> = self.func.blocks().collect();
        for block in blocks {
            if let Some(term) = self.branch_terminator(block) {
                self.index_term(block, term);
            }
        }
        self.cfg_changed = false;
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_ir::{BlockCall, InstData, Signature};

    #[test]
    fn sparse_definitions_survive_updates_spilling_and_promotion() {
        let mut defs = Definitions::default();
        let mut reference = HashMap::new();
        for (i, key) in [900_000, 7, 40_000, 42].into_iter().enumerate() {
            defs.insert(Variable::new(key), Value::new(i));
            reference.insert(Variable::new(key), Value::new(i));
        }
        assert!(matches!(&defs, Definitions::Small(entries) if !entries.spilled()));
        // Updates and sparse IDs must not consume more slots or force a spill.
        defs.insert(Variable::new(900_000), Value::new(12));
        reference.insert(Variable::new(900_000), Value::new(12));
        assert!(
            matches!(&defs, Definitions::Small(entries) if entries.len() == 4 && !entries.spilled())
        );
        assert_eq!(defs.get(Variable::new(899_999)), None);
        // Reverse insertion exercises sorted movement and the wide-block
        // fallback; repeated overwrites must preserve the latest definition.
        for i in (0..96).rev().chain(0..96) {
            let var = Variable::new(i * 1_001);
            let value = Value::new(i + reference.len());
            defs.insert(var, value);
            reference.insert(var, value);
            for (&key, &want) in &reference {
                assert_eq!(defs.get(key), Some(want));
            }
        }
        assert!(matches!(&defs, Definitions::Large(entries) if entries.len() == reference.len()));
    }

    #[test]
    fn raw_block_gaps_do_not_register_unused_ssa_state() {
        let mut func = Function::new("gaps", Signature::new(&[], &[Type::I64]));
        let raw_entry = func.create_block();
        let zero = func.ins(InsertPoint::End(raw_entry)).iconst(Type::I64, 0);
        func.ins(InsertPoint::End(raw_entry)).return_(&[zero]);
        let mut b = FunctionBuilder::new(&mut func);
        let block = b.create_block();
        let var = b.declare_var(Type::I64);
        b.switch_to_block(block);
        b.seal_block(block);
        let result = b.use_var(var);
        b.ins().return_(&[result]);
        assert_eq!(b.blocks[raw_entry.index()].sealed, None);
        b.finalize();
        keel_ir::verify(&func).unwrap();
    }

    // Compare the incremental index to the original full scan after every
    // operation, including edits that bypass FunctionBuilder::ins().
    fn check_index(b: &mut FunctionBuilder<'_>) {
        b.sync_preds();
        for dest in b.func.blocks() {
            let mut expected = Vec::new();
            for src in b.func.blocks() {
                if let Some(&term) = b.func.block_insts(src).last() {
                    for (i, call) in b.func.inst_data(term).branch_dests().iter().enumerate() {
                        if call.block == dest {
                            expected.push((term, i));
                        }
                    }
                }
            }
            let mut got = b.incoming.get(dest.index()).cloned().unwrap_or_default();
            got.sort_unstable_by_key(|&(inst, i)| (b.func.inst_block(inst), i));
            assert_eq!(got, expected, "incoming edges of {dest}");
        }
    }

    #[test]
    fn indexed_edges_follow_raw_cfg_edits() {
        let mut func = Function::new("edges", Signature::new(&[], &[]));
        let mut b = FunctionBuilder::new(&mut func);
        let blocks: Vec<_> = (0..8).map(|_| b.create_block()).collect();
        // Visit sources out of layout order and include duplicate edges.
        for i in [5, 1, 7, 0, 3, 2, 6, 4] {
            b.switch_to_block(blocks[i]);
            let cond = b.ins().iconst(Type::I8, 1);
            b.ins()
                .brif(cond, blocks[(i + 1) % 8], &[], blocks[(i + 1) % 8], &[]);
            check_index(&mut b);
        }
        let term = *b.func.block_insts(blocks[5]).last().unwrap();
        b.function_mut().inst_data_mut(term).branch_dests_mut()[1].block = blocks[2];
        check_index(&mut b);
        b.function_mut().remove_inst(term);
        check_index(&mut b);
        let moved = *b.func.block_insts(blocks[6]).last().unwrap();
        b.function_mut()
            .move_inst(moved, InsertPoint::End(blocks[3]));
        check_index(&mut b);
        b.function_mut().remove_block(blocks[7]);
        check_index(&mut b);
        let extra = b.function_mut().create_block();
        let arg = b.func.inst_results(b.func.block_insts(blocks[0])[0])[0];
        b.function_mut().ins(InsertPoint::End(extra)).switch(
            arg,
            BlockCall {
                block: blocks[2],
                args: vec![],
            },
            vec![
                (
                    1,
                    BlockCall {
                        block: blocks[2],
                        args: vec![],
                    },
                ),
                (
                    2,
                    BlockCall {
                        block: blocks[4],
                        args: vec![],
                    },
                ),
            ],
        );
        check_index(&mut b);
        // Replacing the entire function is also a mutable dereference.
        *b.function_mut() = Function::new("replacement", Signature::new(&[], &[]));
        let block = b.function_mut().create_block();
        b.function_mut().append_inst(
            block,
            InstData::Jump {
                dest: BlockCall {
                    block,
                    args: vec![],
                },
            },
        );
        check_index(&mut b);
    }

    #[test]
    fn revisiting_sources_and_hiding_terminators() {
        let mut func = Function::new("edges", Signature::new(&[], &[]));
        let mut b = FunctionBuilder::new(&mut func);
        let a = b.create_block();
        let c = b.create_block();
        b.switch_to_block(a);
        b.ins().jump(c, &[]);
        check_index(&mut b);
        // A non-branch at the end hides the previous branch, just as in
        // the original scan, even in this temporarily invalid block.
        b.ins().iconst(Type::I64, 0);
        check_index(&mut b);
        b.ins().jump(c, &[]);
        check_index(&mut b);
        b.switch_to_block(c);
        let unused_builder = b.ins();
        drop(unused_builder);
        check_index(&mut b);
        b.switch_to_block(a);
        let unused_builder = b.ins();
        drop(unused_builder);
        check_index(&mut b);
    }
}
