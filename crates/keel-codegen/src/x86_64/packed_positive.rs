//! Closed positive-i32 low-half DAG. Only the low two dwords are observed.
use super::*;
use keel_ir::{VectorBinaryOp, VectorType};
use std::collections::HashMap;

pub(super) fn select(func: &Function, plan: &mut crate::select::Selection) -> HashMap<Inst, Value> {
    // Inline assembly can carry target-specific register and memory contracts.
    if func
        .layout_insts()
        .any(|(_, i)| matches!(func.inst_data(i), InstData::InlineAsm { .. }))
    {
        return HashMap::new();
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, i) in func.layout_insts() {
        func.inst_data(i).for_each_operand(|v| {
            let n = &mut uses[func.resolve(v).index()];
            *n = n.saturating_add(1);
        });
    }
    let mut result = HashMap::new();
    for block in func.blocks() {
        for chain in func.block_insts(block).windows(6) {
            let [load, splat, cast, compare, and, widen] =
                [chain[0], chain[1], chain[2], chain[3], chain[4], chain[5]];
            if chain.iter().any(|&i| {
                plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.constraints().keys().any(|&(at, _, _)| at == i)
            }) {
                continue;
            }
            let location = func.inst_loc(load).map(|l| (l.file, l.line));
            if chain
                .iter()
                .any(|&i| func.inst_loc(i).map(|l| (l.file, l.line)) != location)
            {
                continue;
            }
            let InstData::Load {
                ty: Type::I64,
                addr,
                flags,
            } = *func.inst_data(load)
            else {
                continue;
            };
            let InstData::VectorSplat {
                ty: Type::Vector(VectorType::I64x2),
                arg: l,
            } = *func.inst_data(splat)
            else {
                continue;
            };
            let InstData::Convert {
                op: ConvertOp::Bitcast,
                ty: Type::Vector(VectorType::I32x4),
                arg: s,
            } = *func.inst_data(cast)
            else {
                continue;
            };
            let InstData::VectorIcmp {
                cond: IntCC::Sgt,
                args: [x, zero],
            } = *func.inst_data(compare)
            else {
                continue;
            };
            let InstData::VectorBinary {
                op: VectorBinaryOp::And,
                args,
            } = *func.inst_data(and)
            else {
                continue;
            };
            let InstData::Convert {
                op: ConvertOp::UwidenLow,
                ty: Type::Vector(VectorType::I64x2),
                arg: p,
            } = *func.inst_data(widen)
            else {
                continue;
            };
            let values =
                [load, splat, cast, compare, and].map(|i| func.resolve(func.first_result(i)));
            let [lv, sv, xv, mv, pv] = values;
            let args = args.map(|v| func.resolve(v));
            if flags.volatile
                || func.resolve(l) != lv
                || func.resolve(s) != sv
                || func.resolve(x) != xv
                || func.resolve(p) != pv
                || !(args == [mv, xv] || args == [xv, mv])
                || values
                    .iter()
                    .zip([1, 1, 2, 1, 1])
                    .any(|(&v, n)| uses[v.index()] != n)
                || !literal_zero(func, zero)
            {
                continue;
            }
            // Moving the exact eight-byte access across these adjacent pure
            // nodes preserves fault order. Upper MOVQ lanes are zero instead of
            // duplicated, but compare/AND are lane-wise and UWIDEN observes only
            // the low two dwords. Filter before widening; no i32 accumulation.
            // Retain selected address leaves, including indexed/offset forms.
            let operands = plan.operands(func, load);
            for &i in &chain[..5] {
                plan.cover(i);
            }
            plan.set_operands(widen, operands);
            result.insert(widen, func.resolve(addr));
        }
    }
    result
}

fn literal_zero(func: &Function, v: Value) -> bool {
    let ValueDef::Result(splat, _) = func.value_def(func.resolve(v)) else {
        return false;
    };
    let InstData::VectorSplat {
        ty: Type::Vector(VectorType::I32x4),
        arg,
    } = *func.inst_data(splat)
    else {
        return false;
    };
    let ValueDef::Result(constant, _) = func.value_def(func.resolve(arg)) else {
        return false;
    };
    matches!(
        func.inst_data(constant),
        InstData::Iconst {
            ty: Type::I32,
            imm: 0
        }
    )
}
