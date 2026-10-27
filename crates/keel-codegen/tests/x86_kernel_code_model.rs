use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_64::{self, CodeModel},
};
use object::{Object, ObjectSection};

const SOURCE: &str = "data @defined = align 8 { i64 0 }\ndecl data @external\nfunc @local_address() -> ptr {\nblock0:\n v0 = symbol_addr @defined\n return v0\n}\nfunc @external_address() -> ptr {\nblock0:\n v0 = symbol_addr @external\n return v0\n}\ndecl @hook() weak\nfunc @function_address() -> ptr {\nblock0:\n v0 = symbol_addr @hook\n return v0\n}\nfunc @self_address() -> ptr {\nblock0:\n v0 = symbol_addr @local_address\n return v0\n}";

#[test]
fn kernel_uses_absolute_relocations_in_text_typed_and_chunked_paths() {
    let module = keel_ir::parse(SOURCE).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&x86_64::REGISTERS, &empty] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let source = x86_64::compile_with_pipeline_and_code_model(
                    &module,
                    pools,
                    &pipeline,
                    CodeModel::Kernel,
                )
                .unwrap();
                let program = x86_64::compile_program_with_pipeline_and_code_model(
                    &module,
                    pools,
                    &pipeline,
                    CodeModel::Kernel,
                )
                .unwrap();
                let text = keel_asm::x86::assemble_object(&source).unwrap();
                let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(text, typed, "{selector:?}/{allocator:?}\n{source}");
                let mut chunks = keel_asm::x86::ProgramAssembler::new(Default::default());
                x86_64::compile_program_chunks_with_pipeline_and_code_model(
                    &module,
                    pools,
                    &pipeline,
                    CodeModel::Kernel,
                    &mut |chunk| chunks.push(chunk),
                )
                .unwrap();
                assert_eq!(typed, chunks.finish().unwrap());
                let file = object::File::parse(&*typed).unwrap();
                let relocations: Vec<_> = file
                    .section_by_name(".text")
                    .unwrap()
                    .relocations()
                    .map(|(_, r)| (r.kind(), r.size()))
                    .collect();
                assert_eq!(
                    relocations,
                    vec![(object::RelocationKind::Absolute, 64); 4],
                    "{source}"
                );
                let pic = x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                assert_eq!(
                    pic,
                    x86_64::compile_with_pipeline_and_code_model(
                        &module,
                        pools,
                        &pipeline,
                        CodeModel::PositionIndependent
                    )
                    .unwrap()
                );
                assert!(!pic.contains("movabs"));
                assert!(pic.contains("GOTPCREL"));
            }
        }
    }
}

#[test]
fn kernel_rejects_userspace_tls() {
    let module = keel_ir::parse("data @tls = thread_local align 8 { i64 0 }").unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Baseline,
        allocator: &AllocationTier::Full,
    };
    let error = x86_64::compile_with_pipeline_and_code_model(
        &module,
        &x86_64::REGISTERS,
        &pipeline,
        CodeModel::Kernel,
    )
    .unwrap_err();
    assert!(error.message.contains("userspace TLS"));
}
