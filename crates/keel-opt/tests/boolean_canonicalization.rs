use keel_ir::{BinaryOp, ConvertOp, EntityRef, Function, InstData, IntCC, Type, Value};

fn mask(ty: Type) -> u128 {
    u128::MAX >> (128 - ty.bytes().unwrap() * 8)
}

// Independent integer/control-flow evaluator, including full-width i128.
// No optimizer folding helpers are used to decide the expected truth value.
fn execute(func: &Function, inputs: &[u128]) -> u128 {
    let mut values = vec![0; func.num_values()];
    let mut block = func.entry_block().unwrap();
    for (&param, &input) in func.block_params(block).iter().zip(inputs) {
        values[param.index()] = input & mask(func.value_type(param));
    }
    for _ in 0..100 {
        let mut edge = None;
        for &inst in func.block_insts(block) {
            let read = |v: Value| values[func.resolve(v).index()];
            let result = match func.inst_data(inst) {
                InstData::Iconst { imm, .. } => Some(*imm as i128 as u128),
                InstData::Convert { op, arg, .. } => Some(match op {
                    ConvertOp::Uext | ConvertOp::Trunc => read(*arg),
                    ConvertOp::Sext => {
                        let shift = 128 - func.value_type(*arg).bytes().unwrap() * 8;
                        ((read(*arg) << shift) as i128 >> shift) as u128
                    }
                    _ => panic!("unexpected conversion {op:?}"),
                }),
                InstData::Binary { op, args } => Some(match op {
                    BinaryOp::Iadd => read(args[0]).wrapping_add(read(args[1])),
                    BinaryOp::Xor => read(args[0]) ^ read(args[1]),
                    _ => panic!("unexpected binary operation {op:?}"),
                }),
                InstData::Icmp { cond, args } => {
                    let (a, b) = (read(args[0]), read(args[1]));
                    let shift = 128 - func.value_type(args[0]).bytes().unwrap() * 8;
                    let (sa, sb) = (
                        ((a << shift) as i128) >> shift,
                        ((b << shift) as i128) >> shift,
                    );
                    Some(u128::from(match cond {
                        IntCC::Eq => a == b,
                        IntCC::Ne => a != b,
                        IntCC::Slt => sa < sb,
                        IntCC::Sle => sa <= sb,
                        IntCC::Sgt => sa > sb,
                        IntCC::Sge => sa >= sb,
                        IntCC::Ult => a < b,
                        IntCC::Ule => a <= b,
                        IntCC::Ugt => a > b,
                        IntCC::Uge => a >= b,
                    }))
                }
                InstData::Return { args } => return read(args[0]),
                InstData::Jump { dest } => {
                    edge = Some(dest);
                    None
                }
                InstData::Brif { cond, dests } => {
                    edge = Some(&dests[usize::from(read(*cond) == 0)]);
                    None
                }
                _ => panic!("unexpected instruction {:?}", func.inst_data(inst)),
            };
            if let Some(result) = result {
                let value = func.first_result(inst);
                values[value.index()] = result & mask(func.value_type(value));
            }
        }
        let edge = edge.expect("unterminated block");
        assert!(edge.args.is_empty());
        block = edge.block;
    }
    panic!("execution did not terminate");
}

fn optimize(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut optimized = original.clone();
    keel_opt::run_passes(&mut optimized, &["instcombine", "dce"]).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let snapshot = optimized.to_string();
    assert!(
        !keel_opt::find_pass("instcombine")
            .unwrap()
            .run(&mut optimized.functions[0])
    );
    assert_eq!(snapshot, optimized.to_string());
    (original, optimized)
}

fn inputs(width: u32) -> Vec<u128> {
    if width == 8 {
        return (0..256).collect();
    }
    let mask = u128::MAX >> (128 - width);
    vec![
        0,
        1,
        127,
        128,
        255,
        256,
        mask >> 1,
        1 << (width - 1),
        mask - 1,
        mask,
    ]
}

