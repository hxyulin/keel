//! Tree patterns matched against the SSA DAG before register allocation.
//! A cover is local to a block and consumes single-use nodes. Memory covers
//! additionally preserve the original access position and exact access width.
//! Shared DAG nodes remain materialized; selected leaves are live at the root.
use crate::regalloc::{Constraint, OperandKind, ValuePart};
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, Inst, InstData, IntCC, Type, Value, ValueDef,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Aarch64,
    X86,
}
#[derive(Clone, Copy, Debug)]
pub enum Machine {
    Madd,
    Msub,
    ShiftAdd,
    ShiftSub,
    ShiftAnd,
    ShiftOr,
    ShiftXor,
    Lea,
    /// X86-only modular x * 3 + signed disp32, with one allocated input.
    LeaMul3 {
        displacement: i32,
    },
    /// AArch64-only, exact-width unsigned bitfield extraction.
    Ubfx {
        lsb: u8,
        width: u8,
    },
}
struct Rule {
    target: Target,
    root: BinaryOp,
    child: BinaryOp,
    machine: Machine,
    commute: bool,
}
include!(concat!(env!("OUT_DIR"), "/patterns.rs"));

#[derive(Clone, Debug)]
pub struct Match {
    pub machine: Machine,
    pub args: Vec<Value>,
    pub shift: u8,
}

/// Exact 64-bit modular address arithmetic, evaluated at the memory access.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IndexedAddress {
    pub base: Value,
    pub index: Value,
    pub shift: u8,
}
/// Dense instruction indices, sparse operand lists. Fusion makes overrides
/// common enough that hashing every instruction in each liveness pass is costly.
#[derive(Default)]
struct OperandTable {
    indices: Vec<u32>,
    lists: Vec<Vec<Value>>,
}
impl OperandTable {
    fn get(&self, inst: &Inst) -> Option<&Vec<Value>> {
        let index = *self.indices.get(inst.index())?;
        if index == u32::MAX {
            None
        } else {
            Some(&self.lists[index as usize])
        }
    }
    fn insert(&mut self, inst: Inst, operands: Vec<Value>) {
        if self.indices.len() <= inst.index() {
            self.indices.resize(inst.index() + 1, u32::MAX);
        }
        let index = &mut self.indices[inst.index()];
        if *index == u32::MAX {
            *index = self.lists.len() as u32;
            self.lists.push(operands);
        } else {
            self.lists[*index as usize] = operands;
        }
    }
    fn remove(&mut self, inst: &Inst) {
        if let Some(index) = self.indices.get_mut(inst.index()) {
            if *index != u32::MAX {
                self.lists[*index as usize].clear();
                *index = u32::MAX;
            }
        }
    }
}

#[derive(Default)]
pub struct Selection {
    constraints: HashMap<(Inst, ValuePart, OperandKind), Constraint>,
    /// Dense per-instruction "has any constraint" flags mirroring `constraints`.
    constrained: Vec<bool>,
    /// Target-proven GPRs untouched by a complete call lowering sequence.
    /// Absent entries retain the ordinary ABI clobber set.
    pub(crate) preserved_call_gprs: HashMap<Inst, u64>,
    pub matches: HashMap<Inst, Match>,
    covered: Vec<bool>,
    /// Single-use ptradd results encoded at their original memory consumer.
    pub(crate) addresses: HashMap<Value, (Value, i64)>,
    pub(crate) indexed_addresses: HashMap<Value, IndexedAddress>,
    /// An outer zero test uses the original integer compare, optionally inverted.
    pub(crate) conditions: HashMap<Inst, (Inst, bool)>,
    /// X86 exact-width adjacent ordinary load encoded as an integer CMP RHS.
    pub(crate) compare_memory: HashMap<Inst, Value>,
    /// X86 adjacent load encoded as an integer binary RHS.
    pub(crate) binary_memory: HashMap<Inst, Value>,
    /// X86 adjacent quotient/multiply/subtract using the native remainder.
    pub(crate) division_remainders: HashMap<Inst, Inst>,
    /// Scalar compare consumed directly by a select.
    pub(crate) select_compares: HashMap<Inst, Inst>,
    /// Exact adjacent ordinary i8 load consumed by sext.i32.
    pub(crate) signed_byte_memory: HashMap<Inst, Value>,
    allocation_operands: OperandTable,
    // Operand occurrences removed by fold_operands, so later target folding
    // (including addresses known only after allocation) does not remove them twice.
    allocation_folds: HashMap<Inst, Vec<Value>>,
    /// Result-to-input affinities, never constraints: interference wins.
    pub hints: Hints,
}

