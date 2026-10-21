//! Native and emission gates for source-width conditional widening reductions.
#[path = "../../keel-opt/tests/conditional_widen/fixture.rs"]
mod fixture;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_opt::VectorTarget;
use std::{fs, process::Command};
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};
fn module() -> keel_ir::Module {
    let mut m = keel_ir::parse(&fixture::source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], VectorTarget::Aarch64Neon).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
#[test]
fn both_targets_emit_identical_text_and_typed_objects() {
    let m = module();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &TINY] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                assert_eq!(program.stats().text_bytes, 0);
                assert_eq!(
                    keel_asm::assemble_object(&text).unwrap(),
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                );
            }
            // ARM-optimized IR must remain legal on SSE2 via legalization;
            // recognizing new source-width reductions on x86 stays disabled.
            let pools = &keel_codegen::x86_64::REGISTERS;
            let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
            assert_eq!(program.stats().text_bytes, 0);
            assert_eq!(
                keel_asm::x86::assemble_object_with_features(&text, Default::default()).unwrap(),
                keel_asm::x86::assemble_program_object(&program).unwrap()
            );
        }
    }
}
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_arm_guard_pages_tails_extrema_wrapping_seeds_and_spills() {
    let m = module();
    let dir = std::env::temp_dir().join(format!("keel-conditional-widen-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("../../keel-opt/tests/conditional_widen/oracle.c"),
    )
    .unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for tiny in [false, true] {
                let pools = if tiny {
                    &TINY
                } else {
                    &keel_codegen::aarch64::REGISTERS
                };
                let pipeline = CodegenPipeline {
                    selector: &selection,
                    allocator: &allocation,
                };
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                if tiny {
                    assert!(
                        text.lines()
                            .any(|l| l.contains("str\tq")
                                && (l.contains("[sp") || l.contains("[x29"))),
                        "must exercise vector spills"
                    );
                }
                let object = keel_asm::assemble_object(&text).unwrap();
                fs::write(dir.join("loops.o"), object).unwrap();
                let build = Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("oracle.c"))
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
                if let Some(evidence) = std::env::var_os("KEEL_CONDITIONAL_WIDEN_EVIDENCE") {
                    let evidence = std::path::PathBuf::from(evidence);
                    fs::create_dir_all(&evidence).unwrap();
                    fs::write(evidence.join("optimized.keel"), m.to_string()).unwrap();
                    fs::write(
                        evidence.join(format!("{selection:?}-{allocation:?}-tiny{tiny}.s")),
                        text,
                    )
                    .unwrap();
                    fs::write(
                        evidence.join("native.json"),
                        String::from_utf8(run.stdout).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
