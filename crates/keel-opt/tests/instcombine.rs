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

#[test]
fn sign_zero_i32_shift_masks_exhaust_bytes_counts_and_masks() {
    // Negative literals use the IR's modulo-width count, rather than C shifts.
    for count in (0..=32i64).chain([-1, -32, -33, i64::MIN, i64::MAX]) {
        let n = (count as u64 % 32) as u32;
        let possible = 255u64 >> n;
        for mask in 0..256u64 {
            let source = format!(
                "func @f(i8) -> i32 {{\nblock0(v0: i8):\n v1 = uext.i32 v0\n v2 = iconst.i64 {count}\n v3 = sshr v1, v2\n v4 = iconst.i32 {mask}\n v5 = and v3, v4\n return v5\n}}"
            );
            let (original, optimized) = optimize(&source);
            keel_ir::verify_module(&optimized).unwrap();
            if possible & !mask == 0 {
                assert!(
                    !optimized.to_string().contains(" = and "),
                    "{source}\n{optimized}"
                );
            } else if possible & mask != 0 {
                assert!(
                    optimized.to_string().contains(" = and "),
                    "{source}\n{optimized}"
                );
            }
            for byte in 0..256 {
                let reference = (byte >> n) & mask;
                assert_eq!(execute(&original.functions[0], &[byte]), reference);
                assert_eq!(
                    execute(&optimized.functions[0], &[byte]),
                    reference,
                    "{source} {byte}"
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

#[test]
fn signed_unknown_and_sign_preserving_extensions_keep_the_mask() {
    for producer in ["v1 = iadd v0, v0", "v1 = sext.i32 v0"] {
        let ty = if producer.contains("sext") {
            "i8"
        } else {
            "i32"
        };
        let source = format!(
            "func @f({ty}) -> i32 {{\nblock0(v0: {ty}):\n {producer}\n v2 = iconst.i32 5\n v3 = sshr v1, v2\n v4 = iconst.i32 7\n v5 = and v3, v4\n return v5\n}}"
        );
        let (original, optimized) = optimize(&source);
        assert!(optimized.to_string().contains(" = and "));
        for x in (0..256).chain([0x80000000, 0x7fffffff, 0xffffffff]) {
            assert_eq!(
                execute(&original.functions[0], &[x]),
                execute(&optimized.functions[0], &[x])
            );
        }
    }
}

#[test]
fn load_fed_field_extraction_removes_only_the_covering_mask() {
    use keel_ir::{EntityRef, InstData, Value};
    for (count, mask) in [(5, 7), (4, 15)] {
        let source = format!(
            "decl @observe(ptr)\nfunc @f(ptr) -> i32 {{\nblock0(v0: ptr):\n call @observe(v0)\n v1 = load.i8 v0\n v2 = uext.i32 v1\n v3 = iconst.i32 {count}\n v4 = sshr v2, v3\n v5 = iconst.i32 {mask}\n v6 = and v4, v5\n store v1, v0\n call @observe(v0)\n return v6\n}}"
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let f = &mut module.functions[0];
        let before: Vec<_> = f
            .layout_insts()
            .map(|(_, i)| (i, f.inst_data(i).clone()))
            .collect();
        keel_opt::find_pass("instcombine").unwrap().run(f);
        assert_eq!(f.resolve(Value::new(6)), Value::new(4));
        let after: Vec<_> = f
            .layout_insts()
            .map(|(_, i)| (i, f.inst_data(i).clone()))
            .collect();
        let expected: Vec<_> = before
            .into_iter()
            .filter(|(_, d)| {
                !matches!(
                    d,
                    InstData::Binary {
                        op: keel_ir::BinaryOp::And,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(after, expected);
        keel_ir::verify_module(&module).unwrap();
    }
}

#[test]
fn bounded_mask_and_or_sign_proof_covers_uri_byte_fields() {
    for mask in [7u64, 15] {
        let source = format!(
            "func @f(i32) -> i32 {{\nblock0(v0: i32):\n v1 = iconst.i32 6\n v2 = sshr v0, v1\n v3 = iconst.i32 63\n v4 = and v2, v3\n v5 = iconst.i32 128\n v6 = or v4, v5\n v7 = iconst.i32 4\n v8 = sshr v6, v7\n v9 = iconst.i32 {mask}\n v10 = and v8, v9\n return v10\n}}"
        );
        let (original, optimized) = optimize(&source);
        let f = &optimized.functions[0];
        use keel_ir::{EntityRef, Value};
        assert_eq!(
            f.resolve(Value::new(10)) == f.resolve(Value::new(8)),
            mask == 15
        );
        for x in (0..65536).chain([0x80000000, 0x7fffffff, 0xffffffff]) {
            let reference = ((((x as i32 >> 6) as u32 & 63) | 128) >> 4) as u64 & mask;
            assert_eq!(execute(&original.functions[0], &[x]), reference);
            assert_eq!(execute(f, &[x]), reference);
        }
        keel_ir::verify_module(&optimized).unwrap();
        assert!(
            !keel_opt::find_pass("instcombine")
                .unwrap()
                .run(&mut optimized.clone().functions[0])
        );
    }
}

#[test]
fn signed_shift_i32_mask_boundaries_match_independent_scalar_oracle() {
    let mut inputs: Vec<u64> = (0..256).collect();
    inputs.extend([0x7fffffff, 0x80000000, 0x80000001, 0xfffffffe, 0xffffffff]);
    let mut state = 0x92d6_3af1u32;
    for _ in 0..256 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        inputs.push(u64::from(state));
    }
    for source_mask in [
        0x7fffffff_u32,
        0x40000000,
        0x55555555,
        0x00ff00ff,
        0x80000000,
        0xffffffff,
    ] {
        for count in [0i64, 1, 31, 32, 33, -1, -32, -33, i64::MIN, i64::MAX] {
            let n = (count as u64 % 32) as u32;
            let cover = if source_mask & 0x80000000 == 0 {
                source_mask >> n
            } else {
                u32::MAX
            };
            // Include a covering mask and masks missing potential result bits.
            for output_mask in [cover, cover & 0x55555555, 7] {
                for count_ty in ["i8", "i16", "i32", "i64"] {
                    let source = format!(
                        "func @f(i32) -> i32 {{\nblock0(v0: i32):\n v1 = iconst.i32 {}\n v2 = and v0, v1\n v3 = iconst.{count_ty} {count}\n v4 = sshr v2, v3\n v5 = iconst.i32 {}\n v6 = and v4, v5\n return v6\n}}",
                        source_mask as i32, output_mask as i32,
                    );
                    let (original, optimized) = optimize(&source);
                    keel_ir::verify_module(&optimized).unwrap();
                    // Removing a zero shift can make nested AND grouping
                    // profitable on a later visit: stale user counts are
                    // deliberately conservative. Check both visits and the
                    // change flag without assuming that DCE preserves a fixed
                    // point for single-use profitability rules.
                    let mut second = optimized.clone();
                    let changed = keel_opt::find_pass("instcombine")
                        .unwrap()
                        .run(&mut second.functions[0]);
                    assert_eq!(changed, optimized.to_string() != second.to_string());
                    keel_ir::verify_module(&second).unwrap();
                    for &x in &inputs {
                        let expected =
                            (((x as u32 & source_mask) as i32 >> n) as u32 & output_mask) as u64;
                        assert_eq!(execute(&original.functions[0], &[x]), expected);
                        assert_eq!(
                            execute(&optimized.functions[0], &[x]),
                            expected,
                            "{source} input {x:#x}"
                        );
                        assert_eq!(execute(&second.functions[0], &[x]), expected);
                    }
                }
            }
        }
    }
}
