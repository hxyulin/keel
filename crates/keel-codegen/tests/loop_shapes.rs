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
fn loop_shape_objects_match_text_on_both_targets() {
    let mut module = keel_ir::parse(include_str!("programs/loop_shapes.keel")).unwrap();
    keel_opt::run_passes(
        &mut module,
        &[
            "load-reuse",
            "loop-strength-reduce",
            "canonicalize",
            "instcombine",
            "dce",
            "loop-unroll",
            "loop-rotate",
        ],
    )
    .unwrap();
    for target in keel_codegen::Target::ALL {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
                let evidence =
                    std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-shapes-static");
                std::fs::create_dir_all(&evidence).unwrap();
                std::fs::write(
                    evidence.join(format!("{target:?}-{selector:?}-{allocator:?}.s")),
                    &text,
                )
                .unwrap();
                std::fs::write(evidence.join("transformed.keel"), module.to_string()).unwrap();
                let program =
                    keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                        .unwrap();
                let (structured, parsed) = match target {
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
fn native_loop_shapes_match_independent_oracle() {
    use std::{fs, path::Path, process::Command};
    let source = include_str!("programs/loop_shapes.keel");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-shapes");
    fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    let asm = dir.join("test.s");
    let exe = dir.join("run");
    fs::write(&driver, include_str!("programs/loop_shapes.c")).unwrap();
    let mut runs = 0;
    for (name, passes) in [
        ("none", keel_opt::OptimizationTier::None.pipeline()),
        ("shapes", &[][..]),
        ("fast", keel_opt::OptimizationTier::Fast.pipeline()),
        ("full", keel_opt::OptimizationTier::Full.pipeline()),
        ("cleanup", &["canonicalize", "instcombine", "dce"][..]),
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_opt::run_passes(&mut module, passes).unwrap();
        if name != "none" {
            keel_opt::run_passes(
                &mut module,
                &[
                    "load-reuse",
                    "loop-strength-reduce",
                    "canonicalize",
                    "instcombine",
                    "dce",
                    "loop-unroll",
                    "loop-rotate",
                ],
            )
            .unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        fs::write(dir.join(format!("{name}.keel")), module.to_string()).unwrap();
        if name == "full" {
            assert!(
                module.functions[0]
                    .blocks()
                    .any(|b| module.functions[0].block_params(b).len() >= 2)
            );
        }
        if name == "full" || name == "shapes" || name == "fast" {
            assert_eq!(
                module.functions[1]
                    .layout_insts()
                    .filter(|(_, i)| matches!(
                        module.functions[1].inst_data(*i),
                        keel_ir::InstData::Load { .. }
                    ))
                    .count(),
                6,
                "matmul unroll activation {name}"
            );
        }
        assert_eq!(
            module.functions[4]
                .layout_insts()
                .filter(|(_, i)| matches!(
                    module.functions[4].inst_data(*i),
                    keel_ir::InstData::Load { .. }
                ))
                .count(),
            2,
            "division must reject unrolling"
        );
        if name != "none" {
            for f in &module.functions[2..4] {
                assert_eq!(
                    f.layout_insts()
                        .filter(|(_, i)| matches!(f.inst_data(*i), keel_ir::InstData::Load { .. }))
                        .count(),
                    6,
                    "signed two-stream activation {name}"
                );
            }
        }
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&REGISTERS, &TINY] {
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
        }
    }
    assert_eq!(runs, 40);
}