#[test]
fn extension_zero_tests_cover_all_strict_integer_width_pairs() {
    for (i, from) in [8, 16, 32, 64, 128].into_iter().enumerate() {
        for to in [8, 16, 32, 64, 128].into_iter().skip(i + 1) {
            for op in ["sext", "uext"] {
                for cc in ["eq", "ne"] {
                    for commuted in [false, true] {
                        let args = if commuted { "v2, v1" } else { "v1, v2" };
                        let source = format!(
                            "func @f(i{from}) -> i8 {{
block0(v0: i{from}):
 v1 = {op}.i{to} v0
 v2 = iconst.i{to} 0
 v3 = icmp.{cc} {args}
 return v3
}}"
                        );
                        let (original, optimized) = optimize(&source);
                        let text = optimized.to_string();
                        assert!(!text.contains(op), "{text}");
                        assert!(text.contains(&format!("iconst.i{from} 0")), "{text}");
                        for input in inputs(from) {
                            let expected = u128::from((input == 0) == (cc == "eq"));
                            assert_eq!(execute(&original.functions[0], &[input]), expected);
                            assert_eq!(execute(&optimized.functions[0], &[input]), expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn canonical_integer_boolean_retests_cover_every_predicate_i8_pair_and_wide_edges() {
    for (cc, inverse) in [
        ("eq", "ne"),
        ("ne", "eq"),
        ("slt", "sge"),
        ("sle", "sgt"),
        ("sgt", "sle"),
        ("sge", "slt"),
        ("ult", "uge"),
        ("ule", "ugt"),
        ("ugt", "ule"),
        ("uge", "ult"),
    ] {
        for width in [8, 16, 32, 64, 128] {
            for outer in ["eq", "ne"] {
                let source = format!(
                    "func @f(i{width}, i{width}) -> i8 {{
block0(v0: i{width}, v1: i{width}):
 v2 = icmp.{cc} v0, v1
 v3 = iconst.i8 0
 v4 = icmp.{outer} v3, v2
 return v4
}}"
                );
                let (original, optimized) = optimize(&source);
                let expected_cc = if outer == "eq" { inverse } else { cc };
                assert!(
                    optimized
                        .to_string()
                        .contains(&format!("icmp.{expected_cc} v0, v1")),
                    "{optimized}"
                );
                assert_eq!(optimized.functions[0].layout_insts().count(), 2);
                for a in inputs(width) {
                    for b in inputs(width) {
                        assert_eq!(
                            execute(&original.functions[0], &[a, b]),
                            execute(&optimized.functions[0], &[a, b]),
                            "{cc} {outer}: {a}, {b}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn extension_branch_truth_covers_i128_destinations_and_nonboolean_sources() {
    for (i, from) in [8, 16, 32, 64].into_iter().enumerate() {
        for to in [8, 16, 32, 64, 128].into_iter().skip(i + 1) {
            for op in ["sext", "uext"] {
                let source = format!(
                    "func @f(i{from}) -> i8 {{
block0(v0: i{from}):
 v1 = {op}.i{to} v0
 brif v1, block1, block2
block1:
 v2 = iconst.i8 1
 return v2
block2:
 v3 = iconst.i8 0
 return v3
}}"
                );
                let (original, optimized) = optimize(&source);
                assert!(optimized.to_string().contains("brif v0,"), "{optimized}");
                assert!(!optimized.to_string().contains(op), "{optimized}");
                for input in inputs(from) {
                    assert_eq!(
                        execute(&original.functions[0], &[input]),
                        u128::from(input != 0)
                    );
                    assert_eq!(
                        execute(&optimized.functions[0], &[input]),
                        u128::from(input != 0)
                    );
                }
            }
        }
    }
}

#[test]
fn multiuse_inner_compare_and_outer_identity_type_and_location_are_preserved() {
    for outer_cc in ["eq", "ne"] {
        let mut module = keel_ir::parse(&format!(
            "func @f(i64, i64) -> i8 {{
block0(v0: i64, v1: i64):
 v2 = icmp.slt v0, v1
 v3 = iconst.i8 0
 v4 = icmp.{outer_cc} v2, v3
 v5 = xor v2, v4
 return v5
}}"
        ))
        .unwrap();
        let original = module.clone();
        let file = module.add_file("boolean.c");
        let f = &mut module.functions[0];
        let compares: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| matches!(f.inst_data(i), InstData::Icmp { .. }).then_some(i))
            .collect();
        let (inner, outer) = (compares[0], compares[1]);
        let inner_data = f.inst_data(inner).clone();
        let result = f.first_result(outer);
        let loc = Some(keel_ir::SourceLoc {
            file,
            line: 12,
            col: 3,
            scope: None,
        });
        f.set_inst_loc(outer, loc);
        keel_opt::find_pass("instcombine").unwrap().run(f);
        assert_eq!(f.inst_data(inner), &inner_data);
        assert!(!f.is_removed(outer));
        assert_eq!(f.first_result(outer), result);
        assert_eq!(f.resolve(result), result);
        assert_eq!(f.value_type(result), Type::I8);
        assert_eq!(f.inst_loc(outer), loc);
        assert!(
            matches!(f.inst_data(outer), InstData::Icmp { cond, .. } if *cond == if outer_cc == "eq" { IntCC::Sge } else { IntCC::Slt })
        );
        keel_ir::verify_module(&module).unwrap();
        for a in inputs(64) {
            for b in inputs(64) {
                assert_eq!(
                    execute(&original.functions[0], &[a, b]),
                    execute(&module.functions[0], &[a, b])
                );
            }
        }
    }
}

#[test]
fn aliases_and_later_definitions_notify_rewritten_terminators() {
    let source = "func @f(i8) -> i8 {
block0(v0: i8):
 jump block2
block1:
 v4 = sext.i64 v3
 brif v4, block3, block4
block2:
 v1 = uext.i16 v0
 v2 = iconst.i16 0
 v3 = iadd v1, v2
 jump block1
block3:
 v5 = iconst.i8 1
 return v5
block4:
 v6 = iconst.i8 0
 return v6
}";
    let (original, optimized) = optimize(source);
    for input in 0..256 {
        assert_eq!(
            execute(&original.functions[0], &[input]),
            execute(&optimized.functions[0], &[input])
        );
    }
    assert!(optimized.to_string().contains("brif v0,"), "{optimized}");
    assert!(!optimized.to_string().contains("ext."), "{optimized}");
    // The branch initially registers v3 after peeling v4. When v3 later
    // aliases the extension v1, the updated user list must revisit the branch
    // to peel that newly exposed extension as well.
}

#[test]
fn truncation_nonzero_literals_ordered_and_float_comparisons_stay_out_of_scope() {
    for (from, conversion, to) in [
        ("i64", "trunc", "i8"),
        ("i8", "sext", "i64"),
        ("i8", "uext", "i64"),
    ] {
        for (cc, literal) in [
            ("eq", 1),
            ("ne", -1),
            ("slt", 0),
            ("uge", 0),
            ("eq", 0),
            ("ne", 0),
        ] {
            if conversion != "trunc" && literal == 0 && matches!(cc, "eq" | "ne") {
                continue;
            }
            let source = format!(
                "func @f({from}) -> i8 {{
block0(v0: {from}):
 v1 = {conversion}.{to} v0
 v2 = iconst.{to} {literal}
 v3 = icmp.{cc} v1, v2
 return v3
}}"
            );
            let (original, optimized) = optimize(&source);
            assert!(optimized.to_string().contains(conversion), "{optimized}");
            for input in inputs(from[1..].parse().unwrap()) {
                assert_eq!(
                    execute(&original.functions[0], &[input]),
                    execute(&optimized.functions[0], &[input])
                );
            }
        }
    }
    for fp in ["eq", "ne", "lt", "le", "gt", "ge", "ord", "uno"] {
        for cc in ["eq", "ne"] {
            let (_, optimized) = optimize(&format!(
                "func @f(f64, f64) -> i8 {{
block0(v0: f64, v1: f64):
 v2 = fcmp.{fp} v0, v1
 v3 = iconst.i8 0
 v4 = icmp.{cc} v2, v3
 return v4
}}"
            ));
            assert!(optimized.to_string().contains(&format!("fcmp.{fp}")));
            assert!(optimized.to_string().contains(&format!("icmp.{cc}")));
        }
    }
    let (_, optimized) = optimize(
        "func @f(i64) -> i8 {
block0(v0: i64):
 v1 = trunc.i8 v0
 brif v1, block1, block2
block1:
 v2 = iconst.i8 1
 return v2
block2:
 v3 = iconst.i8 0
 return v3
}",
    );
    assert!(optimized.to_string().contains("trunc.i8"));
    assert!(!optimized.to_string().contains("brif v0,"));
}

#[test]
fn boolean_rewrite_retains_trapping_producers() {
    let (_, optimized) = optimize(
        "func @f(i8, i8) -> i8 {
block0(v0: i8, v1: i8):
 v2 = udiv v0, v1
 v3 = uext.i64 v2
 v4 = iconst.i64 0
 v5 = icmp.ne v3, v4
 return v5
}",
    );
    assert!(optimized.to_string().contains("udiv"));
    assert!(!optimized.to_string().contains("uext"));
}

#[test]
fn direct_i128_branch_keeps_high_half_truth() {
    let source = "func @f(i128) -> i8 {
block0(v0: i128):
 brif v0, block1, block2
block1:
 v1 = iconst.i8 1
 return v1
block2:
 v2 = iconst.i8 0
 return v2
}";
    let (original, optimized) = optimize(source);
    assert!(optimized.to_string().contains("brif v0,"));
    for input in inputs(128) {
        assert_eq!(
            execute(&original.functions[0], &[input]),
            u128::from(input != 0)
        );
        assert_eq!(
            execute(&optimized.functions[0], &[input]),
            u128::from(input != 0)
        );
    }
}
