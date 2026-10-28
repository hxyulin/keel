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

use keel_ir::fxhash::FxHashMap as HashMap;
use keel_ir::{
    Block, ConvertOp, EntityRef, Function, InsertPoint, Inst, InstBuilder, SourceLoc, Type, Value,
};
use smallvec::SmallVec;

keel_ir::entity!(
    /// A mutable variable of the source program.
    Variable,
    "var"
);

/// Sparse definitions within one block. Most blocks use only a handful of
/// variables; keep eight inline, unsorted, and spill only the definitions
/// actually used. A short linear scan beats keeping them sorted, which
/// moved entries on most insertions. Wide blocks use a map so lookups stay
/// cheap. Neither representation allocates a variable-by-block matrix.
#[derive(Default, Debug)]
struct Definitions {
    /// Bit `var % 64` is set for each variable defined here, so most
    /// lookups of an undefined variable (the common case while a read
    /// walks up to its definition) need no scan.
    mask: u64,
    entries: Entries,
}

#[derive(Debug)]
enum Entries {
    Small(SmallVec<[(Variable, Value); 8]>),
    Large(HashMap<Variable, Value>),
}

impl Default for Entries {
    fn default() -> Self {
        Self::Small(SmallVec::new())
    }
}

fn bit(var: Variable) -> u64 {
    1 << (var.index() % 64)
}

impl Definitions {
    fn get(&self, var: Variable) -> Option<Value> {
        if self.mask & bit(var) == 0 {
            return None;
        }
        match &self.entries {
            Entries::Small(entries) => entries.iter().find(|e| e.0 == var).map(|e| e.1),
            Entries::Large(entries) => entries.get(&var).copied(),
        }
    }

    fn insert(&mut self, var: Variable, value: Value) {
        let known = self.mask & bit(var) != 0;
        self.mask |= bit(var);
        match &mut self.entries {
            Entries::Small(entries) => {
                match known
                    .then(|| entries.iter().position(|e| e.0 == var))
                    .flatten()
                {
                    Some(i) => entries[i].1 = value,
                    None if entries.len() < 32 => entries.push((var, value)),
                    None => {
                        let mut map = HashMap::with_capacity_and_hasher(64, Default::default());
                        map.extend(std::mem::take(entries));
                        map.insert(var, value);
                        self.entries = Entries::Large(map);
                    }
                }
            }
            Entries::Large(entries) => {
                entries.insert(var, value);
            }
        }
    }

    /// Empties the table, keeping a spilled small table's allocation.
    fn clear(&mut self) {
        self.mask = 0;
        match &mut self.entries {
            Entries::Small(entries) => entries.clear(),
            Entries::Large(_) => self.entries = Entries::default(),
        }
    }
}

#[derive(Default, Debug)]
struct BlockState {
    /// None for blocks not yet registered with the SSA builder (including
    /// gaps created through raw Function access).
    sealed: Option<bool>,
    defs: Definitions,
    incomplete: Vec<(Variable, Value)>,
    preds: Vec<(Inst, usize)>,
    /// Its predecessors are being read for a variable, with no parameter
    /// yet. A read that comes back here around a loop creates it.
    visiting: bool,
}

impl BlockState {
    /// Back to the unregistered state, keeping allocations.
    fn clear(&mut self) {
        self.sealed = None;
        self.defs.clear();
        self.incomplete.clear();
        self.preds.clear();
        self.visiting = false;
    }
}

