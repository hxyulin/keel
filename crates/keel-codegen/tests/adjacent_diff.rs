//! Standalone pass validation: no root-owned registration or backend edits.
#[path = "../../keel-opt/src/adjacent_abs.rs"]
mod adjacent_abs;
#[path = "../../keel-opt/src/adjacent_diff.rs"]
mod adjacent_diff;
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

fn compile(
    m: &keel_ir::Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    spills: bool,
) -> (String, keel_asm_ir::Program) {
    use keel_codegen::regalloc::Pools;
    // Empty vector pools force every SIMD value through its 16-byte spill home.
    let arm = Pools {
        caller_saved: &[9, 10],
        callee_saved: &[19, 20],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let x86 = Pools {
        caller_saved: &[6, 7],
        callee_saved: &[3],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let (text, program) = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
            let pools = if spills {
                &arm
            } else {
                &keel_codegen::aarch64::REGISTERS
            };
            (
                if target == Target::Aarch64LinuxGnu {
                    keel_codegen::aarch64::compile_linux_with_pipeline(m, pools, pipeline).unwrap()
                } else {
                    keel_codegen::aarch64::compile_with_pipeline(m, pools, pipeline).unwrap()
                },
                if target == Target::Aarch64LinuxGnu {
                    keel_codegen::aarch64::compile_linux_program_with_pipeline(m, pools, pipeline)
                        .unwrap()
                } else {
                    keel_codegen::aarch64::compile_program_with_pipeline(m, pools, pipeline)
                        .unwrap()
                },
            )
        }
        Target::X86_64LinuxGnu => {
            let pools = if spills {
                &x86
            } else {
                &keel_codegen::x86_64::REGISTERS
            };
            (
                keel_codegen::x86_64::compile_with_pipeline(m, pools, pipeline).unwrap(),
                keel_codegen::x86_64::compile_program_with_pipeline(m, pools, pipeline).unwrap(),
            )
        }
    };
    if spills {
        match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => assert!(
                text.lines()
                    .any(|l| l.contains("str\tq") && (l.contains("[sp") || l.contains("[x29"))),
                "{text}"
            ),
            Target::X86_64LinuxGnu => assert!(
                (text.contains("movdqu") || text.contains("movups"))
                    && (text.contains("[rsp") || text.contains("[rbp")),
                "{text}"
            ),
        }
    }
    (text, program)
}

fn module(optimize: bool) -> keel_ir::Module {
    let mut m = keel_ir::parse(include_str!(
        "../../keel-opt/tests/adjacent_abs/adjacent.keel"
    ))
    .unwrap();
    let mut scalar = m.functions[0].clone();
    scalar.name = "rk_adjacent_scalar".into();
    assert!(adjacent_abs::run(&mut m.functions[0]));
    assert!(adjacent_diff::run(&mut m.functions[0]));
    if optimize {
        keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
    }
    m.functions.push(scalar);
    keel_ir::verify_module(&m).unwrap();
    m
}

