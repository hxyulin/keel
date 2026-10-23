use keel_ir::{InstData, Type, VectorType};
use keel_opt::VectorTarget;

fn kernel(ty: Type, index: Type, op: &str, signed: bool) -> String {
    let bytes = ty.bytes().unwrap();
    let extend = if index == Type::I32 {
        " v9 = uext.i64 v5\n"
    } else {
        ""
    };
    let idx = if index == Type::I32 { "v9" } else { "v5" };
    let scale = if bytes == 1 {
        String::new()
    } else {
        format!(" v10 = iconst.i64 {bytes}\n v11 = imul {idx},v10\n")
    };
    let offset = if bytes == 1 { idx } else { "v11" };
    format!(
        "func @map(ptr,ptr,ptr,{index}) {{
block0(v0: ptr,v1: ptr,v2: ptr,v3: {index}):
 v4 = iconst.{index} 0
 jump block1(v4)
block1(v5: {index}):
 v6 = icmp.{} v5,v3
 brif v6,block2,block3
block2:
{extend}{scale} v12 = ptradd v0,{offset}
 v13 = ptradd v1,{offset}
 v14 = ptradd v2,{offset}
 v15 = load.{ty} v13
 v16 = load.{ty} v14
 v17 = {op} v15,v16
 store v17,v12
 v18 = iconst.{index} 1
 v19 = iadd v5,v18
 jump block1(v19)
block3:
 return
}}",
        if signed { "slt" } else { "ult" }
    )
}
fn optimize(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-vectorize"], target).unwrap();
    let text = m.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    m
}
fn packed(m: &keel_ir::Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| m.functions[0].inst_data(*i).is_vector_instruction())
        .count()
}
#[test]
fn vectorizes_counted_maps_preserves_scalar_tail_and_is_deterministic_and_idempotent() {
    // Byte-index addresses are deliberately expressed directly for i8.
    for shape in VectorType::ALL_128 {
        let ty = shape.lane_type();
        let op = if ty.is_float() { "fadd" } else { "iadd" };
        for index in [Type::I32, Type::I64] {
            for signed in [false, true] {
                let src = kernel(ty, index, op, signed);
                let m = optimize(&src, VectorTarget::Portable);
                assert_eq!(packed(&m), 1, "{shape}, {index}, {signed}");
                let text = m.to_string();
                assert!(text.contains(&format!("load.{shape}")));
                assert!(text.contains("load."));
                assert!(m.functions[0].layout_insts().any(
                    |(_, i)| matches!(m.functions[0].inst_data(i),InstData::Load{ty:t,..}if *t==ty)
                ));
                assert_eq!(text, optimize(&src, VectorTarget::Portable).to_string());
                let mut twice = m.clone();
                keel_opt::run_passes(&mut twice, &["loop-vectorize"]).unwrap();
                assert_eq!(text, twice.to_string());
            }
        }
    }
}
#[test]
fn target_cost_gate_avoids_scalarized_integer_multiply() {
    let source = kernel(Type::I32, Type::I64, "imul", false);
    assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 1);
    for target in [VectorTarget::Portable, VectorTarget::X86Sse2] {
        assert_eq!(packed(&optimize(&source, target)), 0);
    }
    let source = kernel(Type::I64, Type::I64, "imul", false);
    assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 0);
}

fn sum_loop(ty: &str) -> String {
    let (bytes, mask) = if ty == "i32" {
        (4, 0xff00ff)
    } else {
        (8, 0xff00ff00ff00ffi64)
    };
    format!(
        "func @sum(ptr,i64,{ty}) -> {ty} {{
block0(v0: ptr,v1: i64,v2: {ty}):
 v3 = iconst.i64 0
 jump block1(v3,v2)
block1(v4: i64,v5: {ty}):
 v6 = icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7 = iconst.i64 {bytes}
 v8 = imul v4,v7
 v9 = ptradd v0,v8
 v10 = load.{ty} v9
 v11 = iconst.{ty} {mask}
 v12 = and v10,v11
 v13 = iadd v5,v12
 v14 = iconst.i64 1
 v15 = iadd v4,v14
 jump block1(v15,v13)
block3:
 return v5
}}"
    )
}

