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
        features: Features::all_known(),
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
        (
            Features {
                popcnt: true,
                sse41: true,
                ..Features::NONE
            },
            128,
            128,
        ),
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
        assert_eq!(caps.preferred_bits(), 128);
        assert!(!caps.prefer_wide_signed_i32_sum());
        assert_eq!(
            caps.with_wide_preference(true).prefer_wide_signed_i32_sum(),
            recipe == 256
        );
        if !features.is_empty() {
            assert!(vector_capabilities(Target::Aarch64LinuxGnu, &selector).is_err());
        } else {
            assert_eq!(
                vector_capabilities(Target::Aarch64LinuxGnu, &selector).unwrap(),
                keel_ir::VectorCapabilities::BASELINE
            );
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

#[test]
fn normalized_effective_feature_fingerprint_preserves_modifiers_reproducibly() {
    use keel_asm_ir::x86::CpuLevel;
    use keel_codegen::x86_features::vector_capabilities;
    let project = |features| {
        vector_capabilities(
            Target::X86_64LinuxGnu,
            &FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            },
        )
        .unwrap()
    };
    let base = Features::for_level(CpuLevel::V3);
    let first = project(base.with_features("-avx2,+aes,+pclmulqdq").unwrap());
    let equivalent = project(base.with_features("+pclmulqdq,+aes,-avx2").unwrap());
    assert_eq!(first, equivalent);
    assert_eq!(first.preferred_bits(), 128);
    assert_eq!(first.signed_i32_sum_bits(), 128);
    assert_eq!(first.effective_x86_features() & (1 << 17), 0);
    assert_ne!(first.effective_x86_features() & 1, 0);
    assert_ne!(first.effective_x86_features() & (1 << 9), 0);
    assert_ne!(first.effective_x86_features() & (1 << 22), 0);
    assert_ne!(
        first.effective_x86_features(),
        project(base).effective_x86_features()
    );
}

#[test]
fn every_optional_extension_changes_the_effective_fingerprint() {
    use keel_codegen::x86_features::vector_capabilities;
    let base = Features::for_level(keel_asm_ir::x86::CpuLevel::V3);
    let project = |features| {
        vector_capabilities(
            Target::X86_64LinuxGnu,
            &FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            },
        )
        .unwrap()
    };
    for name in ["adx", "sha", "vaes", "vpclmulqdq"] {
        let enabled = base.with_features(name).unwrap();
        assert_ne!(
            project(base).effective_x86_features(),
            project(enabled).effective_x86_features(),
            "{name}"
        );
        assert_eq!(
            project(enabled).effective_x86_features(),
            enabled.fingerprint()
        );
    }
}

#[test]
fn invalid_custom_selector_is_rejected_before_emission() {
    use keel_codegen::InstructionSelector;
    struct Invalid;
    impl InstructionSelector for Invalid {
        fn name(&self) -> &'static str {
            "invalid"
        }
        fn x86_features(&self) -> Features {
            Features {
                avx: true,
                ..Features::NONE
            }
        }
        fn select(
            &self,
            _: &keel_ir::Function,
            _: keel_codegen::select::Target,
            _: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        ) -> keel_codegen::select::Selection {
            panic!("invalid features must be rejected before selection")
        }
    }
    let module = keel_ir::parse("func @f() {\nblock0:\n return\n}").unwrap();
    let pipeline = CodegenPipeline {
        selector: &Invalid,
        allocator: &AllocationTier::Full,
    };
    for error in [
        keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap_err(),
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap_err(),
    ] {
        assert!(error.to_string().contains("requires xsave"));
    }
}
