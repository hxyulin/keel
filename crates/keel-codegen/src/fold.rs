//! Folding values into the instructions that use them, shared by the
//! targets. Each target says which operands of an instruction it can
//! encode in the instruction itself (a stack address as a memory operand,
//! a constant as an immediate); a value folded into every one of its uses
//! needs no code of its own.

use keel_ir::{Function, InstData, Type, Value, ValueDef};
use std::collections::{HashMap, HashSet};

/// The value of `v` if an `iconst` defines it.
pub(crate) fn iconst(func: &Function, v: Value) -> Option<i64> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(v)) else {
        return None;
    };
    match *func.inst_data(inst) {
        InstData::Iconst { ty, imm } if ty != Type::I128 => Some(imm),
        _ => None,
    }
}

/// The `stack_addr` and `iconst` results every use of which is folded
/// into the instruction using it, by `folds`, which gives the operands an
/// instruction folds (each once per operand position).
pub(crate) fn folded_values(
    func: &Function,
    selection: &crate::select::Selection,
    folds: impl Fn(&InstData) -> Vec<Value>,
) -> HashSet<Value> {
    let mut values = HashSet::new();
    let mut unfolded = HashSet::new();
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        if let InstData::StackAddr { .. } | InstData::Iconst { .. } = data {
            values.extend(func.inst_results(inst).iter().copied());
        }
        let mut folds: Vec<Value> = if selection.matches.contains_key(&inst) {
            vec![]
        } else {
            folds(data).into_iter().map(|v| func.resolve(v)).collect()
        };
        selection.for_each_operand(func, inst, |v| {
            let v = func.resolve(v);
            match folds.iter().position(|&f| f == v) {
                Some(i) => {
                    folds.swap_remove(i);
                }
                None => {
                    unfolded.insert(v);
                }
            }
        });
    }
    values.retain(|v| !unfolded.contains(v));
    values
}

/// The `icmp` and `fcmp` results only used as the condition of the
/// `brif` right after them: the branch tests the flags of the compare
/// itself, a compare and a conditional branch, with no 0/1 value in between.
pub(crate) fn fused_compares(func: &Function) -> HashSet<Value> {
    let mut uses: HashMap<Value, usize> = HashMap::new();
    let mut candidates = HashSet::new();
    for block in func.blocks() {
        let insts = func.block_insts(block);
        for (i, &inst) in insts.iter().enumerate() {
            let data = func.inst_data(inst);
            data.for_each_operand(|v| *uses.entry(func.resolve(v)).or_default() += 1);
            let compare = match *data {
                InstData::Icmp { args, .. } => func.value_type(func.resolve(args[0])) != Type::I128,
                InstData::Fcmp { .. } => true,
                _ => false,
            };
            let Some(&next) = insts.get(i + 1) else {
                continue;
            };
            if let (true, InstData::Brif { cond, .. }) = (compare, func.inst_data(next)) {
                let result = func.inst_results(inst)[0];
                if func.resolve(*cond) == result {
                    candidates.insert(result);
                }
            }
        }
    }
    candidates.retain(|v| uses.get(v) == Some(&1));
    candidates
}
