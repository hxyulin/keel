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
fn mixed_width_count_recurrence_is_rejected() {
    let source = fixtures::source();
    let source = source[source.find("func @multi_64").unwrap()..]
        .to_owned()
        .replace("i64,i64,i64,ptr)", "i64,i64,i32,ptr)")
        .replace("v5: i64", "v5: i32")
        .replace("v10: i64", "v10: i32")
        .replace("v20 = iconst.i64", "v20 = iconst.i32");
    let m = vectorized(&source);
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
    );
}

#[test]
fn recurrence_budget_accepts_four_and_rejects_five() {
    for count in [4, 5] {
        let mut source = fixtures::source();
        let mut entry = String::new();
        let mut back = String::new();
        let mut updates = String::new();
        for extra in 0..count - 3 {
            let id = 80 + extra * 2;
            entry.push_str(",v3");
            back.push_str(&format!(",v{}", id + 1));
            updates.push_str(&format!(" v{} = iadd v{id},v15\n", id + 1));
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
