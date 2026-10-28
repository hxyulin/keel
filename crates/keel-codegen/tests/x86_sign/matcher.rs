use super::*;
use crate::regalloc::{Constraint, OperandKind, ValuePart};
use crate::select::{Machine, Match, Selection};
fn select(func: &Function, plan: &mut Selection) -> HashMap<Inst, Plan> {
    super::select(&crate::kinds::FunctionFacts::new(func), plan)
}
const SOURCE: &str = "func @clamp(ptr,ptr,i64,i64x2) {\nblock0(v0:ptr,v1:ptr,v20:i64,v21:i64x2):\n v2 = iconst.i64 0\n v3 = vsplat.i64x2 v2\n v4 = load.i64x2 align 1 v0\n v5 = vicmp.sgt v4,v3\n v6 = vand v5,v4\n store align 1 v6,v1\n return\n}\n";
fn function(src: &str) -> Function {
    keel_ir::parse(src).unwrap().functions.remove(0)
}
#[test]
fn canonical_producers_and_alias_operands() {
    let original = function(SOURCE);
    let insts: Vec<_> = original.layout_insts().map(|(_, i)| i).collect();
    assert_eq!(select(&original, &mut Selection::default()).len(), 1);
    for (inst, param) in [(insts[0], 2), (insts[1], 3), (insts[3], 3), (insts[4], 3)] {
        let mut f = original.clone();
        let p = f.block_params(f.entry_block().unwrap())[param];
        f.replace_all_uses(f.first_result(inst), p);
        assert!(select(&f, &mut Selection::default()).is_empty(), "{inst:?}");
    }
    // Alias operands resolve to the canonical live input, not stale producers.
    let mut f = function(&SOURCE.replace("vicmp.sgt v4,v3", "vicmp.sgt v21,v3"));
    let alias = f.block_params(f.entry_block().unwrap())[3];
    f.replace_all_uses(alias, f.first_result(insts[2]));
    assert_eq!(select(&f, &mut Selection::default()).len(), 1);
    let mut f = function(&SOURCE.replace(" return", " store v21,v1\n return"));
    let alias = f.block_params(f.entry_block().unwrap())[3];
    f.replace_all_uses(alias, f.first_result(insts[3]));
    assert!(select(&f, &mut Selection::default()).is_empty());
}
#[test]
fn closure_boundaries_and_retained_leaves() {
    let f = function(SOURCE);
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    let mut plan = Selection::default();
    assert_eq!(select(&f, &mut plan).len(), 1);
    assert!(plan.covered(insts[3]));
    assert!(plan.operands(&f, insts[3]).is_empty());
    assert_eq!(plan.operands(&f, insts[4]), vec![f.first_result(insts[2])]);
    assert!(!plan.covered(insts[2]));
    for i in [insts[3], insts[4]] {
        let mut plan = Selection::default();
        plan.cover(i);
        assert!(select(&f, &mut plan).is_empty());
        let mut plan = Selection::default();
        plan.matches.insert(
            i,
            Match {
                machine: Machine::Madd,
                args: vec![],
                shift: 0,
            },
        );
        assert!(select(&f, &mut plan).is_empty());
        let mut plan = Selection::default();
        plan.conditions.insert(i, (insts[3], false));
        assert!(select(&f, &mut plan).is_empty());
        for kind in [OperandKind::Use, OperandKind::Def] {
            let mut plan = Selection::default();
            plan.set_constraint(i, ValuePart::low(f.first_result(i)), kind, Constraint::Any);
            assert!(select(&f, &mut plan).is_empty());
        }
    }
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 1,
        col: 1,
        scope: None,
    };
    let mut debug = f.clone();
    for &i in &insts[3..5] {
        debug.set_inst_loc(i, Some(loc));
    }
    assert_eq!(select(&debug, &mut Selection::default()).len(), 1);
    for i in [insts[3], insts[4]] {
        for changed in [
            keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(1),
                ..loc
            },
            keel_ir::SourceLoc { line: 2, ..loc },
            keel_ir::SourceLoc { col: 2, ..loc },
            keel_ir::SourceLoc {
                scope: Some(keel_ir::DebugScope::new(0)),
                ..loc
            },
        ] {
            let mut d = debug.clone();
            d.set_inst_loc(i, Some(changed));
            assert!(select(&d, &mut Selection::default()).is_empty());
        }
    }
    for src in [
        SOURCE.replace(" return", " v30 = asm \"\", \"=r\"() -> i32\n return"),
        SOURCE.replace(" v6 =", " fence seq_cst\n v6 ="),
        SOURCE.replace(" return", " store v5,v1\n return"),
        SOURCE.replace(" return", " jump block1(v5)\nblock1(v30:i64x2):\n return"),
        SOURCE.replace("iconst.i64 0", "iconst.i64 1"),
        SOURCE
            .replace(" v3 =", " v22 = iadd v2,v2\n v3 =")
            .replace("vsplat.i64x2 v2", "vsplat.i64x2 v22"),
        SOURCE.replace("vand v5,v4", "vand v5,v21"),
        SOURCE.replace("vand v5,v4", "vselect v5,v4,v21"),
        SOURCE.replace("vicmp.sgt", "vicmp.sge"),
        SOURCE.replace("vicmp.sgt v4,v3", "vicmp.sgt v3,v4"),
        SOURCE
            .replace("i64x2", "i32x4")
            .replace("iconst.i64 0", "iconst.i32 0"),
    ] {
        assert!(
            select(&function(&src), &mut Selection::default()).is_empty(),
            "{src}"
        );
    }
}

