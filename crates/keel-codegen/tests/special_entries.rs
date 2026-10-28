//! Entry contracts stay opaque until a backend implements their calling convention.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn definitions_declarations_and_callee_only_contracts_reject_before_output() {
    for policy in ["naked", "interrupt"] {
        for source in [
            format!("func @entry() {policy} {{\nblock0:\n return\n}}\n"),
            format!(
                "decl @entry() {policy}\nfunc @caller() {{\nblock0:\n call @entry()\n return\n}}\n"
            ),
        ] {
            let module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for target in Target::NATIVE {
                let pipeline = CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                };
                assert!(
                    keel_codegen::compile(&module, target)
                        .unwrap_err()
                        .message
                        .contains(policy)
                );
                assert!(
                    keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                        .unwrap_err()
                        .message
                        .contains(policy)
                );
                let mut called = false;
                let result = keel_codegen::compile_program_chunks_with_pipeline(
                    &module,
                    target,
                    &pipeline,
                    &mut |_| called = true,
                );
                assert!(result.unwrap_err().message.contains(policy));
                assert!(!called);
            }
        }
        let mut module = keel_ir::parse(
            "decl @entry()\nfunc @caller() {\nblock0:\n call @entry()\n return\n}\n",
        )
        .unwrap();
        let callee = module.functions[0].callees().next().unwrap().0;
        let attrs = &mut module.functions[0].callee_mut(callee).attrs;
        if policy == "naked" {
            attrs.naked = true;
        } else {
            attrs.interrupt = true;
        }
        assert!(
            keel_codegen::compile(&module, Target::X86_64LinuxGnu)
                .unwrap_err()
                .message
                .contains(policy)
        );
    }
}