/// The input each result would like to share a register with, if any,
/// indexed by the result rather than hashed: most results have one.
#[derive(Default)]
pub struct Hints(Vec<Option<Value>>);
impl Hints {
    pub fn insert(&mut self, result: Value, input: Value) {
        if self.0.len() <= result.index() {
            self.0.resize(result.index() + 1, None);
        }
        self.0[result.index()] = Some(input);
    }
    pub fn get(&self, result: Value) -> Option<Value> {
        self.0.get(result.index()).copied().flatten()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Selection {
    fn select_indexed_address(
        &mut self,
        func: &Function,
        inst: Inst,
        addr: Value,
        ty: Type,
        uses: &[u8],
        target: Target,
    ) -> bool {
        let addr = func.resolve(addr);
        let Some(size @ (1 | 2 | 4 | 8)) = (if ty == Type::Ptr { Some(8) } else { ty.bytes() })
        else {
            return false;
        };
        let same_source = |child| match (func.inst_loc(child), func.inst_loc(inst)) {
            (Some(_), None) => false,
            (Some(a), Some(b)) => a.file == b.file && a.line == b.line,
            _ => true,
        };
        let ValueDef::Result(address, _) = func.value_def(addr) else {
            return false;
        };
        if uses[addr.index()] != 1
            || func.inst_block(address) != func.inst_block(inst)
            || self.covered(address)
            || func.is_removed(address)
            || !same_source(address)
        {
            return false;
        }
        let InstData::Binary {
            op: BinaryOp::Ptradd,
            args,
        } = *func.inst_data(address)
        else {
            return false;
        };
        let base = func.resolve(args[0]);
        let offset = func.resolve(args[1]);
        // Constants continue through the existing displacement selection path.
        if func.value_type(offset) != Type::I64 || crate::fold::iconst(func, offset).is_some() {
            return false;
        }
        let mut index = offset;
        let mut shift = 0;
        if uses[offset.index()] == 1
            && let ValueDef::Result(producer, _) = func.value_def(offset)
            && func.inst_block(producer) == func.inst_block(inst)
            && !self.covered(producer)
            && !func.is_removed(producer)
            && same_source(producer)
            && let InstData::Binary {
                op: BinaryOp::Shl,
                args: inputs,
            } = *func.inst_data(producer)
            && let Some(raw_shift) = crate::fold::iconst(func, inputs[1])
            // IR shifts mask the count to the value width. ARM allows no
            // shift or log2(access bytes); x86 permits scales 1, 2, 4, 8.
            && let amount = (raw_shift as u64 & 63) as u8
            && match target {
                Target::Aarch64 => amount == 0 || u32::from(amount) == size.ilog2(),
                Target::X86 => amount <= 3,
            }
        {
            index = func.resolve(inputs[0]);
            shift = amount;
            self.cover(producer);
        }
        let mut operands = self.operands(func, inst);
        let position = operands.iter().position(|&v| v == addr).unwrap();
        operands.splice(position..=position, [base, index]);
        self.set_operands(inst, operands);
        self.indexed_addresses
            .insert(addr, IndexedAddress { base, index, shift });
        self.cover(address);
        true
    }

    /// Require a location at a selected sequence boundary. Parts must be
    /// resolved and present in the selected operands/results. Allocators must
    /// satisfy these requirements; emission rejects unsatisfied constraints.
    /// Fixed registers must belong to the target's allocatable pools.
    pub fn set_constraint(
        &mut self,
        inst: Inst,
        part: ValuePart,
        kind: OperandKind,
        constraint: Constraint,
    ) {
        self.constraints.insert((inst, part, kind), constraint);
        if self.constrained.len() <= inst.index() {
            self.constrained.resize(inst.index() + 1, false);
        }
        self.constrained[inst.index()] = true;
    }
    /// Whether any operand or result of `inst` carries a constraint.
    pub(crate) fn has_constraint(&self, inst: Inst) -> bool {
        self.constrained.get(inst.index()).copied().unwrap_or(false)
    }
    pub fn constraint(&self, inst: Inst, part: ValuePart, kind: OperandKind) -> Constraint {
        self.constraints
            .get(&(inst, part, kind))
            .copied()
            .unwrap_or(Constraint::Any)
    }
    pub(crate) fn constraints(&self) -> &HashMap<(Inst, ValuePart, OperandKind), Constraint> {
        &self.constraints
    }

    /// Fusion changes where operands are read. Publish the compare's selected
    /// leaves at the branch before allocation, rather than hiding this in emission.
    /// `uses` are [`crate::fold::use_counts`] of `func`.
    pub(crate) fn fuse_compares(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        uses: &[u8],
    ) -> HashSet<Value> {
        self.fuse_compare_pairs(func, folds, crate::fold::fused_compare_pairs(func, uses))
    }

    /// Only adjacent single-use scalar compares: move their inputs to the
    /// select's allocation boundary before suppressing the Boolean definition.
    pub(crate) fn fuse_x86_selects(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        uses: &[u8],
    ) {
        let mut pairs = Vec::new();
        for block in func.blocks() {
            for pair in func.block_insts(block).windows(2) {
                let [compare, select] = [pair[0], pair[1]];
                let InstData::Select { args } = *func.inst_data(select) else {
                    continue;
                };
                if !matches!(
                    func.value_type(func.resolve(args[1])),
                    Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                ) {
                    continue;
                }
                let scalar = match *func.inst_data(compare) {
                    InstData::Icmp { args, .. } => {
                        func.value_type(func.resolve(args[0])) != Type::I128
                    }
                    InstData::Fcmp { .. } => true,
                    _ => false,
                };
                // Memory-CMP address lowering can use rdx for an index.
                // Keep it outside this staging contract, including outer tests.
                if !scalar
                    || self.compare_memory.contains_key(&compare)
                    || self.conditions.contains_key(&compare)
                {
                    continue;
                }
                let cond = func.resolve(args[0]);
                if func.inst_results(compare).first() != Some(&cond) || uses[cond.index()] != 1 {
                    continue;
                }
                if let Some(a) = func.inst_loc(compare) {
                    if func
                        .inst_loc(select)
                        .is_none_or(|b| a.file != b.file || a.line != b.line)
                    {
                        continue;
                    }
                }
                pairs.push((cond, compare, select));
            }
        }
        for pair in pairs {
            if self
                .fuse_compare_pairs(func, &folds, vec![pair])
                .contains(&pair.0)
            {
                self.select_compares.insert(pair.2, pair.1);
            }
        }
    }

    pub(crate) fn fuse_aarch64_compares(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        uses: &[u8],
    ) -> HashSet<Value> {
        self.fuse_compare_pairs(func, folds, crate::fold::aarch64_compare_pairs(func, uses))
    }

    /// Delay an integer compare to its sole select consumer. Only cross a
    /// bounded window of small, pure integer definitions: never effects, traps,
    /// other condition producers, or an explicit allocation/debug boundary.
    pub(crate) fn fuse_aarch64_selects(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        uses: &[u8],
    ) {
        // Inline assembly can observe NZCV without declaring a flag input,
        // including at a later block. Keep every old flag definition intact.
        if func
            .layout_insts()
            .any(|(_, i)| matches!(func.inst_data(i), InstData::InlineAsm { .. }))
        {
            return;
        }
        let mut pairs = Vec::new();
        for block in func.blocks() {
            let insts = func.block_insts(block);
            for (end, &select) in insts.iter().enumerate() {
                let InstData::Select { args } = *func.inst_data(select) else {
                    continue;
                };
                let ty = func.value_type(func.resolve(args[1]));
                if !matches!(ty, Type::I32 | Type::I64 | Type::Ptr)
                    || func.value_type(func.resolve(args[2])) != ty
                {
                    continue;
                }
                let cond = func.resolve(args[0]);
                let ValueDef::Result(compare, _) = func.value_def(cond) else {
                    continue;
                };
                let InstData::Icmp { args: inputs, .. } = *func.inst_data(compare) else {
                    continue;
                };
                let input_ty = func.value_type(func.resolve(inputs[0]));
                if !matches!(input_ty, Type::I32 | Type::I64)
                    || func.value_type(func.resolve(inputs[1])) != input_ty
                    || uses[cond.index()] != 1
                    || func.inst_results(compare).first() != Some(&cond)
                {
                    continue;
                }
                let begin = end.saturating_sub(5);
                let Some(start) = insts[begin..end]
                    .iter()
                    .position(|&i| i == compare)
                    .map(|offset| begin + offset)
                else {
                    continue;
                };
                let location = func.inst_loc(select).map(|l| (l.file, l.line));
                if insts[start..=end].iter().any(|&i| {
                    func.is_removed(i)
                        // An immediate-only constant may already be covered
                        // by operand folding (e.g. the 17 in balance's SUB).
                        // It has no emission or allocation boundary to move.
                        || (self.covered(i)
                            && (i == compare
                                || i == select
                                || !matches!(func.inst_data(i), InstData::Iconst { .. })))
                        || self.matches.contains_key(&i)
                        || self.conditions.contains_key(&i)
                        || self.has_constraint(i)
                        || func.inst_loc(i).map(|l| (l.file, l.line)) != location
                }) {
                    continue;
                }
                if !insts[start + 1..end].iter().all(|&i| {
                    let small = func.inst_results(i).len() == 1
                        && matches!(func.value_type(func.first_result(i)), Type::I32 | Type::I64);
                    small
                        && matches!(
                            func.inst_data(i),
                            InstData::Iconst { .. }
                                | InstData::Binary {
                                    op: BinaryOp::Iadd
                                        | BinaryOp::Isub
                                        | BinaryOp::Imul
                                        | BinaryOp::And
                                        | BinaryOp::Or
                                        | BinaryOp::Xor
                                        | BinaryOp::Shl
                                        | BinaryOp::Ushr
                                        | BinaryOp::Sshr,
                                    ..
                                }
                        )
                }) {
                    continue;
                }
                pairs.push((cond, compare, select));
            }
        }
        for pair in pairs {
            if self
                .fuse_compare_pairs(func, &folds, vec![pair])
                .contains(&pair.0)
            {
                self.select_compares.insert(pair.2, pair.1);
            }
        }
    }

    fn fuse_compare_pairs(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        pairs: Vec<(Value, Inst, Inst)>,
    ) -> HashSet<Value> {
        let mut fused = HashSet::default();
        for (cond, compare, branch) in pairs {
            let mut args = self.operands(func, branch);
            let Some(i) = args.iter().position(|&v| v == cond) else {
                continue;
            };
            if self.covered(compare)
                || self.covered(branch)
                || self.has_constraint(compare)
                || self.has_constraint(branch)
            {
                continue;
            }
            args.remove(i);
            let mut leaves = self.operands(func, compare);
            let mut folded = folds(self.folded_data(func, compare));
            self.remaining_folds(func, compare, &mut folded);
            for value in folded {
                if let Some(i) = leaves.iter().position(|&v| v == value) {
                    leaves.remove(i);
                }
            }
            args.extend(leaves);
            self.set_operands(branch, args);
            self.cover(compare);
            fused.insert(cond);
        }
        fused
    }

    /// Choose only measured same-block scalar multiply alternatives.
    pub(crate) fn tune_multiplications(&mut self, func: &Function, cpu: crate::tuning::TuningCpu) {
        for (_, inst) in func.layout_insts() {
            // Preserve existing larger covers, including multiply-add fusion.
            if self.covered(inst) || self.matches.contains_key(&inst) {
                continue;
            }
            let InstData::Binary {
                op: BinaryOp::Imul,
                args,
            } = *func.inst_data(inst)
            else {
                continue;
            };
            let ty = func.value_type(func.resolve(args[0]));
            let bits = match ty {
                Type::I32 => 32,
                Type::I64 => 64,
                _ => continue,
            };
            for side in [1, 0] {
                let Some(constant) = crate::fold::iconst(func, args[side]) else {
                    continue;
                };
                let Some(shift) = cpu.multiply_shift(bits, constant) else {
                    continue;
                };
                let value = func.resolve(args[1 - side]);
                self.matches.insert(
                    inst,
                    Match {
                        machine: Machine::ShiftAdd,
                        args: vec![value, value],
                        shift,
                    },
                );
                self.allocation_operands.remove(&inst);
                break;
            }
        }
    }

    /// Suppress an instruction subsumed by a selected root. Custom selectors
    /// must preserve all side effects and every use of its results.
    pub fn cover(&mut self, inst: Inst) {
        if self.covered.len() <= inst.index() {
            self.covered.resize(inst.index() + 1, false);
        }
        self.covered[inst.index()] = true;
    }
    /// Override allocation operands after encoding values in a target operand.
    /// Values must be resolved and include every value read by the emitter.
    pub fn set_operands(&mut self, inst: Inst, operands: Vec<Value>) {
        self.allocation_operands.insert(inst, operands);
        self.allocation_folds.remove(&inst);
    }

    pub(crate) fn remaining_folds(&self, func: &Function, inst: Inst, folds: &mut Vec<Value>) {
        for value in folds.iter_mut() {
            *value = func.resolve(*value);
        }
        if let Some(previous) = self.allocation_folds.get(&inst) {
            for &value in previous {
                if let Some(i) = folds.iter().position(|&v| v == value) {
                    folds.swap_remove(i);
                }
            }
        }
    }

    pub(crate) fn folded_data<'a>(&self, func: &'a Function, inst: Inst) -> &'a InstData {
        let source = self
            .conditions
            .get(&inst)
            .map_or(inst, |&(source, _)| source);
        func.inst_data(source)
    }

