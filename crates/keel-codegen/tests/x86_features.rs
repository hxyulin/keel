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
