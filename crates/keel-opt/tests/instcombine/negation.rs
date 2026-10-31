use crate::support;
use keel_ir::{InstData, UnaryOp};

#[test]
fn complement_plus_one_is_wrapping_negation_in_both_local_tiers() {
    for pass in ["instcombine", "fast-local"] {
        for width in [8, 16, 32, 64, 128] {
            {
                let complement = "bnot v0";
                for add in ["v2, v3", "v3, v2"] {
                    let source = format!(
                        "func @f(i{width}) -> i{width} {{\nblock0(v0:i{width}):\n v1 = iconst.i{width} -1\n v2 = {complement}\n v3 = iconst.i{width} 1\n v4 = iadd {add}\n return v4\n}}"
                    );
                    let original = keel_ir::parse(&source).unwrap();
                    let mut after = original.clone();
                    keel_opt::run_passes(&mut after, &[pass, "dce"]).unwrap();
                    keel_ir::verify_module(&after).unwrap();
                    assert!(
                        after.functions[0].layout_insts().any(|(_, i)| matches!(
                            after.functions[0].inst_data(i),
                            InstData::Unary {
                                op: UnaryOp::Ineg,
                                ..
                            }
                        )),
                        "{after}"
                    );
                    if width <= 64 {
                        let mask = u64::MAX >> (64 - width);
                        for x in [0, 1, 0x81, mask >> 1, (mask >> 1) + 1, mask] {
                            let expected = x.wrapping_neg() & mask;
                            for f in [&original.functions[0], &after.functions[0]] {
                                assert_eq!(support::execute(f, &[x]), expected, "{source} x={x}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_complements_survive_and_other_addends_do_not_become_negation() {
    for pass in ["instcombine", "fast-local"] {
        for amount in [1, 2, -1] {
            let source = format!(
                "func @f(i64) -> i64 {{\nblock0(v0:i64):\n v1 = bnot v0\n v2 = iconst.i64 {amount}\n v3 = iadd v1,v2\n v4 = xor v3,v1\n return v4\n}}"
            );
            let before = keel_ir::parse(&source).unwrap();
            let mut after = before.clone();
            keel_opt::run_passes(&mut after, &[pass, "dce"]).unwrap();
            keel_ir::verify_module(&after).unwrap();
            let negations = after.functions[0]
                .layout_insts()
                .filter(|&(_, i)| {
                    matches!(
                        after.functions[0].inst_data(i),
                        InstData::Unary {
                            op: UnaryOp::Ineg,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(negations, usize::from(amount == 1), "{after}");
            assert!(after.to_string().contains("bnot"));
            for x in [0, 1, 1 << 63, u64::MAX] {
                assert_eq!(
                    support::execute(&before.functions[0], &[x]),
                    support::execute(&after.functions[0], &[x])
                );
            }
        }
    }
}

#[test]
fn explicit_xor_add_remains_binary_for_loop_vectorization() {
    for pass in ["instcombine", "fast-local"] {
        let mut module=keel_ir::parse("func @f(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 -1\n v2 = xor v0,v1\n v3 = iconst.i64 1\n v4 = iadd v2,v3\n return v4\n}").unwrap();
        keel_opt::run_passes(&mut module, &[pass, "dce"]).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(!module.functions[0].layout_insts().any(|(_, i)| matches!(
            module.functions[0].inst_data(i),
            InstData::Unary {
                op: UnaryOp::Ineg,
                ..
            }
        )));
        for value in [0u64, 1, 1 << 63, u64::MAX] {
            assert_eq!(
                support::execute(&module.functions[0], &[value]),
                value.wrapping_neg()
            );
        }
    }
}