fn reduce_sum(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

#[test]
fn read_only_wrapping_integer_sums_vectorize_and_keep_seed_once() {
    for ty in ["i32", "i64"] {
        for target in [
            VectorTarget::Portable,
            VectorTarget::Aarch64Neon,
            VectorTarget::X86Sse2,
        ] {
            for commute in [false, true] {
                let mut src = sum_loop(ty);
                if commute {
                    src = src.replace("iadd v5,v12", "iadd v12,v5");
                }
                assert_eq!(packed(&optimize(&src, target)), 0);
                let m = reduce_sum(&src, target);
                assert!(packed(&m) >= 2, "{m}");
                assert!(m.to_string().contains("vreduce_add"), "{m}");
                let mut twice = m.clone();
                keel_opt::run_passes_for_target(&mut twice, &["loop-reduce"], target).unwrap();
                assert_eq!(m.to_string(), twice.to_string());
            }
        }
    }
}

#[test]
fn sums_use_independent_accumulators_fixed_stream_offsets_and_a_countdown() {
    for ty in ["i32", "i64"] {
        let m = reduce_sum(&sum_loop(ty), VectorTarget::Aarch64Neon);
        let f = &m.functions[0];
        let vector = f
            .blocks()
            .find(|&b| {
                f.block_insts(b).iter().any(|&i| {
                    matches!(
                        f.inst_data(i),
                        InstData::Load {
                            ty: Type::Vector(_),
                            ..
                        }
                    )
                })
            })
            .unwrap();
        // Remaining element count, four independent sums, one stream pointer.
        assert_eq!(f.block_params(vector).len(), 6, "{m}");
        let mut reads = 0;
        let mut additions = 0;
        let mut countdowns = 0;
        for &i in f.block_insts(vector) {
            match f.inst_data(i) {
                InstData::Load {
                    ty: Type::Vector(_),
                    ..
                } => reads += 1,
                InstData::VectorBinary {
                    op: keel_ir::VectorBinaryOp::Iadd,
                    ..
                } => additions += 1,
                InstData::Binary {
                    op: keel_ir::BinaryOp::Isub,
                    ..
                } => countdowns += 1,
                InstData::Binary {
                    op: keel_ir::BinaryOp::Shl,
                    ..
                } => panic!("per-iteration address scaling: {m}"),
                _ => {}
            }
        }
        assert_eq!((reads, additions, countdowns), (4, 4, 1), "{m}");
        // Cheap trip-count rejection dominates every pointer-range check.
        let pre = f.blocks().next().unwrap();
        let InstData::Jump { dest } = f.inst_data(*f.block_insts(pre).last().unwrap()) else {
            panic!("{m}")
        };
        let guard = dest.block;
        assert!(
            !f.block_insts(guard).iter().any(|&i| matches!(
                f.inst_data(i),
                InstData::Convert {
                    op: keel_ir::ConvertOp::PtrToInt,
                    ..
                }
            )),
            "{m}"
        );
        assert!(matches!(
            f.inst_data(*f.block_insts(guard).last().unwrap()),
            InstData::Brif { .. }
        ));
    }
}

#[test]
fn profile_reductions_use_bounded_fast_and_general_full_policies() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(&sum_loop("i64")).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(packed(&m) > 0, "{tier:?}: {m}");
    }
    let large = format!(
        "decl @effect()\n{}",
        sum_loop("i64").replace(
            "jump block1(v3,v2)",
            &format!("{}jump block1(v3,v2)", "call @effect()\n ".repeat(300))
        )
    );
    let mut fast = keel_ir::parse(&large).unwrap();
    keel_opt::run_passes(&mut fast, &["fast-loop-reduce"]).unwrap();
    assert_eq!(packed(&fast), 0, "{fast}");
    assert!(packed(&reduce_sum(&large, VectorTarget::Portable)) > 0);
}

