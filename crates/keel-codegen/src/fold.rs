//! Folding values into the instructions that use them, shared by the
//! targets. Each target says which operands of an instruction it can
//! encode in the instruction itself (a stack address as a memory operand,
//! a constant as an immediate); a value folded into every one of its uses
//! needs no code of its own.

use keel_ir::{EntityRef, Function, InstData, Type, Value, ValueDef};
use std::collections::HashSet;

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
    let mut values = Vec::new();
    let mut unfolded = vec![false; func.num_values()];
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        if let InstData::StackAddr { .. } | InstData::Iconst { .. } = data {
            values.extend(func.inst_results(inst).iter().copied());
        }
        if selection.covered(inst) {
            continue;
        }
        let mut folds: Vec<Value> = if selection.matches.contains_key(&inst) {
            vec![]
        } else {
            let mut folded = folds(data);
            selection.remaining_folds(func, inst, &mut folded);
            folded
        };
        selection.for_each_operand(func, inst, |v| {
            let v = func.resolve(v);
            match folds.iter().position(|&f| f == v) {
                Some(i) => {
                    folds.swap_remove(i);
                }
                None => {
                    unfolded[v.index()] = true;
                }
            }
        });
    }
    values
        .into_iter()
        .filter(|v| !unfolded[v.index()])
        .collect()
}

/// The `icmp` and `fcmp` results only used as the condition of the
/// `brif` right after them: the branch tests the flags of the compare
/// itself, a compare and a conditional branch, with no 0/1 value in between.
#[cfg(test)]
pub(crate) fn fused_compares(func: &Function) -> HashSet<Value> {
    fused_compare_pairs(func)
        .into_iter()
        .map(|(value, _, _)| value)
        .collect()
}

