#[path = "../../keel-codegen/tests/fixtures/reduction_multi.rs"]
mod fixtures;
use keel_opt::VectorTarget;
fn vectorized(source: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["loop-reduce-multi"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
#[test]
fn independent_shared_load_sums_and_constant_count_have_separate_merges() {
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        let mut m = keel_ir::parse(&fixtures::source()).unwrap();
        let before = m.to_string();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-casts"], target).unwrap();
        assert_eq!(before, m.to_string());
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], target).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for f in &m.functions {
            let vector = f
                .blocks()
                .find(|&b| {
                    f.block_insts(b).iter().any(|&i| {
                        matches!(
                            f.inst_data(i),
                            keel_ir::InstData::Load {
                                ty: keel_ir::Type::Vector(_),
                                ..
                            }
                        )
                    })
                })
                .unwrap();
            // Countdown, 2 chunks x 3 recurrences, shared stream pointer.
            assert_eq!(f.block_params(vector).len(), 8);
        }
        let before = m.to_string();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], target).unwrap();
        assert_eq!(before, m.to_string());
    }
}
#[test]
fn coupled_recurrences_extra_traps_and_observable_stores_remain_scalar() {
    for source in [
        fixtures::source().replace("iadd v17,v9", "iadd v18,v9"),
        fixtures::source().replace("iadd v17,v9", "iadd v8,v9"),
        fixtures::source().replace("iadd v10,v20", "isub v10,v20"),
        fixtures::source().replace("v18 = iadd", "store v15,v6\n v18 = iadd"),
        fixtures::source().replace("v18 = iadd", "v40 = load.i64 v0\n v18 = iadd"),
    ] {
        let m = vectorized(&source);
        for f in &m.functions {
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| f.inst_data(i).is_vector_instruction()),
                "{}",
                f.name
            );
        }
    }
}

#[test]
fn mixed_width_unit_count_folds_but_nonunit_count_remains_scalar() {
    let source = fixtures::source();
    let source = source[source.find("func @multi_64").unwrap()..]
        .to_owned()
        .replace("i64,i64,i64,ptr)", "i64,i64,i32,ptr)")
        .replace("v5: i64", "v5: i32")
        .replace("v10: i64", "v10: i32")
        .replace("v20 = iconst.i64", "v20 = iconst.i32");
    for unit in [true, false] {
        let source = if unit {
            source.clone()
        } else {
            source.replace("v20 = iconst.i32 1", "v20 = iconst.i32 2")
        };
        let m = vectorized(&source);
        assert_eq!(
            m.functions[0]
                .layout_insts()
                .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction()),
            unit,
            "{m}"
        );
    }
}

#[test]
fn recurrence_budget_accepts_four_and_rejects_five() {
    for count in [4, 5] {
        // Keep the counter nonunit and every added sum observable, so trial
        // cleanup cannot fold the count or delete an unused recurrence.
        let mut source = fixtures::source()
            .replace("v20 = iconst.i32 1", "v20 = iconst.i32 2")
            .replace("v20 = iconst.i64 1", "v20 = iconst.i64 2");
        let mut entry = String::new();
        let mut back = String::new();
        let mut updates = String::new();
        for extra in 0..count - 3 {
            let id = 80 + extra * 2;
            entry.push_str(",v3");
            back.push_str(&format!(",v{}", id + 1));
            updates.push_str(&format!(" v{} = iadd v{id},v15\n", id + 1));
            source = source.replace(" return", &format!(" store v{id},v6\n return"));
            for width in [32, 64] {
                source = source.replace(
                    &format!("v10: i{width}"),
                    &format!("v10: i{width},v{id}: i{width}"),
                );
            }
        }
        source = source
            .replace(
                "jump block1(v2,v3,v4,v5)",
                &format!("jump block1(v2,v3,v4,v5{entry})"),
            )
            .replace(
                "jump block1(v23,v18,v19,v21)",
                &format!("{updates} jump block1(v23,v18,v19,v21{back})"),
            );
        let m = vectorized(&source);
        for f in &m.functions {
            assert_eq!(
                f.layout_insts()
                    .any(|(_, i)| f.inst_data(i).is_vector_instruction()),
                count == 4
            );
        }
    }
}

#[test]
fn default_pipeline_vectorizes_offset_sum_before_pair_unrolling() {
    // Loop-reduce rejects the sign-extended offset; loop-unroll accepts it.
    // Unrolling first would leave only its remainder to vectorize, behind a
    // 16-element guard that remainder never passes.
    let source = "func @f(ptr, i64, i32) -> i32 {
block0(v0: ptr, v1: i64, v2: i32):
    v3 = iconst.i32 0
    v4 = iconst.i64 0
    jump block1(v4, v3)
block1(v5: i64, v8: i32):
    v7 = icmp.slt v5, v1
    brif v7, block2, block3
block2:
    v11 = sext.i64 v2
    v12 = iadd v5, v11
    v13 = iconst.i64 4
    v14 = imul v12, v13
    v15 = ptradd v0, v14
    v16 = load.i32 v15
    v17 = iadd v8, v16
    v18 = iconst.i64 1
    v19 = iadd v5, v18
    jump block1(v19, v17)
block3:
    return v8
}
";
    let mut m = keel_ir::parse(source).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let (mut scalar, mut vector) = (0, 0);
    for (_, i) in f.layout_insts() {
        if let keel_ir::InstData::Load { ty, .. } = f.inst_data(i) {
            match ty {
                keel_ir::Type::Vector(_) => vector += 1,
                _ => scalar += 1,
            }
        }
    }
    // One scalar tail load: no two-lane copy of the loop.
    assert_eq!((scalar, vector), (1, 4), "{m}");
}
