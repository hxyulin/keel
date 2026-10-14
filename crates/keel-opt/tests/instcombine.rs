mod support;
use support::execute;

fn optimize(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["instcombine", "dce"]).unwrap();
    (original, optimized)
}

#[test]
fn power_of_two_arithmetic_is_exhaustive_at_i8_and_covers_wide_boundaries() {
    for ty in ["i8", "i16", "i32", "i64"] {
        let width = ty[1..].parse::<u32>().unwrap();
        for (op, expected) in [("imul", "shl"), ("udiv", "ushr"), ("urem", "and")] {
            for shift in 1..width {
                let power = (1u64 << shift) as i64;
                let source = format!(
                    "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n    v1 = iconst.{ty} {power}\n    v2 = {op} v0, v1\n    return v2\n}}"
                );
                let (original, optimized) = optimize(&source);
                assert!(
                    optimized.to_string().contains(expected),
                    "{ty}, {op}, {shift}"
                );
                let inputs: Vec<u64> = if ty == "i8" {
                    (0..256).collect()
                } else {
                    vec![0, 1, 127, 128, 65535, u64::MAX, 1 << 63, (1 << 63) - 1]
                };
                for input in inputs {
                    assert_eq!(
                        execute(&original.functions[0], &[input]),
                        execute(&optimized.functions[0], &[input]),
                        "{ty}, {op}, {shift}, {input}"
                    );
                }
                assert!(
                    !keel_opt::find_pass("instcombine")
                        .unwrap()
                        .run(&mut optimized.clone().functions[0])
                );
            }
        }
    }
}

#[test]
fn identities_and_integer_width_roundtrips_preserve_every_i8_input() {
    for expression in [
        "iadd v0, v1",
        "imul v0, v2",
        "and v0, v3",
        "or v0, v1",
        "xor v0, v1",
        "isub v0, v1",
        "shl v0, v4",
    ] {
        let source = format!(
            "func @f(i8) -> i8 {{\nblock0(v0: i8):\n    v1 = iconst.i8 0\n    v2 = iconst.i8 1\n    v3 = iconst.i8 -1\n    v4 = iconst.i64 8\n    v5 = {expression}\n    return v5\n}}"
        );
        let (original, optimized) = optimize(&source);
        assert_eq!(optimized.functions[0].layout_insts().count(), 1);
        for input in 0..256 {
            assert_eq!(
                execute(&original.functions[0], &[input]),
                execute(&optimized.functions[0], &[input])
            );
        }
    }
    for ext in ["uext", "sext"] {
        let source = format!(
            "func @f(i8) -> i8 {{\nblock0(v0: i8):\n    v1 = {ext}.i64 v0\n    v2 = trunc.i8 v1\n    v3 = ineg v2\n    v4 = ineg v3\n    return v4\n}}"
        );
        let (original, optimized) = optimize(&source);
        assert_eq!(optimized.functions[0].layout_insts().count(), 1);
        for input in 0..256 {
            assert_eq!(
                execute(&original.functions[0], &[input]),
                execute(&optimized.functions[0], &[input])
            );
        }
    }
}

#[test]
fn variable_division_traps_and_strict_floating_point_are_not_erased() {
    let (_, optimized) = optimize(
        "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iconst.i64 0
    v3 = udiv v2, v0
    v4 = sdiv v1, v1
    v5 = iadd v3, v4
    return v5
}",
    );
    assert!(optimized.to_string().contains("udiv"));
    assert!(optimized.to_string().contains("sdiv"));
    let (_, optimized) = optimize(
        "func @f(f64) -> f64 {
block0(v0: f64):
    v1 = fconst.f64 0.0
    v2 = fadd v0, v1
    v3 = fmul v2, v1
    return v3
}",
    );
    assert!(optimized.to_string().contains("fadd"));
    assert!(optimized.to_string().contains("fmul"));
}

#[test]
fn combine_notifies_users_when_block_layout_is_not_definition_order() {
    let (_, optimized) = optimize(
        "func @f(i64) -> i64 {
block0(v0: i64):
    jump block2
block1:
    v3 = iadd v2, v1
    return v3
block2:
    v1 = iconst.i64 0
    v2 = iadd v0, v1
    jump block1
}",
    );
    assert!(!optimized.to_string().contains("iadd"));
}

#[test]
fn commuted_multiply_and_i128_identities_are_combined() {
    let (_, optimized) = optimize(
        "func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 -9223372036854775808
    v2 = imul v1, v0
    return v2
}",
    );
    assert!(optimized.to_string().contains("shl"));
    let (_, optimized) = optimize(
        "func @f(i128) -> i128 {
block0(v0: i128):
    v1 = iconst.i128 -1
    v2 = and v0, v1
    v3 = iconst.i128 0
    v4 = iadd v2, v3
    return v4
}",
    );
    assert_eq!(optimized.functions[0].layout_insts().count(), 1);
}

#[test]
fn combining_survives_every_subset_of_existing_passes() {
    let original = keel_ir::parse(
        "func @f(i8) -> i8 {
block0(v0: i8):
    v1 = iconst.i8 8
    v2 = udiv v0, v1
    v3 = imul v1, v2
    v4 = urem v0, v1
    v5 = iadd v3, v4
    return v5
}",
    )
    .unwrap();
    for subset in 0..64 {
        let other = ["const-fold", "sccp", "simplify-cfg", "gvn", "dce", "gcm"];
        let mut names = vec!["instcombine"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        for input in 0..256 {
            assert_eq!(
                execute(&module.functions[0], &[input]),
                execute(&original.functions[0], &[input])
            );
        }
    }
}