#[test]
fn sum_rejects_extra_effects_recurrences_nonunit_steps_and_float_reassociation() {
    let original = sum_loop("i64");
    for source in [
        original.replace("load.i64", "load.i64 volatile"),
        original.replace("v13 = iadd v5,v12", "v13 = isub v5,v12"),
        original.replace("v14 = iconst.i64 1", "v14 = iconst.i64 2"),
        original.replace("v12 = and v10,v11", "v12 = and v10,v5"),
        original.replace("v13 = iadd v5,v12", "store v12,v0\n v13 = iadd v5,v12"),
        format!(
            "decl @effect()\n{}",
            original.replace("v13 = iadd", "call @effect()\n v13 = iadd")
        ),
        original
            .replace("v3 = iconst.i64 0", "v3 = iconst.i64 -1")
            .replace("icmp.ult", "icmp.slt"),
        original
            .replace("v13 = iadd v5,v12", "v13 = fadd v5,v12")
            .replace(
                "v11 = iconst.i64 71777214294589695\n v12 = and v10,v11",
                "v12 = fconst.f64 1.0",
            )
            .replace("v2: i64", "v2: f64")
            .replace("v5: i64", "v5: f64")
            .replace("ptr,i64,i64) -> i64", "ptr,i64,f64) -> f64")
            .replace("load.i64", "load.f64"),
    ] {
        assert_eq!(
            packed(&reduce_sum(&source, VectorTarget::Aarch64Neon)),
            0,
            "{source}"
        );
    }
}

#[test]
fn sum_preserves_fault_order_unless_other_reads_explicitly_cannot_fault() {
    let src = include_str!("../../keel-codegen/tests/fixtures/reduction_fault_order.ir");
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        assert_eq!(packed(&reduce_sum(src, target)), 0);
        // A promise on B lets A remain the only possible faulting stream;
        // a promise on A likewise leaves only B. Do not infer either promise.
        for marked in [
            src.replace("load.i32 v13", "load.i32 notrap v13"),
            src.replace("load.i32 v20", "load.i32 notrap v20"),
        ] {
            assert!(packed(&reduce_sum(&marked, target)) > 0);
        }
    }
}

