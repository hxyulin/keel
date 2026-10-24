use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier, Target};

fn unavailable(error: impl std::fmt::Display) {
    assert!(
        error
            .to_string()
            .contains("unsupported target wasm32-unknown-unknown"),
        "{error}"
    );
}

// A caller's permissive selector must not authorize a native bridge to Wasm.
struct Permissive;
impl InstructionSelector for Permissive {
    fn name(&self) -> &'static str {
        "permissive-test"
    }
    fn validate_target(&self, _: Target) -> Result<(), &'static str> {
        Ok(())
    }
    fn select(
        &self,
        _: &keel_ir::Function,
        _: keel_codegen::select::Target,
        _: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        panic!("Wasm rejection must precede instruction selection")
    }
}

#[test]
fn native_codegen_rejects_wasm_without_invoking_selector_or_sink() {
    let target = Target::Wasm32UnknownUnknown;
    for source in [
        "",
        "func @answer() -> i32 {\nblock0:\n v0 = iconst.i32 42\n return v0\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        unavailable(keel_codegen::compile(&module, target).unwrap_err());
        unavailable(
            keel_codegen::compile_with_options(
                &module,
                target,
                keel_codegen::SelectionOptions::ALL,
            )
            .unwrap_err(),
        );
        unavailable(keel_codegen::compile_program(&module, target).unwrap_err());
        for selector in [
            &SelectionTier::Baseline as &dyn InstructionSelector,
            &Permissive,
        ] {
            let pipeline = CodegenPipeline {
                selector,
                allocator: &keel_codegen::regalloc::AllocationTier::Full,
            };
            unavailable(
                keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap_err(),
            );
            unavailable(
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                    .unwrap_err(),
            );
            let mut chunks = 0;
            unavailable(
                keel_codegen::compile_program_chunks_with_pipeline(
                    &module,
                    target,
                    &pipeline,
                    &mut |_| chunks += 1,
                )
                .unwrap_err(),
            );
            assert_eq!(chunks, 0);
            unavailable(
                keel_codegen::x86_features::vector_capabilities(target, selector).unwrap_err(),
            );
            let features = keel_codegen::x86_features::FeatureSelector {
                inner: selector,
                features: keel_codegen::x86_features::Features::default(),
            };
            unavailable(features.validate_target(target).unwrap_err());
        }
    }
    for cpu in [
        keel_codegen::tuning::TuningCpu::Generic,
        keel_codegen::tuning::TuningCpu::AppleM5,
    ] {
        unavailable(cpu.validate_target(target).unwrap_err());
    }
    unavailable(SelectionTier::Baseline.validate_target(target).unwrap_err());
}
