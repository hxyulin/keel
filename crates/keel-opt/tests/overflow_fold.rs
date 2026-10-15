use keel_ir::{EntityRef, InstData, Value, ValueDef};

fn constant(module: &keel_ir::Module, value: usize) -> i64 {
    let f = &module.functions[0];
    let ValueDef::Result(inst, _) = f.value_def(f.resolve(Value::new(value))) else {
        panic!("not a result")
    };
    let InstData::Iconst { imm, .. } = f.inst_data(inst) else {
        panic!("not constant")
    };
    *imm
}

#[test]
fn all_three_drivers_fold_both_results_at_every_integer_width_boundary() {
    macro_rules! check_width {
        ($u:ty, $i:ty, $ty:literal) => {
            for op in ["uadd", "usub", "umul", "sadd", "ssub", "smul"] {
                for a in [0, 1, <$u>::MAX, <$i>::MAX as $u, <$i>::MIN as $u] {
                    for b in [0, 1, <$u>::MAX, <$i>::MAX as $u, <$i>::MIN as $u] {
                        let (result, flag) = match op {
                            "uadd" => a.overflowing_add(b),
                            "usub" => a.overflowing_sub(b),
                            "umul" => a.overflowing_mul(b),
                            _ => {
                                let (r, f) = match op {
                                    "sadd" => (a as $i).overflowing_add(b as $i),
                                    "ssub" => (a as $i).overflowing_sub(b as $i),
                                    _ => (a as $i).overflowing_mul(b as $i),
                                };
                                (r as $u, f)
                            }
                        };
                        let source = format!("func @f() -> {} {{\nblock0:\n    v0 = iconst.{} {}\n    v1 = iconst.{} {}\n    v2, v3 = {}_overflow v0, v1\n    return v2\n}}", $ty, $ty, a as $i as i64, $ty, b as $i as i64, op);
                        for pass in ["const-fold", "instcombine", "sccp"] {
                            let mut module = keel_ir::parse(&source).unwrap();
                            keel_opt::run_passes(&mut module, &[pass]).unwrap();
                            assert!(!module.to_string().contains("_overflow"));
                            assert_eq!(constant(&module, 2) as $u, result, "{pass} {source}");
                            assert_eq!(constant(&module, 3), i64::from(flag), "{pass} {source}");
                            assert!(!keel_opt::find_pass(pass).unwrap().run(&mut module.functions[0]));
                        }
                    }
                }
            }
        };
    }
    check_width!(u8, i8, "i8");
    check_width!(u16, i16, "i16");
    check_width!(u32, i32, "i32");
    check_width!(u64, i64, "i64");
}

#[test]
fn sccp_propagates_checked_results_through_parameters_and_control_flow() {
    let mut module = keel_ir::parse(
        "func @f() -> i8 {
block0:
    v0 = iconst.i8 255
    v1 = iconst.i8 1
    jump block1(v0, v1)
block1(v2: i8, v3: i8):
    v4, v5 = uadd_overflow v2, v3
    brif v5, block2(v4), block3
block2(v6: i8):
    return v6
block3:
    v7 = iconst.i8 99
    return v7
}",
    )
    .unwrap();
    keel_opt::run_passes(&mut module, &["sccp", "simplify-cfg", "dce"]).unwrap();
    let printed = module.to_string();
    assert!(!printed.contains("brif"));
    assert!(!printed.contains("_overflow"));
    assert!(!printed.contains("99"));
}

#[test]
fn unknown_operands_and_i128_results_remain_checked() {
    for ty in ["i64", "i128"] {
        let source = format!(
            "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n    v1 = iconst.{ty} 1\n    v2, v3 = sadd_overflow v0, v1\n    return v2\n}}"
        );
        for pass in ["const-fold", "instcombine", "sccp"] {
            let mut module = keel_ir::parse(&source).unwrap();
            keel_opt::run_passes(&mut module, &[pass]).unwrap();
            assert!(module.to_string().contains("sadd_overflow"));
        }
    }
    let mut module = keel_ir::parse("func @f() -> i128 {\nblock0:\n    v0 = iconst.i128 1\n    v1, v2 = sadd_overflow v0, v0\n    return v1\n}").unwrap();
    keel_opt::run_passes(&mut module, &["const-fold", "instcombine", "sccp"]).unwrap();
    assert!(module.to_string().contains("sadd_overflow"));
}
