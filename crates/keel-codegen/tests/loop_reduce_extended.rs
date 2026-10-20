use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[path = "fixtures/reduction_casts.rs"]
mod casts;
#[path = "fixtures/reduction_multi.rs"]
mod multi;
fn source() -> String {
    format!(
        "{}{}{}{}",
        include_str!("fixtures/reduction_affine.ir"),
        casts::source(),
        multi::source(),
        include_str!("fixtures/reduction_logic.ir").replace("load.i64 v16", "load.i64 notrap v16")
    )
}
fn module(tier: keel_opt::OptimizationTier) -> keel_ir::Module {
    let mut m = keel_ir::parse(&source()).unwrap();
    // Apply the new named pass first: tier passes may recognize the scalar
    // tail, but must preserve the extended pass's guards and seed merge.
    keel_opt::run_passes(
        &mut m,
        &[
            "loop-reduce-affine",
            "loop-reduce-casts",
            "loop-reduce-multi",
        ],
    )
    .unwrap();
    keel_opt::optimize_with_tier(&mut m, tier).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.functions.iter().all(|f| {
        f.layout_insts()
            .any(|(_, i)| f.inst_data(i).is_vector_instruction())
    }));
    m
}
#[test]
fn extended_reductions_compile_to_both_target_objects() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let m = module(tier);
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selection,
                        allocator: &allocation,
                    };
                    let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
                    let p =
                        keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
                    let evidence = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join("reduce-extended-static");
                    std::fs::create_dir_all(&evidence).unwrap();
                    let name = format!("{tier:?}-{target:?}-{selection:?}-{allocation:?}");
                    std::fs::write(evidence.join(format!("{name}.s")), &text).unwrap();
                    std::fs::write(evidence.join(format!("{name}.keel")), m.to_string()).unwrap();
                    match target {
                        Target::Aarch64AppleDarwin => {
                            assert!(text.contains(".2d"), "{text}");
                            assert_eq!(
                                keel_asm::assemble_object(&text).unwrap(),
                                keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                            );
                        }
                        Target::X86_64LinuxGnu => {
                            assert!(text.contains("paddq"), "{text}");
                            assert_eq!(
                                keel_asm::x86::assemble_object(&text).unwrap(),
                                keel_asm::x86::assemble_program_object(&p).unwrap()
                            );
                        }
                    }
                }
            }
        }
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_affine_offsets_and_wrap_fallback_match_independent_oracle() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reduce-extended");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        format!(
            "{}\n{}\n{}\n{}",
            include_str!("fixtures/reduction_logic.c"),
            include_str!("fixtures/reduction_multi.c"),
            include_str!("fixtures/reduction_casts.c"),
            include_str!("fixtures/reduction_affine.c")
                .replace("return 0;", "return casts_oracle(region, page) || multi_oracle(region, page) || logic_oracle(region, page);")
        ),
    )
    .unwrap();
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let m = module(tier);
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selection,
                    allocator: &allocation,
                };
                let p = keel_codegen::compile_program_with_pipeline(
                    &m,
                    Target::Aarch64AppleDarwin,
                    &pipeline,
                )
                .unwrap();
                fs::write(
                    dir.join("test.o"),
                    keel_asm::assemble_program_object(&p, Default::default()).unwrap(),
                )
                .unwrap();
                let output = Command::new("clang")
                    .arg("-O0")
                    .arg("-fno-lto")
                    .arg(dir.join("driver.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(dir.join("test")).status().unwrap().success(),
                    "{tier:?}/{selection:?}/{allocation:?}"
                );
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn extended_passes_preserve_first_fault_between_two_read_streams() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reduce-extended-faults");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("fixtures/reduction_fault_order.c"),
    )
    .unwrap();
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(include_str!("fixtures/reduction_fault_order.ir")).unwrap();
        keel_opt::run_passes(
            &mut m,
            &[
                "loop-reduce-affine",
                "loop-reduce-casts",
                "loop-reduce-multi",
            ],
        )
        .unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            !m.functions[0]
                .layout_insts()
                .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocation,
            };
            let program = keel_codegen::compile_program_with_pipeline(
                &m,
                Target::Aarch64AppleDarwin,
                &pipeline,
            )
            .unwrap();
            fs::write(
                dir.join("test.o"),
                keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
            )
            .unwrap();
            let output = Command::new("clang")
                .arg("-O0")
                .arg("-fno-lto")
                .arg(dir.join("driver.c"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                Command::new(dir.join("test")).status().unwrap().code(),
                Some(20)
            );
        }
    }
}
