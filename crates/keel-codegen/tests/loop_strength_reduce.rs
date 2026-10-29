//! Execute wrapping-recurrence oracles across tiers, selectors and spill pools.
use keel_codegen::{CodegenPipeline, Pools, SelectionTier, regalloc::AllocationTier};

#[cfg(target_arch = "aarch64")]
use keel_codegen::aarch64::{REGISTERS, compile_with_pipeline};
#[cfg(target_arch = "x86_64")]
use keel_codegen::x86_64::{REGISTERS, compile_with_pipeline};

#[cfg(target_arch = "aarch64")]
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};

#[test]
fn strength_reduced_objects_match_text_on_both_targets() {
    let mut module = keel_ir::parse(include_str!("programs/loop_strength_reduce.keel")).unwrap();
    keel_opt::run_passes(
        &mut module,
        &["loop-strength-reduce", "canonicalize", "instcombine", "dce"],
    )
    .unwrap();
    for target in keel_codegen::Target::NATIVE {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
                let program =
                    keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                        .unwrap();
                let (structured, parsed) = match target {
                    keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                    keel_codegen::Target::Aarch64LinuxGnu => (
                        keel_asm::assemble_elf_program_object(&program, Default::default())
                            .unwrap(),
                        keel_asm::assemble_elf_object(&text).unwrap(),
                    ),
                    keel_codegen::Target::Aarch64AppleDarwin => (
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                        keel_asm::assemble_object(&text).unwrap(),
                    ),
                    keel_codegen::Target::X86_64LinuxGnu => (
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                        keel_asm::x86::assemble_object(&text).unwrap(),
                    ),
                };
                assert_eq!(structured, parsed, "{target:?} {selector:?} {allocator:?}");
            }
        }
    }
}
#[cfg(target_arch = "x86_64")]
const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3, 12],
    float_caller_saved: &[8, 9],
    float_callee_saved: &[],
};

#[test]
fn native_strength_reduction_matches_wrapping_oracle() {
    use std::{fs, path::Path, process::Command};
    let source = include_str!("programs/loop_strength_reduce.keel");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-strength-reduce");
    fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.o");
    let asm = dir.join("test.s");
    let exe = dir.join("run");
    fs::write(
        dir.join("driver.c"),
        include_str!("programs/loop_strength_reduce.c"),
    )
    .unwrap();
    let output = Command::new("clang")
        .arg("-c")
        .arg(dir.join("driver.c"))
        .arg("-o")
        .arg(&driver)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut runs = 0;
    for (name, passes) in [
        ("strength", &["loop-strength-reduce"][..]),
        ("fast", keel_opt::OptimizationTier::Fast.pipeline()),
        ("full", keel_opt::OptimizationTier::Full.pipeline()),
        (
            "strength-cleanup",
            &[
                "loop-strength-reduce",
                "canonicalize",
                "instcombine",
                "dce",
                "gcm",
            ][..],
        ),
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_opt::run_passes(&mut module, passes).unwrap();
        keel_ir::verify_module(&module).unwrap();
        // Every pairing of selector, allocator and pool appears once.
        for (selector, allocator, pools) in [
            (SelectionTier::Baseline, AllocationTier::Full, &REGISTERS),
            (SelectionTier::Baseline, AllocationTier::Fast, &TINY),
            (SelectionTier::Full, AllocationTier::Full, &TINY),
            (SelectionTier::Full, AllocationTier::Fast, &REGISTERS),
        ] {
            let assembly = compile_with_pipeline(
                &module,
                pools,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                },
            )
            .unwrap();
            fs::write(&asm, assembly).unwrap();
            let output = Command::new("clang")
                .args([&asm, &driver])
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&exe).output().unwrap();
            assert!(
                output.status.success(),
                "{name}, {selector:?}, {allocator:?}, pools={pools:?}: {output:?}"
            );
            runs += 1;
        }
    }
    assert_eq!(runs, 16);
}
