use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn compile(source: &str, features: Features, pools: &Pools) -> String {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    assert_eq!(
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
        keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap()
    );
    text
}
#[test]
fn exact_width_transport_and_spills_use_one_simd_bank_and_aligned_frame() {
    for shape in keel_ir::VectorType::ALL_256 {
        let source = format!(
            "decl @clobber()\nfunc @copy(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n call @clobber()\n store align 1 v2,v1\n return\n}}"
        );
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            let text = compile(&source, Features::for_level(CpuLevel::V3), pools);
            assert!(text.contains("ymm"));
            assert!(text.contains("ymmword ptr"));
            assert!(
                text.contains("and\trsp, -32") || text.contains("and\trsp, -0x20"),
                "{text}"
            );
            assert!(text.contains("push\tr15"));
        }
        let module = keel_ir::parse(&source).unwrap();
        assert!(keel_codegen::x86_64::compile(&module).is_err());
        assert!(keel_codegen::aarch64::compile(&module).is_err());
    }
}