/// Allocations a [`FunctionBuilder`] can hand on to the next one, so
/// building many functions does not reallocate its per-block tables for
/// each. [`FunctionBuilder::finalize_with_context`] returns it; pass it to
/// [`FunctionBuilder::with_context`]. It carries no state between
/// functions: the IR built is the same as with [`FunctionBuilder::new`].
#[derive(Default, Debug)]
pub struct FunctionBuilderContext {
    var_types: Vec<Type>,
    result_tys: Vec<Type>,
    /// SSA state indexed directly by block, with sparse variable definitions.
    /// Entries from `blocks_used` on are unregistered (cleared, possibly
    /// left over from an earlier function).
    blocks: Vec<BlockState>,
    blocks_used: usize,
    /// Every parameter the builder added, for the final trivial-parameter
    /// cleanup, in the order it was requested. A join that turned out not
    /// to need its parameter keeps an empty slot, so the order of the rest
    /// (which the cleanup depends on) is unchanged.
    added_params: Vec<(Block, Option<Value>)>,
    /// Incoming edges built so far, indexed by destination block. Sealing
    /// moves a sorted bucket into `preds`, preserving source-layout order.
    /// Buckets from `incoming_used` on are empty.
    incoming: Vec<Vec<(Inst, usize)>>,
    incoming_used: usize,
    /// The branch terminator already indexed for each source block.
    indexed_terms: Vec<Option<Inst>>,
    /// The only predecessor of each sealed block that has one: reads
    /// follow these chains without looking at the edges.
    chains: Vec<Option<Block>>,
}

impl FunctionBuilderContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears what the last function used. Entries past the `*_used`
    /// marks are already clear, so this costs that function's size, not
    /// the largest one's.
    fn clear(&mut self) {
        self.var_types.clear();
        self.result_tys.clear();
        for state in &mut self.blocks[..self.blocks_used] {
            state.clear();
        }
        self.blocks_used = 0;
        self.added_params.clear();
        for edges in &mut self.incoming[..self.incoming_used] {
            edges.clear();
        }
        self.incoming_used = 0;
        self.indexed_terms.clear();
        self.chains.clear();
    }
}

#[derive(Debug)]
pub struct FunctionBuilder<'f> {
    func: &'f mut Function,
    cfg_changed: bool,
    current: Option<Block>,
    /// The location instructions built from now on get.
    srcloc: Option<SourceLoc>,
    context: FunctionBuilderContext,
    /// `ins()` lends out a builder; its insertion is observed on the next
    /// source change or seal, once that mutable borrow has ended.
    pending: Option<Block>,
}

impl<'f> FunctionBuilder<'f> {
    pub fn new(func: &'f mut Function) -> Self {
        Self::with_context(func, FunctionBuilderContext::default())
    }

