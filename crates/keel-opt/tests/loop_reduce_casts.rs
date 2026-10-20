#[path = "../../keel-codegen/tests/fixtures/reduction_casts.rs"]
mod fixtures;
use keel_opt::VectorTarget;
#[test]
fn packed_roundtrips_support_each_width_and_signedness_on_both_targets() {
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        let mut m = keel_ir::parse(&fixtures::source()).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-affine"], target).unwrap();
        assert_eq!(m.to_string(), before);
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-casts"], target).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for f in &m.functions {
            assert!(
                f.layout_insts().any(|(_, i)| matches!(
                    f.inst_data(i),
                    keel_ir::InstData::VectorBinary {
                        op: keel_ir::VectorBinaryOp::And,
                        ..
                    }
                )),
                "{}",
                f.name
            );
        }
        let before = m.to_string();
        keel_opt::run_passes_for_target(&mut m, &["loop-reduce-casts"], target).unwrap();
        assert_eq!(m.to_string(), before);
    }
}
#[test]
fn reject_narrow_loads_other_casts_and_escaping_intermediates() {
    for source in [
        fixtures::source().replace("trunc.i8 v10", "trunc.i8 v5"),
        fixtures::source()
            .replace("load.i32 v9", "load.i32 volatile v9")
            .replace("load.i64 v9", "load.i64 volatile v9"),
        fixtures::source().replace("v13 = iadd v5,v12", "v13 = iadd v5,v10"),
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_opt::run_passes(&mut m, &["loop-reduce-casts"]).unwrap();
        for f in &m.functions {
            // Only i8 variants changed in the first case.
            if source.contains("trunc.i8 v5") && !f.name.contains("_8_") {
                continue;
            }
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| f.inst_data(i).is_vector_instruction()),
                "{}",
                f.name
            );
        }
    }
}
