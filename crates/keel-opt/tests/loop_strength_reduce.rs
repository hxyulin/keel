mod support;
use keel_ir::{BinaryOp, InstData, Type};

fn source(ty: Type, step: i64) -> String {
    format!(
        "func @hot({ty}, {ty}, {ty}, {ty}) -> {ty} cc(c) {{
block0(v0: {ty}, v1: {ty}, v2: {ty}, v3: {ty}):
 v4 = iconst.{ty} 0
 jump block1(v2, v3, v4)
block1(v5: {ty}, v6: {ty}, v7: {ty}):
 v8 = icmp.ult v7, v0
 brif v8, block2, block3(v6)
block2:
 v9 = imul v5, v1
 v10 = iadd v6, v9
 v11 = iconst.{ty} {step}
 v12 = iadd v5, v11
 v13 = iconst.{ty} 1
 v14 = iadd v7, v13
 jump block1(v12, v10, v14)
block3(v15: {ty}):
 return v15
}}"
    )
}
fn optimize(s: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["loop-strength-reduce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}

#[test]
fn modular_recurrences_match_independent_oracle_across_wrap_and_zero_trip() {
    for ty in [Type::I32, Type::I64] {
        let mask = if ty == Type::I32 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };
        for step in [-7, -1, 0, 1, 3] {
            let s = source(ty, step);
            let before = keel_ir::parse(&s).unwrap();
            let after = optimize(&s);
            let f = &after.functions[0];
            let body = f.blocks().nth(2).unwrap();
            assert!(!f.block_insts(body).iter().any(|&i| matches!(
                f.inst_data(i),
                InstData::Binary {
                    op: BinaryOp::Imul,
                    ..
                }
            )));
            for stride in [0, 1, 3, 7, u32::MAX as u64, 1 << 31, 1 << 63, u64::MAX] {
                for start in [0u64, 1, u32::MAX as u64 - 1, (1 << 63) - 1, u64::MAX] {
                    for n in [0, 1, 2, 3, 7, 16, 31] {
                        let mut x = start & mask;
                        let mut expected = 17u64;
                        for _ in 0..n {
                            expected = expected.wrapping_add(x.wrapping_mul(stride)) & mask;
                            x = x.wrapping_add(step as u64) & mask;
                        }
                        let args = [n, stride, start, 17];
                        assert_eq!(support::execute(&before.functions[0], &args), expected);
                        assert_eq!(support::execute(f, &args), expected);
                    }
                }
            }
            let mut twice = after.clone();
            keel_opt::run_passes(&mut twice, &["loop-strength-reduce"]).unwrap();
            assert_eq!(twice.to_string(), after.to_string());
        }
    }
}

#[test]
fn varying_multiplier_dynamic_steps_and_unsupported_shapes_are_unchanged() {
    let s = source(Type::I64, 1);
    for input in [
        s.replace("imul v5, v1", "imul v5, v6"),
        s.replace("iadd v5, v11", "iadd v5, v1"),
        s.replace("brif v8, block2, block3(v6)", "brif v8, block3(v6), block2"),
        s.replace(
            " jump block1(v2, v3, v4)",
            " brif v0, block1(v2, v3, v4), block3(v3)",
        ),
        source(Type::I16, 1),
    ] {
        let before = keel_ir::parse(&input).unwrap();
        let after = optimize(&input);
        assert_eq!(after.to_string(), before.to_string());
    }
}

#[test]
fn body_division_stays_guarded_on_zero_trip_and_traps_on_taken_path() {
    let s = source(Type::I64, 1).replace(
        " v10 = iadd v6, v9",
        " v16 = udiv v9, v4\n v10 = iadd v6, v16",
    );
    let before = keel_ir::parse(&s).unwrap();
    let after = optimize(&s);
    for m in [&before, &after] {
        assert_eq!(support::execute(&m.functions[0], &[0, 9, 3, 17]), 17);
        assert!(
            std::panic::catch_unwind(|| support::execute(&m.functions[0], &[1, 9, 3, 17])).is_err()
        );
    }
}

// Address-like recurrence: keep the signed extension after the original-width
// wrap. A wide accumulator makes an incorrect extension-before-add observable.
fn offset_source(ty: Type) -> String {
    format!(
        "func @offset({ty}, {ty}, {ty}, {ty}) -> i64 cc(c) {{
block0(v0: {ty}, v1: {ty}, v2: {ty}, v3: {ty}):
 v4 = iconst.{ty} 0
 v5 = iconst.i64 17
 jump block1(v2, v4, v5)
block1(v6: {ty}, v7: {ty}, v8: i64):
 v9 = icmp.ult v7, v0
 brif v9, block2, block3(v8)
block2:
 v10 = iadd v6, v3
 {extend}
 v12 = iadd v8, {term}
 v13 = iadd v6, v1
 v14 = iconst.{ty} 1
 v15 = iadd v7, v14
 jump block1(v13, v15, v12)
block3(v16: i64):
 return v16
}}",
        extend = if ty != Type::I64 {
            "v11 = sext.i64 v10"
        } else {
            ""
        },
        term = if ty != Type::I64 { "v11" } else { "v10" },
    )
}

