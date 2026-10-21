use super::*;
use crate::regalloc::{Constraint, OperandKind, ValuePart};
use crate::select::{Machine, Match, Selection};
const SOURCE: &str = "func @min(i32x4,i32x4,i32x4) -> i32x4 {
block0(v0:i32x4,v1:i32x4,v4:i32x4):
 v2 = vicmp.slt v0,v1
 v3 = vselect v2,v0,v1
 return v3
}";
fn value(n: usize) -> Value {
    Value::new([0, 1, 3, 4, 2][n])
}
fn function(s: &str) -> Function {
    let m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m.functions.into_iter().next().unwrap()
}
fn reject(f: &Function) {
    let mut s = Selection::default();
    assert!(select(f, &mut s).is_empty());
    assert!(f.layout_insts().all(|(_, i)| !s.covered(i)));
}
#[test]
fn all_conditions_arm_orders_and_leaf_multiplicity() {
    for (cc, signed, less) in [
        ("slt", true, true),
        ("sle", true, true),
        ("sgt", true, false),
        ("sge", true, false),
        ("ult", false, true),
        ("ule", false, true),
        ("ugt", false, false),
        ("uge", false, false),
    ] {
        for reverse in [false, true] {
            for same in [false, true] {
                let src = SOURCE.replace("vicmp.slt", &format!("vicmp.{cc}"));
                let src = if reverse {
                    src.replace("vselect v2,v0,v1", "vselect v2,v1,v0")
                } else {
                    src
                };
                let mut f = function(&src);
                if same {
                    f.replace_all_uses(value(1), value(0));
                }
                let mut s = Selection::default();
                let found = select(&f, &mut s);
                assert_eq!(found.len(), 1);
                let (&root, m) = found.iter().next().unwrap();
                let min = less != reverse || same && less;
                let expected = match (signed, min) {
                    (true, true) => Kind::Smin,
                    (true, false) => Kind::Smax,
                    (false, true) => Kind::Umin,
                    (false, false) => Kind::Umax,
                };
                // With identical leaves either operation is equivalent; matcher
                // consistently prefers the original arm order.
                if !same || !reverse {
                    assert_eq!(m.kind, expected);
                }
                assert_eq!(
                    s.operands(&f, root),
                    vec![value(0), value(if same { 0 } else { 1 })]
                );
                let cmp = f.block_insts(f.entry_block().unwrap())[0];
                assert!(s.covered(cmp));
                assert!(s.operands(&f, cmp).is_empty());
            }
        }
    }
}
#[test]
fn boundaries_external_uses_and_arbitrary_masks() {
    for src in [
        SOURCE.replace("vicmp.slt", "vicmp.eq"),
        SOURCE.replace("vselect v2,v0,v1", "vselect v4,v0,v1"),
        SOURCE.replace("vselect v2,v0,v1", "vselect v2,v0,v4"),
        SOURCE.replace("vselect v2,v0,v1", "vselect v2,v4,v1"),
        SOURCE.replace(" v3 =", " v5 = viadd v0,v1\n v3 ="),
        SOURCE.replace(" return v3", " v5 = viadd v2,v3\n return v5"),
        SOURCE.replace(
            " return v3",
            " jump block1(v3,v2,v2)\nblock1(v5:i32x4,v6:i32x4,v7:i32x4):\n return v5",
        ),
        SOURCE.replace(" return v3", " v5 = asm \"\", \"=r\"() -> i32\n return v3"),
        SOURCE.replace("i32x4", "i64x2"),
        SOURCE.replace("i32x4", "i16x8"),
        SOURCE.replace("i32x4", "i8x16"),
    ] {
        reject(&function(&src));
    }
    let mut f = function(&SOURCE.replace(
        " return v3",
        " jump block1(v3,v4,v4)\nblock1(v5:i32x4,v6:i32x4,v7:i32x4):\n return v5",
    ));
    f.replace_all_uses(value(4), value(2));
    reject(&f);
}
#[test]
fn canonical_producers_operand_aliases_and_prior_claims() {
    let f = function(SOURCE);
    let pair = &f.block_insts(f.entry_block().unwrap())[..2];
    for &i in pair {
        let mut d = f.clone();
        d.replace_all_uses(f.first_result(i), value(4));
        reject(&d);
        for kind in [OperandKind::Use, OperandKind::Def] {
            let mut s = Selection::default();
            s.set_constraint(i, ValuePart::low(value(0)), kind, Constraint::Any);
            assert!(select(&f, &mut s).is_empty());
        }
        for claim in 0..9 {
            let mut s = Selection::default();
            match claim {
                0 => s.cover(i),
                1 => {
                    s.matches.insert(
                        i,
                        Match {
                            machine: Machine::Madd,
                            args: vec![],
                            shift: 0,
                        },
                    );
                }
                2 => {
                    s.conditions.insert(i, (pair[0], false));
                }
                3 => {
                    s.compare_memory.insert(i, value(0));
                }
                4 => {
                    s.binary_memory.insert(i, value(0));
                }
                5 => {
                    s.division_remainders.insert(i, pair[0]);
                }
                6 => {
                    s.select_compares.insert(i, pair[0]);
                }
                7 => s.set_operands(i, vec![]),
                _ => s.set_operands(i, vec![value(0); 4]),
            }
            assert!(select(&f, &mut s).is_empty(), "claim {claim}");
        }
    }
    // Every operand position independently accepts an alias to a live leaf or mask.
    for (old, new) in [
        ("vicmp.slt v0,v1", "vicmp.slt v4,v1"),
        ("vicmp.slt v0,v1", "vicmp.slt v0,v4"),
        ("vselect v2,v0,v1", "vselect v4,v0,v1"),
        ("vselect v2,v0,v1", "vselect v2,v4,v1"),
        ("vselect v2,v0,v1", "vselect v2,v0,v4"),
    ] {
        let mut d = function(&SOURCE.replace(old, new));
        let target = if new.contains("vicmp.slt v0,v4") || new.ends_with("v0,v4") {
            1
        } else if new.starts_with("vselect v4") {
            2
        } else {
            0
        };
        d.replace_all_uses(value(4), value(target));
        assert_eq!(select(&d, &mut Selection::default()).len(), 1, "{new}");
    }
}
#[test]
fn complete_source_locations() {
    let mut f = function(SOURCE);
    let pair = f.block_insts(f.entry_block().unwrap())[..2].to_vec();
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 1,
        col: 1,
        scope: None,
    };
    for &i in &pair {
        f.set_inst_loc(i, Some(loc));
    }
    assert_eq!(select(&f, &mut Selection::default()).len(), 1);
    for &i in &pair {
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
            let mut d = f.clone();
            d.set_inst_loc(i, changed);
            reject(&d);
        }
    }
}

