#[path = "../common/avx512_vl_integer.rs"]
mod common;
#[test]
fn vl_integer_scalar_vendor_truth_tables_and_live_sources() {
    let (ir, _, _) = common::fixtures();
    let mut module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for f in &module.functions {
        for (_, i) in f.layout_insts() {
            if let keel_ir::InstData::Intrinsic { id, .. } = f.inst_data(i) {
                assert!(id.info().pure);
                assert_eq!(
                    f.inst_data(i).memory_effects(),
                    keel_ir::MemoryEffects::None
                );
                assert!(!f.inst_data(i).may_trap());
            }
        }
    }
    keel_opt::optimize(&mut module).unwrap();
    common::check(&module, "vl-integer-ir", None);
}
#[test]
fn vl_integer_controls_and_local_permissions_fail_closed() {
    let (source, _, _) = common::fixtures();
    for feature in ["avx512f", "sse3", "-avx512vl"] {
        let ir = source.replacen(
            "target_features(\"avx512vl\")",
            &format!("target_features(\"{feature}\")"),
            1,
        );
        let mut module = keel_ir::parse(&ir).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        assert!(keel_codegen::x86_64::compile(&module).is_err(), "{feature}");
    }
    let module =
        keel_ir::parse(&source.replacen("v10=iconst.i32 255", "v10=iconst.i32 256", 1)).unwrap();
    assert!(keel_ir::verify_module(&module).is_err());
}
