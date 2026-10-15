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