const ZERO: &str = "func @zero(i32x4,i32x4,i32) -> i32x4 {
block0(v0:i32x4,v1:i32x4,v2:i32):
 v3 = iconst.i32 0
 v4 = vsplat.i32x4 v3
 v5 = vicmp.sgt v0,v4
 v6 = vand v5,v0
 return v6
}";
#[test]
fn strict_zero_filters_and_fresh_literal_proofs() {
    for cc in ["sgt", "slt"] {
        for commuted in [false, true] {
            let src = ZERO.replace("vicmp.sgt", &format!("vicmp.{cc}"));
            let src = if commuted {
                src.replace("vand v5,v0", "vand v0,v5")
            } else {
                src
            };
            let f = function(&src);
            let mut s = Selection::default();
            let found = select(&f, &mut s);
            assert_eq!(found.len(), 1);
            let (&root, mm) = found.iter().next().unwrap();
            assert_eq!(mm.kind, if cc == "sgt" { Kind::Smax } else { Kind::Smin });
            assert_eq!(s.operands(&f, root), vec![Value::new(0), Value::new(4)]);
            // Scalar, splat, compare, root: stale result owners must all reject.
            for (from, to) in [(3, 2), (4, 1), (5, 1), (6, 1)] {
                let mut d = f.clone();
                d.replace_all_uses(Value::new(from), Value::new(to));
                reject(&d);
            }
        }
    }
    for src in [
        ZERO.replace("iconst.i32 0", "iconst.i32 1"),
        ZERO.replace("iconst.i32 0", "iconst.i32 -1"),
        ZERO.replace("vsplat.i32x4 v3", "vsplat.i32x4 v2"),
        ZERO.replace("vicmp.sgt", "vicmp.sge"),
        ZERO.replace("vicmp.sgt", "vicmp.sle"),
        ZERO.replace("vicmp.sgt", "vicmp.ugt"),
        ZERO.replace("vicmp.sgt", "vicmp.ult"),
        ZERO.replace("vicmp.sgt v0,v4", "vicmp.slt v4,v0"),
        ZERO.replace("vand v5,v0", "vand v5,v1"),
        ZERO.replace("vand v5,v0", "vor v5,v0"),
        ZERO.replace(" v6 =", " fence seq_cst\n v6 ="),
        ZERO.replace(" return v6", " v7 = viadd v5,v6\n return v7"),
        ZERO.replace(
            " return v6",
            " jump block1(v6,v5,v5)\nblock1(v7:i32x4,v8:i32x4,v9:i32x4):\n return v7",
        ),
        ZERO.replace(" return v6", " v7 = asm \"\", \"=r\"() -> i32\n return v6"),
        ZERO.replace("i32x4", "i64x2")
            .replace("iconst.i32", "iconst.i64")
            .replace(",i32)", ",i64)")
            .replace("v2:i32", "v2:i64"),
        ZERO.replace(" v4 =", " v7 = iadd v3,v3\n v4 =")
            .replace("vsplat.i32x4 v3", "vsplat.i32x4 v7"),
    ] {
        reject(&function(&src));
    }
    // Resolved aliases may point at canonical literals or leaves.
    for (from, to) in [(1, 0), (1, 4)] {
        let src = if to == 0 {
            ZERO.replace("vicmp.sgt v0,v4", "vicmp.sgt v1,v4")
                .replace("vand v5,v0", "vand v5,v1")
        } else {
            ZERO.replace("vicmp.sgt v0,v4", "vicmp.sgt v0,v1")
        };
        let mut f = function(&src);
        f.replace_all_uses(Value::new(from), Value::new(to));
        assert_eq!(select(&f, &mut Selection::default()).len(), 1);
    }
    let mut f = function(&ZERO.replace(
        " return v6",
        " jump block1(v6,v1,v1)\nblock1(v7:i32x4,v8:i32x4,v9:i32x4):\n return v7",
    ));
    f.replace_all_uses(Value::new(1), Value::new(5));
    reject(&f);
}
