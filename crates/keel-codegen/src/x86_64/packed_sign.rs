//! Closed i64x2 clamps and sign selects. Covered masks are never published.
use super::*;
use keel_ir::fxhash::FxHashMap as HashMap;
use keel_ir::{VectorBinaryOp, VectorType};

#[derive(Clone, Copy, Debug)]
pub(super) enum Plan {
    Positive(Value),
    Select { x: Value, otherwise: Value },
}

pub(super) fn select(
    facts: &crate::kinds::FunctionFacts<'_>,
    plan: &mut crate::select::Selection,
) -> HashMap<Inst, Plan> {
    let func = facts.func;
    // Every cover starts at a vector compare; skip the use counts without one.
    if facts.kinds.inline_asm || !facts.kinds.vector_icmp {
        return HashMap::default();
    }
    let uses = facts.raw_uses();
    let mut result = HashMap::default();
    for block in func.blocks() {
        for pair in func.block_insts(block).windows(2) {
            let compare = pair[0];
            let root = pair[1];
            if !matches!(func.inst_data(compare), InstData::VectorIcmp { .. }) {
                continue;
            }
            if pair.iter().any(|&i| {
                func.inst_results(i).is_empty()
                    || plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.has_constraint(i)
                    || func.resolve(func.first_result(i)) != func.first_result(i)
            }) || func.inst_loc(compare) != func.inst_loc(root)
            {
                continue;
            }
            let InstData::VectorIcmp {
                cond,
                args: [x, zero],
            } = *func.inst_data(compare)
            else {
                continue;
            };
            let x = func.resolve(x);
            let mask = func.first_result(compare);
            if func.value_type(x) != Type::Vector(VectorType::I64x2)
                || uses[mask.index()] != 1
                || func.value_type(mask) != Type::Vector(VectorType::I64x2)
                || func.value_type(func.first_result(root)) != Type::Vector(VectorType::I64x2)
            {
                continue;
            }
            let literal = vector::literal_i64x2(func, zero);
            // Keep the strict-positive clamp first. Its predicate differs from
            // ~sign at zero, so it must never share the sign-select proof.
            let positive = cond == IntCC::Sgt && literal == Some(0);
            let negative = matches!(
                (cond, literal),
                (IntCC::Slt, Some(0)) | (IntCC::Ugt, Some(i64::MAX))
            );
            let selected = match *func.inst_data(root) {
                InstData::VectorBinary {
                    op: VectorBinaryOp::And,
                    args,
                } => {
                    let args = args.map(|v| func.resolve(v));
                    (positive && (args == [mask, x] || args == [x, mask]))
                        .then_some(Plan::Positive(x))
                }
                InstData::VectorSelect { args: [m, a, b] } => {
                    let b = func.resolve(b);
                    if func.resolve(m) != mask || func.resolve(a) != x {
                        None
                    } else if positive && vector::literal_i64x2(func, b) == Some(0) {
                        Some(Plan::Positive(x))
                    } else if negative && func.value_type(b) == Type::Vector(VectorType::I64x2) {
                        Some(Plan::Select { x, otherwise: b })
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let Some(selected) = selected else { continue };
            // Only pure compare is suppressed. Loads stay in place, with their
            // original 16-byte instruction, ordering, and debug information.
            plan.cover(compare);
            plan.set_operands(
                root,
                match selected {
                    Plan::Positive(x) => vec![x],
                    // Occurrences, including X == F, match the actual leaf reads.
                    Plan::Select { x, otherwise } => vec![x, otherwise],
                },
            );
            result.insert(root, selected);
        }
    }
    result
}

#[cfg(test)]
#[path = "../../tests/x86_sign/matcher.rs"]
mod tests;
