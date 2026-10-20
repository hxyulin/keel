mod support;

use keel_ir::{EntityRef, InstData, Value};

fn combine(source: &str, pass: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &[pass]).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    (original, optimized)
}

#[test]
fn zero_extension_masks_cover_all_widening_pairs_and_literal_upper_bits() {
    for pass in ["instcombine", "fast-local"] {
        for from in [8u32, 16, 32, 64] {
            for to in [16u32, 32, 64, 128].into_iter().filter(|&to| to > from) {
                // i128 literals sign-extend i64; -1 covers a widened i64 source,
                // while i64::MAX does not cover its top source bit.
                let low = if from == 64 {
                    -1
                } else {
                    ((1u64 << from) - 1) as i64
                };
                let missing = if from == 64 { i64::MAX } else { low & !1 };
                for literal in [low, -1, missing, 0, i64::MIN, i64::MAX] {
                    for reverse in [false, true] {
                        let operands = if reverse { "v2, v1" } else { "v1, v2" };
                        let source = format!(
                            "func @f(i{from}) -> i{to} {{\nblock0(v0: i{from}):\n v1 = uext.i{to} v0\n v2 = iconst.i{to} {literal}\n v3 = and {operands}\n return v3\n}}"
                        );
                        let (original, optimized) = combine(&source, pass);
                        let source_mask = u128::MAX >> (128 - from);
                        let literal_bits = (literal as i128 as u128) & (u128::MAX >> (128 - to));
                        let expected = literal_bits & source_mask == source_mask;
                        let f = &optimized.functions[0];
                        assert_eq!(
                            f.resolve(Value::new(3)) == f.resolve(Value::new(1)),
                            expected,
                            "{source}\n{optimized}"
                        );
                        if to <= 64 {
                            for input in [0, 1, 127, 128, 255, 65535, 1u64 << 63, u64::MAX] {
                                assert_eq!(
                                    support::execute(&original.functions[0], &[input]),
                                    support::execute(f, &[input]),
                                    "{source} {input}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn zero_extension_mask_semantics_exhaust_all_i8_inputs_and_low_masks() {
    for pass in ["instcombine", "fast-local"] {
        for mask in 0..256u64 {
            for upper in [0, 0xff00] {
                let literal = mask | upper;
                let source = format!(
                    "func @f(i8) -> i16 {{\nblock0(v0: i8):\n v1 = uext.i16 v0\n v2 = iconst.i16 {literal}\n v3 = and v1, v2\n return v3\n}}"
                );
                let (original, optimized) = combine(&source, pass);
                for x in 0..256 {
                    let oracle = x & mask;
                    assert_eq!(support::execute(&original.functions[0], &[x]), oracle);
                    assert_eq!(support::execute(&optimized.functions[0], &[x]), oracle);
                }
            }
        }
    }
}

#[test]
fn zero_extension_mask_handles_raw_aliases_and_shared_producers() {
    let mut module = keel_ir::parse("func @f(i8, i8) -> i16 {\nblock0(v0: i8, v1: i8):\n v2 = uext.i16 v1\n v3 = iconst.i16 255\n v4 = uext.i16 v1\n v5 = iconst.i16 0\n v6 = and v5, v4\n v7 = xor v6, v2\n return v7\n}").unwrap();
    let f = &mut module.functions[0];
    for (old, new) in [(1, 0), (4, 2), (5, 3)] {
        f.replace_all_uses(Value::new(old), Value::new(new));
    }
    keel_ir::verify_module(&module).unwrap();
    assert!(
        keel_opt::find_pass("instcombine")
            .unwrap()
            .run(&mut module.functions[0])
    );
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    assert_eq!(f.resolve(Value::new(6)), f.resolve(Value::new(2)));
    let def = f.value_def(Value::new(2));
    assert!(matches!(def, keel_ir::ValueDef::Result(_, 0)));
}

#[test]
fn sign_extension_and_unknown_masks_do_not_use_zero_extension_proof() {
    for pass in ["instcombine", "fast-local"] {
        for op in ["sext", "uext"] {
            let source = format!(
                "func @f(i8, i16) -> i16 {{\nblock0(v0: i8, v1: i16):\n v2 = {op}.i16 v0\n v3 = iconst.i16 255\n v4 = and v2, v3\n v5 = and v4, v1\n return v5\n}}"
            );
            let (original, optimized) = combine(&source, pass);
            if op == "sext" {
                assert!(matches!(
                    optimized.functions[0].value_def(Value::new(4)),
                    keel_ir::ValueDef::Result(_, 0)
                ));
            }
            for x in 0..256 {
                for m in [0, 1, 127, 255, 256, 65535] {
                    assert_eq!(
                        support::execute(&original.functions[0], &[x, m]),
                        support::execute(&optimized.functions[0], &[x, m])
                    );
                }
            }
            assert!(optimized.to_string().contains("and"));
        }
    }
}

#[test]
fn zero_extension_mask_accepts_checked_result_and_flag_without_confusing_them() {
    for pass in ["instcombine", "fast-local"] {
        for value in ["v2", "v3"] {
            let source = format!(
                "func @f(i8, i8) -> i16 {{\nblock0(v0: i8, v1: i8):\n v2, v3 = sadd_overflow v0, v1\n v4 = uext.i16 {value}\n v5 = iconst.i16 255\n v6 = and v4, v5\n return v6\n}}"
            );
            let (original, optimized) = combine(&source, pass);
            assert_eq!(optimized.functions[0].resolve(Value::new(6)), Value::new(4));
            assert!(optimized.functions[0].layout_insts().any(|(_, i)| matches!(
                optimized.functions[0].inst_data(i),
                InstData::Overflow { .. }
            )));
            for a in 0..256 {
                for b in 0..256 {
                    assert_eq!(
                        support::execute(&original.functions[0], &[a, b]),
                        support::execute(&optimized.functions[0], &[a, b])
                    );
                }
            }
        }
    }
}

#[test]
fn masked_sign_extension_checks_source_sign_bit_for_every_widening_pair() {
    for pass in ["instcombine", "fast-local"] {
        for from in [8u32, 16, 32, 64] {
            for to in [16u32, 32, 64, 128].into_iter().filter(|&to| to > from) {
                let sign_clear = if from == 64 {
                    i64::MAX
                } else {
                    ((1u64 << (from - 1)) - 1) as i64
                };
                let sign_only = (1u64 << (from - 1)) as i64;
                for literal in [1, 127, sign_clear, sign_only, -1, i64::MIN] {
                    for reverse in [false, true] {
                        let operands = if reverse { "v1, v0" } else { "v0, v1" };
                        let source = format!(
                            "func @f(i{from}) -> i{to} {{\nblock0(v0: i{from}):\n v1 = iconst.i{from} {literal}\n v2 = and {operands}\n v3 = sext.i{to} v2\n return v3\n}}"
                        );
                        let (original, optimized) = combine(&source, pass);
                        let f = &optimized.functions[0];
                        // A zero typed mask may fold the whole expression instead.
                        let typed = (literal as i128 as u128) & (u128::MAX >> (128 - from));
                        if typed != 0 && typed != (u128::MAX >> (128 - from)) {
                            let expected = typed & (1u128 << (from - 1)) == 0;
                            let convert = f
                                .layout_insts()
                                .find_map(|(_, i)| match f.inst_data(i) {
                                    InstData::Convert { op, .. } => Some(*op),
                                    _ => None,
                                })
                                .unwrap();
                            assert_eq!(
                                convert == keel_ir::ConvertOp::Uext,
                                expected,
                                "{source}\n{optimized}"
                            );
                        }
                        if to <= 64 {
                            for x in [0, 1, 127, 128, 255, 65535, 1u64 << 63, u64::MAX] {
                                assert_eq!(
                                    support::execute(&original.functions[0], &[x]),
                                    support::execute(f, &[x]),
                                    "{source} {x}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn masked_sign_extension_exhausts_i8_inputs_and_masks_against_bitvector_oracle() {
    for pass in ["instcombine", "fast-local"] {
        for mask in 0..256u64 {
            let source = format!(
                "func @f(i8) -> i16 {{\nblock0(v0: i8):\n v1 = iconst.i8 {mask}\n v2 = and v0, v1\n v3 = sext.i16 v2\n return v3\n}}"
            );
            let (original, optimized) = combine(&source, pass);
            for x in 0..256 {
                let low = x & mask;
                let oracle = if low & 128 != 0 { low | 0xff00 } else { low };
                assert_eq!(support::execute(&original.functions[0], &[x]), oracle);
                assert_eq!(support::execute(&optimized.functions[0], &[x]), oracle);
            }
        }
    }
}

#[test]
fn masked_sign_extension_resolves_raw_aliases_without_rewriting_shared_and() {
    let mut module = keel_ir::parse("func @f(i8, i8) -> i16 {\nblock0(v0: i8, v1: i8):\n v2 = iconst.i8 127\n v3 = iconst.i8 -1\n v4 = and v3, v1\n v5 = and v0, v2\n v6 = sext.i16 v4\n v7 = uext.i16 v5\n v8 = xor v6, v7\n return v8\n}").unwrap();
    let f = &mut module.functions[0];
    for (old, new) in [(1, 0), (3, 2), (4, 5)] {
        f.replace_all_uses(Value::new(old), Value::new(new));
    }
    keel_ir::verify_module(&module).unwrap();
    keel_opt::find_pass("instcombine")
        .unwrap()
        .run(&mut module.functions[0]);
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    let keel_ir::ValueDef::Result(convert, 0) = f.value_def(Value::new(6)) else {
        panic!("conversion disappeared")
    };
    assert!(
        matches!(f.inst_data(convert),InstData::Convert { op: keel_ir::ConvertOp::Uext, arg, .. } if f.resolve(*arg)==f.resolve(Value::new(5)))
    );
    let keel_ir::ValueDef::Result(and, 0) = f.value_def(Value::new(5)) else {
        panic!("shared AND disappeared")
    };
    assert!(matches!(
        f.inst_data(and),
        InstData::Binary {
            op: keel_ir::BinaryOp::And,
            ..
        }
    ));
}

#[test]
fn unknown_masks_and_truncation_do_not_get_sign_clear_proof() {
    for pass in ["instcombine", "fast-local"] {
        for conversion in ["sext.i64", "uext.i64", "trunc.i16"] {
            let to = if conversion.starts_with("trunc") {
                "i16"
            } else {
                "i64"
            };
            let source = format!(
                "func @f(i32, i32) -> {to} {{\nblock0(v0: i32, v1: i32):\n v2 = and v0, v1\n v3 = {conversion} v2\n return v3\n}}"
            );
            let (_, optimized) = combine(&source, pass);
            assert!(optimized.to_string().contains(conversion), "{optimized}");
        }
    }
}

#[test]
fn redundant_mask_preserves_stale_result_alias_and_raw_return() {
    for pass in ["instcombine", "fast-local"] {
        let source = "func @f(i8, i16) -> i16 {\nblock0(v0: i8, v1: i16):\n v2 = uext.i16 v0\n v3 = iconst.i16 255\n v4 = and v2, v3\n return v4\n}";
        let mut module = keel_ir::parse(source).unwrap();
        let stale = Value::new(4);
        let replacement = Value::new(1);
        module.functions[0].replace_all_uses(stale, replacement);
        keel_ir::verify_module(&module).unwrap();
        let before = module.clone();
        assert_eq!(support::execute(&before.functions[0], &[7, 42]), 42);
        let raw_return = module.functions[0]
            .layout_insts()
            .last()
            .map(|(_, i)| module.functions[0].inst_data(i).clone())
            .unwrap();
        keel_opt::find_pass(pass)
            .unwrap()
            .run(&mut module.functions[0]);
        keel_ir::verify_module(&module).unwrap();
        let f = &module.functions[0];
        assert_eq!(support::execute(f, &[7, 42]), 42);
        assert_eq!(f.resolve(stale), replacement);
        assert_eq!(f.value_def(stale), keel_ir::ValueDef::Alias(replacement));
        assert_eq!(f.inst_data(f.layout_insts().last().unwrap().1), &raw_return);
    }
}

fn result_matching(f: &keel_ir::Function, predicate: impl Fn(&InstData) -> bool) -> Value {
    f.first_result(
        f.layout_insts()
            .find(|&(_, i)| predicate(f.inst_data(i)))
            .unwrap()
            .1,
    )
}

#[test]
fn stale_mask_alias_chain_and_earlier_live_replacements_stay_bound() {
    for pass in ["instcombine", "fast-local"] {
        for chain in [false, true] {
            let mut module = keel_ir::parse("func @f(i8, i16) -> i16 {\nblock0(v0: i8, v1: i16):\n v2 = uext.i16 v0\n v3 = iconst.i16 255\n v5 = iadd v1, v1\n v4 = and v2, v3\n return v4\n}").unwrap();
            let stale = result_matching(&module.functions[0], |d| {
                matches!(
                    d,
                    InstData::Binary {
                        op: keel_ir::BinaryOp::And,
                        ..
                    }
                )
            });
            let earlier = result_matching(&module.functions[0], |d| {
                matches!(
                    d,
                    InstData::Binary {
                        op: keel_ir::BinaryOp::Iadd,
                        ..
                    }
                )
            });
            module.functions[0].replace_all_uses(stale, earlier);
            if chain {
                module.functions[0].replace_all_uses(earlier, Value::new(1));
            }
            keel_ir::verify_module(&module).unwrap();
            let expected = if chain { 42 } else { 84 };
            assert_eq!(support::execute(&module.functions[0], &[7, 42]), expected);
            let alias = module.functions[0].value_def(stale);
            keel_opt::find_pass(pass)
                .unwrap()
                .run(&mut module.functions[0]);
            keel_ir::verify_module(&module).unwrap();
            assert_eq!(module.functions[0].value_def(stale), alias);
            assert_eq!(support::execute(&module.functions[0], &[7, 42]), expected);
            assert!(
                !keel_opt::find_pass(pass)
                    .unwrap()
                    .run(&mut module.functions[0])
            );
        }
    }
}

#[test]
fn stale_mask_result_can_alias_a_loop_header_parameter() {
    for pass in ["instcombine", "fast-local"] {
        let mut module = keel_ir::parse("func @f(i8, i16, i8) -> i16 {\nblock0(v0: i8, v1: i16, v9: i8):\n jump block1(v1, v9)\nblock1(v5: i16, v10: i8):\n v2 = uext.i16 v0\n v3 = iconst.i16 255\n v4 = and v2, v3\n brif v10, block2, block3\nblock2:\n v11 = iconst.i8 0\n jump block1(v5, v11)\nblock3:\n return v4\n}").unwrap();
        let stale = result_matching(&module.functions[0], |d| {
            matches!(
                d,
                InstData::Binary {
                    op: keel_ir::BinaryOp::And,
                    ..
                }
            )
        });
        let keel_ir::ValueDef::Result(owner, _) = module.functions[0].value_def(stale) else {
            unreachable!()
        };
        let header_param =
            module.functions[0].block_params(module.functions[0].inst_block(owner))[0];
        module.functions[0].replace_all_uses(stale, header_param);
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(support::execute(&module.functions[0], &[7, 42, 1]), 42);
        keel_opt::find_pass(pass)
            .unwrap()
            .run(&mut module.functions[0]);
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(
            module.functions[0].value_def(stale),
            keel_ir::ValueDef::Alias(header_param)
        );
        assert_eq!(support::execute(&module.functions[0], &[7, 42, 1]), 42);
        assert!(
            !keel_opt::find_pass(pass)
                .unwrap()
                .run(&mut module.functions[0])
        );
    }
}

#[test]
fn live_mask_resolves_a_raw_extension_source_alias_to_overflow_flag() {
    let mut module = keel_ir::parse("func @f(i8, i8, i8) -> i16 {\nblock0(v0: i8, v1: i8, v8: i8):\n v2, v3 = sadd_overflow v0, v1\n v4 = uext.i16 v8\n v5 = iconst.i16 255\n v6 = and v4, v5\n return v6\n}").unwrap();
    let overflow = result_matching(&module.functions[0], |d| {
        matches!(d, InstData::Overflow { .. })
    });
    let keel_ir::ValueDef::Result(owner, _) = module.functions[0].value_def(overflow) else {
        unreachable!()
    };
    let flag = module.functions[0].inst_results(owner)[1];
    let input = module.functions[0].block_params(module.functions[0].entry_block().unwrap())[2];
    let masked = result_matching(&module.functions[0], |d| {
        matches!(
            d,
            InstData::Binary {
                op: keel_ir::BinaryOp::And,
                ..
            }
        )
    });
    let extended = result_matching(&module.functions[0], |d| {
        matches!(d, InstData::Convert { .. })
    });
    module.functions[0].replace_all_uses(input, flag);
    keel_ir::verify_module(&module).unwrap();
    assert_eq!(support::execute(&module.functions[0], &[127, 1, 7]), 1);
    keel_opt::find_pass("instcombine")
        .unwrap()
        .run(&mut module.functions[0]);
    keel_ir::verify_module(&module).unwrap();
    assert_eq!(module.functions[0].resolve(masked), extended);
    assert_eq!(support::execute(&module.functions[0], &[127, 1, 7]), 1);
    assert!(
        !keel_opt::find_pass("instcombine")
            .unwrap()
            .run(&mut module.functions[0])
    );
}

#[test]
fn live_mask_folds_and_converges_with_reversed_cross_block_layout() {
    for pass in ["instcombine", "fast-local"] {
        let source = "func @f(i8) -> i16 {\nblock0(v0: i8):\n jump block2\nblock1:\n v3 = iconst.i16 255\n v4 = and v3, v2\n return v4\nblock2:\n v2 = uext.i16 v0\n jump block1\n}";
        let (original, mut optimized) = combine(source, pass);
        let masked = result_matching(&original.functions[0], |d| {
            matches!(
                d,
                InstData::Binary {
                    op: keel_ir::BinaryOp::And,
                    ..
                }
            )
        });
        let extended = result_matching(&original.functions[0], |d| {
            matches!(d, InstData::Convert { .. })
        });
        assert_eq!(optimized.functions[0].resolve(masked), extended);
        for x in 0..256 {
            assert_eq!(
                support::execute(&original.functions[0], &[x]),
                support::execute(&optimized.functions[0], &[x])
            );
        }
        assert!(
            !keel_opt::find_pass(pass)
                .unwrap()
                .run(&mut optimized.functions[0])
        );
    }
}

#[test]
fn stale_single_result_aliases_survive_local_folds_and_all_optimizer_tiers() {
    // Cover existing identity/constant/select/strength-reduction folds as well
    // as extension masks: the ownership check protects every single result.
    for body in [
        "v2 = iconst.i16 0\n v3 = iadd v0, v2",
        "v2 = iconst.i16 3\n v3 = iadd v2, v2",
        "v2 = iconst.i16 1\n v3 = select v2, v0, v1",
        "v2 = iconst.i16 8\n v3 = imul v0, v2",
    ] {
        let source = format!(
            "func @f(i16, i16) -> i16 {{\nblock0(v0: i16, v1: i16):\n {body}\n return v3\n}}"
        );
        let mut original = keel_ir::parse(&source).unwrap();
        let stale = Value::new(3);
        let replacement = Value::new(1);
        original.functions[0].replace_all_uses(stale, replacement);
        keel_ir::verify_module(&original).unwrap();
        for pass in ["instcombine", "fast-local"] {
            let mut optimized = original.clone();
            // The stale instruction must neither be replaced nor rewritten.
            assert!(
                !keel_opt::find_pass(pass)
                    .unwrap()
                    .run(&mut optimized.functions[0])
            );
            assert_eq!(optimized.to_string(), original.to_string());
            assert_eq!(
                optimized.functions[0].value_def(stale),
                keel_ir::ValueDef::Alias(replacement)
            );
            keel_ir::verify_module(&optimized).unwrap();
        }
        for tier in [
            keel_opt::OptimizationTier::None,
            keel_opt::OptimizationTier::Fast,
            keel_opt::OptimizationTier::Basic,
            keel_opt::OptimizationTier::Full,
        ] {
            let mut optimized = original.clone();
            keel_opt::optimize_with_tier(&mut optimized, tier).unwrap();
            keel_ir::verify_module(&optimized).unwrap();
            assert_eq!(
                support::execute(&optimized.functions[0], &[7, 42]),
                42,
                "{tier:?}: {source}"
            );
        }
    }
}

#[test]
fn fast_and_full_tiers_simplify_extension_masks_while_none_keeps_input() {
    let source = "func @f(i8) -> i16 {\nblock0(v0: i8):\n v1 = uext.i16 v0\n v2 = iconst.i16 255\n v3 = and v1, v2\n v4 = iconst.i8 127\n v5 = and v0, v4\n v6 = sext.i16 v5\n v7 = xor v3, v6\n return v7\n}";
    let original = keel_ir::parse(source).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut optimized = original.clone();
        keel_opt::optimize_with_tier(&mut optimized, tier).unwrap();
        keel_ir::verify_module(&optimized).unwrap();
        if tier == keel_opt::OptimizationTier::None {
            assert_eq!(optimized.to_string(), original.to_string());
        } else {
            assert_eq!(optimized.functions[0].resolve(Value::new(3)), Value::new(1));
            assert!(
                !optimized.to_string().contains("sext"),
                "{tier:?}: {optimized}"
            );
        }
        for input in 0..256 {
            assert_eq!(
                support::execute(&optimized.functions[0], &[input]),
                input ^ (input & 127)
            );
        }
    }
}
