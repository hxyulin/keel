//! Canonical masks are a lane-value proof, independent of register allocation.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const ARM_SPILLED: Pools = Pools {
    float_caller_saved: &[],
    float_callee_saved: &[],
    ..keel_codegen::aarch64::REGISTERS
};
const X86_SPILLED: Pools = Pools {
    float_caller_saved: &[],
    float_callee_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};

#[test]
fn direct_masks_and_conservative_boundaries() {
    for shape in ["i8x16", "i16x8", "i32x4", "i64x2"] {
        for mode in ["direct", "loaded", "phi", "bitcast", "alias"] {
            let mask = match mode {
                "direct" => " v5 = vicmp.sgt v3,v4\n".into(),
                "alias" => format!(" v6 = vicmp.sgt v3,v4\n v5 = load.{shape} v0\n"),
                "loaded" => format!(" v5 = load.{shape} v0\n"),
                "phi" => format!(" v6 = vicmp.sgt v3,v4\n jump block1(v6)\nblock1(v5: {shape}):\n"),
                "bitcast" => format!(
                    " v6 = bitcast.i8x16 v3\n v7 = bitcast.i8x16 v4\n v8 = vicmp.sgt v6,v7\n v5 = bitcast.{shape} v8\n"
                ),
                _ => unreachable!(),
            };
            let source = format!(
                "func @proof(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} v0\n v4 = load.{shape} v1\n{mask} v9 = vselect v5,v3,v4\n store v9,v2\n return\n}}"
            );
            let mut module = keel_ir::parse(&source).unwrap();
            if mode == "alias" {
                let f = &mut module.functions[0];
                let comparison = f
                    .layout_insts()
                    .find_map(|(_, i)| {
                        matches!(f.inst_data(i), keel_ir::InstData::VectorIcmp { .. })
                            .then(|| f.inst_results(i)[0])
                    })
                    .unwrap();
                let mask = f
                    .layout_insts()
                    .find_map(|(_, i)| match f.inst_data(i) {
                        keel_ir::InstData::VectorSelect { args } => Some(args[0]),
                        _ => None,
                    })
                    .unwrap();
                f.replace_all_uses(mask, comparison);
            }
            keel_ir::verify_module(&module).unwrap();
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocation,
                    };
                    for spilled in [false, true] {
                        let pools = if spilled {
                            &ARM_SPILLED
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let text =
                            keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::aarch64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            keel_asm::assemble_object(&text).unwrap(),
                            keel_asm::assemble_program_object(&program, Default::default())
                                .unwrap()
                        );
                        let select = text.split(" = vselect ").nth(1).unwrap();
                        assert_eq!(
                            select.contains("cmeq\t"),
                            !matches!(mode, "direct" | "alias"),
                            "{mode}/{shape}: {text}"
                        );
                        let pools = if spilled {
                            &X86_SPILLED
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            keel_asm::x86::assemble_object(&text).unwrap(),
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        );
                        let select = text.split(" = vselect ").nth(1).unwrap();
                        assert_eq!(
                            select.contains("andnps"),
                            !matches!(mode, "direct" | "alias"),
                            "{mode}/{shape}: {text}"
                        );
                    }
                }
            }
        }
    }
}

#[path = "common/mask_oracle.rs"]
mod mask_oracle;

#[test]
fn canonical_widening_oracle_objects() {
    let (source, c) = mask_oracle::fixture();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let evidence = std::env::var_os("KEEL_MASK_EVIDENCE").map(std::path::PathBuf::from);
    if let Some(dir) = &evidence {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("oracle.keel"), &source).unwrap();
        std::fs::write(dir.join("oracle.c"), c).unwrap();
    }
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            for spilled in [false, true] {
                for arch in ["arm", "x86"] {
                    let (text, parsed, typed) = if arch == "arm" {
                        let pools = if spilled {
                            &ARM_SPILLED
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let text =
                            keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::aarch64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        let parsed = keel_asm::assemble_object(&text).unwrap();
                        let typed = keel_asm::assemble_program_object(&program, Default::default())
                            .unwrap();
                        (text, parsed, typed)
                    } else {
                        let pools = if spilled {
                            &X86_SPILLED
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        let parsed = keel_asm::x86::assemble_object(&text).unwrap();
                        let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
                        (text, parsed, typed)
                    };
                    assert_eq!(
                        parsed, typed,
                        "{arch}/{selector:?}/{allocation:?}/{spilled}"
                    );
                    let mut selects = 0;
                    for function in text.split(".globl").skip(1) {
                        let name = function.lines().next().unwrap();
                        let canonical = ["_direct", "_signed", "_chain"]
                            .iter()
                            .any(|suffix| name.trim().trim_matches('"').ends_with(suffix))
                            && !name.contains("unsigned_chain");
                        let count = function.matches(" = vselect ").count();
                        assert_eq!(count, 4, "{name}");
                        let normalization = if arch == "arm" { "cmeq\t" } else { "andnps\t" };
                        assert_eq!(
                            function.matches(normalization).count(),
                            if canonical { 0 } else { 4 },
                            "{arch}/{name}: {function}"
                        );
                        selects += count;
                    }
                    assert_eq!(selects, 84);
                    if spilled {
                        let has_spill = text.lines().any(|line| {
                            if arch == "arm" {
                                line.contains("str\tq") && line.contains("[sp")
                            } else {
                                line.contains("movups\txmmword ptr [rbp")
                            }
                        });
                        assert!(has_spill, "missing forced vector spills for {arch}");
                    }
                    if let Some(dir) = &evidence {
                        let stem = format!("{arch}-{selector:?}-{allocation:?}-spilled{spilled}");
                        std::fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                        std::fs::write(dir.join(format!("{stem}-text.o")), parsed).unwrap();
                        std::fs::write(dir.join(format!("{stem}-typed.o")), typed).unwrap();
                    }
                }
            }
        }
    }
}
