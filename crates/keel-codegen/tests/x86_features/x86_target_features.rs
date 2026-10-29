use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{Module, Target};

fn program(
    module: &Module,
    features: Features,
) -> Result<keel_asm_ir::Program, keel_codegen::CodegenError> {
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    keel_codegen::compile_program_with_pipeline(
        module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
}

#[test]
fn local_permissions_control_lowering_and_encoding_without_granting_neighbors() {
    let source = "func @wide(ptr,ptr) target_features(\"avx\") {\nblock0(v0:ptr,v1:ptr):\n v2 = load.f32x8 align 1 v0\n v3 = intrinsic x86_avx_vsqrtps256(v2)\n store align 1 v3,v1\n return\n}\nfunc @dispatch(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n call @wide(v0,v1)\n return\n}\n";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let code = program(&module, Features::NONE).unwrap();
    assert!(keel_asm::x86::assemble_program_object_with_features(&code, Features::NONE).is_err());
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx && !encoding.avx2);
    keel_asm::x86::assemble_program_object_with_features(&code, encoding).unwrap();
    let without = keel_ir::parse(&source.replace(" target_features(\"avx\")", "")).unwrap();
    assert!(program(&without, Features::NONE).is_err());
    let disabled = keel_ir::parse(&source.replace(
        "target_features(\"avx\")",
        "target_features(\"-avx2,-avx\")",
    ))
    .unwrap();
    assert!(
        program(
            &disabled,
            Features::for_level(keel_codegen::x86_features::CpuLevel::V3)
        )
        .is_err()
    );
    for target in [Target::Aarch64LinuxGnu, Target::Aarch64AppleDarwin] {
        let error = keel_codegen::compile(&module, target).unwrap_err();
        assert!(
            error.message.contains("x86 function target features"),
            "{error:?}"
        );
    }
}

#[test]
fn direct_call_wide_transport_requires_both_ends_to_enable_avx() {
    for attributes in ["", "target_features(\"-avx\")"] {
        let module=keel_ir::parse(&format!("decl @foreign(f32x8) -> f32x8 {attributes}\nfunc @caller(f32x8) -> f32x8 target_features(\"avx\") {{\nblock0(v0:f32x8):\n v1 = call @foreign(v0)\n return v1\n}}\n")).unwrap();
        assert!(program(&module, Features::NONE).is_err());
    }
    let module=keel_ir::parse("decl @foreign(f32x8) -> f32x8 target_features(\"avx\")\nfunc @caller(f32x8) -> f32x8 target_features(\"avx\") {\nblock0(v0:f32x8):\n v1 = call @foreign(v0)\n return v1\n}\n").unwrap();
    program(&module, Features::NONE).unwrap();
}
