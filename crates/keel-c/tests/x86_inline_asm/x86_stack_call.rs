use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{AsmOperand, InstData, Module, Target};

const SOURCE: &str = include_str!("../fixtures/x86_stack_call.c");

fn compile(source: &str) -> Result<Module, String> {
    keel_c::compile(
        source,
        "stack-call.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn balanced_stack_calls_keep_the_reserved_register_and_call_barrier() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    // Configurations often produce identical objects; run each distinct one.
    let mut distinct = std::collections::HashSet::new();
    for optimize in [false, true] {
        let mut module = compile(SOURCE).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let reparsed = keel_ir::parse(&module.to_string()).unwrap();
        keel_ir::verify_module(&reparsed).unwrap();
        let calls = module
            .functions
            .iter()
            .flat_map(|f| {
                f.layout_insts().filter_map(|(_, i)| {
                    if let InstData::InlineAsm { asm, args } = f.inst_data(i)
                        && let Some(index) = asm.stack_pointer_input().unwrap()
                    {
                        assert!(keel_ir::is_stack_pointer_snapshot(f, args[index]));
                        assert!(
                            !asm.outputs
                                .iter()
                                .any(|(op, _)| *op == AsmOperand::StackPointer)
                        );
                        Some(())
                    } else {
                        None
                    }
                })
            })
            .count();
        assert!(calls >= 8);
        for (cpu, level) in [("v1", CpuLevel::V1), ("v3", CpuLevel::V3)] {
            let features = Features::for_level(level);
            for selection in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                let selector = FeatureSelector {
                    inner: &selection,
                    features,
                };
                for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    for (pool, pools) in [
                        ("normal", &keel_codegen::x86_64::REGISTERS),
                        ("empty", &empty),
                    ] {
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        let bytes = keel_asm::x86::assemble_program_object_with_features(
                            &program, features,
                        )
                        .unwrap();
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        assert!(
                            text.contains("movq %rsp, stack_before(%rip)"),
                            "physical RSP substitution missing"
                        );
                        assert_eq!(
                            bytes,
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                        );
                        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                            &module,
                            pools,
                            &pipeline,
                            &mut |p| chunks.push(p),
                        )
                        .unwrap();
                        assert_eq!(bytes, chunks.finish().unwrap());
                        if distinct.insert(bytes.clone()) {
                            let name =
                                format!("{cpu}-{selection:?}-{allocator:?}-{pool}-opt{optimize}");
                            let file = format!("{name}.o");
                            let features: &[&str] = if level == CpuLevel::V3 {
                                &["avx2"]
                            } else {
                                &[]
                            };
                            keel_test_support::native_program!(name)
                                .requires(features)
                                .file(
                                    "main.c",
                                    include_str!("../fixtures/x86_stack_call_runner.c"),
                                )
                                .file(
                                    "callees.s",
                                    include_str!("../fixtures/x86_stack_call_callees.s"),
                                )
                                .file(&*file, bytes)
                                .link(["-O2", "main.c", "callees.s", &file])
                                .stdout("reserved-stack-call PASS\n")
                                .check();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn reserved_stack_sharing_rejects_write_conflicts_and_rsp_views_remain_full_width() {
    for body in [
        r#"unsigned long out; asm("" : "+r"(sp), "=&D"(out) : "D"(x));"#,
        r#"unsigned long a,b; asm("" : "+r"(sp), "=D"(a), "=D"(b));"#,
        r#"unsigned long out; asm("" : "+r"(sp), "=D"(out) : "D"(x) : "rdi");"#,
    ] {
        let source =
            format!("register unsigned long sp asm(\"rsp\"); void f(unsigned long x) {{ {body} }}");
        let module = compile(&source).unwrap();
        let error = keel_codegen::compile(&module, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(error.contains("overlaps"), "{source}: {error}");
    }
    for modifier in ['b', 'w', 'k', 'a'] {
        let source = format!(
            "register unsigned long sp asm(\"rsp\"); void f(void) {{asm(\"mov %{modifier}0, %%rax\" : \"+r\"(sp));}}"
        );
        let module = compile(&source).unwrap();
        let error = keel_codegen::compile(&module, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(error.contains("64-bit RSP view"), "{source}: {error}");
    }
}
