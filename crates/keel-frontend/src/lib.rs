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
use std::collections::HashMap;

keel_ir::entity!(
    /// A mutable variable of the source program.
    Variable,
    "var"
);

pub struct FunctionBuilder<'f> {
    func: &'f mut Function,
    cfg_changed: bool,
    current: Option<Block>,
    /// The location instructions built from now on get.
    srcloc: Option<SourceLoc>,
    var_types: Vec<Type>,
    /// The latest definition of each variable in each block.
    defs: HashMap<(Variable, Block), Value>,
    sealed: HashMap<Block, bool>,
    /// Parameters added to unsealed blocks, filled in by `seal_block`.
    incomplete: HashMap<Block, Vec<(Variable, Value)>>,
    /// Branch edges into each sealed block: (branch, destination index).
    preds: HashMap<Block, Vec<(Inst, usize)>>,
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
            defs: HashMap::new(),
            sealed: HashMap::new(),
            incomplete: HashMap::new(),
            preds: HashMap::new(),
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
        self.sealed.insert(block, false);
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
        self.func.ins(InsertPoint::End(block)).loc(self.srcloc)
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
        self.defs.insert((var, block), value);
    }

    /// The value of `var` at the end of the current block so far.
    pub fn use_var(&mut self, var: Variable) -> Value {
        let block = self.current_block();
        self.read(var, block)
    }

    /// Declares that every branch into `block` has been built, and fills
    /// in the parameters added while it was unsealed.
    pub fn seal_block(&mut self, block: Block) {
        if self.sealed.insert(block, true) == Some(true) {
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
        self.preds.insert(block, preds);
        for (var, param) in self.incomplete.remove(&block).unwrap_or_default() {
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
            self.sealed.values().all(|&s| s),
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

    fn read(&mut self, var: Variable, block: Block) -> Value {
        if let Some(&value) = self.defs.get(&(var, block)) {
            return self.func.resolve(value);
        }
        let ty = self.var_types[var.index()];
        let value = if !self.sealed[&block] {
            let param = self.add_param(block, ty);
            self.incomplete.entry(block).or_default().push((var, param));
            param
        } else {
            match self.preds[&block].as_slice() {
                [] => self.zero(block, ty),
                &[(branch, _)] => {
                    let pred = self.func.inst_block(branch);
                    self.read(var, pred)
                }
                _ => {
                    // Define the parameter first, so a loop that reaches
                    // this block again finds it instead of recursing.
                    let param = self.add_param(block, ty);
                    self.defs.insert((var, block), param);
                    self.add_param_args(var, block, param)
                }
            }
        };
        self.defs.insert((var, block), value);
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
        for (branch, i) in self.preds[&block].clone() {
            let pred = self.func.inst_block(branch);
            let value = self.read(var, pred);
            let index = self.param_index(block, param);
            let args = &mut self.func.inst_data_mut(branch).branch_dests_mut()[i].args;
            debug_assert_eq!(args.len(), index, "arguments are added in parameter order");
            args.push(value);
        }
        self.try_remove_trivial(block, param)
    }

    fn param_index(&self, block: Block, param: Value) -> usize {
        self.func
            .block_params(block)
            .iter()
            .position(|&p| p == param)
            .expect("parameter of its block")
    }

    /// If every argument for `param` is the same value (ignoring `param`
    /// itself, passed around a loop), removes it and returns that value.
    fn try_remove_trivial(&mut self, block: Block, param: Value) -> Value {
        let index = self.param_index(block, param);
        let mut same = None;
        for &(branch, i) in &self.preds[&block] {
            let arg = self
                .func
                .resolve(self.func.inst_data(branch).branch_dests()[i].args[index]);
            if arg == param || Some(arg) == same {
                continue;
            }
            if same.is_some() {
                return param;
            }
            same = Some(arg);
        }
        // No value besides itself: no branch into the block defines it.
        let ty = self.func.value_type(param);
        let same = same.unwrap_or_else(|| self.zero(block, ty));
        self.func.remove_block_param(block, index);
        for &(branch, i) in &self.preds[&block] {
            self.func.inst_data_mut(branch).branch_dests_mut()[i]
                .args
                .remove(index);
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