#[test]
fn independent_false_literal_requires_its_own_fresh_proof() {
    let source = SOURCE
        .replace(
            " v4 =",
            " v22 = iconst.i64 0\n v23 = vsplat.i64x2 v22\n v4 =",
        )
        .replace("vand v5,v4", "vselect v5,v4,v23");
    let original = function(&source);
    assert_eq!(select(&original, &mut Selection::default()).len(), 1);
    for (index, param) in [(2, 2), (3, 3)] {
        let mut f = original.clone();
        let inst = f.layout_insts().nth(index).unwrap().1;
        let replacement = f.block_params(f.entry_block().unwrap())[param];
        f.replace_all_uses(f.first_result(inst), replacement);
        assert!(select(&f, &mut Selection::default()).is_empty());
    }
    // Aliased operands may still lead to fresh canonical literals.
    let mut f = function(&SOURCE.replace("vicmp.sgt v4,v3", "vicmp.sgt v4,v21"));
    let alias = f.block_params(f.entry_block().unwrap())[3];
    let splat = f.layout_insts().nth(1).unwrap().1;
    f.replace_all_uses(alias, f.first_result(splat));
    assert_eq!(select(&f, &mut Selection::default()).len(), 1);
}

#[test]
fn closed_sign_select_proofs_and_leaf_occurrences() {
    for (cc, literal) in [("slt", "0"), ("ugt", "9223372036854775807")] {
        let src = SOURCE
            .replace("vicmp.sgt", &format!("vicmp.{cc}"))
            .replace("iconst.i64 0", &format!("iconst.i64 {literal}"))
            .replace("vand v5,v4", "vselect v5,v4,v21");
        let f = function(&src);
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        let x = f.first_result(insts[2]);
        let otherwise = f.block_params(f.entry_block().unwrap())[3];
        let mut plan = Selection::default();
        assert!(matches!(
            select(&f, &mut plan)[&insts[4]],
            Plan::Select { .. }
        ));
        assert_eq!(plan.operands(&f, insts[4]), vec![x, otherwise]);
        assert!(plan.operands(&f, insts[3]).is_empty());
        assert!(!plan.covered(insts[2]));
        let equal = function(&src.replace("vselect v5,v4,v21", "vselect v5,v4,v4"));
        let mut plan = Selection::default();
        assert_eq!(select(&equal, &mut plan).len(), 1);
        assert_eq!(plan.operands(&equal, insts[4]), vec![x, x]);
        // Every inspected producer, including root, must remain canonical.
        for (index, param) in [(0, 2), (1, 3), (3, 3), (4, 3)] {
            let mut d = f.clone();
            let p = d.block_params(d.entry_block().unwrap())[param];
            d.replace_all_uses(d.first_result(insts[index]), p);
            assert!(select(&d, &mut Selection::default()).is_empty());
        }
        // Resolve ordinary aliases for either leaf and threshold.
        for (from, to) in [(otherwise, x), (otherwise, f.first_result(insts[1]))] {
            let mut d = f.clone();
            d.replace_all_uses(from, to);
            let mut plan = Selection::default();
            assert_eq!(select(&d, &mut plan).len(), 1);
            assert_eq!(plan.operands(&d, insts[4]), vec![x, to]);
        }
        for i in [insts[3], insts[4]] {
            let mut plan = Selection::default();
            plan.cover(i);
            assert!(select(&f, &mut plan).is_empty());
            let mut plan = Selection::default();
            plan.matches.insert(
                i,
                Match {
                    machine: Machine::Madd,
                    args: vec![],
                    shift: 0,
                },
            );
            assert!(select(&f, &mut plan).is_empty());
            let mut plan = Selection::default();
            plan.conditions.insert(i, (insts[3], false));
            assert!(select(&f, &mut plan).is_empty());
            for kind in [OperandKind::Use, OperandKind::Def] {
                let mut plan = Selection::default();
                plan.set_constraint(i, ValuePart::low(f.first_result(i)), kind, Constraint::Any);
                assert!(select(&f, &mut plan).is_empty());
            }
        }
        let loc = keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 1,
            col: 1,
            scope: None,
        };
        let mut debug = f.clone();
        for &i in &insts[3..5] {
            debug.set_inst_loc(i, Some(loc));
        }
        assert_eq!(select(&debug, &mut Selection::default()).len(), 1);
        for i in [insts[3], insts[4]] {
            for changed in [
                None,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(1),
                    ..loc
                }),
                Some(keel_ir::SourceLoc { line: 2, ..loc }),
                Some(keel_ir::SourceLoc { col: 2, ..loc }),
                Some(keel_ir::SourceLoc {
                    scope: Some(keel_ir::DebugScope::new(0)),
                    ..loc
                }),
            ] {
                let mut d = debug.clone();
                d.set_inst_loc(i, changed);
                assert!(select(&d, &mut Selection::default()).is_empty());
            }
        }
        for rejected in [
            src.replace(" return", " v30 = asm \"\", \"=r\"() -> i32\n return"),
            src.replace(" v6 =", " fence seq_cst\n v6 ="),
            src.replace(" return", " store v5,v1\n return"),
            src.replace(" return", " jump block1(v5)\nblock1(v30:i64x2):\n return"),
            src.replace(&format!("iconst.i64 {literal}"), "iconst.i64 1"),
            src.replace("vsplat.i64x2 v2", "vsplat.i64x2 v20"),
            src.replace("vselect v5,v4,v21", "vselect v5,v21,v4"),
            src.replace("vselect v5,v4,v21", "vselect v5,v4,v5"),
            src.replace(&format!("vicmp.{cc}"), "vicmp.sge"),
            src.replace(&format!("vicmp.{cc} v4,v3"), &format!("vicmp.{cc} v3,v4")),
            src.replace("i64x2", "i32x4")
                .replace("iconst.i64", "iconst.i32"),
        ] {
            assert!(
                select(&function(&rejected), &mut Selection::default()).is_empty(),
                "{rejected}"
            );
        }
        for (predicate, k) in [
            ("sge", 0),
            ("uge", i64::MIN),
            ("ult", i64::MIN),
            ("ule", i64::MAX),
        ] {
            let unlisted = src
                .replace(&format!("vicmp.{cc}"), &format!("vicmp.{predicate}"))
                .replace(&format!("iconst.i64 {literal}"), &format!("iconst.i64 {k}"));
            assert!(select(&function(&unlisted), &mut Selection::default()).is_empty());
        }
        let swapped = if cc == "slt" { "sgt" } else { "ult" };
        let unlisted = src.replace(
            &format!("vicmp.{cc} v4,v3"),
            &format!("vicmp.{swapped} v3,v4"),
        );
        assert!(select(&function(&unlisted), &mut Selection::default()).is_empty());
        let mut d = function(
            &src.replace(&format!("vicmp.{cc} v4,v3"), &format!("vicmp.{cc} v21,v3"))
                .replace("vselect v5,v4,v21", "vselect v5,v21,v4"),
        );
        let alias = d.block_params(d.entry_block().unwrap())[3];
        d.replace_all_uses(alias, d.first_result(insts[2]));
        assert_eq!(select(&d, &mut Selection::default()).len(), 1);
        let mut d =
            function(&src.replace(&format!("vicmp.{cc} v4,v3"), &format!("vicmp.{cc} v4,v21")));
        let alias = d.block_params(d.entry_block().unwrap())[3];
        d.replace_all_uses(alias, d.first_result(insts[1]));
        assert_eq!(select(&d, &mut Selection::default()).len(), 1);
        // An extra mask occurrence hidden behind an SSA alias still escapes.
        let mut d = function(&src.replace(" return", " store v21,v1\n return"));
        let alias = d.block_params(d.entry_block().unwrap())[3];
        d.replace_all_uses(alias, d.first_result(insts[3]));
        assert!(select(&d, &mut Selection::default()).is_empty());
    }
}