#[test]
fn offset_translation_removes_work_without_adding_live_recurrences() {
    for ty in [Type::I32, Type::I64] {
        for source in [
            offset_source(ty),
            offset_source(ty).replace("iadd v6, v3", "iadd v3, v6"),
        ] {
            let before = keel_ir::parse(&source).unwrap();
            let after = optimize(&source);
            let f = &after.functions[0];
            let body = f.blocks().nth(2).unwrap();
            assert_eq!(f.block_params(f.blocks().nth(1).unwrap()).len(), 3);
            assert_eq!(
                f.block_insts(body).len() + 1,
                before.functions[0].block_insts(body).len()
            );
            assert_eq!(f.num_values(), before.functions[0].num_values() + 1);
            for start in [0u64, 1, 0x7fff_ffff, 0xffff_ffff, i64::MAX as u64, u64::MAX] {
                for offset in [0u64, 1, 7, 0x8000_0000, 0xffff_ffff, u64::MAX] {
                    for delta in [0u64, 1, 3, 0x8000_0001, u64::MAX - 6] {
                        for n in [0, 1, 2, 3, 16, 31] {
                            let mut r = start;
                            let mut expected = 17u64;
                            for _ in 0..n {
                                let address = r.wrapping_add(offset);
                                let signed = if ty == Type::I32 {
                                    address as i32 as i64 as u64
                                } else {
                                    address
                                };
                                expected = expected.wrapping_add(signed);
                                r = r.wrapping_add(delta);
                            }
                            let args = [n, delta, start, offset];
                            assert_eq!(support::execute(&before.functions[0], &args), expected);
                            assert_eq!(support::execute(f, &args), expected);
                        }
                    }
                }
            }
            let mut twice = after.clone();
            keel_opt::run_passes(&mut twice, &["loop-strength-reduce"]).unwrap();
            assert_eq!(after.to_string(), twice.to_string());
        }
    }
}

#[test]
fn untranslated_uses_varying_offsets_and_observable_next_values_reject_translation() {
    let s = offset_source(Type::I32);
    for source in [
        s.replace("icmp.ult v7, v0", "icmp.ult v6, v0"),
        s.replace("iadd v6, v3", "iadd v6, v7"),
        s.replace("v11 = sext.i64 v10", "v11 = sext.i64 v6"),
        s.replace("v11 = sext.i64 v10", "v11 = sext.i64 v13")
            .replace(" v13 = iadd v6, v1", "")
            .replace(
                " v10 = iadd v6, v3",
                " v13 = iadd v6, v1\n v10 = iadd v6, v3",
            ),
        s.replace(
            "v11 = sext.i64 v10",
            "v17 = iadd v6, v2\n v11 = sext.i64 v17",
        ),
        s.replace("jump block1(v13, v15, v12)", "jump block1(v13, v13, v12)"),
        offset_source(Type::I16),
    ] {
        let before = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&before).unwrap();
        assert_eq!(optimize(&source).to_string(), before.to_string());
    }
}

