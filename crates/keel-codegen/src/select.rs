//! Tree patterns matched against the SSA DAG before register allocation.
//! A cover is local to a block and only consumes single-use, pure nodes.
//! Shared DAG nodes remain materialized; selected leaves are live at the root.
use crate::regalloc::{Constraint, OperandKind, ValuePart};
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, Inst, InstData, IntCC, Type, Value, ValueDef,
};
use std::collections::{HashMap, HashSet};

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
    pub matches: HashMap<Inst, Match>,
    covered: Vec<bool>,
    /// Single-use ptradd results encoded at their original memory consumer.
    pub(crate) addresses: HashMap<Value, (Value, i64)>,
    /// An outer zero test uses the original integer compare, optionally inverted.
    pub(crate) conditions: HashMap<Inst, (Inst, bool)>,
    allocation_operands: OperandTable,
    // Operand occurrences removed by fold_operands, so later target folding
    // (including addresses known only after allocation) does not remove them twice.
    allocation_folds: HashMap<Inst, Vec<Value>>,
    /// Result-to-input affinities, never constraints: interference wins.
    pub hints: HashMap<Value, Vec<Value>>,
}
impl Selection {
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
    pub(crate) fn fuse_compares(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
    ) -> HashSet<Value> {
        self.fuse_compare_pairs(func, folds, crate::fold::fused_compare_pairs(func))
    }

    pub(crate) fn fuse_aarch64_compares(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
    ) -> HashSet<Value> {
        self.fuse_compare_pairs(func, folds, crate::fold::aarch64_compare_pairs(func))
    }

    fn fuse_compare_pairs(
        &mut self,
        func: &Function,
        folds: impl Fn(&InstData) -> Vec<Value>,
        pairs: Vec<(Value, Inst, Inst)>,
    ) -> HashSet<Value> {
        let mut fused = HashSet::new();
        for (cond, compare, branch) in pairs {
            let mut args = self.operands(func, branch);
            let Some(i) = args.iter().position(|&v| v == cond) else {
                continue;
            };
            if self.covered(compare)
                || self.covered(branch)
                || self
                    .constraints
                    .keys()
                    .any(|&(inst, _, _)| inst == compare || inst == branch)
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
    pub fn fold_operands(&mut self, func: &Function, folds: impl Fn(&InstData) -> Vec<Value>) {
        let elided = crate::fold::folded_values(func, self, &folds);
        for (_, inst) in func.layout_insts() {
            if func
                .inst_results(inst)
                .first()
                .is_some_and(|v| elided.contains(v))
            {
                self.cover(inst);
            }
            if self.covered(inst) || self.matches.contains_key(&inst) {
                continue;
            }
            let mut folded = folds(self.folded_data(func, inst));
            self.remaining_folds(func, inst, &mut folded);
            if folded.is_empty() {
                continue;
            }
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
        let mut plan = Self::default();
        if !options.trees && !options.register_hints {
            return plan;
        }
        // Tree legality only distinguishes exactly one use from zero/many.
        // Hint-only selection does not need any whole-function use counts.
        let mut uses = Vec::new();
        if options.trees {
            uses.resize(func.num_values(), 0u8);
            for (_, inst) in func.layout_insts() {
                func.inst_data(inst).for_each_operand(|v| {
                    let count = &mut uses[func.resolve(v).index()];
                    *count = (*count + 1).min(2);
                });
            }
        }
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
                            .filter(|&size| size <= 8)
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
                        if let (Some(a), Some(b)) = (func.inst_loc(child), func.inst_loc(inst))
                            && (a.file != b.file || a.line != b.line)
                        {
                            continue;
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
        // Generate affinities only for instructions that survive selection.
        // Three-address fused instructions need no destructive-input copy.
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
                plan.hints.insert(result, vec![func.resolve(input)]);
            }
        }
        plan
    }
}