    pub fn covered(&self, inst: Inst) -> bool {
        self.covered.get(inst.index()).copied().unwrap_or(false)
    }
    pub fn operands(&self, func: &Function, inst: Inst) -> Vec<Value> {
        if self.covered(inst) {
            return vec![];
        }
        if let Some(args) = self.allocation_operands.get(&inst) {
            return args.clone();
        }
        if let Some(m) = self.matches.get(&inst) {
            return m.args.clone();
        }
        let mut args = vec![];
        func.inst_data(inst)
            .for_each_operand(|v| args.push(func.resolve(v)));
        args
    }
    pub fn for_each_operand(&self, func: &Function, inst: Inst, mut f: impl FnMut(Value)) {
        if self.covered(inst) {
            return;
        }
        if let Some(args) = self.allocation_operands.get(&inst) {
            args.iter().copied().for_each(f);
        } else if let Some(m) = self.matches.get(&inst) {
            m.args.iter().copied().for_each(f);
        } else {
            func.inst_data(inst)
                .for_each_operand(|v| f(func.resolve(v)));
        }
    }
    /// Remove operands encoded as immediates/addresses from the allocation
    /// graph. A value used elsewhere remains materialized for those uses.
    ///
    /// `folds` names only `stack_addr` and `iconst` results, as for
    /// [`crate::fold::folded_values`], which also gives each instruction's.
    pub fn fold_operands(&mut self, func: &Function, folds: impl Fn(&InstData) -> Vec<Value>) {
        let mut record = crate::fold::FoldRecord::default();
        let elided = crate::fold::folded_values_recording(
            func,
            self,
            |data, out| out.extend(folds(data)),
            &mut record,
        );
        // Elided results are defined by instructions without operands, which
        // fold nothing themselves.
        for (_, inst) in func.layout_insts() {
            if func
                .inst_results(inst)
                .first()
                .is_some_and(|v| elided.contains(v))
            {
                self.cover(inst);
            }
        }
        for &(inst, start, end) in &record.insts {
            let mut folded = record.values[start as usize..end as usize].to_vec();
            let mut args = self.operands(func, inst);
            folded.retain(|value| {
                if let Some(i) = args.iter().position(|v| v == value) {
                    args.remove(i);
                    true
                } else {
                    false
                }
            });
            if folded.is_empty() {
                continue;
            }
            self.allocation_folds
                .entry(inst)
                .or_default()
                .extend(folded);
            self.allocation_operands.insert(inst, args);
        }
    }
    #[cfg(test)]
    pub fn compute(func: &Function, target: Target) -> Self {
        Self::compute_with_options(func, target, crate::SelectionOptions::ALL)
    }
    pub fn compute_with_options(
        func: &Function,
        target: Target,
        options: crate::SelectionOptions,
    ) -> Self {
        Self::compute_with_uses(func, target, options, None)
    }
    /// [`Self::compute_with_options`], with the raw IR use counts of
    /// [`crate::fold::use_counts`] if the caller has them.
    pub fn compute_with_uses(
        func: &Function,
        target: Target,
        options: crate::SelectionOptions,
        counted: Option<&[u8]>,
    ) -> Self {
        let mut plan = Self::default();
        if !options.trees && !options.register_hints {
            return plan;
        }
        // Tree legality only distinguishes exactly one use from zero/many.
        // Hint-only selection does not need any whole-function use counts.
        let uses = match counted {
            _ if !options.trees => std::borrow::Cow::Owned(Vec::new()),
            Some(uses) => std::borrow::Cow::Borrowed(uses),
            None => std::borrow::Cow::Owned(crate::fold::use_counts(func)),
        };
        // Reverse block/instruction order gives larger consumers first choice
        // without allocating another entry for every instruction. Block ownership
        // is already stored in Function; no Inst-to-Block hash table is needed.
        let blocks: Vec<_> = if options.trees {
            func.blocks().collect()
        } else {
            Vec::new()
        };
        for &block in blocks.iter().rev() {
            for (end, &inst) in func.block_insts(block).iter().enumerate().rev() {
                if plan.covered(inst) {
                    continue;
                }
                // Preserve the quotient and its trap guard at the original
                // instruction. With exactly this adjacent sequence, emission
                // between IDIV/DIV and the remainder read only moves registers
                // or spill homes; no operation can clobber reserved RDX.
                if target == Target::X86 && end >= 2 {
                    let insts = func.block_insts(block);
                    let (division, product) = (insts[end - 2], insts[end - 1]);
                    if let InstData::Binary {
                        op: BinaryOp::Isub,
                        args: sub,
                    } = *func.inst_data(inst)
                        && let InstData::Binary {
                            op: BinaryOp::Imul,
                            args: mul,
                        } = *func.inst_data(product)
                        && let InstData::Binary {
                            op: BinaryOp::Sdiv | BinaryOp::Udiv,
                            args: div,
                        } = *func.inst_data(division)
                        && matches!(
                            func.value_type(func.resolve(div[0])),
                            Type::I8 | Type::I16 | Type::I32 | Type::I64
                        )
                        && crate::integer_division::reducible_divisor(
                            func,
                            func.inst_data(division),
                        )
                        .is_none()
                        && let quotient = func.resolve(func.first_result(division))
                        && quotient == func.first_result(division)
                        && func.resolve(sub[0]) == func.resolve(div[0])
                        && func.resolve(sub[1]) == func.first_result(product)
                        && uses[func.first_result(product).index()] == 1
                        && ((func.resolve(mul[0]) == quotient
                            && func.resolve(mul[1]) == func.resolve(div[1]))
                            || (func.resolve(mul[1]) == quotient
                                && func.resolve(mul[0]) == func.resolve(div[1])))
                        && !plan.covered(product)
                        && !plan.covered(division)
                        && [division, product].into_iter().all(|i| {
                            match (func.inst_loc(i), func.inst_loc(inst)) {
                                (Some(a), Some(b)) => a.file == b.file && a.line == b.line,
                                (Some(_), None) => false,
                                _ => true,
                            }
                        })
                    {
                        plan.cover(product);
                        plan.set_operands(inst, vec![quotient]);
                        plan.division_remainders.insert(inst, division);
                        continue;
                    }
                }
                // A compare produces exactly 0 or 1. Its unsigned extension
                // and an equality test against zero can use the original
                // condition. Keep this cover contiguous (constants aside).
                if let InstData::Icmp {
                    cond: cc @ (IntCC::Eq | IntCC::Ne),
                    args,
                } = *func.inst_data(inst)
                    && crate::fold::iconst(func, args[1]) == Some(0)
                    && let ValueDef::Result(convert, _) = func.value_def(func.resolve(args[0]))
                    && let InstData::Convert {
                        op: ConvertOp::Uext,
                        ty,
                        arg,
                    } = *func.inst_data(convert)
                    && matches!(ty, Type::I16 | Type::I32 | Type::I64)
                    && let ValueDef::Result(compare, _) = func.value_def(func.resolve(arg))
                    && let InstData::Icmp { args: inputs, .. } = *func.inst_data(compare)
                    && func.value_type(func.resolve(inputs[0])) != Type::I128
                    && uses[func.resolve(arg).index()] == 1
                    && uses[func.resolve(args[0]).index()] == 1
                    && func.inst_block(compare) == block
                    && func.inst_block(convert) == block
                    && !plan.covered(compare)
                    && !plan.covered(convert)
                    && !func.is_removed(compare)
                    && !func.is_removed(convert)
                {
                    let insts = func.block_insts(block);
                    let start = insts[..end].iter().rposition(|&i| i == compare).unwrap();
                    let local = insts[start..=end].iter().all(|&i| {
                        (i == compare
                            || i == convert
                            || i == inst
                            || matches!(func.inst_data(i), InstData::Iconst { .. }))
                            && match (func.inst_loc(i), func.inst_loc(inst)) {
                                (Some(a), Some(b)) => a.file == b.file && a.line == b.line,
                                _ => true,
                            }
                    });
                    if local {
                        plan.conditions.insert(inst, (compare, cc == IntCC::Eq));
                        plan.set_operands(inst, plan.operands(func, compare));
                        plan.cover(compare);
                        plan.cover(convert);
                    }
                    continue;
                }
                // Keep the effect at its original instruction. Only the pure address
                // calculation is consumed; atomic operations are deliberately excluded.
                let memory = match *func.inst_data(inst) {
                    InstData::Load { ty, addr, .. } => Some((addr, ty)),
                    InstData::Store { value, addr, .. } => {
                        Some((addr, func.value_type(func.resolve(value))))
                    }
                    _ => None,
                };
                if let Some((addr, ty)) = memory {
                    if plan.select_indexed_address(func, inst, addr, ty, &uses, target) {
                        continue;
                    }
                    let addr = func.resolve(addr);
                    if uses[addr.index()] == 1
                        && let ValueDef::Result(child, _) = func.value_def(addr)
                        && func.inst_block(child) == block
                        && !plan.covered(child)
                        && !func.is_removed(child)
                        && let InstData::Binary {
                            op: BinaryOp::Ptradd,
                            args,
                        } = *func.inst_data(child)
                        && func.value_type(func.resolve(args[1])) == Type::I64
                        && let Some(offset) = crate::fold::iconst(func, args[1])
                        && let Some(size) = (if ty == Type::Ptr { Some(8) } else { ty.bytes() })
                            .filter(|&size| {
                                size <= 8
                                    || target == Target::Aarch64 && ty.is_vector() && size == 16
                                    // Bounded x86 pilot: this ordinary load emits one
                                    // exact m128 MOVDQU, or feeds the closed AVX2
                                    // m128 widening cover. Do not admit i128 (split
                                    // accesses), stores, volatile or other vectors.
                                    || target == Target::X86
                                        && matches!(
                                            func.inst_data(inst),
                                            InstData::Load {
                                                ty: Type::Vector(keel_ir::VectorType::I32x4),
                                                flags,
                                                ..
                                            } if !flags.volatile
                                        )
                                        && !plan.has_constraint(inst)
                                        && !plan.has_constraint(child)
                                        && !matches!(
                                            (func.inst_loc(child), func.inst_loc(inst)),
                                            (Some(_), None)
                                        )
                            })
                        && match target {
                            Target::Aarch64 => {
                                offset >= 0
                                    && offset % i64::from(size) == 0
                                    && offset / i64::from(size) < 4096
                            }
                            Target::X86 => i32::try_from(offset).is_ok(),
                        }
                        && match (func.inst_loc(child), func.inst_loc(inst)) {
                            (Some(a), Some(b)) => a.file == b.file && a.line == b.line,
                            _ => true,
                        }
                    {
                        let base = func.resolve(args[0]);
                        let mut operands = plan.operands(func, inst);
                        let index = operands.iter().position(|&v| v == addr).unwrap();
                        operands[index] = base;
                        plan.set_operands(inst, operands);
                        plan.addresses.insert(addr, (base, offset));
                        plan.cover(child);
                    }
                    continue;
                }
                let InstData::Binary { op, args } = *func.inst_data(inst) else {
                    continue;
                };
                let ty = func.value_type(func.resolve(args[0]));
                if !matches!(ty, Type::I32 | Type::I64 | Type::Ptr) {
                    continue;
                }
                // The measured x86 recurrence is x * 3 + 7. Keep this
                // deliberately bounded to factor three: one LEA reads one live
                // input and replaces a single-use product plus constant add.
                if target == Target::X86
                    && op == BinaryOp::Iadd
                    && matches!(ty, Type::I32 | Type::I64)
                    // An aliased dead definition can still be in the layout.
                    && func.inst_results(inst).first().is_some_and(|&v| func.resolve(v) == v)
                {
                    for side in [0, 1] {
                        let product = func.resolve(args[side]);
                        if uses[product.index()] != 1 {
                            continue;
                        }
                        let Some(offset) = crate::fold::iconst(func, args[1 - side]) else {
                            continue;
                        };
                        // A 32-bit LEA keeps the low 32 result bits; a 64-bit
                        // LEA sign-extends disp32. Normalize only the i32 case.
                        let offset = if ty == Type::I32 {
                            i64::from(offset as i32)
                        } else {
                            offset
                        };
                        let Ok(displacement) = i32::try_from(offset) else {
                            continue;
                        };
                        let ValueDef::Result(child, _) = func.value_def(product) else {
                            continue;
                        };
                        if func.is_removed(child)
                            || func.inst_block(child) != block
                            || plan.covered(child)
                        {
                            continue;
                        }
                        // An unlocated root inherits the last emitted .loc,
                        // which may have changed between child and root. Do
                        // not erase a located child's executable source range.
                        match (func.inst_loc(child), func.inst_loc(inst)) {
                            (Some(_), None) => continue,
                            (Some(a), Some(b)) if a.file != b.file || a.line != b.line => continue,
                            _ => {}
                        }
                        let InstData::Binary {
                            op: BinaryOp::Imul,
                            args: inputs,
                        } = *func.inst_data(child)
                        else {
                            continue;
                        };
                        for constant_side in [1, 0] {
                            let Some(factor) = crate::fold::iconst(func, inputs[constant_side])
                            else {
                                continue;
                            };
                            let factor = if ty == Type::I32 {
                                i64::from(factor as i32)
                            } else {
                                factor
                            };
                            let input = func.resolve(inputs[1 - constant_side]);
                            if factor != 3 || func.value_type(input) != ty {
                                continue;
                            }
                            plan.cover(child);
                            plan.matches.insert(
                                inst,
                                Match {
                                    machine: Machine::LeaMul3 { displacement },
                                    args: vec![input],
                                    shift: 1,
                                },
                            );
                            break;
                        }
                        if plan.matches.contains_key(&inst) {
                            break;
                        }
                    }
                    if plan.matches.contains_key(&inst) {
                        continue;
                    }
                }
                // ARM unsigned extraction consumes only a single-use shift.
                // Masks wider than the remaining field deliberately fall back.
                if target == Target::Aarch64
                    && op == BinaryOp::And
                    && matches!(ty, Type::I32 | Type::I64)
                {
                    let bits = if ty == Type::I32 { 32u64 } else { 64 };
                    for side in [0, 1] {
                        let value = func.resolve(args[side]);
                        if uses[value.index()] != 1 {
                            continue;
                        }
                        let Some(raw_mask) = crate::fold::iconst(func, args[1 - side]) else {
                            continue;
                        };
                        let mask = raw_mask as u64 & (u64::MAX >> (64 - bits));
                        // A nonempty proper low run; all-ones remains a plain shift.
                        let width = mask.trailing_ones() as u64;
                        if width == 0 || width >= bits || mask != (1u64 << width) - 1 {
                            continue;
                        }
                        let ValueDef::Result(child, _) = func.value_def(value) else {
                            continue;
                        };
                        if func.is_removed(child)
                            || func.inst_block(child) != block
                            || plan.covered(child)
                        {
                            continue;
                        }
                        let InstData::Binary {
                            op: BinaryOp::Ushr,
                            args: inputs,
                        } = *func.inst_data(child)
                        else {
                            continue;
                        };
                        if func.value_type(func.resolve(inputs[0])) != ty {
                            continue;
                        }
                        let Some(raw_shift) = crate::fold::iconst(func, inputs[1]) else {
                            continue;
                        };
                        let lsb = raw_shift as u64 & (bits - 1);
                        if lsb + width > bits {
                            continue;
                        }
                        if let (Some(a), Some(b)) = (func.inst_loc(child), func.inst_loc(inst))
                            && (a.file != b.file || a.line != b.line)
                        {
                            continue;
                        }
                        plan.cover(child);
                        plan.matches.insert(
                            inst,
                            Match {
                                machine: Machine::Ubfx {
                                    lsb: lsb as u8,
                                    width: width as u8,
                                },
                                args: vec![func.resolve(inputs[0])],
                                shift: 0,
                            },
                        );
                        break;
                    }
                    if plan.matches.contains_key(&inst) {
                        continue;
                    }
                }
                'rules: for rule in RULES.iter().filter(|r| r.target == target && r.root == op) {
                    for &side in if rule.commute { &[1, 0][..] } else { &[1][..] } {
                        let value = func.resolve(args[side]);
                        if uses[value.index()] != 1 {
                            continue;
                        }
                        let ValueDef::Result(child, _) = func.value_def(value) else {
                            continue;
                        };
                        if func.is_removed(child)
                            || func.inst_block(child) != block
                            || plan.covered(child)
                        {
                            continue;
                        }
                        // Keep separately breakable source lines when explicit
                        // debug locations are present. Same-line expressions can fuse.
                        if let (Some(a), Some(b)) = (func.inst_loc(child), func.inst_loc(inst))
                            && (a.file != b.file || a.line != b.line)
                        {
                            continue;
                        }
                        let InstData::Binary {
                            op: child_op,
                            args: child_args,
                        } = *func.inst_data(child)
                        else {
                            continue;
                        };
                        if child_op != rule.child {
                            continue;
                        }
                        let shift = if child_op == BinaryOp::Shl {
                            let Some(k) = crate::fold::iconst(func, child_args[1]) else {
                                continue;
                            };
                            // IR masks shift counts by the operand width.
                            let child_ty = func.value_type(func.resolve(child_args[0]));
                            let k = (k as u64 & (child_ty.bytes().unwrap() as u64 * 8 - 1)) as u8;
                            if target == Target::X86 && k > 3 {
                                continue;
                            }
                            k
                        } else {
                            0
                        };
                        // Allocate leaves only after all legality checks pass.
                        let mut leaves =
                            Vec::with_capacity(if child_op == BinaryOp::Shl { 2 } else { 3 });
                        leaves.push(func.resolve(args[1 - side]));
                        leaves.push(func.resolve(child_args[0]));
                        if child_op != BinaryOp::Shl {
                            leaves.push(func.resolve(child_args[1]));
                        }
                        plan.cover(child);
                        plan.matches.insert(
                            inst,
                            Match {
                                machine: rule.machine,
                                args: leaves,
                                shift,
                            },
                        );
                        break 'rules;
                    }
                }
            }
        }
        if target == Target::X86 && options.trees {
            plan.fold_memory_rhs(func, &uses);
        }
        // Generate affinities only for instructions that survive selection.
        // Three-address fused instructions need no destructive-input copy.
        if options.register_hints {
            plan.hints.0.reserve(func.num_values());
        }
        for (_, inst) in func.layout_insts().filter(|_| options.register_hints) {
            if plan.covered(inst) {
                continue;
            }
            let Some(&result) = func.inst_results(inst).first() else {
                continue;
            };
            if !plan.matches.contains_key(&inst)
                && let Some(input) = generated_hint(target, func.inst_data(inst))
            {
                plan.hints.insert(result, func.resolve(input));
            }
        }
        plan
    }

    /// Preserve the byte access position; only inert constants may separate
    /// the load and conversion. Run after address selection and before allocation.
    pub(crate) fn fold_signed_byte_loads(&mut self, func: &Function, uses: &[u8]) {
        for block in func.blocks() {
            let mut previous = None;
            for &consumer in func.block_insts(block) {
                if matches!(func.inst_data(consumer), InstData::Iconst { .. }) {
                    // A constrained or separately located constant is a boundary.
                    if self.has_constraint(consumer)
                        || self.matches.contains_key(&consumer)
                        || self.conditions.contains_key(&consumer)
                        || func.is_removed(consumer)
                        || previous.is_some_and(|i| func.inst_loc(i) != func.inst_loc(consumer))
                    {
                        previous = None;
                    }
                    continue;
                }
                let candidate = previous;
                previous = Some(consumer);
                let Some(load) = candidate else { continue };
                let InstData::Load {
                    ty: Type::I8,
                    addr,
                    flags,
                } = *func.inst_data(load)
                else {
                    continue;
                };
                let InstData::Convert {
                    op: ConvertOp::Sext,
                    ty: Type::I32,
                    arg,
                } = *func.inst_data(consumer)
                else {
                    continue;
                };
                let value = func.first_result(load);
                if flags.volatile
                    || func.resolve(arg) != value
                    || uses[value.index()] != 1
                    || [load, consumer].iter().any(|&i| {
                        func.is_removed(i)
                            || self.covered(i)
                            || self.matches.contains_key(&i)
                            || self.has_constraint(i)
                            || self.conditions.contains_key(&i)
                            || func.inst_loc(i) != func.inst_loc(load)
                            || func.resolve(func.first_result(i)) != func.first_result(i)
                    })
                {
                    continue;
                }
                // Transfer all selected leaves, including indexed and symbolic
                // addresses. Never resurrect the covered address's SSA result.
                let operands = self.operands(func, load);
                self.set_operands(consumer, operands);
                self.signed_byte_memory.insert(consumer, func.resolve(addr));
                self.cover(load);
            }
        }
    }

    /// Do this after address selection, so the load's selected address leaves
    /// (including a covered ptradd) move to the consumer before allocation.
    /// There is no intervening IR instruction, including a second memory read.
    /// Keep debug functions on their existing materialization/location path.
    fn fold_memory_rhs(&mut self, func: &Function, uses: &[u8]) {
        if func.debug_loc.is_some()
            || !func.debug_vars.is_empty()
            || func.layout_insts().any(|(_, i)| func.inst_loc(i).is_some())
        {
            return;
        }
        for block in func.blocks() {
            for pair in func.block_insts(block).windows(2) {
                let [load, consumer] = [pair[0], pair[1]];
                let InstData::Load { ty, addr, flags } = *func.inst_data(load) else {
                    continue;
                };
                if !matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64) {
                    continue;
                }
                let (args, binary) = match *func.inst_data(consumer) {
                    InstData::Icmp { args, .. } => (args, false),
                    InstData::Binary { op, args }
                        if matches!(
                            op,
                            BinaryOp::Iadd
                                | BinaryOp::Isub
                                | BinaryOp::And
                                | BinaryOp::Or
                                | BinaryOp::Xor
                        ) || (op == BinaryOp::Imul && ty != Type::I8) =>
                    {
                        (args, true)
                    }
                    _ => continue,
                };
                let value = func.inst_results(load)[0];
                let result = func.inst_results(consumer)[0];
                if flags.volatile
                    || func.resolve(value) != value
                    || func.resolve(result) != result
                    || func.resolve(args[1]) != value
                    || uses[value.index()] != 1
                    || self.covered(load)
                    || self.covered(consumer)
                    || self.matches.contains_key(&consumer)
                {
                    continue;
                }
                let mut operands = vec![func.resolve(args[0])];
                operands.extend(self.operands(func, load));
                self.set_operands(consumer, operands);
                if binary {
                    self.binary_memory.insert(consumer, addr);
                } else {
                    self.compare_memory.insert(consumer, addr);
                }
                self.cover(load);
            }
        }
    }
}

