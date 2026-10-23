//! Allocation boundaries for selected instructions.
//!
//! A selected instruction is currently a lowering *sequence*, not a single
//! machine instruction. Its inputs are read before its outputs are written.
//! ABI shuffles, two-address operations and scratch temporaries remain inside
//! that sequence; registers outside `pools` are reserved throughout it. Do not
//! add those registers to a pool until the sequence's internal constraints have
//! been exposed as separate allocation instructions.
use super::{Allocation, Loc, Pools};
use crate::select::Selection;
use keel_ir::fxhash::FxHashMap as HashMap;
use keel_ir::{Block, EntityRef, Function, Inst, InstData, Type, Value};

/// The independently allocated half of an SSA value (high only for i128).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValuePart {
    pub value: Value,
    pub high: bool,
}
impl ValuePart {
    pub fn low(value: Value) -> Self {
        Self { value, high: false }
    }
    pub fn high(value: Value) -> Self {
        Self { value, high: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperandKind {
    Use,
    Def,
}

/// Requirements at a selected instruction's boundary. Internal fixed and tied
/// machine operands are handled by lowering using the globally reserved regs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// Register or stack; lowering reloads/stores stack operands as necessary.
    Any,
    Register,
    Fixed(Loc),
    /// Definition reuses the location of this instruction's input value-half.
    Reuse(ValuePart),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub part: ValuePart,
    pub kind: OperandKind,
    pub float: bool,
    pub constraint: Constraint,
}

/// Borrowed input: enumerating operands does not build a second function IR.
/// Duplicate reads are retained for cost accounting. All occurrences of the
/// same value-half at a sequence boundary share a location; internal fixed-use
/// copies are lowering's responsibility.
pub struct AllocationInput<'a> {
    pub func: &'a Function,
    pub pools: &'a Pools<'a>,
    pub selection: &'a Selection,
    pub is_call: &'a dyn Fn(&InstData) -> bool,
    pub returns_twice: &'a dyn Fn(&InstData) -> bool,
}
impl AllocationInput<'_> {
    pub fn for_each_operand(&self, inst: Inst, mut visit: impl FnMut(Operand)) {
        if self.selection.covered(inst) {
            return;
        }
        let mut emit = |value, kind| {
            let ty = self.func.value_type(value);
            for high in [false, true] {
                if high && ty != Type::I128 {
                    continue;
                }
                let part = ValuePart { value, high };
                visit(Operand {
                    part,
                    kind,
                    float: ty.is_float() || ty.is_vector(),
                    constraint: self.selection.constraint(inst, part, kind),
                });
            }
        };
        self.selection
            .for_each_operand(self.func, inst, |v| emit(v, OperandKind::Use));
        for &v in self.func.inst_results(inst) {
            if self.func.resolve(v) == v {
                emit(v, OperandKind::Def);
            }
        }
    }

    /// Allocatable registers destroyed by this sequence. Other registers are
    /// globally reserved, even in sequences that do not explicitly use them.
    pub fn for_each_clobber(&self, inst: Inst, mut visit: impl FnMut(Loc)) {
        if (self.is_call)(self.func.inst_data(inst)) {
            let preserved = self
                .selection
                .preserved_call_gprs
                .get(&inst)
                .copied()
                .unwrap_or(0);
            for &r in self.pools.caller_saved {
                if preserved & 1u64.checked_shl(r.into()).unwrap_or(0) == 0 {
                    visit(Loc::Reg(r));
                }
            }
            for &r in self.pools.float_caller_saved {
                visit(Loc::FReg(r));
            }
        }
    }
    pub fn returns_twice(&self, inst: Inst) -> bool {
        (self.returns_twice)(self.func.inst_data(inst))
    }
    pub fn permits(&self, loc: Loc) -> bool {
        match loc {
            Loc::Reg(r) => {
                self.pools.caller_saved.contains(&r) || self.pools.callee_saved.contains(&r)
            }
            Loc::FReg(r) => {
                self.pools.float_caller_saved.contains(&r)
                    || self.pools.float_callee_saved.contains(&r)
            }
            Loc::Stack(_) => true,
            Loc::Remat(value) => super::remat_recipe(self.func, value).is_some(),
        }
    }
}

