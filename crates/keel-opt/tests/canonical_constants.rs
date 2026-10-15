mod support;
use keel_ir::{InstData, ValueDef};

#[test]
fn integer_constants_get_a_stable_right_hand_position_at_every_width() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in ["iadd", "imul", "and", "or", "xor", "umulhi", "smulhi"] {
            let source = format!(
                "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} 3\n v2 = {op} v1, v0\n return v2\n}}"
            );
            let original = keel_ir::parse(&source).unwrap();
            let mut module = original.clone();
            keel_opt::run_passes(&mut module, &["instcombine"]).unwrap();
            let f = &module.functions[0];
            let (_, inst) = f
                .layout_insts()
                .find(|(_, i)| matches!(f.inst_data(*i), InstData::Binary { .. }))
                .unwrap();
            let InstData::Binary { args, .. } = *f.inst_data(inst) else {
                unreachable!()
            };
            assert!(matches!(
                f.value_def(f.resolve(args[0])),
                ValueDef::Param(..)
            ));
            let ValueDef::Result(constant, _) = f.value_def(f.resolve(args[1])) else {
                panic!("expected literal")
            };
            assert!(matches!(
                f.inst_data(constant),
                InstData::Iconst { imm: 3, .. }
            ));
            assert!(
                !keel_opt::find_pass("instcombine")
                    .unwrap()
                    .run(&mut module.clone().functions[0])
            );
            if ty != "i128" && !matches!(op, "umulhi" | "smulhi") {
                for input in (0..256).chain([u64::MAX, 1 << 63]) {
                    assert_eq!(
                        support::execute(&original.functions[0], &[input]),
                        support::execute(&module.functions[0], &[input]),
                        "{ty} {op} {input}"
                    );
                }
            }
        }
    }
}

#[test]
fn noncommutative_operations_and_strict_floats_keep_operand_order() {
    for op in [
        "isub", "sdiv", "udiv", "srem", "urem", "shl", "ushr", "sshr",
    ] {
        let source = format!(
            "func @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 37\n v2 = {op} v1, v0\n return v2\n}}"
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let original = module.to_string();
        keel_opt::run_passes(&mut module, &["instcombine"]).unwrap();
        assert_eq!(module.to_string(), original);
    }
    for op in ["fadd", "fmul", "fsub", "fdiv"] {
        let source = format!(
            "func @f(f64) -> f64 {{\nblock0(v0: f64):\n v1 = fconst.f64 37.0\n v2 = {op} v1, v0\n return v2\n}}"
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let original = module.to_string();
        keel_opt::run_passes(&mut module, &["instcombine"]).unwrap();
        assert_eq!(module.to_string(), original);
    }
}

#[test]
fn power_of_two_literals_do_not_bypass_canonicalization_of_other_operations() {
    for op in ["iadd", "and", "or", "xor", "umulhi", "smulhi"] {
        for constant in [2, 4, 8, 128] {
            let source = format!(
                "func @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 {constant}\n v2 = {op} v1, v0\n return v2\n}}"
            );
            let mut module = keel_ir::parse(&source).unwrap();
            keel_opt::run_passes(&mut module, &["instcombine"]).unwrap();
            let f = &module.functions[0];
            let (_, inst) = f
                .layout_insts()
                .find(|(_, i)| matches!(f.inst_data(*i), InstData::Binary { .. }))
                .unwrap();
            let InstData::Binary { args, .. } = *f.inst_data(inst) else {
                unreachable!()
            };
            assert!(
                matches!(f.value_def(f.resolve(args[0])), ValueDef::Param(..)),
                "{module}"
            );
        }
    }
}
