//! Closed positive-i32 low-half DAG. Only the low two dwords are observed.
use super::*;
use keel_ir::{VectorBinaryOp, VectorType};
use std::collections::HashMap;

pub(super) fn select(func: &Function, plan: &mut crate::select::Selection) -> HashMap<Inst, Value> {
    // Inline assembly can carry target-specific register and memory contracts.
    // Every chain contains a vector splat; skip the use counts without one.
    let mut candidate = false;
    for (_, i) in func.layout_insts() {
        match func.inst_data(i) {
            InstData::InlineAsm { .. } => return HashMap::new(),
            InstData::VectorSplat { .. } => candidate = true,
            _ => {}
        }
    }
    if !candidate {
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
            if !matches!(func.inst_data(splat), InstData::VectorSplat { .. }) {
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
            let location = func.inst_loc(load);
            if chain.iter().any(|&i| func.inst_loc(i) != location) {
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
            // A retained definition may have had its result replaced by an
            // unrelated value. Resolving operands is valid; treating that old
            // producer as the replacement's definition is not.
            if chain.iter().any(|&i| {
                let value = func.first_result(i);
                func.resolve(value) != value
            }) {
                continue;
            }
            let values = [load, splat, cast, compare, and].map(|i| func.first_result(i));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::Selection;
    const SOURCE: &str = "func @positive(ptr,ptr,i32x4) {\nblock0(v0:ptr,v1:ptr,v20:i32x4):\n v2 = iconst.i32 0\n v3 = vsplat.i32x4 v2\n v4 = load.i64 v0\n v5 = vsplat.i64x2 v4\n v6 = bitcast.i32x4 v5\n v7 = vicmp.sgt v6,v3\n v8 = vand v7,v6\n v9 = uwiden_low.i64x2 v8\n store v9,v1\n return\n}\n";
    #[test]
    fn aliased_producer_results_are_not_definitions_of_their_replacements() {
        let source = SOURCE
            .replace("ptr,ptr,i32x4)", "ptr,ptr,i32x4,i64,i64x2)")
            .replace("v20:i32x4):", "v20:i32x4,v21:i64,v22:i64x2):");
        let original = keel_ir::parse(&source).unwrap().functions.remove(0);
        let chain: Vec<_> = original
            .layout_insts()
            .skip(2)
            .take(6)
            .map(|(_, i)| i)
            .collect();
        assert_eq!(select(&original, &mut Selection::default()).len(), 1);
        let params = original.block_params(original.entry_block().unwrap());
        for (&inst, param) in chain.iter().zip([
            params[3], params[4], params[2], params[2], params[2], params[4],
        ]) {
            let mut f = original.clone();
            let value = f.first_result(inst);
            f.replace_all_uses(value, param);
            assert!(
                select(&f, &mut Selection::default()).is_empty(),
                "aliased {inst:?}"
            );
        }
    }
    #[test]
    fn inline_assembly_is_a_boundary() {
        let source = SOURCE.replace(" return", " v30 = asm \"\", \"=r\"() -> i32\n return");
        let m = keel_ir::parse(&source).unwrap();
        assert!(select(&m.functions[0], &mut Selection::default()).is_empty());
    }
    #[test]
    fn closed_operands_alias_resolution_and_existing_covers() {
        let m = keel_ir::parse(SOURCE).unwrap();
        let f = &m.functions[0];
        let chain: Vec<_> = f.layout_insts().skip(2).take(6).map(|(_, i)| i).collect();
        let mut plan = Selection::default();
        let address = plan.operands(f, chain[0]);
        let matches = select(f, &mut plan);
        assert_eq!(matches.len(), 1);
        assert_eq!(plan.operands(f, chain[5]), address);
        for &i in &chain[..5] {
            assert!(plan.covered(i));
            assert!(plan.operands(f, i).is_empty());
        }
        assert!(!plan.covered(chain[5]));
        for &i in &chain {
            let mut plan = Selection::default();
            plan.cover(i);
            assert!(select(f, &mut plan).is_empty());
        }
        // Columns and lexical scopes are source boundaries too.
        let location = keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 2,
            col: 1,
            scope: None,
        };
        let mut debug = f.clone();
        for &i in &chain {
            debug.set_inst_loc(i, Some(location));
        }
        assert_eq!(select(&debug, &mut Selection::default()).len(), 1);
        for &i in &chain {
            for changed in [
                keel_ir::SourceLoc { col: 2, ..location },
                keel_ir::SourceLoc {
                    scope: Some(keel_ir::DebugScope::new(0)),
                    ..location
                },
            ] {
                let mut different = debug.clone();
                different.set_inst_loc(i, Some(changed));
                assert!(select(&different, &mut Selection::default()).is_empty());
            }
        }
        // An aliased compare operand resolves to X and counts as one occurrence.
        let mut f = keel_ir::parse(&SOURCE.replace("vicmp.sgt v6,v3", "vicmp.sgt v20,v3"))
            .unwrap()
            .functions
            .remove(0);
        let alias = f.block_params(f.entry_block().unwrap())[2];
        f.replace_all_uses(alias, f.first_result(chain[2]));
        assert_eq!(select(&f, &mut Selection::default()).len(), 1);
        // An external observation through that same alias closes the opportunity.
        let mut f = keel_ir::parse(&SOURCE.replace(" return", " store v20,v1\n return"))
            .unwrap()
            .functions
            .remove(0);
        let alias = f.block_params(f.entry_block().unwrap())[2];
        f.replace_all_uses(alias, f.first_result(chain[2]));
        assert!(select(&f, &mut Selection::default()).is_empty());
    }
}
