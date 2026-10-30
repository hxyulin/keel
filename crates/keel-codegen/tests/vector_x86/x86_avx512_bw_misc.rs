//! BW miscellaneous operations preserve low scalar and independent SAD lanes.
#[path = "../common/avx512_bw_misc.rs"]
mod common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
#[test]
fn bw_misc_native_absolute_broadcast_blend_sad_windows_aliases_and_spills() {
    let cases = common::cases();
    let (source, harness) = common::sources(&cases, false);
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    common::run(&module, &harness, "native");
}
#[test]
fn bw_misc_contracts_controls_and_encoded_width_features() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_bw_misc().is_some())
        .collect();
    assert_eq!(ids.len(), 24);
    let cases = common::cases();
    for id in ids {
        let d = id.x86_avx512_bw_misc().unwrap();
        let info = id.info();
        assert!(info.pure && id.is_x86_target());
        assert_eq!(info.returns, [Type::Vector(d.result)]);
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(!inst.may_trap() && !inst.is_pinned());
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
        let case = cases.iter().find(|c| c.id == id).unwrap();
        let (source, _) = common::sources(&[*case], false);
        let source = source
            .replace(" target_features(\"avx512bw\")", "")
            .replace(" target_features(\"avx512bw,avx512vl\")", "");
        let module = keel_ir::parse(&source).unwrap();
        for features in [
            Features::NONE
                .with_features("avx512f,avx512dq,avx512vl")
                .unwrap(),
            Features::NONE.with_features("avx512bw").unwrap(),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let result = keel_codegen::x86_64::compile_program_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            );
            assert_eq!(
                result.is_ok(),
                features.avx512bw && case.width == 512,
                "{}",
                info.name
            );
        }
    }
    for control in ["-1", "256"] {
        let source = format!(
            "func @bad(i16x8,i8,i8x16,i8x16) target_features(\"avx512bw,avx512vl\") {{\nblock0(v0:i16x8,v1:i8,v2:i8x16,v3:i8x16):\n v4=iconst.i32 {control}\n v5=intrinsic x86_avx512bw_dbsad128(v0,v1,v2,v3,v4)\n return\n}}\n"
        );
        assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
    }
    let source = "func @bad(i16x8,i8,i8x16,i8x16,i32) target_features(\"avx512bw,avx512vl\") {\nblock0(v0:i16x8,v1:i8,v2:i8x16,v3:i8x16,v4:i32):\n v5=intrinsic x86_avx512bw_dbsad128(v0,v1,v2,v3,v4)\n return\n}\n";
    assert!(keel_ir::verify_module(&keel_ir::parse(source).unwrap()).is_err());
}
