//! Shared register initialization preserves results and input lifetimes.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
};
use keel_ir::{InstData, Target};

const SOURCE: &str = include_str!("../fixtures/x86_asm_shared_registers.c");

fn compile(source: &str) -> keel_ir::Module {
    keel_c::compile(
        source,
        "shared-registers.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap()
}

struct Rematerialize;
impl RegisterAllocator for Rematerialize {
    fn name(&self) -> &'static str {
        "rematerialize"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_ir::EntityRef;
        let mut allocation = AllocationTier::Full.allocate(input);
        for (_, inst) in input.func.layout_insts() {
            if matches!(input.func.inst_data(inst), InstData::Iconst { ty: keel_ir::Type::I64, imm } if *imm >= 0)
            {
                for &value in input.func.inst_results(inst) {
                    if allocation.locs[0][value.index()].is_some() {
                        allocation.locs[0][value.index()] =
                            Some(keel_codegen::regalloc::Loc::Remat(value));
                    }
                }
            }
        }
        allocation
    }
}

#[test]
fn shared_inputs_survive_optimization_allocation_and_emission() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let export = std::env::var_os("KEEL_X86_SHARED_REGISTERS_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("source.c"), SOURCE).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(SOURCE);
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let module = keel_ir::parse(&module.to_string()).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for (allocator_name, allocator) in [
                ("fast", &AllocationTier::Fast as &dyn RegisterAllocator),
                ("full", &AllocationTier::Full),
                ("remat", &Rematerialize),
            ] {
                for (pool_name, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &empty),
                ] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator,
                    };
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    let object = keel_asm::x86::assemble_object(&text).unwrap();
                    let program = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    );
                    let mut chunks = keel_asm::x86::ProgramAssembler::new(Default::default());
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |p| chunks.push(p),
                    )
                    .unwrap();
                    assert_eq!(object, chunks.finish().unwrap());
                    if let Some(dir) = &export {
                        std::fs::write(
                            dir.join(format!(
                                "shared-{optimize}-{selector:?}-{allocator_name}-{pool_name}.o"
                            )),
                            object,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
    if let Some(dir) = &export {
        // The external execution harness must reject this control: discarding
        // high bits models loading a narrow initializer after the wide one.
        let broken = compile(&SOURCE.replace("addq $29,%0", "movzwq %w0,%0; addq $29,%0"));
        let program = keel_codegen::compile_program(&broken, Target::X86_64LinuxGnu).unwrap();
        std::fs::write(
            dir.join("wrong-wide-initializer.control"),
            keel_asm::x86::assemble_program_object(&program).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn conflicting_and_unproven_initializers_are_rejected() {
    for body in [
        r#"long out; asm("" : "=r"(out) : "0"(x), "0"(y));"#,
        r#"long out=x; asm("" : "+r"(out) : "0"(y));"#,
        r#"long out; asm("" : "=a"(out) : "a"(x), "a"(y));"#,
        r#"long out; asm("" : "=a"(out) : "0"(x), "a"(y));"#,
        r#"long out; asm("" : "=a"(out) : "a"(1), "a"(2));"#,
        r#"long out; asm("" : "=a"(out) : "a"(0x1234UL), "a"((unsigned char)0x35));"#,
        r#"long out; asm("" : "=a"(out) : "a"(*p), "a"(*p));"#,
    ] {
        for optimize in [false, true] {
            let source = format!("void f(long x,long y,volatile long *p){{{body}}}");
            let result = keel_c::compile(
                &source,
                "conflicting.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false,
                },
            );
            let mut module = match result {
                Ok(module) => module,
                Err(error) => {
                    assert!(
                        error.to_string().contains("conflicting or unproven"),
                        "{body}: {error}"
                    );
                    continue;
                }
            };
            // The verifier rejects explicit conflicting ties. Fixed register
            // grouping is target-specific and is checked by backend planning.
            if optimize && keel_ir::verify_module(&module).is_ok() {
                keel_opt::optimize(&mut module).unwrap();
            }
            let error = keel_codegen::compile(&module, Target::X86_64LinuxGnu)
                .unwrap_err()
                .to_string();
            assert!(error.contains("conflicting or unproven"), "{body}: {error}");
        }
    }
}

#[test]
fn write_lifetime_conflicts_are_rejected_but_explicit_early_ties_are_legal() {
    for body in [
        r#"long out; asm("" : "=&a"(out) : "a"(x));"#,
        r#"long a,b; asm("" : "=a"(a), "=a"(b));"#,
        r#"long out; asm("" : "=a"(out) : "a"(x) : "rax");"#,
        r#"long out; asm("" : "=&a"(out) : "0"(x), "a"(x));"#,
    ] {
        let module = compile(&format!("void f(long x){{{body}}}"));
        let error = keel_codegen::compile(&module, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(error.contains("overlaps"), "{body}: {error}");
    }
}
