use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{AsmOperand, InstData, Target};

const SOURCE: &str = include_str!("../fixtures/x86_asm_goto.c");

fn compile(source: &str) -> Result<keel_ir::Module, String> {
    keel_c::compile(
        source,
        "asm-goto.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn restoration_trampolines_use_ordinary_cfg_and_survive_codegen() {
    check_codegen(SOURCE, "KEEL_X86_ASM_GOTO_EXPORT");
}

#[test]
fn goto_outputs_reach_all_successors_after_register_restoration() {
    let source = include_str!("../fixtures/x86_asm_goto_outputs.c");
    if let Some(dir) = std::env::var_os("KEEL_X86_ASM_GOTO_OUTPUTS_EXPORT") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.c"), source).unwrap();
        std::fs::write(
            dir.join("main.c"),
            include_str!("../fixtures/x86_asm_goto_outputs_main.c"),
        )
        .unwrap();
    }
    check_codegen(source, "KEEL_X86_ASM_GOTO_OUTPUTS_EXPORT");
}

fn check_codegen(source: &str, export_name: &str) {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V1);
    let export = std::env::var_os(export_name).map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(source).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        for function in &module.functions {
            for (_, inst) in function.layout_insts() {
                if let InstData::InlineAsm { asm, .. } = function.inst_data(inst) {
                    assert_eq!(asm.outputs[0], (AsmOperand::Reg, keel_ir::Type::I32));
                    assert!(asm.early_clobbers.contains(&0));
                    assert!(asm.template.contains(".Lkeel_asm_goto_%=_end:"));
                }
            }
        }
        let printed = module.to_string();
        keel_ir::verify_module(&keel_ir::parse(&printed).unwrap()).unwrap();
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let selector = FeatureSelector {
                inner: &selection,
                features,
            };
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &empty),
                ] {
                    let program = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    let object =
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap();
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |p| chunks.push(p),
                    )
                    .unwrap();
                    assert_eq!(object, chunks.finish().unwrap());
                    if let Some(path) = &export {
                        std::fs::write(
                            path.join(format!(
                                "{selection:?}-{allocator:?}-{pool}-opt{optimize}.o"
                            )),
                            object,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn unsupported_goto_contracts_are_diagnosed() {
    for (source, expected) in [
        (
            "int f(void){asm goto(\"\"::::hit,hit); hit:return 0;}",
            "duplicate asm operand",
        ),
        (
            "int f(int x){asm goto(\"\"::[hit]\"r\"(x)::hit); hit:return 0;}",
            "duplicate asm operand",
        ),
        (
            "int f(void){asm goto(\"\"::::); return 0;}",
            "at least one label",
        ),
        (
            "int f(void){asm goto(\"jmp %l[absent]\"::::hit); hit:return 0;}",
            "unknown asm label",
        ),
        (
            "int f(int x){asm goto(\"jmp %l0\"::\"r\"(x)::hit); hit:return 0;}",
            "label operand number",
        ),
        (
            "int f(void){asm goto(\"jmp %l1\"::::hit); hit:return 0;}",
            "label operand number",
        ),
        (
            "int f(void){asm goto(\"jmp %l[hit\"::::hit); hit:return 0;}",
            "unclosed asm label",
        ),
        (
            "int f(void){asm goto(\"jmp %l0\"::::absent); return 0;}",
            "undefined label",
        ),
        (
            "int f(void){asm(\"\"::::hit); hit:return 0;}",
            "goto qualifier",
        ),
        (
            "int f(int n){int a[n]; asm goto(\"\"::::hit); hit:return 0;}",
            "VLAs",
        ),
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains(expected),
            "{source}: expected {expected}: {error}"
        );
    }
    let error = keel_c::compile(
        "void f(void){asm goto(\"\"::::hit); hit:;}",
        "aarch64.c",
        keel_c::Options {
            target: Target::Aarch64AppleDarwin,
            debug_info: false,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("asm goto requires x86-64 Linux"), "{error}");
}
