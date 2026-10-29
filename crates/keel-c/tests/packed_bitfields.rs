#[path = "packed_bitfields/packed_enums.rs"]
mod packed_enums;

use std::{path::Path, process::Command};

use keel_ir::{InstData, Target, Type};

const SOURCE: &str = include_str!("fixtures/packed_bitfields.c");

fn compile(source: &str, target: Target, debug_info: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "packed-bitfields.c",
        keel_c::Options { target, debug_info },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn packed_layout_and_initializers_work_on_native_targets() {
    for target in Target::NATIVE {
        for debug in [false, true] {
            let (size, align) = if target == Target::Aarch64LinuxGnu {
                (8, 4)
            } else {
                (5, 1)
            };
            let source = format!(
                "{SOURCE}\n_Static_assert(sizeof(struct Zero) == {size}, \"zero size\");\n_Static_assert(_Alignof(struct Zero) == {align}, \"zero align\");"
            );
            let m = compile(&source, target, debug);
            let text = m.to_string();
            keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

#[test]
fn partial_accesses_are_byte_bounded_and_keep_volatile_flags() {
    let m = compile(
        "struct __attribute__((packed)) S { unsigned a:7; signed long b:64; };\n\
         long f(volatile struct S *p, long v) { p->b = v; return p->b; }",
        Target::X86_64LinuxGnu,
        false,
    );
    let f = &m.functions[0];
    let mut loads = 0;
    let mut stores = 0;
    for (_, inst) in f.layout_insts() {
        match f.inst_data(inst) {
            InstData::Load { ty, flags, .. } => {
                assert_eq!(*ty, Type::I8);
                assert!(flags.volatile);
                loads += 1;
            }
            InstData::Store { flags, .. } => {
                assert!(flags.volatile);
                stores += 1;
            }
            _ => {}
        }
    }
    assert_eq!(loads, 11, "nine read bytes plus two boundary updates");
    assert_eq!(stores, 9);
}

fn command(cmd: &mut Command) -> std::process::Output {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn packed_accesses_match_clang_and_independent_bit_oracle() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("packed-bitfields");
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.c");
    let oracle = dir.join("oracle.c");
    std::fs::write(&source, SOURCE).unwrap();
    std::fs::write(&oracle, include_str!("fixtures/packed_bitfields_oracle.c")).unwrap();
    let reference = dir.join("reference");
    command(
        Command::new("clang")
            .args(["-O2", "-w"])
            .arg(&source)
            .arg(&oracle)
            .arg("-o")
            .arg(&reference),
    );
    let expected = command(&mut Command::new(&reference));
    assert_eq!(expected.stdout, b"PASS\n");
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut m = compile(SOURCE, Target::host().unwrap(), debug);
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            let asm = keel_codegen::compile(&m, Target::host().unwrap()).unwrap();
            let asm_file = dir.join(format!("packed-{debug}-{optimize}.s"));
            std::fs::write(&asm_file, asm).unwrap();
            let exe = asm_file.with_extension("exe");
            command(
                Command::new("clang")
                    .arg(&asm_file)
                    .arg(&oracle)
                    .arg("-o")
                    .arg(&exe),
            );
            assert_eq!(command(&mut Command::new(&exe)).stdout, expected.stdout);
        }
    }
}

#[test]
fn packed_fields_have_text_typed_and_chunked_parity_across_codegen_choices() {
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
    let export = std::env::var_os("KEEL_X86_PACKED_BITS_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut module = keel_c::compile(
                SOURCE,
                "packed-bitfields.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: debug,
                },
            )
            .unwrap();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let module = keel_ir::parse(&module.to_string()).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for tier in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                let features = Features::for_level(CpuLevel::V1);
                let selector = FeatureSelector {
                    inner: &tier,
                    features,
                };
                for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    for (pool_name, pools) in [
                        ("normal", &keel_codegen::x86_64::REGISTERS),
                        ("empty", &empty),
                    ] {
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let bytes = keel_asm::x86::assemble_object(&text).unwrap();
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            bytes,
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        );
                        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                            &module,
                            pools,
                            &pipeline,
                            &mut |program| chunks.push(program),
                        )
                        .unwrap();
                        assert_eq!(bytes, chunks.finish().unwrap());
                        if let Some(path) = &export {
                            std::fs::write(path.join(format!("{tier:?}-{allocator:?}-{pool_name}-opt{optimize}-debug{debug}.o")), bytes).unwrap();
                        }
                    }
                }
            }
        }
    }
}