/// One parallel copy. All sources refer to state before any destination is
/// written. The emitter supplies reserved scratch registers to break cycles.
pub type Transfer = (Loc, Loc); // (destination, source)

#[derive(Default, Debug)]
pub struct InstructionAllocation {
    pub uses: Vec<(ValuePart, Loc)>,
    pub defs: Vec<(ValuePart, Loc)>,
    pub before: Vec<Transfer>,
    /// Only for instructions that fall through. Terminator transfers belong
    /// on the appropriate successor edge (including an invoke's normal edge).
    pub after: Vec<Transfer>,
}
impl InstructionAllocation {
    #[inline]
    fn location(&self, part: ValuePart, kind: OperandKind) -> Option<Loc> {
        let bindings = match kind {
            OperandKind::Use => &self.uses,
            OperandKind::Def => &self.defs,
        };
        bindings
            .iter()
            .find(|(v, _)| *v == part)
            .map(|(_, loc)| *loc)
    }
}

/// Sparse changes to the function-wide fallback locations. The default
/// allocators leave this empty: no per-operand storage or hash lookup is needed
/// on their hot emission path. Splitting allocators populate only boundaries
/// where a value changes location, and own the associated dataflow transfers.
#[derive(Default, Debug)]
pub struct AllocationEdits {
    indices: HashMap<Inst, usize>,
    instructions: Vec<InstructionAllocation>,
    params: HashMap<(Block, ValuePart), Loc>,
    edges: HashMap<(Inst, usize), Vec<Transfer>>,
}
impl AllocationEdits {
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty() && self.params.is_empty() && self.edges.is_empty()
    }
    /// Visit typed sparse bindings without exposing the storage layout.
    pub(crate) fn for_each_binding(&self, mut visit: impl FnMut(ValuePart, Loc)) {
        for edit in &self.instructions {
            for &(part, loc) in edit.uses.iter().chain(&edit.defs) {
                visit(part, loc);
            }
        }
        for (&(_, part), &loc) in &self.params {
            visit(part, loc);
        }
    }
    pub(crate) fn for_each_transfer(&self, mut visit: impl FnMut(Transfer)) {
        for edit in &self.instructions {
            for &pair in edit.before.iter().chain(&edit.after) {
                visit(pair);
            }
        }
        for moves in self.edges.values() {
            for &pair in moves {
                visit(pair);
            }
        }
    }
    /// Rewrite every sparse location together with a fallback-frame rewrite.
    pub(crate) fn map_locations(&mut self, mut map: impl FnMut(Loc) -> Loc) {
        for edit in &mut self.instructions {
            for (_, loc) in edit.uses.iter_mut().chain(&mut edit.defs) {
                *loc = map(*loc);
            }
            for (dst, src) in edit.before.iter_mut().chain(&mut edit.after) {
                *dst = map(*dst);
                *src = map(*src);
            }
        }
        for loc in self.params.values_mut() {
            *loc = map(*loc);
        }
        for moves in self.edges.values_mut() {
            for (dst, src) in moves {
                *dst = map(*dst);
                *src = map(*src);
            }
        }
    }
    pub fn instruction_mut(&mut self, inst: Inst) -> &mut InstructionAllocation {
        let index = *self.indices.entry(inst).or_insert_with(|| {
            let index = self.instructions.len();
            self.instructions.push(InstructionAllocation::default());
            index
        });
        &mut self.instructions[index]
    }
    #[inline]
    pub fn instruction_index(&self, inst: Inst) -> Option<usize> {
        if self.instructions.is_empty() {
            None
        } else {
            self.indices.get(&inst).copied()
        }
    }
    pub fn instruction(&self, index: usize) -> &InstructionAllocation {
        &self.instructions[index]
    }
    pub fn set_param(&mut self, block: Block, part: ValuePart, loc: Loc) {
        self.params.insert((block, part), loc);
    }
    /// Successor ordinals follow `InstData::branch_dests`, including distinct
    /// edges that happen to have the same target block. Edge transfers and
    /// ordinary block-parameter copies execute as ONE parallel copy.
    pub fn edge_mut(&mut self, inst: Inst, successor: usize) -> &mut Vec<Transfer> {
        self.edges.entry((inst, successor)).or_default()
    }
    pub fn edge(&self, inst: Inst, successor: usize) -> &[Transfer] {
        self.edges
            .get(&(inst, successor))
            .map_or(&[], Vec::as_slice)
    }
}