#[test]
fn offset_translation_preserves_zero_trip_and_guarded_traps() {
    let s = offset_source(Type::I32)
        .replace(
            "v12 = iadd v8, v11",
            "v17 = udiv v11, v5\n v12 = iadd v8, v17",
        )
        .replace("iconst.i64 17", "iconst.i64 0");
    let before = keel_ir::parse(&s).unwrap();
    let after = optimize(&s);
    assert_ne!(before.to_string(), after.to_string());
    for module in [&before, &after] {
        assert_eq!(support::execute(&module.functions[0], &[0, 1, 0, 1]), 0);
        assert!(
            std::panic::catch_unwind(|| support::execute(&module.functions[0], &[1, 1, 0, 1]))
                .is_err()
        );
    }
}

#[test]
fn in_memory_aliases_do_not_reinterpret_dead_or_untranslated_values() {
    for alias in ["parameter", "offset-result", "next-result"] {
        let mut m = keel_ir::parse(&offset_source(Type::I32)).unwrap();
        let f = &mut m.functions[0];
        let entry = f.entry_block().unwrap();
        let invariant = f.block_params(entry)[2];
        let header = f.blocks().nth(1).unwrap();
        let body = f.blocks().nth(2).unwrap();
        let value = match alias {
            "parameter" => f.block_params(header)[0],
            "offset-result" => f.first_result(f.block_insts(body)[0]),
            _ => f.first_result(f.block_insts(body)[3]),
        };
        f.replace_all_uses(value, invariant);
        keel_ir::verify_module(&m).unwrap();
        let before = format!("{m:?}");
        keel_opt::run_passes(&mut m, &["loop-strength-reduce"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(format!("{m:?}"), before, "{alias}");
    }
}

#[test]
fn retained_actual_matmul_folds_column_offset_before_unroll_and_rotation() {
    let source = include_str!("loop_strength_reduce/matmul.keel");
    let before = keel_ir::parse(source).unwrap();
    let after = optimize(source);
    let count = |m: &keel_ir::Module, op| {
        m.functions[0].layout_insts().filter(|&(_, i)|
        matches!(m.functions[0].inst_data(i), InstData::Binary {op: b, ..} if *b == op)).count()
    };
    let hot_adds = |m: &keel_ir::Module| {
        let f = &m.functions[0];
        let body = f
            .blocks()
            .find(|&b| {
                f.block_insts(b)
                    .iter()
                    .any(|&i| matches!(f.inst_data(i), InstData::Load { .. }))
            })
            .unwrap();
        f.block_insts(body)
            .iter()
            .filter(|&&i| {
                matches!(
                    f.inst_data(i),
                    InstData::Binary {
                        op: BinaryOp::Iadd,
                        ..
                    }
                )
            })
            .count()
    };
    assert_eq!(hot_adds(&after) + 1, hot_adds(&before));
    let params = |m: &keel_ir::Module| {
        m.functions[0]
            .blocks()
            .map(|b| m.functions[0].block_params(b).len())
            .sum::<usize>()
    };
    assert_eq!(params(&after), params(&before));
    // Only pure arithmetic and the initial edge argument may change. Preserve
    // each load/store/conversion and its position relative to other effects.
    let effects = |m: &keel_ir::Module| {
        m.functions[0]
            .layout_insts()
            .filter_map(|(_, i)| match m.functions[0].inst_data(i) {
                InstData::Load { ty, flags, .. } => Some(format!("load {ty:?} {flags:?}")),
                InstData::Store { flags, .. } => Some(format!("store {flags:?}")),
                InstData::Convert { op, ty, .. } => Some(format!("convert {op:?} {ty:?}")),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(effects(&after), effects(&before));
    let mut full = before;
    keel_opt::optimize_with_tier(&mut full, keel_opt::OptimizationTier::Full).unwrap();
    keel_ir::verify_module(&full).unwrap();
    assert_eq!(count(&full, BinaryOp::Iadd), 16);
}