    /// Like [`FunctionBuilder::new`], reusing the allocations in `context`
    /// (returned by an earlier builder's [`FunctionBuilder::finalize_with_context`]).
    pub fn with_context(func: &'f mut Function, mut context: FunctionBuilderContext) -> Self {
        context.clear();
        Self {
            func,
            cfg_changed: true,
            current: None,
            srcloc: None,
            context,
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
            .ins_with_scratch(InsertPoint::End(block), &mut self.context.result_tys)
            .loc(self.srcloc)
    }

    pub fn declare_var(&mut self, ty: Type) -> Variable {
        self.context.var_types.push(ty);
        Variable::new(self.context.var_types.len() - 1)
    }

    /// Assigns `value` to `var` in the current block.
    pub fn def_var(&mut self, var: Variable, value: Value) {
        let (want, got) = (
            self.context.var_types[var.index()],
            self.func.value_type(value),
        );
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
        let edges = bucket(
            &mut self.context.incoming,
            &mut self.context.incoming_used,
            block,
        );
        let state = &mut self.context.blocks[block.index()];
        // Swap rather than take, so both buckets keep their allocations.
        state.preds.clear();
        std::mem::swap(&mut state.preds, edges);
        // Blocks are created in layout order. Construction may visit them
        // in a different order; keep SSA arguments/values deterministic.
        let func = &*self.func;
        state
            .preds
            .sort_unstable_by_key(|&(inst, dest)| (func.inst_block(inst), dest));
        if let &[(branch, _)] = state.preds.as_slice() {
            if self.context.chains.len() <= block.index() {
                self.context.chains.resize(block.index() + 1, None);
            }
            self.context.chains[block.index()] = Some(func.inst_block(branch));
        }
        let incomplete = std::mem::take(&mut self.context.blocks[block.index()].incomplete);
        for &(var, param) in &incomplete {
            self.add_param_args(var, block, param);
        }
        // Nothing reads an incomplete list once sealed; recycle it.
        let mut incomplete = incomplete;
        incomplete.clear();
        self.context.blocks[block.index()].incomplete = incomplete;
    }

    pub fn seal_all_blocks(&mut self) {
        let blocks: Vec<Block> = self.func.blocks().collect();
        for block in blocks {
            self.seal_block(block);
        }
    }

    /// Removes the trivial parameters that only became trivial after
    /// others were removed. Every block must be sealed.
    pub fn finalize(self) {
        let _ = self.finalize_with_context();
    }

    /// Finalizes the function and returns the builder's allocations for
    /// reuse by the next function's builder. Every block must be sealed.
    pub fn finalize_with_context(mut self) -> FunctionBuilderContext {
        assert!(
            self.context.blocks[..self.context.blocks_used]
                .iter()
                .all(|s| s.sealed != Some(false)),
            "finalize needs every block sealed"
        );
        let mut params = std::mem::take(&mut self.context.added_params);
        loop {
            let mut changed = false;
            // Keep the parameters that are still there and not trivial, in
            // order, and look at them again if anything was removed.
            let mut kept = 0;
            for i in 0..params.len() {
                let (block, Some(param)) = params[i] else {
                    continue;
                };
                if self.func.block_params(block).contains(&param) {
                    if self.try_remove_trivial(block, param) != param {
                        changed = true;
                    } else {
                        params[kept] = (block, Some(param));
                        kept += 1;
                    }
                }
            }
            params.truncate(kept);
            if !changed {
                break;
            }
        }
        self.context.added_params = params;
        self.context
    }

    fn block_state(&mut self, block: Block) -> &mut BlockState {
        let i = block.index();
        if self.context.blocks_used <= i {
            if self.context.blocks.len() <= i {
                self.context.blocks.resize_with(i + 1, BlockState::default);
            }
            self.context.blocks_used = i + 1;
        }
        &mut self.context.blocks[i]
    }

    /// The value of `var` at the end of `block`. Along a chain of sealed
    /// blocks with one predecessor each, the value is the predecessor's, so
    /// the walk follows the chain to the nearest block that defines `var`
    /// or must work it out. The value is recorded there and in `block`,
    /// where the next reads look, but not in the blocks between: they
    /// would only memoize the same walk.
    fn read(&mut self, var: Variable, block: Block) -> Value {
        if let Some(value) = self
            .context
            .blocks
            .get(block.index())
            .and_then(|s| s.defs.get(var))
        {
            return self.func.resolve(value);
        }
        let mut at = block;
        while let Some(&Some(pred)) = self.context.chains.get(at.index()) {
            at = pred;
            if let Some(value) = self.context.blocks[at.index()].defs.get(var) {
                let value = self.func.resolve(value);
                self.context.blocks[block.index()].defs.insert(var, value);
                return value;
            }
        }
        let value = self.read_here(var, at);
        if at != block {
            self.context.blocks[block.index()].defs.insert(var, value);
        }
        value
    }

    /// The value of `var` at the end of `block`, which does not define it
    /// and is not a sealed block with one predecessor.
    fn read_here(&mut self, var: Variable, block: Block) -> Value {
        let ty = self.context.var_types[var.index()];
        let state = &self.context.blocks[block.index()];
        let value = if !state.sealed.expect("unregistered SSA block") {
            let param = self.add_param(block, ty);
            self.context.blocks[block.index()]
                .incomplete
                .push((var, param));
            param
        } else if state.visiting {
            // A loop led back here: the parameter is needed after all.
            // Defining it lets further reads find it; the pending read
            // finishes it and records it in its reserved cleanup slot.
            self.func.append_block_param(block, ty)
        } else {
            match state.preds.as_slice() {
                [] => self.zero(block, ty),
                &[(branch, _)] => {
                    let pred = self.func.inst_block(branch);
                    self.read(var, pred)
                }
                _ => self.read_join(var, block, ty),
            }
        };
        self.block_state(block).defs.insert(var, value);
        value
    }

    /// Reads `var` in a sealed block with several predecessors. The
    /// parameter is created only if a loop reaches this block again while
    /// its predecessors are read, or if they disagree; most joins pass the
    /// same value on every edge and need none. The result is the same as
    /// creating it first and removing it once found trivial, except that
    /// no value number is spent on it.
    fn read_join(&mut self, var: Variable, block: Block, ty: Type) -> Value {
        // Reserve the parameter's place in the cleanup order.
        let slot = self.context.added_params.len();
        self.context.added_params.push((block, None));
        self.context.blocks[block.index()].visiting = true;
        let values = self.read_preds(var, block);
        self.context.blocks[block.index()].visiting = false;
        match self.context.blocks[block.index()].defs.get(var) {
            // Created by a read around a loop: finish it as an ordinary
            // parameter. Its cleanup slot is the one reserved here.
            Some(param) => {
                self.context.added_params[slot].1 = Some(param);
                self.finish_param(block, param, values)
            }
            None => match self.trivial_value(None, values.iter().copied()) {
                Some(same) => same.expect("a join has predecessors"),
                None => {
                    let param = self.func.append_block_param(block, ty);
                    self.context.added_params[slot].1 = Some(param);
                    let index = self.func.block_params(block).len() - 1;
                    self.write_args(block, index, values);
                    param
                }
            },
        }
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
        self.context.added_params.push((block, Some(param)));
        param
    }

    /// Passes each predecessor's value of `var` to `param`, then removes
    /// `param` if it is trivial. Returns what `param` stands for.
    fn add_param_args(&mut self, var: Variable, block: Block, param: Value) -> Value {
        let values = self.read_preds(var, block);
        self.finish_param(block, param, values)
    }

    /// The value of `var` at the end of each predecessor, in edge order.
    /// Reading can add/remove SSA parameters of other blocks, but sealed
    /// predecessor edges stay fixed. `var` already has a definition (or a
    /// pending one) in this block, so no read reaches this block's branch
    /// arguments. Every edge is read before any argument is written, so
    /// a trivial parameter needs none.
    fn read_preds(&mut self, var: Variable, block: Block) -> SmallVec<[Value; 8]> {
        let edges = self.context.blocks[block.index()].preds.len();
        let mut values = SmallVec::with_capacity(edges);
        for edge in 0..edges {
            let (branch, _) = self.context.blocks[block.index()].preds[edge];
            let pred = self.func.inst_block(branch);
            values.push(self.read(var, pred));
        }
        values
    }

    /// Removes `param` if `values` (one per edge) make it trivial, else
    /// passes them as its branch arguments. Returns what `param` stands for.
    fn finish_param(&mut self, block: Block, param: Value, values: SmallVec<[Value; 8]>) -> Value {
        let index = self.param_index(block, param);
        match self.trivial_value(Some(param), values.iter().copied()) {
            Some(same) => self.remove_trivial(block, index, param, same, false),
            None => {
                self.write_args(block, index, values);
                param
            }
        }
    }

    /// Appends `values` (one per edge) as the arguments for parameter
    /// `index` of `block`.
    fn write_args(&mut self, block: Block, index: usize, values: SmallVec<[Value; 8]>) {
        for (edge, value) in values.into_iter().enumerate() {
            let (branch, i) = self.context.blocks[block.index()].preds[edge];
            let args = &mut self.func.inst_data_mut(branch).branch_dests_mut()[i].args;
            debug_assert_eq!(args.len(), index, "arguments are added in parameter order");
            args.push(value);
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
        param: Option<Value>,
        args: impl Iterator<Item = Value>,
    ) -> Option<Option<Value>> {
        let mut same = None;
        for arg in args {
            let arg = self.func.resolve(arg);
            if Some(arg) == param || Some(arg) == same {
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
        let preds = &self.context.blocks[block.index()].preds;
        let args = preds
            .iter()
            .map(|&(branch, i)| self.func.inst_data(branch).branch_dests()[i].args[index]);
        match self.trivial_value(Some(param), args) {
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
            for &(branch, i) in &self.context.blocks[block.index()].preds {
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
            if self.context.indexed_terms.len() <= block.index() {
                self.context.indexed_terms.resize(block.index() + 1, None);
            }
            match (self.context.indexed_terms[block.index()], term) {
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
        if self.context.indexed_terms.len() <= block.index() {
            self.context.indexed_terms.resize(block.index() + 1, None);
        }
        self.context.indexed_terms[block.index()] = Some(term);
        for (i, dest) in self.func.inst_data(term).branch_dests().iter().enumerate() {
            bucket(
                &mut self.context.incoming,
                &mut self.context.incoming_used,
                dest.block,
            )
            .push((term, i));
        }
    }

    fn rebuild_preds(&mut self) {
        for edges in &mut self.context.incoming[..self.context.incoming_used] {
            edges.clear();
        }
        self.context.indexed_terms.fill(None);
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

/// The incoming-edge bucket of `block`, marking it used.
fn bucket<'a>(
    incoming: &'a mut Vec<Vec<(Inst, usize)>>,
    used: &mut usize,
    block: Block,
) -> &'a mut Vec<(Inst, usize)> {
    let i = block.index();
    if *used <= i {
        if incoming.len() <= i {
            incoming.resize_with(i + 1, Vec::new);
        }
        *used = i + 1;
    }
    &mut incoming[i]
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_ir::{BlockCall, InstData, Signature};

    #[test]
    fn sparse_definitions_survive_updates_spilling_and_promotion() {
        let mut defs = Definitions::default();
        let mut reference = HashMap::default();
        for (i, key) in [900_000, 7, 40_000, 42].into_iter().enumerate() {
            defs.insert(Variable::new(key), Value::new(i));
            reference.insert(Variable::new(key), Value::new(i));
        }
        assert!(
            matches!(&defs, Definitions { entries: Entries::Small(entries), .. } if !entries.spilled())
        );
        // Updates and sparse IDs must not consume more slots or force a spill.
        defs.insert(Variable::new(900_000), Value::new(12));
        reference.insert(Variable::new(900_000), Value::new(12));
        assert!(
            matches!(&defs, Definitions { entries: Entries::Small(entries), .. } if entries.len() == 4 && !entries.spilled())
        );
        assert_eq!(defs.get(Variable::new(899_999)), None);
        // Reverse insertion exercises the wide-block fallback; repeated
        // overwrites must preserve the latest definition.
        for i in (0..96).rev().chain(0..96) {
            let var = Variable::new(i * 1_001);
            let value = Value::new(i + reference.len());
            defs.insert(var, value);
            reference.insert(var, value);
            for (&key, &want) in &reference {
                assert_eq!(defs.get(key), Some(want));
            }
        }
        assert!(
            matches!(&defs, Definitions { entries: Entries::Large(entries), .. } if entries.len() == reference.len())
        );
        // Clearing forgets everything; a wide table goes back to inline.
        defs.clear();
        assert!(
            matches!(&defs, Definitions { entries: Entries::Small(entries), .. } if entries.is_empty())
        );
        assert_eq!(defs.get(Variable::new(0)), None);
    }

    #[test]
    fn cleared_definitions_keep_their_spilled_allocation() {
        let mut defs = Definitions::default();
        for i in 0..20 {
            defs.insert(Variable::new(i), Value::new(i));
        }
        let Entries::Small(entries) = &defs.entries else {
            panic!("20 definitions stay small")
        };
        assert!(entries.spilled());
        defs.clear();
        assert!(
            matches!(&defs, Definitions { entries: Entries::Small(entries), .. } if entries.is_empty() && entries.spilled())
        );
        for i in (0..20).rev() {
            assert_eq!(defs.get(Variable::new(i)), None);
            defs.insert(Variable::new(i), Value::new(100 + i));
        }
        for i in 0..20 {
            assert_eq!(defs.get(Variable::new(i)), Some(Value::new(100 + i)));
        }
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
        assert_eq!(b.context.blocks[raw_entry.index()].sealed, None);
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
            let mut got = b
                .context
                .incoming
                .get(dest.index())
                .cloned()
                .unwrap_or_default();
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
        {
            let _unused_builder = b.ins();
        }
        check_index(&mut b);
        b.switch_to_block(a);
        {
            let _unused_builder = b.ins();
        }
        check_index(&mut b);
    }
}
