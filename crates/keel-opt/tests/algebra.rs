mod support;
fn apply(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert!(
        !keel_opt::find_pass("instcombine")
            .unwrap()
            .run(&mut module.functions[0])
    );
    (original, module)
}
#[test]
fn wrapping_cancellation_covers_every_i8_pair_and_wide_boundaries() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for (inner, args, outer, outer_args, expected) in [
            ("iadd", "v0, v1", "isub", "v2, v0", "v1"),
            ("iadd", "v1, v0", "isub", "v2, v1", "v0"),
            ("isub", "v0, v1", "iadd", "v2, v1", "v0"),
            ("isub", "v1, v0", "iadd", "v0, v2", "v1"),
            ("isub", "v0, v1", "isub", "v0, v2", "v1"),
        ] {
            let source = format!(
                "func @f({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = {inner} {args}\n v3 = {outer} {outer_args}\n return v3\n}}"
            );
            let (original, module) = apply(&source);
            assert!(
                module.to_string().contains(&format!("return {expected}")),
                "{source}\n{module}"
            );
            if ty == "i128" {
                continue;
            }
            let inputs: Vec<u64> = if ty == "i8" {
                (0..256).collect()
            } else {
                vec![0, 1, 127, 128, u64::MAX, 1 << 63, (1 << 63) - 1]
            };
            for &x in &inputs {
                for &y in &inputs {
                    assert_eq!(
                        support::execute(&original.functions[0], &[x, y]),
                        support::execute(&module.functions[0], &[x, y]),
                        "{source} {x} {y}"
                    );
                }
            }
        }
    }
}
#[test]
fn shared_addends_cancel_in_all_operand_positions() {
    for swapped in [false, true] {
        for other in [false, true] {
            let left = if swapped { "v1, v0" } else { "v0, v1" };
            let right = if other { "v2, v0" } else { "v0, v2" };
            let source = format!(
                "func @f(i8, i8, i8) -> i8 {{\nblock0(v0: i8, v1: i8, v2: i8):\n v3 = iadd {left}\n v4 = iadd {right}\n v5 = isub v3, v4\n return v5\n}}"
            );
            let (original, module) = apply(&source);
            assert_eq!(module.functions[0].layout_insts().count(), 2, "{module}");
            for x in 0..16 {
                for y in 0..16 {
                    for z in 0..16 {
                        assert_eq!(
                            support::execute(&original.functions[0], &[x, y, z]),
                            support::execute(&module.functions[0], &[x, y, z])
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn constant_grouping_wraps_at_the_result_width() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in ["iadd", "imul", "and", "or", "xor"] {
            for c1 in [3, 7, 37, 127, -1] {
                for c2 in [3, 7, 37, 127, -1] {
                    let source = format!(
                        "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} {c1}\n v2 = {op} v0, v1\n v3 = iconst.{ty} {c2}\n v4 = {op} v2, v3\n return v4\n}}"
                    );
                    let (original, module) = apply(&source);
                    assert!(
                        module.functions[0].layout_insts().count() <= 3,
                        "{source}\n{module}"
                    );
                    if ty == "i128" {
                        continue;
                    }
                    for x in (0..256).chain([u64::MAX, 1 << 63]) {
                        assert_eq!(
                            support::execute(&original.functions[0], &[x]),
                            support::execute(&module.functions[0], &[x]),
                            "{source} {x}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn shared_chains_and_unrepresentable_i128_constants_remain_intact() {
    let source = "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 37\n v2 = iadd v0, v1\n v3 = iconst.i64 7\n v4 = iadd v2, v3\n v5 = xor v2, v4\n return v5\n}";
    let (_, module) = apply(source);
    assert_eq!(module.to_string().matches(" = iadd").count(), 2);
    let source = "func @f(i128) -> i128 {\nblock0(v0: i128):\n v1 = iconst.i128 9223372036854775807\n v2 = iadd v0, v1\n v3 = iconst.i128 1\n v4 = iadd v2, v3\n return v4\n}";
    let (_, module) = apply(source);
    assert_eq!(module.to_string().matches(" = iadd").count(), 2);
}

#[test]
fn negation_cancels_with_wrapping_semantics_and_traps_stay_observable() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        let source = format!(
            "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = ineg v0\n v2 = iadd v1, v0\n return v2\n}}"
        );
        let (original, module) = apply(&source);
        assert!(!module.to_string().contains(" = iadd"));
        if ty != "i128" {
            for x in (0..256).chain([u64::MAX, 1 << 63]) {
                assert_eq!(
                    support::execute(&original.functions[0], &[x]),
                    support::execute(&module.functions[0], &[x])
                );
            }
        }
    }
    let source = "func @f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = udiv v0, v1\n v3 = ineg v2\n v4 = iadd v2, v3\n return v4\n}";
    let (_, module) = apply(source);
    assert!(module.to_string().contains(" = udiv"));
    assert!(!module.to_string().contains(" = iadd"));
}
