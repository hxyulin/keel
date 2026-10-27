//! Nonzero frame queries walk readable native ABI frame records, not a guessed
//! stack offset. Callers must keep the records being inspected alive.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, Module, Target, Type};
use std::{fs, path::Path, process::Command};

const SOURCE: &str = include_str!("fixtures/frame_address.c");
const RUNNER: &str = include_str!("fixtures/frame_address_runner.c");

fn compile(source: &str, target: Target) -> Result<Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "frame-address.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

fn object(module: &Module, target: Target) -> Vec<u8> {
    let text = keel_codegen::compile(module, target).unwrap();
    let program = keel_codegen::compile_program(module, target).unwrap();
    let (typed, textual) = match target {
        Target::Aarch64AppleDarwin => (
            keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
            keel_asm::assemble_object(&text).unwrap(),
        ),
        Target::Aarch64LinuxGnu => (
            keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap(),
            keel_asm::assemble_elf_object(&text).unwrap(),
        ),
        Target::X86_64LinuxGnu => (
            keel_asm::x86::assemble_program_object(&program).unwrap(),
            keel_asm::x86::assemble_object(&text).unwrap(),
        ),
        _ => unreachable!(),
    };
    assert_eq!(typed, textual, "{target:?}");
    typed
}

fn execute(dir: &Path, compiler: &str, input: &Path, output: &Path) {
    let run = Command::new(compiler)
        .args([
            "-O2",
            "-fno-omit-frame-pointer",
            "-fno-optimize-sibling-calls",
        ])
        .arg(input)
        .arg(dir.join("main.c"))
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{compiler}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result = Command::new(output).output().unwrap();
    assert!(
        result.status.success(),
        "{compiler}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, b"frame-address PASS\n");
}

#[test]
fn native_frame_records_survive_optimization_and_dynamic_stack_frames() {
    for target in Target::NATIVE {
        for optimize in [false, true] {
            let mut module = compile(SOURCE, target).unwrap();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            for depth in 0..=3 {
                let f = module
                    .functions
                    .iter()
                    .find(|f| f.name == format!("keel_frame_{depth}"))
                    .unwrap();
                assert_eq!(f.sig.returns.len(), 1);
                assert_eq!(f.sig.returns[0].ty, Type::Ptr);
                let frames = f
                    .layout_insts()
                    .filter(|(_, i)| {
                        matches!(
                            f.inst_data(*i),
                            InstData::Intrinsic {
                                id: Intrinsic::FrameAddress,
                                ..
                            }
                        )
                    })
                    .count();
                assert_eq!(frames, 1, "depth {depth}");
                let loads = f
                    .layout_insts()
                    .filter(|(_, i)| {
                        matches!(f.inst_data(*i), InstData::Load { ty: Type::Ptr, .. })
                    })
                    .count();
                assert_eq!(loads, depth, "depth {depth}");
            }
            let roundtrip = keel_ir::parse(&module.to_string()).unwrap();
            keel_ir::verify_module(&roundtrip).unwrap();
            let bytes = object(&roundtrip, target);
            if Target::host() == Some(target) {
                let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("frame-address-{target:?}-{optimize}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("frame.o"), bytes).unwrap();
                fs::write(dir.join("reference.c"), SOURCE).unwrap();
                fs::write(dir.join("main.c"), RUNNER).unwrap();
                let compilers: &[&str] = if target == Target::X86_64LinuxGnu {
                    &["gcc", "clang"]
                } else {
                    &["clang"]
                };
                for &compiler in compilers {
                    execute(
                        &dir,
                        compiler,
                        &dir.join("frame.o"),
                        &dir.join(format!("keel-{compiler}")),
                    );
                    execute(
                        &dir,
                        compiler,
                        &dir.join("reference.c"),
                        &dir.join(format!("reference-{compiler}")),
                    );
                }
            }
        }
    }
}

#[test]
fn frame_depths_are_checked_integer_constants_with_bounded_loads() {
    for target in Target::NATIVE {
        for expression in ["0", "1", "64", "(1 << 2) - 1", "(int)1.0"] {
            let source = format!("void *f(void){{return __builtin_frame_address({expression});}}");
            let m = compile(&source, target).unwrap();
            keel_ir::verify_module(&m).unwrap();
            object(&m, target);
        }
        for expression in [
            "-1",
            "65",
            "n",
            "n++",
            "(n++, 0)",
            "(0, 1)",
            "1.0",
            "1 / 0",
            "1 << 64",
            "2147483647 + 1",
            "18446744073709551615ULL",
            "((__int128)1 << 64)",
        ] {
            let source = format!("void *f(int n){{return __builtin_frame_address({expression});}}");
            let error = compile(&source, target).unwrap_err();
            assert!(
                error
                    .message
                    .contains("integer constant expression in 0..64"),
                "{target:?} {expression}: {error}"
            );
        }
    }
    for depth in [0, 1] {
        let source = format!("void *f(void){{return __builtin_frame_address({depth});}}");
        let error = compile(&source, Target::Wasm32UnknownUnknown).unwrap_err();
        assert!(error.message.contains("native frame-record ABI"), "{error}");
    }
}

#[test]
fn x86_frame_walks_have_text_typed_chunked_parity_across_allocators() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V1);
    let export = std::env::var_os("KEEL_FRAME_ADDRESS_EXPORT");
    if let Some(path) = &export {
        fs::create_dir_all(path).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(SOURCE, Target::X86_64LinuxGnu).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
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
                    let bytes =
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap();
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    assert_eq!(
                        bytes,
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
                    assert_eq!(bytes, chunks.finish().unwrap());
                    if let Some(path) = &export {
                        fs::write(
                            Path::new(path).join(format!(
                                "{selection:?}-{allocator:?}-{pool}-opt{optimize}.o"
                            )),
                            bytes,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn required_c_inlining_observes_the_emitted_callers_frame() {
    let source = r#"
static __attribute__((always_inline)) inline void *frame_helper(void) {
    return __builtin_frame_address(1);
}
__attribute__((noinline)) void *keel_frame_inlined(void) {
    asm volatile("" ::: "memory");
    return frame_helper();
}
"#;
    let runner = r#"
#include <stdio.h>
extern void *keel_frame_inlined(void);
int main(void) {
    if (keel_frame_inlined() != __builtin_frame_address(0)) return 1;
    puts("frame-address PASS");
    return 0;
}
"#;
    let export =
        std::env::var_os("KEEL_FRAME_ADDRESS_EXPORT").map(|p| Path::new(&p).join("inline"));
    for optimize in [false, true] {
        let mut module = compile(source, Target::X86_64LinuxGnu).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let function = module
            .functions
            .iter()
            .find(|f| f.name == "keel_frame_inlined")
            .unwrap();
        assert!(
            !function
                .layout_insts()
                .any(|(_, i)| matches!(function.inst_data(i), InstData::Call { .. }))
        );
        let bytes = object(&module, Target::X86_64LinuxGnu);
        if let Some(path) = &export {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join(format!("inline-opt{optimize}.o")), &bytes).unwrap();
            fs::write(path.join("source.c"), source).unwrap();
            fs::write(path.join("main.c"), runner).unwrap();
        }
        if Target::host() == Some(Target::X86_64LinuxGnu) {
            let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("frame-address-inlined-{optimize}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("frame.o"), bytes).unwrap();
            fs::write(dir.join("reference.c"), source).unwrap();
            fs::write(dir.join("main.c"), runner).unwrap();
            for compiler in ["gcc", "clang"] {
                execute(
                    &dir,
                    compiler,
                    &dir.join("frame.o"),
                    &dir.join(format!("keel-{compiler}")),
                );
                execute(
                    &dir,
                    compiler,
                    &dir.join("reference.c"),
                    &dir.join(format!("reference-{compiler}")),
                );
            }
        }
    }
}
