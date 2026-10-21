//! Closed positive-i64 clamp. The mask is deliberately never published:
//! ~sign differs from (x > 0) at zero, but the selected result is exactly zero.
use super::*;
use keel_ir::{VectorBinaryOp, VectorType};
use std::collections::HashMap;

pub(super) fn select(func: &Function, plan: &mut crate::select::Selection) -> HashMap<Inst, Value> {
    if func
        .layout_insts()
        .any(|(_, i)| matches!(func.inst_data(i), InstData::InlineAsm { .. }))
    {
        return HashMap::new();
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, i) in func.layout_insts() {
        func.inst_data(i).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut result = HashMap::new();
    for block in func.blocks() {
        for pair in func.block_insts(block).windows(2) {
            let compare = pair[0];
            let root = pair[1];
            if pair.iter().any(|&i| {
                func.inst_results(i).is_empty()
                    || plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.constraints().keys().any(|&(at, _, _)| at == i)
                    || func.resolve(func.first_result(i)) != func.first_result(i)
            }) || func.inst_loc(compare) != func.inst_loc(root)
            {
                continue;
            }
            let InstData::VectorIcmp {
                cond: IntCC::Sgt,
                args: [x, zero],
            } = *func.inst_data(compare)
            else {
                continue;
            };
            let x = func.resolve(x);
            let mask = func.first_result(compare);
            if func.value_type(x) != Type::Vector(VectorType::I64x2)
                || uses[mask.index()] != 1
                || vector::literal_i64x2(func, zero) != Some(0)
            {
                continue;
            }
            let accepted = match *func.inst_data(root) {
                InstData::VectorBinary {
                    op: VectorBinaryOp::And,
                    args,
                } => {
                    let args = args.map(|v| func.resolve(v));
                    args == [mask, x] || args == [x, mask]
                }
                InstData::VectorSelect { args: [m, a, b] } => {
                    func.resolve(m) == mask
                        && func.resolve(a) == x
                        && vector::literal_i64x2(func, b) == Some(0)
                }
                _ => false,
            };
            if !accepted {
                continue;
            }
            // Only pure compare is suppressed. Loads stay in place, with their
            // original 16-byte instruction, ordering, and debug information.
            plan.cover(compare);
            plan.set_operands(root, vec![x]);
            result.insert(root, x);
        }
    }
    result
}

#[cfg(test)]
#[path = "../../tests/x86_sign/matcher.rs"]
mod tests;
