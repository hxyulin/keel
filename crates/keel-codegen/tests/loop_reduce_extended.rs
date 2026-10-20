use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

fn source() -> &'static str {
    include_str!("fixtures/reduction_affine.ir")
}
fn module(tier: keel_opt::OptimizationTier) -> keel_ir::Module {
    let mut m = keel_ir::parse(source()).unwrap();
    // Apply the new named pass first: tier passes may recognize the scalar
    // tail, but must preserve the extended pass's guards and seed merge.
    keel_opt::run_passes(&mut m, &["loop-reduce-affine"]).unwrap();
    keel_opt::optimize_with_tier(&mut m, tier).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .layout_insts()
            .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
    );
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
        include_str!("fixtures/reduction_affine.c"),
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
