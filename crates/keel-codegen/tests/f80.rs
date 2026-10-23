#[test]
fn unsupported_targets_reject_f80_foreign_abi() {
    for src in [
        "decl @external(f80) -> f80\n",
        "decl @external(ptr byval {f80})\n",
        "func @constant() -> f80 {\nentry:\n v0 = fconst.f80 0x3fff, 0x8000000000000001\n return v0\n}\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            keel_codegen::aarch64::compile(&m)
                .unwrap_err()
                .message
                .contains("f80")
        );
        keel_codegen::x86_64::compile(&m).unwrap();
    }
}

#[test]
fn scalar_x87_transport_and_arithmetic() {
    let src = "decl @external(f80) -> f80\nfunc @arithmetic(f80, f80, ptr) -> f80 {\nentry(v0:f80,v1:f80,v2:ptr):\n v3 = fsub v0,v1\n v4 = fdiv v3,v1\n store align 1 v4,v2\n v5 = call @external(v4)\n return v5\n}\n";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let text = keel_codegen::x86_64::compile(&m).unwrap();
    assert!(text.contains("fsubp"), "{text}");
    assert!(text.contains("fdivp"), "{text}");
    assert!(text.contains("tbyte ptr"), "{text}");
    assert!(!text.contains("addsd") && !text.contains("divsd"));
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        &m,
        &keel_codegen::x86_64::REGISTERS,
        &keel_codegen::CodegenPipeline {
            selector: &keel_codegen::SelectionTier::Full,
            allocator: &keel_codegen::regalloc::AllocationTier::Full,
        },
    )
    .unwrap();
    keel_asm::x86::assemble_program_object(&program).unwrap();
    keel_asm::x86::assemble_object(&text).unwrap();
}

#[test]
fn values_matrix_and_export() {
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
    const TINY: Pools = Pools {
        callee_saved: &[3],
        float_caller_saved: &[0],
        ..EMPTY
    };
    let source = include_str!("f80/values.keel");
    let export = std::env::var_os("KEEL_F80_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    for (cpu, level) in [
        ("v1", CpuLevel::V1),
        ("v2", CpuLevel::V2),
        ("v3", CpuLevel::V3),
    ] {
        let features = Features::for_level(level);
        for (selection, tier) in [
            ("baseline", SelectionTier::Baseline),
            ("trees", SelectionTier::Trees),
            ("full", SelectionTier::Full),
        ] {
            for (allocation, allocator) in [
                ("fast", AllocationTier::Fast),
                ("full", AllocationTier::Full),
            ] {
                for (pressure, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &EMPTY),
                ] {
                    for optimized in [false, true] {
                        let name = format!(
                            "{cpu}-{selection}-{allocation}-{pressure}-{}",
                            if optimized { "opt" } else { "raw" }
                        );
                        let mut m = keel_ir::parse(source).unwrap();
                        keel_ir::verify_module(&m).unwrap();
                        if optimized {
                            keel_opt::optimize(&mut m).unwrap();
                        }
                        let selector = FeatureSelector {
                            inner: &tier,
                            features,
                        };
                        let pipeline = CodegenPipeline {
                            selector: &selector,
                            allocator: &allocator,
                        };
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)
                                .unwrap_or_else(|e| panic!("{name}: {e}"));
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &m, pools, &pipeline,
                        )
                        .unwrap();
                        let object = keel_asm::x86::assemble_program_object_with_features(
                            &program, features,
                        )
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                        assert_eq!(
                            object,
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
                            "{name}"
                        );
                        if let Some(dir) = &export {
                            std::fs::write(dir.join(format!("{name}.typed.o")), &object).unwrap();
                            std::fs::write(
                                dir.join(format!("{name}.text.o")),
                                keel_asm::x86::assemble_object_with_features(&text, features)
                                    .unwrap(),
                            )
                            .unwrap();
                            std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
                        }
                    }
                }
            }
        }
    }
}