#[test]
fn dead_nontrapping_read_cleanup_unlocks_vectorization_without_relaxing_faults() {
    let original = kernel(Type::I32, Type::I64, "iadd", false);
    for (flags, vectorizes) in [("notrap", true), ("", false), ("volatile notrap", false)] {
        let source = original.replace(
            "store v17,v12",
            &format!("store v17,v12\n v50 = load.i32 {flags} v13\n v51 = iadd v50,v50"),
        );
        let mut m = keel_ir::parse(&source).unwrap();
        keel_opt::run_passes_for_target(
            &mut m,
            &["dce", "loop-vectorize"],
            VectorTarget::Aarch64Neon,
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(packed(&m) != 0, vectorizes, "{source}");
    }
}
#[test]
fn rejects_volatile_calls_nonunit_strides_offsets_reductions_and_escaping_values() {
    let source = kernel(Type::I32, Type::I64, "iadd", false);
    let cases = [
        source
            .replace(
                " jump block1(v4)",
                " v50 = iconst.i32 0\n jump block1(v4,v50)",
            )
            .replace("block1(v5: i64)", "block1(v5: i64,v51: i32)")
            .replace("v17 = iadd v15,v16", "v17 = iadd v15,v51")
            .replace("jump block1(v19)", "jump block1(v19,v17)"),
        source.replace("load.i32", "load.i32 volatile"),
        source.replace("store v17", "store volatile v17"),
        format!(
            "decl @effect()\n{}",
            source.replace(" store v17", " call @effect()\n store v17")
        ),
        source.replace("iconst.i64 1\n", "iconst.i64 2\n"),
        source.replace("iconst.i64 4\n", "iconst.i64 8\n"),
        source.replace("icmp.ult", "icmp.ule"),
        source.replace("block3:\n return", "block3:\n store v5,v0\n return"),
        source.replace("v17 = iadd v15,v16", "v17 = udiv v15,v16"),
    ];
    for src in cases {
        let m = optimize(&src, VectorTarget::Aarch64Neon);
        assert_eq!(packed(&m), 0, "{src}");
    }
}

#[test]
fn leaves_known_short_and_negative_loops_unchanged() {
    for n in [0, 1, 15, -1] {
        let source = kernel(Type::I32, Type::I64, "iadd", true)
            .replace(
                " v4 = iconst.i64 0",
                &format!(" v60 = iconst.i64 {n}\n v4 = iconst.i64 0"),
            )
            .replace("icmp.slt v5,v3", "icmp.slt v5,v60");
        assert_eq!(packed(&optimize(&source, VectorTarget::Aarch64Neon)), 0);
    }
}

#[test]
fn ignores_two_instruction_blocks_starting_with_a_store_or_void_call() {
    for first in ["store v2,v0", "call @effect()"] {
        let source = format!(
            "decl @effect()\nfunc @not_a_counted_loop(ptr,i64) {{\nblock0(v0: ptr,v1: i64):\n jump block1(v1)\nblock1(v2: i64):\n {first}\n brif v2,block1(v2),block2\nblock2:\n return\n}}"
        );
        let before = keel_ir::parse(&source).unwrap().to_string();
        let after = optimize(&source, VectorTarget::Portable);
        assert_eq!(before, after.to_string());
    }
}

#[test]
fn analyzes_later_loops_after_rejections_and_cfg_changes() {
    let mut source = kernel(Type::I32, Type::I64, "udiv", false);
    for loop_index in 1..=2 {
        let mut next = kernel(Type::I32, Type::I64, "iadd", false);
        next = next.lines().skip(1).collect::<Vec<_>>().join("\n");
        next = next.replace("block0(v0: ptr,v1: ptr,v2: ptr,v3: i64):", "block0:");
        for value in (4..=19).rev() {
            next = next.replace(
                &format!("v{value}"),
                &format!("v{}", value + loop_index * 20),
            );
        }
        for block in (0..=3).rev() {
            next = next.replace(
                &format!("block{block}"),
                &format!("block{}", block + loop_index * 4),
            );
        }
        source = source.replace(
            " return\n}",
            &format!(" jump block{}\n{next}", loop_index * 4),
        );
    }
    let m = optimize(&source, VectorTarget::Portable);
    assert_eq!(packed(&m), 2);
    let text = m.to_string();
    let mut twice = m.clone();
    keel_opt::run_passes(&mut twice, &["loop-vectorize"]).unwrap();
    assert_eq!(text, twice.to_string());
}

#[test]
fn vectorizes_invariant_starts_and_both_branch_orientations() {
    for shape in VectorType::ALL_128 {
        let ty = shape.lane_type();
        let op = if ty.is_float() { "fadd" } else { "iadd" };
        for index in [Type::I32, Type::I64] {
            for signed in [false, true] {
                for dynamic in [false, true] {
                    let mut source = kernel(ty, index, op, signed);
                    if dynamic {
                        source = source
                            .replace(
                                &format!("ptr,ptr,ptr,{index})"),
                                &format!("ptr,ptr,ptr,{index},{index})"),
                            )
                            .replace(
                                &format!("v3: {index}):"),
                                &format!("v3: {index},v40: {index}):"),
                            )
                            .replace("jump block1(v4)", "jump block1(v40)");
                    } else {
                        source = source.replace(
                            &format!("v4 = iconst.{index} 0"),
                            &format!("v4 = iconst.{index} 7"),
                        );
                    }
                    // Cover operand swapping independently of branch inversion.
                    let (lt, gt, ge, le) = if signed {
                        ("slt", "sgt", "sge", "sle")
                    } else {
                        ("ult", "ugt", "uge", "ule")
                    };
                    for (predicate, operands, inverse) in [
                        (lt, "v5,v3", false),
                        (gt, "v3,v5", false),
                        (ge, "v5,v3", true),
                        (le, "v3,v5", true),
                    ] {
                        let mut variant = source.replace(
                            &format!("icmp.{lt} v5,v3"),
                            &format!("icmp.{predicate} {operands}"),
                        );
                        if inverse {
                            variant =
                                variant.replace("brif v6,block2,block3", "brif v6,block3,block2");
                        }
                        let m = optimize(&variant, VectorTarget::Portable);
                        assert_eq!(packed(&m), 1, "{variant}");
                        let text = m.to_string();
                        let mut twice = m.clone();
                        keel_opt::run_passes(&mut twice, &["loop-vectorize"]).unwrap();
                        assert_eq!(text, twice.to_string());
                    }
                }
            }
        }
    }
}

#[test]
fn rejects_known_empty_short_and_negative_start_ranges() {
    for index in [Type::I32, Type::I64] {
        for signed in [false, true] {
            for (start, bound) in [(7, 7), (17, 7), (7, 22), (-1, 100), (i64::MAX, 0)] {
                let source = kernel(Type::I32, index, "iadd", signed)
                    .replace(
                        &format!("v4 = iconst.{index} 0"),
                        &format!("v40 = iconst.{index} {bound}\n v4 = iconst.{index} {start}"),
                    )
                    .replace("v5,v3", "v5,v40");
                assert_eq!(
                    packed(&optimize(&source, VectorTarget::Portable)),
                    0,
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn accepts_commuted_element_size_multiplication() {
    let source = kernel(Type::I32, Type::I64, "iadd", false).replace("imul v5,v10", "imul v10,v5");
    assert_eq!(packed(&optimize(&source, VectorTarget::Portable)), 1);
}

#[test]
fn conditional_reductions_use_packed_predicates_only_where_available() {
    let source = include_str!("conditional_reduce/loops.keel");
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        for predicate in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            let source = source.replace("icmp.slt v11", &format!("icmp.{predicate} v11"));
            let mut m = keel_ir::parse(&source).unwrap();
            keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], target).unwrap();
            keel_ir::verify_module(&m).unwrap();
            for (index, f) in m.functions.iter().enumerate() {
                let supported = matches!(target, VectorTarget::Aarch64Neon | VectorTarget::X86Sse2)
                    || (index == 0 && !predicate.starts_with('u'))
                    || matches!(predicate, "eq" | "ne");
                assert_eq!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::VectorIcmp { .. })),
                    supported,
                    "{target:?} {index} {predicate}"
                );
            }
            let text = m.to_string();
            keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], target).unwrap();
            assert_eq!(text, m.to_string());
        }
    }
}

