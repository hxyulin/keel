mod support;
use keel_ir::{InstData, parse, verify_module};
#[test]
fn complementary_signed_and_unsigned_tests_fold_only_on_dominating_edges() {
    for (first, second) in [("uge", "ult"), ("sge", "slt"), ("eq", "ne"), ("ugt", "ule")] {
        let src = format!(
            "func @f(i64,i64) -> i64 {{
block0(v0:i64,v1:i64):
 v2 = icmp.{first} v0,v1
 brif v2,block1,block2
block1:
 v3 = iconst.i64 0
 return v3
block2:
 v4 = icmp.{second} v0,v1
 brif v4,block3,block4
block3:
 v5 = iconst.i64 1
 return v5
block4:
 v6 = iconst.i64 99
 return v6
}}"
        );
        let original = parse(&src).unwrap();
        let mut m = original.clone();
        keel_opt::run_passes(&mut m, &["branch-facts"]).unwrap();
        verify_module(&m).unwrap();
        assert_eq!(
            m.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Brif { .. }))
                .count(),
            1
        );
        for a in [0, 1, i64::MIN as u64, i64::MAX as u64, u64::MAX] {
            for b in [0, 1, i64::MIN as u64, i64::MAX as u64, u64::MAX] {
                assert_eq!(
                    support::execute(&m.functions[0], &[a, b]),
                    support::execute(&original.functions[0], &[a, b])
                );
            }
        }
    }
}
#[test]
fn duplicate_edges_cannot_establish_comparison_truth() {
    let src = "func @f(i64,i64) -> i64 {
block0(v0:i64,v1:i64):
 v2=icmp.uge v0,v1
 brif v2,block1,block1
block1:
 v3=icmp.ult v0,v1
 brif v3,block2,block3
block2:
 v4=iconst.i64 1
 return v4
block3:
 v5=iconst.i64 0
 return v5
}";
    let mut m = parse(src).unwrap();
    let original = m.to_string();
    keel_opt::run_passes(&mut m, &["branch-facts"]).unwrap();
    assert_eq!(original, m.to_string());
}

fn sum_bounds(ty: &str, guard: &str) -> String {
    format!(
        "func @f({ty},{ty})->i64 {{
block0(v0:{ty},v1:{ty}):
 v2=iconst.{ty} 8
 v3=icmp.{guard} v0,v2
 brif v3,block1,block4
block1:
 v4=icmp.{guard} v1,v2
 brif v4,block2,block4
block2:
 v5=iadd v0,v1
 v6=iconst.{ty} 16
 v7=icmp.ult v5,v6
 brif v7,block3,block5
block3:
 v8=iconst.i64 1
 return v8
block4:
 v9=iconst.i64 0
 return v9
block5:
 v10=iconst.i64 99
 return v10
}}"
    )
}
fn branches(f: &keel_ir::Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Brif { .. }))
        .count()
}

#[test]
fn bounded_addition_proves_a_later_check_for_every_integer_width() {
    for ty in ["i8", "i16", "i32", "i64"] {
        let original = parse(&sum_bounds(ty, "ult")).unwrap();
        let mut optimized = original.clone();
        keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
        verify_module(&optimized).unwrap();
        assert_eq!(branches(&optimized.functions[0]), 2, "{ty}");
        let values = [
            0,
            1,
            7,
            8,
            15,
            16,
            127,
            128,
            255,
            256,
            i64::MAX as u64,
            i64::MIN as u64,
            u64::MAX,
        ];
        for a in values {
            for b in values {
                assert_eq!(
                    support::execute(&original.functions[0], &[a, b]),
                    support::execute(&optimized.functions[0], &[a, b]),
                    "{ty} {a} {b}"
                );
            }
        }
    }
}

#[test]
fn signed_guards_do_not_invent_nonnegative_or_nonwrapping_unsigned_sums() {
    let original = parse(&sum_bounds("i8", "slt")).unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
    verify_module(&optimized).unwrap();
    assert_eq!(branches(&optimized.functions[0]), 3);
    // Exhaust the narrow domain, including both kinds of signed wrap and all
    // negative bit patterns admitted by the signed guards.
    for a in 0..256 {
        for b in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[a, b]),
                support::execute(&optimized.functions[0], &[a, b]),
                "{a} {b}"
            );
        }
    }
}

#[test]
fn range_facts_are_scoped_and_keep_the_false_edge_bound() {
    let original = parse(
        "func @f(i8)->i64 {
block0(v0:i8):
 v1=iconst.i8 8
 v2=icmp.ult v0,v1
 brif v2,block1,block2
block1:
 v3=iconst.i8 16
 v4=icmp.ult v0,v3
 brif v4,block3,block4
block2:
 v5=iconst.i8 16
 v6=icmp.ult v0,v5
 brif v6,block3,block4
block3:
 v7=iconst.i64 1
 return v7
block4:
 v8=iconst.i64 2
 return v8
}",
    )
    .unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
    verify_module(&optimized).unwrap();
    assert_eq!(branches(&optimized.functions[0]), 2);
    for a in 0..256 {
        assert_eq!(
            support::execute(&original.functions[0], &[a]),
            support::execute(&optimized.functions[0], &[a]),
            "{a}"
        );
    }
}

#[test]
fn signed_negative_ranges_and_unsigned_high_ranges_remain_distinct() {
    for (guard, limit, check, check_limit) in [
        ("slt", -4, "slt", 0),
        ("uge", -4, "slt", 0),
        ("sge", 0, "ule", 127),
    ] {
        let source = format!(
            "func @f(i8)->i64 {{
block0(v0:i8):
 v1=iconst.i8 {limit}
 v2=icmp.{guard} v0,v1
 brif v2,block1,block4
block1:
 v3=iconst.i8 {check_limit}
 v4=icmp.{check} v0,v3
 brif v4,block2,block3
block2:
 v5=iconst.i64 1
 return v5
block3:
 v6=iconst.i64 99
 return v6
block4:
 v7=iconst.i64 0
 return v7
}}"
        );
        let original = parse(&source).unwrap();
        let mut optimized = original.clone();
        keel_opt::run_passes(&mut optimized, &["range-facts"]).unwrap();
        verify_module(&optimized).unwrap();
        assert_eq!(branches(&optimized.functions[0]), 1, "{source}");
        for a in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[a]),
                support::execute(&optimized.functions[0], &[a]),
                "{guard} {a}"
            );
        }
    }
}
