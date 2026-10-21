//! Exact adjacent packed wrapping absolute value, shared by both targets.
use crate::select::Selection;
use keel_ir::{
    EntityRef, Function, Inst, InstData, IntCC, Type, UnaryOp, Value, ValueDef, VectorType,
};
use std::collections::HashMap;

pub(super) fn select(func: &Function, plan: &mut Selection) -> HashMap<Inst, Value> {
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
        for chain in func.block_insts(block).windows(3) {
            let [first, second, select] = [chain[0], chain[1], chain[2]];
            if chain.iter().any(|&i| {
                plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.constraints().keys().any(|&(at, _, _)| at == i)
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
                    InstData::VectorIcmp { .. },
                    InstData::VectorUnary {
                        op: UnaryOp::Ineg, ..
                    },
                ) => (first, second),
                (
                    InstData::VectorUnary {
                        op: UnaryOp::Ineg, ..
                    },
                    InstData::VectorIcmp { .. },
                ) => (second, first),
                _ => continue,
            };
            let InstData::VectorIcmp {
                cond: IntCC::Slt,
                args: [x, zero],
            } = *func.inst_data(compare)
            else {
                continue;
            };
            let InstData::VectorUnary { arg, .. } = *func.inst_data(negate) else {
                unreachable!();
            };
            let InstData::VectorSelect { args } = *func.inst_data(select) else {
                continue;
            };
            let x = func.resolve(x);
            if func.value_type(x) != Type::Vector(VectorType::I64x2)
                || !zero_splat(func, zero)
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

fn zero_splat(func: &Function, value: Value) -> bool {
    let value = func.resolve(value);
    let ValueDef::Result(inst, _) = func.value_def(value) else {
        return false;
    };
    let InstData::VectorSplat {
        ty: Type::Vector(VectorType::I64x2),
        arg,
    } = *func.inst_data(inst)
    else {
        return false;
    };
    crate::fold::iconst(func, arg) == Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::{Constraint, OperandKind, ValuePart};
    const SOURCE: &str = "func @abs(i64x2,i64x2) -> i64x2 {\nblock0(v0:i64x2,v6:i64x2):\n v1 = iconst.i64 0\n v2 = vsplat.i64x2 v1\n v3 = vicmp.slt v0,v2\n v4 = vineg v0\n v5 = vselect v3,v4,v0\n return v5\n}\n";
    #[test]
    fn exact_operands_and_rejection_boundaries() {
        let m = keel_ir::parse(SOURCE).unwrap();
        let f = &m.functions[0];
        let mut s = Selection::default();
        let matches = select(f, &mut s);
        assert_eq!(matches.len(), 1);
        let (&root, &x) = matches.iter().next().unwrap();
        assert_eq!(s.operands(f, root), vec![x]);
        for (_, i) in f.layout_insts() {
            if matches!(
                f.inst_data(i),
                InstData::VectorIcmp { .. } | InstData::VectorUnary { .. }
            ) {
                assert!(s.covered(i));
                assert!(s.operands(f, i).is_empty());
            }
        }
        for source in [
            SOURCE.replace("iconst.i64 0", "iconst.i64 1"),
            SOURCE.replace(
                "return v5",
                "v7 = asm \"cset %w0, eq\", \"=r\"() -> i32\n return v5",
            ),
            SOURCE.replace("vselect v3,v4,v0", "vselect v3,v0,v4"),
            SOURCE.replace("vselect v3,v4,v0", "vselect v6,v4,v0"),
            SOURCE.replace("vicmp.slt", "vicmp.ult"),
            SOURCE.replace("vicmp.slt v0,v2", "vicmp.slt v2,v0"),
            SOURCE.replace("vineg v0", "vineg v6"),
            SOURCE.replace("return v5", "v7 = viadd v3,v5\n return v7"),
            SOURCE.replace("return v5", "v7 = viadd v4,v5\n return v7"),
            SOURCE.replace("v4 = vineg v0", "v7 = iconst.i64 7\n v4 = vineg v0"),
            SOURCE.replace("vsplat.i64x2 v1", "bitcast.i64x2 v6"),
            SOURCE
                .replace("i64x2", "i32x4")
                .replace("iconst.i64", "iconst.i32"),
        ] {
            let m = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert!(
                select(&m.functions[0], &mut Selection::default()).is_empty(),
                "{source}"
            );
        }
    }
    #[test]
    fn constraints_and_debug_barriers_are_preserved() {
        let m = keel_ir::parse(SOURCE).unwrap();
        let f = &m.functions[0];
        let chain: Vec<_> = f
            .layout_insts()
            .map(|(_, i)| i)
            .filter(|&i| {
                matches!(
                    f.inst_data(i),
                    InstData::VectorIcmp { .. }
                        | InstData::VectorUnary { .. }
                        | InstData::VectorSelect { .. }
                )
            })
            .collect();
        for &at in &chain {
            let mut plan = Selection::default();
            plan.set_constraint(
                at,
                ValuePart::low(f.first_result(at)),
                OperandKind::Def,
                Constraint::Register,
            );
            assert!(select(f, &mut plan).is_empty());
            assert!(!chain.iter().any(|&i| plan.covered(i)));
            let mut debug = f.clone();
            debug.set_inst_loc(
                at,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: 2,
                    col: 1,
                    scope: None,
                }),
            );
            assert!(select(&debug, &mut Selection::default()).is_empty());
        }
    }
}