#[test]
fn conditional_reductions_reject_coupled_values_fault_reordering_and_traps() {
    let source = include_str!("conditional_reduce/loops.keel");
    for rejected in [
        source.replace("select v12, v11, v13", "select v12, v11, v6"),
        source
            .replace(
                "v14 = select",
                "v18 = load.i32 v10\n v19 = icmp.ne v18, v13\n v14 = select",
            )
            .replace("select v12,", "select v19,"),
        source
            .replace("v14 = select", "v18 = sdiv v11, v2\n v14 = select")
            .replace("select v12, v11,", "select v12, v18,"),
        source
            .replace("load.i32 v10", "load.i32 volatile v10")
            .replace("load.i64 v10", "load.i64 volatile v10"),
    ] {
        let mut m = keel_ir::parse(&rejected).unwrap();
        // The mixed-width second-read mutation only needs the i32 function.
        if rejected.contains("v18 = load.i32") {
            m.functions.truncate(1);
        }
        keel_ir::verify_module(&m).unwrap();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], VectorTarget::Aarch64Neon)
            .unwrap();
        assert!(m.functions.iter().all(|f| {
            !f.layout_insts()
                .any(|(_, i)| f.inst_data(i).is_vector_instruction())
        }));
    }
}

#[test]
fn bounded_c_diamond_normalization_commits_only_successful_vectorization() {
    let source = include_str!("conditional_reduce/triangle.keel");
    let mut m = keel_ir::parse(source).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], VectorTarget::Aarch64Neon).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::VectorIcmp { .. }))
    );
    for rejected in [
        source.replace("load.i32 v12", "load.i32 volatile v12"),
        source
            .replace("i32", "i64")
            .replace("iconst.i64 2", "iconst.i64 3"),
    ] {
        let mut m = keel_ir::parse(&rejected).unwrap();
        let before = m.to_string();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], VectorTarget::Portable).unwrap();
        assert_eq!(before, m.to_string());
    }
}