fn evidence() -> std::path::PathBuf {
    let p = std::env::var_os("KEEL_ADJACENT_DIFF_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("adjacent-diff"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

// Check the generated main block rather than counting the pair cleanup or
// scalar fallback. Register names and edge-copy strategy remain allocator-owned.
fn assert_arm_unrolled_load_cover(text: &str, selection: SelectionTier) {
    let function = text.split("Lrk_adjacent_end:").next().unwrap();
    let blocks: Vec<_> = function
        .split("Lrk_adjacent_block")
        .filter(|block| {
            block
                .lines()
                .filter(|line| line.starts_with("\tldr\td"))
                .count()
                == 4
        })
        .collect();
    assert_eq!(blocks.len(), 1, "{text}");
    let instructions: Vec<_> = blocks[0]
        .lines()
        .filter(|line| line.starts_with('\t') && !line.starts_with("\t;"))
        .collect();
    assert!(
        !instructions.iter().any(|line| line.contains("lsl")),
        "{text}"
    );
    // Full selection folds the three constant pointer offsets into LDR D.
    if matches!(selection, SelectionTier::Full) {
        let loads: Vec<_> = instructions
            .iter()
            .filter(|line| line.starts_with("\tldr\td"))
            .collect();
        let first = loads[0].split('[').nth(1).unwrap().trim_end_matches(']');
        assert!(first.starts_with('x') && !first.contains(','), "{text}");
        for (j, line) in loads.iter().enumerate().skip(1) {
            assert!(line.ends_with(&format!("[{first}, #{}]", 8 * j)), "{text}");
        }
    }
    assert_eq!(
        instructions
            .iter()
            .filter(|line| line.starts_with("\tsub\tx"))
            .count(),
        1,
        "{text}"
    );
    for (i, line) in instructions.iter().enumerate() {
        if line.starts_with("\tldr\td") {
            assert!(instructions[i + 1].starts_with("\tsshll\t"), "{text}");
        }
        if (line.starts_with("\tldr\t") || line.starts_with("\tstr\t"))
            && !line.contains("[sp")
            && !line.contains("[x29")
        {
            assert!(line.starts_with("\tldr\td"), "{text}");
        }
    }
    assert_eq!(
        instructions
            .iter()
            .filter(|line| line.starts_with("\tabs\t"))
            .count(),
        4,
        "{text}"
    );
}

#[test]
fn simd_objects_on_both_targets_with_both_allocators_and_selectors() {
    for optimize in [false, true] {
        let m = module(optimize);
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    for spills in [false, true] {
                        let pipeline = CodegenPipeline {
                            selector: &selection,
                            allocator: &allocation,
                        };
                        let (text, program) = compile(&m, target, &pipeline, spills);
                        let obj = match target {
                            keel_ir::Target::Wasm32UnknownUnknown => {
                                panic!("native-only test target")
                            }
                            Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
                                assert!(text.contains(".2d"), "{text}");
                                if !spills {
                                    assert_arm_unrolled_load_cover(&text, selection);
                                }
                                let obj = if target == Target::Aarch64LinuxGnu {
                                    keel_asm::assemble_elf_program_object(
                                        &program,
                                        Default::default(),
                                    )
                                    .unwrap()
                                } else {
                                    keel_asm::assemble_program_object(&program, Default::default())
                                        .unwrap()
                                };
                                assert_eq!(
                                    obj,
                                    if target == Target::Aarch64LinuxGnu {
                                        keel_asm::assemble_elf_object(&text).unwrap()
                                    } else {
                                        keel_asm::assemble_object(&text).unwrap()
                                    }
                                );
                                obj
                            }
                            Target::X86_64LinuxGnu => {
                                assert!(text.contains("paddq"), "{text}");
                                let obj = keel_asm::x86::assemble_program_object(&program).unwrap();
                                assert_eq!(obj, keel_asm::x86::assemble_object(&text).unwrap());
                                obj
                            }
                        };
                        let stem = format!(
                            "opt{optimize}-{target:?}-{selection:?}-{allocation:?}-spills{spills}"
                        );
                        std::fs::write(evidence().join(format!("{stem}.s")), text).unwrap();
                        std::fs::write(evidence().join(format!("{stem}.o")), obj).unwrap();
                        std::fs::write(evidence().join(format!("{stem}.keel")), m.to_string())
                            .unwrap();
                    }
                }
            }
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_oracle_extrema_tails_unaligned_guard_pages_and_first_faults() {
    use std::{fs, process::Command};
    let target = if cfg!(target_arch = "aarch64") {
        Target::Aarch64AppleDarwin
    } else {
        Target::X86_64LinuxGnu
    };
    let dir = evidence();
    fs::write(dir.join("oracle.c"), include_str!("adjacent_diff/oracle.c")).unwrap();
    for optimize in [false, true] {
        let m = module(optimize);
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for spills in [false, true] {
                    let pipeline = CodegenPipeline {
                        selector: &selection,
                        allocator: &allocation,
                    };
                    let (_, program) = compile(&m, target, &pipeline, spills);
                    let obj = if cfg!(target_arch = "aarch64") {
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                    } else {
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    };
                    let stem =
                        format!("native-opt{optimize}-{selection:?}-{allocation:?}-spills{spills}");
                    let object = dir.join(format!("{stem}.o"));
                    let binary = dir.join(&stem);
                    fs::write(&object, obj).unwrap();
                    let out = Command::new("clang")
                        .args(["-O0", "-fno-lto"])
                        .arg(dir.join("oracle.c"))
                        .arg(&object)
                        .arg("-o")
                        .arg(&binary)
                        .output()
                        .unwrap();
                    assert!(
                        out.status.success(),
                        "{}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let out = Command::new(&binary).output().unwrap();
                    fs::write(dir.join(format!("{stem}.json")), &out.stdout).unwrap();
                    assert!(
                        out.status.success(),
                        "{stem}: {} {}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
            }
        }
    }
}
