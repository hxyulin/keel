//! Frontend declarations left behind by optimization consume no normal frame.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};
use keel_ir::InstData;

#[test]
fn optimized_c_dead_declarations_do_not_change_generated_code() {
    for target in Target::ALL {
        let compile_c = |count| {
            let source = format!("int f(int x) {{ int dead[{count}]; return x + 1; }}");
            let mut module = keel_c::compile(
                &source,
                "dead.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap();
            keel_opt::optimize(&mut module).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let f = &module.functions[0];
            assert!(
                f.num_stack_slots() >= 2,
                "frontend must retain declarations"
            );
            assert!(
                !f.layout_insts()
                    .any(|(_, inst)| matches!(f.inst_data(inst), InstData::StackAddr { .. })),
                "optimized fixture must have no executing slot references"
            );
            module
        };
        let small = compile_c(1);
        let large = compile_c(8192);
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                assert_eq!(
                    keel_codegen::compile_with_pipeline(&small, target, &pipeline).unwrap(),
                    keel_codegen::compile_with_pipeline(&large, target, &pipeline).unwrap(),
                );
            }
        }
    }
}
