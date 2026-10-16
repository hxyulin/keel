mod support;

#[test]
fn all_valid_integer_cast_pairs_preserve_bits_and_only_valid_pairs_collapse() {
    let types = ["i8", "i16", "i32", "i64", "i128"];
    for (s, source_ty) in types.iter().enumerate() {
        for (m, middle_ty) in types.iter().enumerate() {
            for (d, dest_ty) in types.iter().enumerate() {
                for inner in ["trunc", "uext", "sext"] {
                    for outer in ["trunc", "uext", "sext"] {
                        if !(if inner == "trunc" { m < s } else { m > s })
                            || !(if outer == "trunc" { d < m } else { d > m })
                        {
                            continue;
                        }
                        let source = format!(
                            "func @f({source_ty}) -> {dest_ty} {{\nblock0(v0: {source_ty}):\n v1 = {inner}.{middle_ty} v0\n v2 = {outer}.{dest_ty} v1\n return v2\n}}"
                        );
                        let original = keel_ir::parse(&source).unwrap();
                        keel_ir::verify_module(&original).unwrap();
                        let mut module = original.clone();
                        keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
                        let should_combine = outer == "trunc"
                            || inner == "uext"
                            || (outer == "sext" && inner == "sext");
                        let count = module.functions[0].layout_insts().count();
                        assert_eq!(
                            count,
                            if should_combine {
                                if s == d { 1 } else { 2 }
                            } else {
                                3
                            },
                            "{source}\n{module}"
                        );
                        assert!(
                            !keel_opt::find_pass("instcombine")
                                .unwrap()
                                .run(&mut module.functions[0])
                        );
                        if s == 4 || m == 4 || d == 4 {
                            continue;
                        }
                        let mut inputs: Vec<u64> = (0..256).collect();
                        for width in [8, 16, 32, 64] {
                            let sign = 1u64 << (width - 1);
                            inputs.extend([sign - 1, sign, sign + 1]);
                        }
                        inputs.extend([u64::MAX, u64::MAX - 1]);
                        for x in inputs {
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
}

#[test]
fn zero_extension_after_sign_extension_retains_intermediate_sign_bits() {
    let source = "func @f(i8) -> i64 {\nblock0(v0: i8):\n v1 = sext.i16 v0\n v2 = uext.i64 v1\n return v2\n}";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert_eq!(support::execute(&module.functions[0], &[255]), 65535);
    assert_eq!(module.functions[0].layout_insts().count(), 3);
}

fn combine_comparison(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    let stable = module.to_string();
    keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
    assert_eq!(stable, module.to_string());
    assert!(
        !keel_opt::find_pass("instcombine")
            .unwrap()
            .run(&mut module.functions[0])
    );
    keel_ir::verify_module(&module).unwrap();
    (original, module)
}

#[test]
fn matching_extension_equality_covers_all_widths_opcodes_and_operand_orders() {
    let types = ["i8", "i16", "i32", "i64", "i128"];
    for (s, from) in types.iter().enumerate() {
        for to in &types[s + 1..] {
            for op in ["sext", "uext"] {
                for cond in ["eq", "ne"] {
                    for args in ["v2, v3", "v3, v2"] {
                        let source = format!(
                            "func @f({from}, {from}) -> i8 {{\nblock0(v0: {from}, v1: {from}):\n v2 = {op}.{to} v0\n v3 = {op}.{to} v1\n v4 = icmp.{cond} {args}\n return v4\n}}"
                        );
                        let (original, module) = combine_comparison(&source);
                        assert_eq!(
                            module.functions[0].layout_insts().count(),
                            2,
                            "{source}\n{module}"
                        );
                        assert!(
                            module
                                .to_string()
                                .contains(&format!("v4 = icmp.{cond} v0, v1")),
                            "{module}"
                        );
                        if *to == "i128" {
                            // The independent executor deliberately supports only <= i64.
                            continue;
                        }
                        if *from == "i8" {
                            for a in 0..256 {
                                for b in 0..256 {
                                    assert_eq!(
                                        support::execute(&original.functions[0], &[a, b]),
                                        support::execute(&module.functions[0], &[a, b]),
                                        "{source}: {a}, {b}"
                                    );
                                }
                            }
                        } else {
                            let width = from[1..].parse::<u32>().unwrap();
                            let sign = 1u64 << (width - 1);
                            for a in [0, 1, sign - 1, sign, sign + 1, u64::MAX] {
                                for b in [0, 1, sign - 1, sign, sign + 1, u64::MAX] {
                                    assert_eq!(
                                        support::execute(&original.functions[0], &[a, b]),
                                        support::execute(&module.functions[0], &[a, b]),
                                        "{source}: {a}, {b}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn extension_equality_excludes_nonmatching_and_ordered_comparisons() {
    for (params, left, right) in [
        ("i8, i8", "sext.i64 v0", "uext.i64 v1"),
        ("i16, i16", "trunc.i8 v0", "trunc.i8 v1"),
        ("i8, i16", "sext.i64 v0", "sext.i64 v1"),
        ("i8, i8", "uext.i64 v0", "iconst.i64 0"),
        ("ptr, ptr", "ptrtoint.i64 v0", "ptrtoint.i64 v1"),
        ("f32, f32", "fpext.f64 v0", "fpext.f64 v1"),
    ] {
        let params: Vec<_> = params.split(", ").collect();
        for cond in ["eq", "ne"] {
            let source = format!(
                "func @f({}, {}) -> i8 {{\nblock0(v0: {}, v1: {}):\n v2 = {left}\n v3 = {right}\n v4 = icmp.{cond} v2, v3\n return v4\n}}",
                params[0], params[1], params[0], params[1]
            );
            // Floating-point comparisons use their own instruction family.
            let source = if params[0] == "f32" {
                source.replace("icmp.", "fcmp.")
            } else {
                source
            };
            let (_, module) = combine_comparison(&source);
            assert_eq!(module.functions[0].layout_insts().count(), 4, "{module}");
        }
    }
    for op in ["uext", "sext"] {
        for cond in ["slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge"] {
            let source = format!(
                "func @f(i8, i8) -> i8 {{\nblock0(v0: i8, v1: i8):\n v2 = {op}.i16 v0\n v3 = {op}.i16 v1\n v4 = icmp.{cond} v2, v3\n return v4\n}}"
            );
            let (_, module) = combine_comparison(&source);
            assert_eq!(module.functions[0].layout_insts().count(), 4, "{module}");
        }
    }
}

#[test]
fn matching_extensions_preserve_shared_uses_effects_result_and_location() {
    let source = "file f0 = \"comparison.c\"
decl @effect(i64)
func @f(i8, i8, ptr) -> i8 {
block0(v0: i8, v1: i8, v2: ptr):
 v3 = uext.i64 v0
 v4 = uext.i64 v1
 call @effect(v3)
 store volatile v4, v2
 v5 = icmp.eq v4, v3 !f0:7:12
 return v5
}";
    let (original, module) = combine_comparison(source);
    let cmp = |m: &keel_ir::Module| {
        m.functions[0]
            .layout_insts()
            .find(|&(_, i)| matches!(m.functions[0].inst_data(i), keel_ir::InstData::Icmp { .. }))
            .unwrap()
            .1
    };
    let old = cmp(&original);
    let new = cmp(&module);
    assert_eq!(old, new);
    assert_eq!(
        original.functions[0].inst_results(old),
        module.functions[0].inst_results(new)
    );
    assert_eq!(
        module.functions[0].value_type(module.functions[0].first_result(new)),
        keel_ir::Type::I8
    );
    assert_eq!(
        original.functions[0].inst_loc(old),
        module.functions[0].inst_loc(new)
    );
    assert!(module.to_string().contains("v5 = icmp.eq v0, v1 !f0:7:12"));
    assert_eq!(
        original.functions[0].layout_insts().count(),
        module.functions[0].layout_insts().count()
    );

    let (_, module) = combine_comparison(
        "decl @read() -> i8
func @f(ptr, i8, i8) -> i8 {
block0(v0: ptr, v1: i8, v2: i8):
 v3 = load.i8 volatile v0
 v4 = call @read()
 v5 = udiv v1, v2
 v6 = uext.i64 v3
 v7 = uext.i64 v4
 v8 = icmp.ne v6, v7
 v9 = uext.i64 v5
 v10 = icmp.eq v9, v6
 v11 = xor v8, v10
 return v11
}",
    );
    let text = module.to_string();
    for expected in [
        "load.i8 volatile",
        "call @read()",
        "udiv v1, v2",
        "icmp.ne v3, v4",
        "icmp.eq v3, v5",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert!(!text.contains("uext"), "{text}");

    // Even when the comparison becomes constant, its trapping producers remain.
    let (_, module) = combine_comparison(
        "func @f(i8, i8) -> i8 {\nblock0(v0: i8, v1: i8):\n v2 = udiv v0, v1\n v3 = sext.i64 v2\n v4 = sext.i64 v2\n v5 = icmp.eq v3, v4\n return v5\n}",
    );
    assert!(module.to_string().contains("udiv v0, v1"));
    assert!(module.to_string().contains("iconst.i8 1"));
}

#[test]
fn i128_extension_comparisons_keep_i64_boundary_producers() {
    for op in ["sext", "uext"] {
        for cond in ["eq", "ne"] {
            for boundary in [i64::MIN, i64::MAX, -1, 0] {
                let source = format!(
                    "func @f(i64, i64) -> i8 {{\nblock0(v0: i64, v1: i64):\n v2 = iconst.i64 {boundary}\n v3 = xor v0, v2\n v4 = {op}.i128 v3\n v5 = {op}.i128 v1\n v6 = icmp.{cond} v4, v5\n return v6\n}}"
                );
                let (_, module) = combine_comparison(&source);
                assert!(!module.to_string().contains("i128"), "{module}");
                assert!(module.to_string().contains(&format!("icmp.{cond}")));
                if boundary != 0 {
                    assert!(
                        module
                            .to_string()
                            .contains(&format!("iconst.i64 {boundary}")),
                        "{module}"
                    );
                }
            }
        }
    }
}
