use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier, x86_features::FeatureSelector,
};
#[test]
fn features_are_explicit_and_reject_the_wrong_target() {
    let module =
        keel_ir::parse("func @f() -> i64 {\nblock0:\n v0 = iconst.i64 1\n return v0\n}").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::all(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let p = keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
        .unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    assert!(
        keel_codegen::compile_program_with_pipeline(&module, Target::Aarch64AppleDarwin, &pipeline)
            .unwrap_err()
            .message
            .contains("x86 target features require")
    );
}

#[test]
fn effective_capabilities_distinguish_transport_recipe_and_preference() {
    use keel_asm_ir::x86::CpuLevel;
    use keel_codegen::x86_features::vector_capabilities;
    for (features, value, recipe) in [
        (Features::legacy(), 128, 128),
        (Features::for_level(CpuLevel::V1), 128, 128),
        (Features::for_level(CpuLevel::V2), 128, 128),
        (Features::for_level(CpuLevel::V3), 256, 256),
        (
            Features::for_level(CpuLevel::V3)
                .with_features("-avx2")
                .unwrap(),
            256,
            128,
        ),
        (
            Features::for_level(CpuLevel::V1)
                .with_features("+avx,+avx2")
                .unwrap(),
            256,
            256,
        ),
        (
            Features::for_level(CpuLevel::V1)
                .with_features("+aes,+pclmulqdq")
                .unwrap(),
            128,
            128,
        ),
    ] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features,
        };
        let caps = vector_capabilities(Target::X86_64LinuxGnu, &selector).unwrap();
        assert_eq!(caps.value_bits(), value);
        assert_eq!(caps.signed_i32_sum_bits(), recipe);
        assert_eq!(caps.prefer_wide_signed_i32_sum(), recipe == 256);
        if features.explicit {
            assert!(vector_capabilities(Target::Aarch64LinuxGnu, &selector).is_err());
        } else {
            assert_eq!(vector_capabilities(Target::Aarch64LinuxGnu, &selector).unwrap(), keel_ir::VectorCapabilities::BASELINE);
        }
    }
    let mut invalid = Features::for_level(CpuLevel::V1);
    invalid.avx2 = true;
    assert!(
        vector_capabilities(
            Target::X86_64LinuxGnu,
            &FeatureSelector {
                inner: &SelectionTier::Full,
                features: invalid
            }
        )
        .is_err()
    );
}
