//! Keep a bounded, sole-use scalar abs expression on NZCV.
use super::*;

/// `uses` are [`crate::fold::use_counts`] of `func`.
pub(super) fn select(
    func: &Function,
    plan: &mut crate::select::Selection,
    uses: &[u8],
) -> HashMap<Inst, Value> {
    // Every cover ends at a scalar select; skip the use counts without one.
    let mut candidate = false;
    for (_, i) in func.layout_insts() {
        match func.inst_data(i) {
            InstData::InlineAsm { .. } => return HashMap::default(),
            InstData::Select { .. } => candidate = true,
            _ => {}
        }
    }
    if !candidate {
        return HashMap::default();
    }
    let mut result = HashMap::default();
    for block in func.blocks() {
        for chain in func.block_insts(block).windows(3) {
            let [first, second, select] = [chain[0], chain[1], chain[2]];
            if !matches!(func.inst_data(select), InstData::Select { .. }) {
                continue;
            }
            if chain.iter().any(|&i| {
                plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.has_constraint(i)
            }) {
                continue;
            }
            let location = func.inst_loc(first).map(|l| (l.file, l.line));
            if chain
                .iter()
                .any(|&i| func.inst_loc(i).map(|l| (l.file, l.line)) != location)
            {
                continue;
            }
            let (compare, negate) = match (func.inst_data(first), func.inst_data(second)) {
                (
                    InstData::Icmp { .. },
                    InstData::Unary {
                        op: UnaryOp::Ineg, ..
                    },
                ) => (first, second),
                (
                    InstData::Unary {
                        op: UnaryOp::Ineg, ..
                    },
                    InstData::Icmp { .. },
                ) => (second, first),
                _ => continue,
            };
            let InstData::Icmp {
                cond: IntCC::Slt,
                args: [x, zero],
            } = *func.inst_data(compare)
            else {
                continue;
            };
            let InstData::Unary { arg, .. } = *func.inst_data(negate) else {
                unreachable!();
            };
            let InstData::Select { args } = *func.inst_data(select) else {
                continue;
            };
            let x = func.resolve(x);
            if !matches!(func.value_type(x), Type::I32 | Type::I64)
                || iconst(func, zero) != Some(0)
                || func.resolve(arg) != x
                || args.map(|v| func.resolve(v))
                    != [func.first_result(compare), func.first_result(negate), x]
                || uses[func.first_result(compare).index()] != 1
                || uses[func.first_result(negate).index()] != 1
            {
                continue;
            }
            plan.cover(compare);
            plan.cover(negate);
            plan.set_operands(select, vec![x]);
            result.insert(select, x);
        }
    }
    result
}
