//! Different-width GPR ties share hardware while retaining operand types.
use keel_ir::Target;
use std::fs;
const SOURCE: &str = include_str!("../fixtures/x86_asm_ties.c");
const RUNNER: &str = include_str!("../fixtures/x86_asm_ties_runner.c");
const TYPES: &[&str] = &[
    "unsigned char",
    "unsigned short",
    "unsigned int",
    "unsigned long",
];
fn source() -> String {
    let mut text = SOURCE.to_owned();
    for (i, input) in TYPES.iter().enumerate() {
        for (j, output) in TYPES.iter().enumerate() {
            let mov = match i {
                0 => "movzbq %b1,%q0",
                1 => "movzwq %w1,%q0",
                2 => "movl %k1,%k0",
                _ => "movq %q1,%q0",
            };
            for fixed in [false, true] {
                let constraint = if fixed { "a" } else { "r" };
                text.push_str(&format!("{output} tie_{i}_{j}_{fixed}({input} value) {{ {output} out; asm(\"{mov}; xorq $0x1234,%q0\":\"={constraint}\"(out):\"0\"(value):\"cc\"); return out; }}\n"));
            }
        }
    }
    text
}
fn runner() -> String {
    let mut text = RUNNER.to_owned();
    for (i, input) in TYPES.iter().enumerate() {
        for (j, output) in TYPES.iter().enumerate() {
            for fixed in [false, true] {
                text.push_str(&format!("extern {output} tie_{i}_{j}_{fixed}({input});\n"));
            }
        }
    }
    text.push_str("int main(void) { int status=base_test(); if(status)return status; unsigned long values[]={0,1,0x80,0x8000,0x80000000UL,0xfedcba9876543210UL}; for(unsigned k=0;k<6;++k){unsigned long x=values[k];\n");
    for (i, input) in TYPES.iter().enumerate() {
        for (j, output) in TYPES.iter().enumerate() {
            for fixed in [false, true] {
                text.push_str(&format!("if(tie_{i}_{j}_{fixed}(({input})x)!=({output})((unsigned long)({input})x^0x1234))return {};\n",10+i*4+j));
            }
        }
    }
    text.push_str("}return 0;}\n");
    text
}
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "mixed-ties.c",
        keel_c::Options {
            target,
            debug_info: debug,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
#[test]
fn verifier_preserves_classes_and_arm_rejects_different_widths() {
    let module=keel_ir::parse("func @tie(i64) -> i32 {\nblock0(v0:i64):\n v1 = asm \"\", \"=r,0\"(v0) -> i32\n return v1\n}").unwrap();
    keel_ir::verify_module(&module).unwrap();
    let error = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap_err();
    assert!(error.to_string().contains("different-width"), "{error}");
    for ty in ["f64", "i128"] {
        let source = format!(
            "func @bad({ty}) -> i32 {{\nblock0(v0:{ty}):\n v1 = asm \"\", \"=r,0\"(v0) -> i32\n return v1\n}}"
        );
        assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
    }
}
#[test]
fn mixed_tie_objects_preserve_text_typed_and_chunked_parity() {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::{AllocationTier, Pools},
        x86_features::{CpuLevel, FeatureSelector, Features},
    };
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let export = std::env::var_os("KEEL_X86_ASM_TIES_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), source()).unwrap();
        fs::write(dir.join("main.c"), runner()).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(&source(), Target::X86_64LinuxGnu, false);
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let module = keel_ir::parse(&module.to_string()).unwrap();
        for tier in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let selector = FeatureSelector {
                inner: &tier,
                features: Features::for_level(CpuLevel::V1),
            };
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                for (name, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &empty),
                ] {
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    let object = keel_asm::x86::assemble_object(&text).unwrap();
                    let program = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    );
                    let mut chunks =
                        keel_asm::x86::ProgramAssembler::new(Features::for_level(CpuLevel::V1));
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |program| chunks.push(program),
                    )
                    .unwrap();
                    assert_eq!(object, chunks.finish().unwrap());
                    if let Some(dir) = &export {
                        fs::write(
                            dir.join(format!("ties-{optimize}-{tier:?}-{allocator:?}-{name}.o")),
                            object,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
