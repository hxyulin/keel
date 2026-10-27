//! Named locals constrain direct extended-asm operands without reserving registers.
use keel_ir::Target;
use std::fs;

const SOURCE: &str = include_str!("fixtures/x86_local_register.c");
const RUNNER: &str = include_str!("fixtures/x86_local_register_runner.c");
const REGISTERS: &[&str] = &[
    "rax", "rcx", "rdx", "rbx", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
];

fn source() -> String {
    let mut source = SOURCE.to_owned();
    for (i, reg) in REGISTERS.iter().enumerate() {
        source.push_str(&format!(
            "unsigned long reg_input_{i}(unsigned long x) {{ register unsigned long v asm(\"{reg}\")=x; unsigned long out; asm(\"movq %%{reg},%0\":\"=r\"(out):\"r\"(v)); return out; }}\n\
             unsigned long reg_output_{i}(unsigned long x) {{ register unsigned long v asm(\"{reg}\"); asm(\"movq %1,%%{reg}\":\"=r\"(v):\"r\"(x)); return v; }}\n\
             unsigned long reg_rw_{i}(unsigned long x) {{ register unsigned long v asm(\"{reg}\")=x; asm(\"xorq $47,%%{reg}\":\"+r\"(v)::\"cc\"); return v; }}\n"
        ));
    }
    source
}
fn runner() -> String {
    let mut source = RUNNER.replace("int main(void)", "int fixture_main(void)");
    for i in 0..REGISTERS.len() {
        source.push_str(&format!("extern unsigned long reg_input_{i}(unsigned long), reg_output_{i}(unsigned long), reg_rw_{i}(unsigned long);\n"));
    }
    source.push_str("int main(void) { int result=fixture_main(); if(result) return result; unsigned long x=0xfedcba9876543210UL;\n");
    for i in 0..REGISTERS.len() {
        source.push_str(&format!(
            "if(reg_input_{i}(x)!=x || reg_output_{i}(x)!=x || reg_rw_{i}(x)!=(x^47)) return {};\n",
            20 + i
        ));
    }
    source.push_str("return 0; }\n");
    source
}
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "local-register.c",
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
fn invalid_bindings_are_diagnosed() {
    for (body, expected) in [
        ("long v asm(\"rax\");", "require register storage"),
        ("register long v asm(\"rsp\");", "nonreserved"),
        ("register long v asm(\"rbp\");", "nonreserved"),
        ("register long v asm(\"ah\");", "high-byte"),
        ("register long v asm(\"xmm0\");", "nonreserved"),
        ("long v; asm(\"\":\"={r10}\"(v));", "unsupported"),
        ("long v=1; asm(\"\"::\"{r10}\"(v));", "unsupported"),
        ("register double v asm(\"rax\");", "requires an integer"),
        (
            "register long v asm(\"rax\"); long *p=&v;",
            "cannot take the address",
        ),
        (
            "register long v asm(\"rax\"); asm(\"\"::\"m\"(v));",
            "conflicts",
        ),
        (
            "register long v asm(\"rax\"); asm(\"\"::\"d\"(v));",
            "conflicts",
        ),
        (
            "register long v asm(\"rax\"); asm(\"\"::\"i\"(v));",
            "conflicts",
        ),
        (
            "register long v asm(\"rax\"),w asm(\"rdx\"); asm(\"\":\"=r\"(v):\"0\"(w));",
            "conflicts",
        ),
    ] {
        let error = keel_c::compile(
            &format!("void f(void){{{body}}}"),
            "bad-register.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(error.message.contains(expected), "{body}: {error}");
    }
}

#[test]
fn named_operands_remain_fixed_after_optimization_and_ir_roundtrip() {
    for optimize in [false, true] {
        let mut module = compile(&source(), Target::X86_64LinuxGnu, true);
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let text = module.to_string();
        let module = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for reg in REGISTERS {
            assert!(text.contains(&format!("{{{reg}}}")), "missing {reg}");
        }
        keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
    }
}

#[test]
fn local_register_objects_preserve_text_typed_and_chunked_parity() {
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
    let export = std::env::var_os("KEEL_LOCAL_REGISTER_EXPORT").map(std::path::PathBuf::from);
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
                            dir.join(format!(
                                "register-{optimize}-{tier:?}-{allocator:?}-{name}.o"
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
