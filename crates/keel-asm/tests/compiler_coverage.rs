//! Every compiler fixture must assemble through Keel, in addition to the
//! compiler suites that use Clang for reference assembly and execution.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use keel_codegen::{Pools, aarch64::REGISTERS};
use std::{path::Path, process::Command};

#[test]
fn all_backend_fixtures_assemble_in_normal_tiny_and_optimized_configurations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let tiny = Pools {
        caller_saved: &[9, 10],
        callee_saved: &[19, 20],
        float_caller_saved: &[19, 20],
        float_callee_saved: &[8, 9],
    };
    let mut count = 0;
    for path in std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
    {
        let source = std::fs::read_to_string(&path).unwrap();
        if source
            .lines()
            .find_map(|s| s.strip_prefix("; target: "))
            .is_some_and(|t| t != "aarch64-apple-darwin")
        {
            continue;
        }
        use keel_codegen::{SelectionTier, regalloc::AllocationTier};
        for (pools, opt, selection, allocation) in [
            (
                REGISTERS,
                false,
                SelectionTier::Baseline,
                AllocationTier::Full,
            ),
            (tiny, false, SelectionTier::Baseline, AllocationTier::Full),
            (
                REGISTERS,
                true,
                SelectionTier::Baseline,
                AllocationTier::Full,
            ),
            (REGISTERS, false, SelectionTier::Full, AllocationTier::Full),
            (tiny, true, SelectionTier::Full, AllocationTier::Fast),
        ] {
            let mut module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            if opt {
                keel_opt::optimize(&mut module).unwrap();
            }
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let source =
                keel_codegen::aarch64::compile_with_pipeline(&module, &pools, &pipeline).unwrap();
            let reference = keel_asm::assemble_object(&source).unwrap_or_else(|e| {
                panic!("{} optimize={opt} pools={pools:?}: {e}", path.display())
            });
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&module, &pools, &pipeline)
                    .unwrap();
            let has_inline_asm = module.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::InlineAsm { .. }))
            });
            if !has_inline_asm {
                assert_eq!(
                    program.stats().text_bytes,
                    0,
                    "{} must emit no generated text",
                    path.display()
                );
            }
            let direct =
                keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
                    .unwrap_or_else(|e| {
                        panic!("{} structured optimize={opt}: {e}", path.display())
                    });
            assert_eq!(
                direct,
                reference,
                "{} structured optimize={opt}",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count > 100, "fixture discovery failed");
}
#[test]
fn all_c_fixtures_assemble_with_and_without_optimization_and_debug_info() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-c/tests/programs");
    let mut count = 0;
    for path in std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
    {
        let preprocessed = Command::new("clang")
            .args(["-E", "-std=c11"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(preprocessed.status.success());
        let source = String::from_utf8(preprocessed.stdout).unwrap();
        for debug_info in [false, true] {
            for opt in [false, true] {
                let mut module = keel_c::compile(
                    &source,
                    path.to_str().unwrap(),
                    keel_c::Options {
                        debug_info,
                        ..Default::default()
                    },
                )
                .unwrap();
                keel_ir::verify_module(&module).unwrap();
                if opt {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let source =
                    keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin)
                        .unwrap();
                let reference = keel_asm::assemble_object(&source).unwrap_or_else(|e| {
                    panic!("{} optimize={opt} debug={debug_info}: {e}", path.display())
                });
                let program = keel_codegen::compile_program(
                    &module,
                    keel_codegen::Target::Aarch64AppleDarwin,
                )
                .unwrap();
                let has_inline_asm = module.functions.iter().any(|f| {
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::InlineAsm { .. }))
                });
                if !has_inline_asm {
                    assert_eq!(
                        program.stats().text_bytes,
                        0,
                        "{} must emit no generated text",
                        path.display()
                    );
                }
                let direct = keel_asm::assemble_program_object(
                    &program,
                    keel_asm::TargetFeatures::default(),
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "{} structured optimize={opt} debug={debug_info}: {e}",
                        path.display()
                    )
                });
                assert_eq!(
                    direct,
                    reference,
                    "{} optimize={opt} debug={debug_info}",
                    path.display()
                );
                count += 1;
            }
        }
    }
    assert!(count > 40, "fixture discovery failed");
}
