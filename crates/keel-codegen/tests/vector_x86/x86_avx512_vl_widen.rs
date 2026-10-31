#[path = "../common/avx512_vl_widen.rs"]
mod common;
#[test]
fn vl_widen_scalar_vendor_low_elements_live_sources_and_masks() {
    let (ir, _, _) = common::fixtures();
    let mut module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for f in &module.functions {
        for (_, i) in f.layout_insts() {
            if let keel_ir::InstData::Intrinsic { id, .. } = f.inst_data(i) {
                let d = id.x86_avx512_vl_widen().unwrap();
                assert_eq!(d.source.bytes(), 16);
                assert!(id.is_x86_target());
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
    common::check(&module, "vl-widen-ir");
}
#[test]
fn vl_widen_requires_function_local_f_and_vl_with_exact_source_shapes() {
    let (ir, _, _) = common::fixtures();
    for feature in ["avx512f", "sse3", "-avx512vl"] {
        let text = ir.replacen(
            "target_features(\"avx512vl\")",
            &format!("target_features(\"{feature}\")"),
            1,
        );
        let mut module = keel_ir::parse(&text).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        assert!(keel_codegen::x86_64::compile(&module).is_err());
    }
    let module = keel_ir::parse(&ir.replacen("v5=load.i8x16", "v5=load.i8x32", 1)).unwrap();
    assert!(keel_ir::verify_module(&module).is_err());
}