#[test]
fn conditional_zero_arms_use_canonical_mask_and_but_other_arms_keep_lane_selects() {
    for zero in [true, false] {
        let source = include_str!("conditional_reduce/loops.keel");
        let source = if zero {
            source.to_string()
        } else {
            source
                .replace("v13 = iconst.i32 0", "v13 = iconst.i32 7")
                .replace("v13 = iconst.i64 0", "v13 = iconst.i64 7")
        };
        let mut m = keel_ir::parse(&source).unwrap();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], VectorTarget::Aarch64Neon)
            .unwrap();
        keel_ir::verify_module(&m).unwrap();
        for f in &m.functions {
            assert_eq!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::VectorSelect { .. })),
                !zero
            );
        }
    }
}

fn widening_sum(from: Type, to: Type, op: &str) -> String {
    let bytes = from.bytes().unwrap();
    let source = format!(
        "func @widen(ptr,i64,{to}) -> {to} {{
block0(v0: ptr,v1: i64,v2: {to}):
 v3 = iconst.i64 0
 jump block1(v3,v2)
block1(v4: i64,v5: {to}):
 v6 = icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7 = iconst.i64 {bytes}
 v8 = imul v4,v7
 v9 = ptradd v0,v8
 v10 = load.{from} v9
 v11 = {op}.{to} v10
 v12 = iadd v5,v11
 v13 = iconst.i64 1
 v14 = iadd v4,v13
 jump block1(v14,v12)
block3:
 return v5
}}"
    );
    if bytes == 1 {
        source
            .replace(" v8 = imul v4,v7\n", "")
            .replace("ptradd v0,v8", "ptradd v0,v4")
    } else {
        source
    }
}

#[test]
fn mixed_width_load_reductions_are_full_native_only_exact_footprint_and_idempotent() {
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        for (from, to) in [
            (Type::I8, Type::I32),
            (Type::I8, Type::I64),
            (Type::I16, Type::I32),
            (Type::I16, Type::I64),
            (Type::I32, Type::I64),
        ] {
            for op in ["sext", "uext"] {
                let source = widening_sum(from, to, op);
                let m = reduce_sum(&source, target);
                let text = m.to_string();
                let widen = if op == "sext" {
                    "swiden_low"
                } else {
                    "uwiden_low"
                };
                assert!(text.contains(widen), "{from} -> {to}: {text}");
                assert_eq!(text, reduce_sum(&source, target).to_string());
                let mut twice = m.clone();
                keel_opt::run_passes_for_target(&mut twice, &["loop-reduce"], target).unwrap();
                assert_eq!(text, twice.to_string());
                keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
                // Packed reads cover exactly destination_lanes source elements.
                let expected = 16 / to.bytes().unwrap() * from.bytes().unwrap();
                let f = &m.functions[0];
                for (_, i) in f.layout_insts() {
                    if let InstData::Load { ty, .. } = f.inst_data(i) {
                        assert!([Some(expected), from.bytes()].contains(&ty.bytes()));
                        assert!(!ty.is_vector());
                    }
                }
                for target in [VectorTarget::Portable] {
                    assert_eq!(packed(&reduce_sum(&source, target)), 0);
                }
                let mut fast = keel_ir::parse(&source).unwrap();
                keel_opt::run_passes_for_target(&mut fast, &["fast-loop-reduce"], target).unwrap();
                assert_eq!(packed(&fast), 0);
                let mut volatile_module = keel_ir::parse(&source).unwrap();
                let f = &mut volatile_module.functions[0];
                let load = f
                    .layout_insts()
                    .find(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
                    .unwrap()
                    .1;
                if let InstData::Load { flags, .. } = f.inst_data_mut(load) {
                    flags.volatile = true;
                }
                keel_opt::run_passes_for_target(&mut volatile_module, &["loop-reduce"], target)
                    .unwrap();
                assert_eq!(packed(&volatile_module), 0);
            }
        }
    }
}
#[test]
fn widening_never_commutes_across_narrow_wrapping_arithmetic_or_hidden_reads() {
    let source = widening_sum(Type::I16, Type::I64, "sext");
    for extra in [
        "v20 = iconst.i16 3\n v21 = imul v10,v20\n v11 = sext.i64 v21",
        "v20 = load.i16 v9\n v11 = sext.i64 v10",
    ] {
        let source = source.replace("v11 = sext.i64 v10", extra);
        assert_eq!(packed(&reduce_sum(&source, VectorTarget::Aarch64Neon)), 0);
    }
}