pub(crate) fn fused_compare_pairs(func: &Function) -> Vec<(Value, keel_ir::Inst, keel_ir::Inst)> {
    // Only zero, one, or multiple raw IR uses matter for compare fusion.
    let mut uses = vec![0u8; func.num_values()];
    let mut candidates = Vec::new();
    for block in func.blocks() {
        let insts = func.block_insts(block);
        for (i, &inst) in insts.iter().enumerate() {
            let data = func.inst_data(inst);
            data.for_each_operand(|v| {
                let count = &mut uses[func.resolve(v).index()];
                *count = (*count + 1).min(2);
            });
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
                    candidates.push((result, inst, next));
                }
            }
        }
    }
    candidates
        .into_iter()
        .filter(|(v, _, _)| uses[v.index()] == 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::Selection;
    use std::collections::HashMap;

    fn function(source: &str) -> Function {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        module.functions.into_iter().next().unwrap()
    }

    fn immediates(func: &Function, data: &InstData) -> Vec<Value> {
        match *data {
            InstData::Binary { args, .. } if iconst(func, args[1]).is_some() => vec![args[1]],
            _ => vec![],
        }
    }

    // Former hash-based analysis, before optional allocation operand filtering
    // (where it had a double-subtraction bug).
    fn reference_folded(func: &Function, selection: &Selection) -> HashSet<Value> {
        let mut values = HashSet::new();
        let mut unfolded = HashSet::new();
        for (_, inst) in func.layout_insts() {
            let data = func.inst_data(inst);
            if matches!(data, InstData::StackAddr { .. } | InstData::Iconst { .. }) {
                values.extend(func.inst_results(inst).iter().copied());
            }
            let mut folds: Vec<_> = if selection.matches.contains_key(&inst) {
                vec![]
            } else {
                immediates(func, data)
                    .into_iter()
                    .map(|v| func.resolve(v))
                    .collect()
            };
            selection.for_each_operand(func, inst, |v| {
                let v = func.resolve(v);
                if let Some(i) = folds.iter().position(|&f| f == v) {
                    folds.swap_remove(i);
                } else {
                    unfolded.insert(v);
                }
            });
        }
        values.retain(|v| !unfolded.contains(v));
        values
    }

    #[test]
    fn dense_folding_matches_hash_analysis_with_aliases_and_selected_leaves() {
        let mut func = function(
            "func @f(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 7
 v2 = iconst.i64 9
 v3 = iadd v1, v1
 v4 = imul v0, v2
 v5 = iadd v3, v4
 return v5
}",
        );
        let constants: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Iconst { .. })
                    .then(|| func.inst_results(inst)[0])
            })
            .collect();
        for alias in [false, true] {
            if alias {
                func.replace_all_uses(constants[1], constants[0]);
            }
            for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                for selection in [Selection::default(), Selection::compute(&func, target)] {
                    assert_eq!(
                        folded_values(&func, &selection, |data| immediates(&func, data)),
                        reference_folded(&func, &selection)
                    );
                }
            }
        }
    }

    #[test]
    fn allocation_folds_are_not_applied_twice_and_later_addresses_still_fold() {
        let func = function(
            "func @f() -> i64 {
 ss0 = stack_slot 8, align 8
block0:
 v0 = iconst.i64 7
 v1 = iadd v0, v0
 v2 = stack_addr ss0
 v3 = load.i64 v2
 v4 = iadd v1, v3
 return v4
}",
        );
        let insts: Vec<_> = func.layout_insts().map(|(_, inst)| inst).collect();
        let constant = func.inst_results(insts[0])[0];
        let address = func.inst_results(insts[2])[0];
        let mut selection = Selection::default();
        for _ in 0..2 {
            selection.fold_operands(&func, |data| immediates(&func, data));
            assert_eq!(selection.operands(&func, insts[1]), vec![constant]);
            let folded = folded_values(&func, &selection, |data| match *data {
                InstData::Load { addr, .. } => vec![addr],
                _ => immediates(&func, data),
            });
            assert_eq!(folded, HashSet::from([address]));
        }
    }

    #[test]
    fn covered_instructions_do_not_call_target_folding() {
        let func = function(
            "func @f() -> i64 {
block0:
 v0 = iconst.i64 7
 v1 = iadd v0, v0
 return v0
}",
        );
        let mut selection = Selection::default();
        let binary = func
            .layout_insts()
            .find_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Binary { .. }).then_some(inst)
            })
            .unwrap();
        selection.cover(binary);
        folded_values(&func, &selection, |data| {
            assert!(!matches!(data, InstData::Binary { .. }));
            vec![]
        });
    }

    fn reference_compares(func: &Function) -> HashSet<Value> {
        let mut uses = HashMap::<Value, usize>::new();
        let mut candidates = HashSet::new();
        for block in func.blocks() {
            let insts = func.block_insts(block);
            for (i, &inst) in insts.iter().enumerate() {
                let data = func.inst_data(inst);
                data.for_each_operand(|v| *uses.entry(func.resolve(v)).or_default() += 1);
                let compare = match *data {
                    InstData::Icmp { args, .. } => {
                        func.value_type(func.resolve(args[0])) != Type::I128
                    }
                    InstData::Fcmp { .. } => true,
                    _ => false,
                };
                if compare
                    && let Some(&next) = insts.get(i + 1)
                    && let InstData::Brif { cond, .. } = func.inst_data(next)
                {
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

    #[test]
    fn saturated_compare_counts_match_raw_use_analysis() {
        for ty in ["i64", "i128", "f64"] {
            for extra in ["", "v3 = uext.i64 v2\n", "v3 = and v2, v2\n"] {
                let compare = if ty == "f64" { "fcmp.eq" } else { "icmp.eq" };
                let source = format!(
                    "func @f({ty}, {ty}) -> i64 {{
block0(v0: {ty}, v1: {ty}):
 v2 = {compare} v0, v1
 {extra}brif v2, block1, block2
block1:
 v4 = iconst.i64 1
 return v4
block2:
 v5 = iconst.i64 0
 return v5
}}"
                );
                let func = function(&source);
                assert_eq!(fused_compares(&func), reference_compares(&func));
                assert_eq!(
                    fused_compares(&func).len(),
                    usize::from(extra.is_empty() && ty != "i128")
                );
            }
        }
    }

    #[test]
    fn compare_branch_arguments_and_aliases_count_as_raw_uses() {
        let mut func = function(
            "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = icmp.eq v0, v1
 v3 = icmp.ne v0, v1
 brif v3, block1(v2), block2
block1(v4: i8):
 v5 = uext.i64 v4
 return v5
block2:
 v6 = iconst.i64 0
 return v6
}",
        );
        let compares: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Icmp { .. })
                    .then(|| func.inst_results(inst)[0])
            })
            .collect();
        assert_eq!(fused_compares(&func), HashSet::from([compares[1]]));
        // The branch condition and its edge argument now resolve to the same
        // adjacent comparison: two reads, so materialize the comparison.
        func.replace_all_uses(compares[0], compares[1]);
        assert_eq!(fused_compares(&func), reference_compares(&func));
        assert!(fused_compares(&func).is_empty());
    }
}
