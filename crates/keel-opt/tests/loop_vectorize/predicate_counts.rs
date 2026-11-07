use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

#[test]
fn materialized_comparisons_are_zero_or_one_not_vector_masks() {
    for (ty, bytes) in [("i32", 4), ("i64", 8)] {
        for cc in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            for (extension, scale) in [("uext", false), ("sext", false), ("uext", true)] {
                let materialize = if scale {
                    format!(
                        "v18 = {extension}.i32 v11\n v19 = iconst.i32 7\n v20 = imul v18, v19\n v12 = sext.i64 v20"
                    )
                } else {
                    format!("v12 = {extension}.i64 v11")
                };
                let source = format!(
                    "func @count(ptr, i64) -> i64 {{\nblock0(v0: ptr, v1: i64):\n v2 = iconst.i64 0\n jump block1(v2, v2)\nblock1(v3: i64, v4: i64):\n v5 = icmp.ult v3, v1\n brif v5, block2, block3\nblock2:\n v6 = iconst.i64 {bytes}\n v7 = imul v3, v6\n v8 = ptradd v0, v7\n v9 = load.{ty} trap_ub v8\n v10 = iconst.{ty} 3\n v11 = icmp.{cc} v9, v10\n {materialize}\n v13 = iadd v4, v12\n v14 = iconst.i64 1\n v15 = iadd v3, v14\n jump block1(v15, v13)\nblock3:\n return v4\n}}"
                );
                let mut before = keel_ir::parse(&source).unwrap();
                let file = before.add_file("counts.c");
                let loc = Some(keel_ir::SourceLoc {
                    file,
                    line: 17,
                    col: 9,
                    scope: None,
                });
                let f = &mut before.functions[0];
                let conversions: Vec<_> = f
                    .layout_insts()
                    .filter_map(|(_, i)| {
                        matches!(f.inst_data(i), keel_ir::InstData::Convert { arg, .. }
                        if f.value_type(*arg) == keel_ir::Type::I8)
                        .then_some(i)
                    })
                    .collect();
                for i in conversions {
                    f.set_inst_loc(i, loc);
                }

                for target in [
                    VectorTarget::Aarch64Neon,
                    VectorTarget::X86Sse2,
                    VectorTarget::Portable,
                ] {
                    let mut after = before.clone();
                    keel_opt::run_passes_for_target(&mut after, &["loop-reduce"], target).unwrap();
                    keel_ir::verify_module(&after).unwrap();
                    let packed = after.functions[0]
                        .layout_insts()
                        .any(|(_, i)| after.functions[0].inst_data(i).is_vector_instruction());
                    assert_eq!(
                        packed,
                        target == VectorTarget::Aarch64Neon && !(scale && ty == "i64"),
                        "{cc}, {ty}: {after}"
                    );
                    if packed {
                        let f = &after.functions[0];
                        let masks: Vec<_> = f
                            .layout_insts()
                            .filter_map(|(_, i)| {
                                matches!(
                                    f.inst_data(i),
                                    keel_ir::InstData::VectorBinary {
                                        op: keel_ir::VectorBinaryOp::And,
                                        ..
                                    }
                                )
                                .then_some(i)
                            })
                            .collect();
                        assert!(!masks.is_empty());
                        for i in masks {
                            assert_eq!(f.inst_loc(i), loc);
                        }
                    }
                    let values = [
                        i64::MIN,
                        i64::MAX,
                        i32::MIN as i64,
                        i32::MAX as i64,
                        -1,
                        0,
                        1,
                        2,
                        3,
                        4,
                        64,
                    ];
                    let data: Vec<u8> = (0..65)
                        .flat_map(|i| values[i % values.len()].to_le_bytes()[..bytes].to_vec())
                        .collect();
                    for n in [0, 1, 7, 8, 9, 15, 16, 17, 31, 65] {
                        let expected = (0..n)
                            .filter(|i| {
                                let x = values[i % values.len()];
                                let x = if ty == "i32" { x as i32 as i64 } else { x };
                                let unsigned = if ty == "i32" {
                                    x as u32 as u64
                                } else {
                                    x as u64
                                };
                                match cc {
                                    "eq" => x == 3,
                                    "ne" => x != 3,
                                    "slt" => x < 3,
                                    "sle" => x <= 3,
                                    "sgt" => x > 3,
                                    "sge" => x >= 3,
                                    "ult" => unsigned < 3,
                                    "ule" => unsigned <= 3,
                                    "ugt" => unsigned > 3,
                                    "uge" => unsigned >= 3,
                                    _ => unreachable!(),
                                }
                            })
                            .count();
                        let mut interpreter = Interpreter::new(&after);
                        let pointer = if n == 0 {
                            0
                        } else {
                            interpreter.memory_mut().alloc_bytes(&data[..n * bytes], 16)
                        };
                        assert_eq!(
                            interpreter.call("count", &[Val::Ptr(pointer), Val::I64(n as u64)]),
                            Outcome::Return(vec![Val::I64(
                                expected as u64 * if scale { 7 } else { 1 }
                            )]),
                            "{ty}, {cc}, {extension}, {target:?}, n={n}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn narrow_load_comparisons_read_only_active_elements_and_keep_signedness() {
    for (memory_ty, bytes) in [("i8", 1), ("i16", 2)] {
        for extension in ["sext", "uext"] {
            for cc in ["eq", "ne", "slt", "ult"] {
                let source = format!(
                    "func @count(ptr, i64) -> i64 {{\nblock0(v0: ptr, v1: i64):\n v2 = iconst.i64 0\n jump block1(v2, v2)\nblock1(v3: i64, v4: i64):\n v5 = icmp.ult v3, v1\n brif v5, block2, block3\nblock2:\n v6 = iconst.i64 {bytes}\n v7 = imul v3, v6\n v8 = ptradd v0, v7\n v9 = load.{memory_ty} v8\n v10 = {extension}.i32 v9\n v11 = iconst.i32 3\n v12 = icmp.{cc} v10, v11\n v13 = uext.i64 v12\n v14 = iadd v4, v13\n v15 = iconst.i64 1\n v16 = iadd v3, v15\n jump block1(v16, v14)\nblock3:\n return v4\n}}"
                );
                let source = if bytes == 1 {
                    source
                        .replace("v7 = imul v3, v6", "v7 = iconst.i64 0")
                        .replace("v8 = ptradd v0, v7", "v8 = ptradd v0, v3")
                } else {
                    source
                };
                let mut module = keel_ir::parse(&source).unwrap();
                keel_opt::run_passes_for_target(
                    &mut module,
                    &["loop-reduce"],
                    VectorTarget::Aarch64Neon,
                )
                .unwrap();
                keel_ir::verify_module(&module).unwrap();
                assert!(module.to_string().contains("widen_high.i64x2"), "{module}");
                let values = [0u16, 1, 2, 3, 4, 127, 128, 255, 32767, 32768, 65535];
                for n in [0, 1, 7, 8, 9, 15, 16, 17, 65] {
                    let raw: Vec<_> = (0..n)
                        .flat_map(|i| values[i % values.len()].to_le_bytes()[..bytes].to_vec())
                        .collect();
                    let expected = (0..n)
                        .filter(|&i| {
                            let v = values[i % values.len()];
                            let x = match (bytes, extension) {
                                (1, "sext") => v as i8 as i32,
                                (1, _) => v as u8 as i32,
                                (_, "sext") => v as i16 as i32,
                                _ => v as i32,
                            };
                            match cc {
                                "eq" => x == 3,
                                "ne" => x != 3,
                                "slt" => x < 3,
                                "ult" => (x as u32) < 3,
                                _ => unreachable!(),
                            }
                        })
                        .count();
                    let mut interpreter = Interpreter::new(&module);
                    let pointer = if n == 0 {
                        0
                    } else {
                        interpreter.memory_mut().alloc_bytes(&raw, 1)
                    };
                    assert_eq!(
                        interpreter.call("count", &[Val::Ptr(pointer), Val::I64(n as u64)]),
                        Outcome::Return(vec![Val::I64(expected as u64)]),
                        "{memory_ty}, {extension}, {cc}, n={n}"
                    );
                }
            }
        }
    }
}
