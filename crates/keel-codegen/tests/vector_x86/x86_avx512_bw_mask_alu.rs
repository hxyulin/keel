//! Native BW scalar-mask operations preserve full dword/qword values.
#[path = "../common/avx512_bw_mask_alu.rs"]
mod common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
#[test]
fn bw_mask_alu_native_flags_unpacks_high_bits_liveness_and_spills() {
    let (source, harness) = common::fixtures(false);
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    common::run(&module, &harness, "native");
}
#[test]
fn bw_mask_alu_contracts_controls_and_function_permissions() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_bw_mask_alu().is_some())
        .collect();
    assert_eq!(ids.len(), 24);
    for id in ids {
        assert!(id.info().pure);
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(!inst.may_trap() && !inst.is_pinned());
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
    }
    let (source, _) = common::fixtures(false);
    for optimize in [false, true] {
        // An unrelated permitted function must not authorize the unannotated caller.
        let text = source.replacen(" target_features(\"avx512bw\")", "", 1);
        let mut module = keel_ir::parse(&text).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE
                .with_features("avx512f,avx512dq,avx512vl")
                .unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline
            )
            .is_err()
        );
    }
    for ty in ["i32", "i64"] {
        let suffix = if ty == "i32" { "d" } else { "q" };
        for value in ["-1", "256"] {
            let source = format!(
                "func @bad({ty}) target_features(\"avx512bw\") {{\nblock0(v0:{ty}):\n v1=iconst.i32 {value}\n v2=intrinsic x86_avx512bw_kshiftl{suffix}(v0,v1)\n return\n}}\n"
            );
            assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
        }
        let source = format!(
            "func @bad({ty},i32) target_features(\"avx512bw\") {{\nblock0(v0:{ty},v1:i32):\n v2=intrinsic x86_avx512bw_kshiftl{suffix}(v0,v1)\n return\n}}\n"
        );
        assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
    }
}