impl Allocation {
    #[inline]
    pub fn operand_loc(
        &self,
        index: Option<usize>,
        part: ValuePart,
        kind: OperandKind,
    ) -> Option<Loc> {
        index
            .and_then(|i| self.edits.instruction(i).location(part, kind))
            .or_else(|| self.locs[usize::from(part.high)][part.value.index()])
    }
    #[inline]
    pub fn param_loc(&self, block: Block, part: ValuePart) -> Option<Loc> {
        self.edits
            .params
            .get(&(block, part))
            .copied()
            .or_else(|| self.locs[usize::from(part.high)][part.value.index()])
    }

    /// Conservative register mentions in a finalized allocation, including
    /// sparse overrides and transfer-only temporaries. Registers >=64 cannot
    /// participate in the current target's compact preservation proofs.
    pub(crate) fn mentioned_gprs(&self) -> u64 {
        let mut mask = 0;
        let mut mention = |loc| {
            if let Loc::Reg(r) = loc {
                mask |= 1u64.checked_shl(r.into()).unwrap_or(0);
            }
        };
        for loc in self.locs.iter().flatten().flatten() {
            mention(*loc);
        }
        for edit in &self.edits.instructions {
            for &(_, loc) in edit.uses.iter().chain(&edit.defs) {
                mention(loc);
            }
            for &(dst, src) in edit.before.iter().chain(&edit.after) {
                mention(dst);
                mention(src);
            }
        }
        for &loc in self.edits.params.values() {
            mention(loc);
        }
        for moves in self.edits.edges.values() {
            for &(dst, src) in moves {
                mention(dst);
                mention(src);
            }
        }
        mask
    }

    /// Account for every physical register mentioned by sparse edits before
    /// laying out the frame, including transfer-only temporaries.
    pub(crate) fn finish(&mut self, input: &AllocationInput<'_>) {
        let mut gprs = [false; 256];
        let mut fprs = [false; 256];
        let mut mention = |loc| match loc {
            Loc::Reg(r) => gprs[r as usize] = true,
            Loc::FReg(r) => fprs[r as usize] = true,
            Loc::Stack(_) | Loc::Remat(_) => {}
        };
        for edit in &self.edits.instructions {
            for &(_, loc) in edit.uses.iter().chain(&edit.defs) {
                mention(loc);
            }
            for &(dst, src) in edit.before.iter().chain(&edit.after) {
                mention(dst);
                mention(src);
            }
        }
        for &loc in self.edits.params.values() {
            mention(loc);
        }
        for moves in self.edits.edges.values() {
            for &(dst, src) in moves {
                mention(dst);
                mention(src);
            }
        }
        self.callee_saved_used.extend(
            input
                .pools
                .callee_saved
                .iter()
                .copied()
                .filter(|&r| gprs[r as usize]),
        );
        self.float_callee_saved_used.extend(
            input
                .pools
                .float_callee_saved
                .iter()
                .copied()
                .filter(|&r| fprs[r as usize]),
        );
        self.callee_saved_used.sort_unstable();
        self.callee_saved_used.dedup();
        self.float_callee_saved_used.sort_unstable();
        self.float_callee_saved_used.dedup();
    }

