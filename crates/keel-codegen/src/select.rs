//! Tree patterns matched against the SSA DAG before register allocation.
//! A cover is local to a block and only consumes single-use, pure nodes.
//! Shared DAG nodes remain materialized; selected leaves are live at the root.
use keel_ir::{BinaryOp, ConvertOp, EntityRef, Function, Inst, InstData, Type, Value, ValueDef};
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
#[derive(Default)]
pub struct Selection {
    pub matches: HashMap<Inst, Match>,
    covered: HashSet<Inst>,
    allocation_operands: HashMap<Inst, Vec<Value>>,
    /// Result-to-input affinities, never constraints: interference wins.
    pub hints: HashMap<Value, Vec<Value>>,
}
impl Selection {
    /// Suppress an instruction subsumed by a selected root. Custom selectors
    /// must preserve all side effects and every use of its results.
    pub fn cover(&mut self, inst: Inst) {
        self.covered.insert(inst);
    }
    /// Override allocation operands after encoding values in a target operand.
    /// Values must be resolved and include every value read by the emitter.
    pub fn set_operands(&mut self, inst: Inst, operands: Vec<Value>) {
        self.allocation_operands.insert(inst, operands);
    }

    pub fn covered(&self, inst: Inst) -> bool {
        self.covered.contains(&inst)
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
                self.covered.insert(inst);
            }
            if self.covered(inst) || self.matches.contains_key(&inst) {
                continue;
            }
            let folded = folds(func.inst_data(inst));
            if folded.is_empty() {
                continue;
            }
            let mut args = self.operands(func, inst);
            for value in folded {
                let value = func.resolve(value);
                if let Some(i) = args.iter().position(|&v| v == value) {
                    args.remove(i);
                }
            }
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
        let mut uses = vec![0usize; func.num_values()];
        let mut blocks = HashMap::new();
        for (block, inst) in func.layout_insts() {
            blocks.insert(inst, block);
            func.inst_data(inst)
                .for_each_operand(|v| uses[func.resolve(v).index()] += 1);
        }
        // Reverse traversal gives larger consumers first choice of their trees.
        let insts: Vec<_> = func.layout_insts().map(|(_, i)| i).collect();
        for &inst in insts.iter().rev().filter(|_| options.trees) {
            if plan.covered(inst) {
                continue;
            }
            let InstData::Binary { op, args } = *func.inst_data(inst) else {
                continue;
            };
            let ty = func.value_type(func.resolve(args[0]));
            if !matches!(ty, Type::I32 | Type::I64 | Type::Ptr) {
                continue;
            }
            'rules: for rule in RULES.iter().filter(|r| r.target == target && r.root == op) {
                for side in if rule.commute { vec![1, 0] } else { vec![1] } {
                    let value = func.resolve(args[side]);
                    if uses[value.index()] != 1 {
                        continue;
                    }
                    let ValueDef::Result(child, _) = func.value_def(value) else {
                        continue;
                    };
                    if blocks.get(&child) != blocks.get(&inst) || plan.covered(child) {
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
                    let other = func.resolve(args[1 - side]);
                    let mut leaves = vec![other, func.resolve(child_args[0])];
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
                        leaves.push(func.resolve(child_args[1]));
                        0
                    };
                    plan.covered.insert(child);
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
        // Generate affinities only for instructions that survive selection.
        // Three-address fused instructions need no destructive-input copy.
        for &inst in insts.iter().filter(|_| options.register_hints) {
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
