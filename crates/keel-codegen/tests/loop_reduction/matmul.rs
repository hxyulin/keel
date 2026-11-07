use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_opt::VectorTarget;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use std::{fs, process::Command};

fn module() -> keel_ir::Module {
    let mut m = keel_ir::parse(include_str!(
        "../../../keel-opt/tests/loop_vectorize/matmul.keel"
    ))
    .unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-matmul"], VectorTarget::Aarch64Neon).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(m.functions.len(), 2);
    m
}
#[test]
fn matrix_recipe_has_identical_text_and_typed_objects() {
    let m = module();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &super::TINY] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                assert_eq!(
                    keel_asm::assemble_object(&text).unwrap(),
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                );
            }
            // Explicit vector IR remains portable even though recognition is ARM-only.
            let pools = &keel_codegen::x86_64::REGISTERS;
            let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
            assert_eq!(
                keel_asm::x86::assemble_object_with_features(&text, Default::default()).unwrap(),
                keel_asm::x86::assemble_program_object(&program).unwrap()
            );
        }
    }
}
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_matrix_preserves_alias_order_wrapping_and_guarded_tails() {
    let m = module();
    let dir = std::env::temp_dir().join(format!("keel-matmul-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("../fixtures/matmul_guarded.c"),
    )
    .unwrap();
    let build = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &super::TINY] {
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                fs::write(
                    dir.join("loops.o"),
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                )
                .unwrap();
                let build = Command::new("clang")
                    .arg(dir.join("oracle.o"))
                    .arg(dir.join("loops.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(
                    build.status.success(),
                    "{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                let run = Command::new(dir.join("test")).output().unwrap();
                assert!(
                    run.status.success(),
                    "{:?}: {}",
                    run.status,
                    String::from_utf8_lossy(&run.stderr)
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
