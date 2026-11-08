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
