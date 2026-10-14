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
//!
//! ```text
//! let mut b = FunctionBuilder::new(&mut func);
//! let i = b.declare_var(Type::I64);
//! b.def_var(i, zero);
//! // ... loop header: b.use_var(i) ...
//! b.seal_block(header);   // after the back edge
//! b.finalize();
//! ```

use keel_ir::{Block, EntityRef, Function, InsertPoint, Inst, InstBuilder, SourceLoc, Type, Value};
use std::collections::HashMap;

keel_ir::entity!(
    /// A mutable variable of the source program.
    Variable,
    "var"
);

pub struct FunctionBuilder<'f> {
    pub func: &'f mut Function,
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
}

impl<'f> FunctionBuilder<'f> {
    pub fn new(func: &'f mut Function) -> Self {
        Self {
            func,
            current: None,
            srcloc: None,
            var_types: Vec::new(),
            defs: HashMap::new(),
            sealed: HashMap::new(),
            incomplete: HashMap::new(),
            preds: HashMap::new(),
            added_params: Vec::new(),
        }
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
        let preds = self.find_preds(block);
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
                [] => panic!("{var} is used in {block} before it is defined"),
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
        let same = same.unwrap_or_else(|| panic!("{param} in {block} has no value besides itself"));
        self.func.remove_block_param(block, index);
        for &(branch, i) in &self.preds[&block] {
            self.func.inst_data_mut(branch).branch_dests_mut()[i]
                .args
                .remove(index);
        }
        self.func.replace_all_uses(param, same);
        same
    }

    /// Branch edges into `block`, read from the terminators built so far.
    fn find_preds(&self, block: Block) -> Vec<(Inst, usize)> {
        let mut preds = Vec::new();
        for b in self.func.blocks() {
            let Some(&term) = self.func.block_insts(b).last() else {
                continue;
            };
            for (i, dest) in self.func.inst_data(term).branch_dests().iter().enumerate() {
                if dest.block == block {
                    preds.push((term, i));
                }
            }
        }
        preds
    }
}
