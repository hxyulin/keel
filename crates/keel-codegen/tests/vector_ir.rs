use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn vector_values_compile_on_both_native_targets() {
    for source in [
        "func @identity(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n return v0\n}",
        "func @local(ptr) {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n store v1, v0\n return\n}",
        "decl @external(f32x4) -> f32x4",
        "func @extract(ptr) -> i32 {\nblock0(v0: ptr):\n v1 = load.i32x4 v0\n v2 = vextract v1, 0\n return v2\n}",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
            keel_codegen::compile(&module, target).unwrap();
            let p = keel_codegen::compile_program(&module, target).unwrap();
            assert_eq!(p.stats().text_bytes, 0);
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Fast,
            };
            keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
            let p =
                keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
            assert_eq!(p.stats().text_bytes, 0);
        }
    }
}

#[test]
fn vector_aggregate_abi_is_rejected_until_sseup_classification_exists() {
    for source in [
        "decl @byval(ptr byval {f32x4})",
        "decl @sret(ptr sret {i64x2})",
        "decl @nested(ptr byval {[2 x i8x16]})",
    ] {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for error in [
            keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap_err(),
            keel_codegen::compile_program(&m, Target::X86_64LinuxGnu).unwrap_err(),
        ] {
            assert!(error.message.contains("SSEUP"), "{error}");
        }
    }
}
