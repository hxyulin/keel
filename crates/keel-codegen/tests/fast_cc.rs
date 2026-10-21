use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::{CodegenPipeline, SelectionTier};

const SOURCE: &str = include_str!("../../../suite/llvm_import_fast_fixtures/fast.keel");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn private_fast_calls_cover_stack_args_recursion_and_spills() {
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for arm in [true, false] {
                let (text, program) = if arm {
                    (
                        keel_codegen::aarch64::compile_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                        keel_codegen::aarch64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                } else {
                    (
                        keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap(),
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                };
                let (parsed, typed) = if arm {
                    (
                        keel_asm::assemble_object(&text).unwrap(),
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                    )
                } else {
                    (
                        keel_asm::x86::assemble_object(&text).unwrap(),
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                    )
                };
                assert_eq!(parsed, typed);
            }
        }
    }
}
