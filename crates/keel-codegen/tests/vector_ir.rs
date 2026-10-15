use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn unlowered_vectors_are_diagnosed_before_native_allocation() {
    for source in [
        "func @identity(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n return v0\n}",
        "func @local(ptr) {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n store v1, v0\n return\n}",
        "decl @external(f32x4) -> f32x4",
        "decl @aggregate(ptr byval {i64, f64x2})",
        "func @extract(ptr) -> i32 {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n v2 = vextract v1, 0\n return v2\n}",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
            let error = keel_codegen::compile(&module, target).unwrap_err();
            assert!(error.message.contains("SIMD vector IR"), "{error}");
            let error = keel_codegen::compile_program(&module, target).unwrap_err();
            assert!(error.message.contains("SIMD vector IR"), "{error}");
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Fast,
            };
            assert!(
                keel_codegen::compile_with_pipeline(&module, target, &pipeline)
                    .unwrap_err()
                    .message
                    .contains("SIMD vector IR")
            );
            assert!(
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                    .unwrap_err()
                    .message
                    .contains("SIMD vector IR")
            );
        }
    }
}