    /// Check boundary contracts before emission. This checks shape, physical
    /// constraints and frame accounting; allocators remain responsible for
    /// liveness/interference and transfer dataflow (including returns-twice).
    pub fn validate(&self, input: &AllocationInput<'_>) -> Result<(), String> {
        let mask = |caller: &[u8], callee: &[u8]| {
            caller.iter().chain(callee).fold(0u64, |bits, &r| {
                bits | 1u64.checked_shl(r.into()).unwrap_or(0)
            })
        };
        let gpr = mask(input.pools.caller_saved, input.pools.callee_saved);
        let fpr = mask(
            input.pools.float_caller_saved,
            input.pools.float_callee_saved,
        );
        let valid_loc = |loc: Loc| match loc {
            Loc::Reg(r) => gpr & 1u64.checked_shl(r.into()).unwrap_or(0) != 0,
            Loc::FReg(r) => fpr & 1u64.checked_shl(r.into()).unwrap_or(0) != 0,
            Loc::Stack(s) => s < self.spill_slots,
            Loc::Remat(value) => super::remat_recipe(input.func, value).is_some(),
        };
        for (high, half) in self.locs.iter().enumerate() {
            if half.len() != input.func.num_values() {
                return Err("allocator returned an invalid location table".into());
            }
            if half.iter().enumerate().any(|(i, loc)| {
                loc.is_some_and(|l| {
                    !valid_loc(l) || matches!(l, Loc::Remat(v) if high != 0 || v.index() != i)
                })
            }) {
                return Err("allocator used a reserved register or invalid spill slot".into());
            }
        }
        for (&inst, &index) in &self.edits.indices {
            let edit = self.edits.instruction(index);
            if input.selection.covered(inst) {
                return Err("allocation edits on a covered instruction".into());
            }
            if input.func.inst_data(inst).is_terminator() && !edit.after.is_empty() {
                return Err("terminator transfers must be placed on successor edges".into());
            }
            for (kind, bindings) in [
                (OperandKind::Use, &edit.uses),
                (OperandKind::Def, &edit.defs),
            ] {
                for (i, &(part, loc)) in bindings.iter().enumerate() {
                    if !valid_loc(loc)
                        || matches!(loc, Loc::Remat(v) if part.high || v != part.value || kind == OperandKind::Def)
                        || bindings[..i].iter().any(|&(p, _)| p == part)
                    {
                        return Err("invalid or duplicate allocation operand binding".into());
                    }
                    let mut found = false;
                    input.for_each_operand(inst, |op| {
                        if op.part == part && op.kind == kind {
                            found = matches!(loc, Loc::Stack(_))
                                || matches!(loc, Loc::FReg(_)) == op.float;
                        }
                    });
                    if !found {
                        return Err(format!("binding for an absent operand at {inst}"));
                    }
                }
            }
            for moves in [&edit.before, &edit.after] {
                validate_moves(moves, &valid_loc)?;
            }
        }
        for (&(block, part), &loc) in &self.edits.params {
            if !input.func.block_params(block).contains(&part.value)
                || !valid_loc(loc)
                || matches!(loc, Loc::Remat(_))
                || (part.high && input.func.value_type(part.value) != Type::I128)
            {
                return Err("invalid block parameter location".into());
            }
        }
        for (&(inst, successor), moves) in &self.edits.edges {
            let calls = input.func.inst_data(inst).branch_dests();
            if successor >= calls.len() {
                return Err("invalid allocation edge".into());
            }
            // The unwinder jumps directly into a landing pad, so an exceptional
            // edge cannot execute an ordinary inline copy sequence.
            if matches!(input.func.inst_data(inst), InstData::Invoke { .. })
                && successor == 1
                && !moves.is_empty()
            {
                return Err("exceptional-edge transfers require landing-pad lowering".into());
            }
            validate_moves(moves, &valid_loc)?;
            for &param in input.func.block_params(calls[successor].block) {
                for high in [false, true] {
                    if let Some(dst) =
                        self.param_loc(calls[successor].block, ValuePart { value: param, high })
                    {
                        if moves.iter().any(|&(d, _)| d == dst) {
                            return Err("edge transfer overwrites a block parameter copy".into());
                        }
                    }
                }
            }
        }
        // Only constrained instructions require an operand walk for legacy
        // allocations; the overwhelmingly common unconstrained path is cheap.
        for (&(inst, part, kind), &constraint) in input.selection.constraints() {
            let mut found = false;
            input.for_each_operand(inst, |op| found |= op.part == part && op.kind == kind);
            if !found {
                return Err(format!("constraint for an absent operand at {inst}"));
            }
            let index = self.edits.instruction_index(inst);
            let loc = self.operand_loc(index, part, kind);
            let satisfies = match constraint {
                Constraint::Any => true,
                Constraint::Register => matches!(loc, Some(Loc::Reg(_) | Loc::FReg(_))),
                Constraint::Fixed(fixed) => loc == Some(fixed),
                Constraint::Reuse(other) => {
                    kind == OperandKind::Def
                        && loc.is_some()
                        && loc == self.operand_loc(index, other, OperandKind::Use)
                }
            };
            if !satisfies {
                return Err(format!("unsatisfied allocation constraint at {inst}"));
            }
        }
        Ok(())
    }
}
fn validate_moves(moves: &[Transfer], valid: &impl Fn(Loc) -> bool) -> Result<(), String> {
    for (i, &(dst, src)) in moves.iter().enumerate() {
        if !valid(dst)
            || !valid(src)
            || matches!(dst, Loc::Remat(_))
            || moves[..i].iter().any(|&(d, _)| d == dst)
        {
            return Err("invalid allocation parallel copy".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::{AllocationTier, RegisterAllocator};
    const SOURCE: &str = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iadd v0, v1
 return v2
}";
    const POOLS: Pools = Pools {
        caller_saved: &[9, 10],
        callee_saved: &[19],
        float_caller_saved: &[20],
        float_callee_saved: &[],
    };
    fn check(edit: impl FnOnce(&AllocationInput<'_>, &mut Allocation)) -> Result<(), String> {
        let module = keel_ir::parse(SOURCE).unwrap();
        let func = &module.functions[0];
        let selection = Selection::default();
        let input = AllocationInput {
            func,
            pools: &POOLS,
            selection: &selection,
            is_call: &|_| false,
            returns_twice: &|_| false,
        };
        let mut alloc = AllocationTier::Full.allocate(&input);
        edit(&input, &mut alloc);
        alloc.validate(&input)
    }
    #[test]
    fn rejects_invalid_edits_before_emission() {
        for bad in [Loc::Reg(16), Loc::FReg(20), Loc::Stack(999)] {
            assert!(
                check(|input, alloc| {
                    let inst = input.func.block_insts(input.func.entry_block().unwrap())[0];
                    alloc
                        .edits
                        .instruction_mut(inst)
                        .uses
                        .push((ValuePart::low(Value::new(0)), bad));
                })
                .is_err()
            );
        }
        assert!(
            check(|input, alloc| {
                let inst = input.func.block_insts(input.func.entry_block().unwrap())[1];
                alloc
                    .edits
                    .instruction_mut(inst)
                    .after
                    .push((Loc::Reg(9), Loc::Reg(10)));
            })
            .unwrap_err()
            .contains("terminator")
        );
        assert!(
            check(|input, alloc| {
                let inst = input.func.block_insts(input.func.entry_block().unwrap())[0];
                alloc
                    .edits
                    .instruction_mut(inst)
                    .before
                    .extend([(Loc::Reg(9), Loc::Reg(10)), (Loc::Reg(9), Loc::Reg(19))]);
            })
            .unwrap_err()
            .contains("parallel copy")
        );
    }
    #[test]
    fn operand_roles_preserve_repeated_uses_and_both_integer_halves() {
        let module = keel_ir::parse(
            "func @f(i128) -> i128 {\nblock0(v0: i128):\n v1 = iadd v0, v0\n return v1\n}",
        )
        .unwrap();
        let func = &module.functions[0];
        let selection = Selection::default();
        let input = AllocationInput {
            func,
            pools: &POOLS,
            selection: &selection,
            is_call: &|_| false,
            returns_twice: &|_| false,
        };
        let mut operands = Vec::new();
        input.for_each_operand(func.block_insts(func.entry_block().unwrap())[0], |op| {
            operands.push(op)
        });
        assert_eq!(operands.len(), 6);
        assert_eq!(operands[0], operands[2]);
        assert_eq!(operands[1], operands[3]);
        assert_eq!(operands[4].kind, OperandKind::Def);
        assert!(operands[5].part.high);
    }
    #[test]
    fn fused_compare_leaves_are_inputs_at_the_branch() {
        let module = keel_ir::parse("func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 7\n v2 = icmp.eq v0, v1\n brif v2, block1(v1), block1(v0)\nblock1(v3: i64):\n return v3\n}").unwrap();
        let func = &module.functions[0];
        let mut selection = Selection::default();
        selection.fuse_compares(
            func,
            |_| vec![Value::new(1)],
            &crate::fold::use_counts(func),
        );
        let insts = func.block_insts(func.entry_block().unwrap());
        assert!(selection.covered(insts[1]));
        // The immediate compare operand disappears, but the edge's v1 use stays.
        assert_eq!(
            selection.operands(func, insts[2]),
            vec![Value::new(1), Value::new(0), Value::new(0)]
        );
    }
}