#[cfg(test)]
mod signed_byte_tests {
    use super::*;
    const SOURCE: &str = "func @f(ptr,i64,i8,i32) {\nblock0(v0:ptr,v1:i64,v2:i8,v3:i32):\n v4 = load.i8 v0\n v5 = sext.i32 v4\n return\n}\n";
    #[test]
    fn boundaries_and_selected_address_leaves() {
        for at in 0..2 {
            for boundary in 0..6 {
                let mut m = keel_ir::parse(SOURCE).unwrap();
                let f = &mut m.functions[0];
                let pair: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
                let mut s = Selection::default();
                match boundary {
                    0 => {
                        s.conditions.insert(pair[at], (pair[0], false));
                    }
                    1 => s.cover(pair[at]),
                    2 => {
                        s.matches.insert(
                            pair[at],
                            Match {
                                machine: Machine::Lea,
                                args: vec![],
                                shift: 0,
                            },
                        );
                    }
                    3 => s.set_constraint(
                        pair[at],
                        ValuePart::low(f.first_result(pair[at])),
                        OperandKind::Def,
                        Constraint::Register,
                    ),
                    4 => f.replace_all_uses(f.first_result(pair[at]), Value::new(at + 2)),
                    _ => f.set_inst_loc(
                        pair[at],
                        Some(keel_ir::SourceLoc {
                            file: keel_ir::SourceFile::new(0),
                            line: 1,
                            col: 1,
                            scope: None,
                        }),
                    ),
                }
                s.fold_signed_byte_loads(f, &crate::fold::use_counts(f));
                assert!(s.signed_byte_memory.is_empty(), "{at} {boundary}");
            }
        }
        let m = keel_ir::parse(SOURCE).unwrap();
        let f = &m.functions[0];
        let pair: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
        let mut s = Selection::default();
        let leaves = vec![Value::new(0), Value::new(1)];
        s.set_operands(pair[0], leaves.clone());
        s.fold_signed_byte_loads(f, &crate::fold::use_counts(f));
        assert!(s.covered(pair[0]));
        assert_eq!(s.operands(f, pair[1]), leaves);
    }
}
