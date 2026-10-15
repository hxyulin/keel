mod support;

#[test]
fn absorption_and_cancellation_preserve_all_i8_pairs_and_wide_edges() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for (outer, inner, expected) in [
            ("and", "or", "return v0"),
            ("or", "and", "return v0"),
            ("xor", "xor", "return v1"),
            ("and", "and", "and"),
            ("or", "or", "or"),
        ] {
            for swapped in [false, true] {
                let (a, b) = if swapped { ("v1", "v0") } else { ("v0", "v1") };
                let source = format!(
                    "func @f({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = {inner} {a}, {b}\n v3 = {outer} v2, v0\n return v3\n}}"
                );
                let original = keel_ir::parse(&source).unwrap();
                let mut module = original.clone();
                keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
                assert!(module.to_string().contains(expected), "{module}");
                assert!(module.functions[0].layout_insts().count() <= 2, "{module}");
                assert!(
                    !keel_opt::find_pass("instcombine")
                        .unwrap()
                        .run(&mut module.functions[0])
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
}

#[test]
fn complements_fold_at_every_width_without_erasing_effects() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in ["and", "or", "xor"] {
            let source = format!(
                "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = bnot v0\n v2 = {op} v1, v0\n return v2\n}}"
            );
            let original = keel_ir::parse(&source).unwrap();
            let mut module = original.clone();
            keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
            assert!(
                module
                    .to_string()
                    .contains(if op == "and" { " 0" } else { " -1" }),
                "{module}"
            );
            if ty != "i128" {
                for x in (0..256).chain([u64::MAX, 1 << 63]) {
                    assert_eq!(
                        support::execute(&original.functions[0], &[x]),
                        support::execute(&module.functions[0], &[x])
                    );
                }
            }
        }
    }
    let mut module = keel_ir::parse("func @f(ptr, i64) -> i64 {\nblock0(v0: ptr, v1: i64):\n v2 = load.i64 v0\n v3 = or v2, v1\n v4 = and v2, v3\n return v4\n}").unwrap();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert!(module.to_string().contains("load.i64"));
    assert!(!module.to_string().contains(" = and"));
}

#[test]
fn unrelated_operands_are_not_absorbed_and_local_rewrites_expose_cancellation() {
    let source = "func @f(i8, i8, i8) -> i8 {
block0(v0: i8, v1: i8, v2: i8):
 v3 = or v1, v2
 v4 = and v0, v3
 return v4
}";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert!(module.to_string().contains(" = and"));
    let source = "func @f(i8, i8, i8) -> i8 {
block0(v0: i8, v1: i8, v2: i8):
 v3 = and v0, v1
 v4 = or v0, v3
 v5 = xor v4, v2
 v6 = xor v0, v5
 return v6
}";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert!(module.to_string().contains("return v2"), "{module}");
}
