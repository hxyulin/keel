//! Packed enums use their selected integer representation throughout layout and ABI.
use keel_ir::Target;
use std::{fs, path::Path, process::Command};

const HEADER: &str = include_str!("fixtures/packed_enums.h");
const SOURCE: &str = include_str!("fixtures/packed_enums.c");
const RUNNER: &str = include_str!("fixtures/packed_enums_runner.c");

fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "packed-enums.c",
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
fn packed_enums_reach_all_native_backends_and_roundtrip_ir() {
    for target in Target::NATIVE {
        for debug in [false, true] {
            let module = compile(&format!("{HEADER}\n{SOURCE}"), target, debug);
            let module = keel_ir::parse(&module.to_string()).unwrap();
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn packed_enum_ranges_choose_the_smallest_integer() {
    for (range, size, signed) in [
        ("0", 1, false),
        ("255", 1, false),
        ("256", 2, false),
        ("65535", 2, false),
        ("65536", 4, false),
        ("4294967295U", 4, false),
        ("4294967296ULL", 8, false),
        ("18446744073709551615ULL", 8, false),
        ("-128, LAST=127", 1, true),
        ("-129", 2, true),
        ("-32768", 2, true),
        ("-32769", 4, true),
        ("-2147483649LL", 8, true),
    ] {
        let source = format!(
            "enum __attribute__((packed)) E {{ FIRST={range} }}; \
             _Static_assert(sizeof(enum E)=={size}, \"size\"); \
             _Static_assert(((enum E)-1 < 0)=={}, \"signedness\");",
            u8::from(signed)
        );
        for target in Target::NATIVE
            .into_iter()
            .chain([Target::Wasm32UnknownUnknown])
        {
            compile(&source, target, false);
        }
    }
}

#[test]
fn invalid_enum_layouts_are_diagnosed() {
    for (source, expected) in [
        ("enum E { A }; enum E { B };", "already defined"),
        ("struct E { int a; }; enum E value;", "not an enum"),
        (
            "enum E; enum E { A } __attribute__((packed));",
            "incomplete enum representation",
        ),
        (
            "enum E __attribute__((packed));",
            "requires an enum definition",
        ),
        (
            "enum E { A } __attribute__((packed,aligned(8)));",
            "aligned enum",
        ),
        ("enum E : unsigned char { A = 256 };", "does not fit"),
        (
            "enum E { A=-1, B=18446744073709551615ULL } __attribute__((packed));",
            "more than 64 bits",
        ),
        ("enum E { A=18446744073709551615ULL, B };", "64-bit range"),
    ] {
        let error = keel_c::compile(
            source,
            "invalid-enum.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(error.message.contains(expected), "{source}: {error}");
    }
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: status {}, {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn enum_storage_calls_and_returns_match_the_host_c_compiler() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("packed-enums");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.c");
    let runner = dir.join("runner.c");
    fs::write(&source, format!("{HEADER}\n{SOURCE}")).unwrap();
    fs::write(&runner, format!("{HEADER}\n{RUNNER}")).unwrap();
    let reference = dir.join("reference");
    run(Command::new("clang")
        .args(["-std=gnu2x", "-O2", "-w"])
        .arg(&source)
        .arg(&runner)
        .arg("-o")
        .arg(&reference));
    run(&mut Command::new(&reference));
    let target = Target::host().unwrap();
    for optimize in [false, true] {
        let mut module = compile(&format!("{HEADER}\n{SOURCE}"), target, false);
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let assembly = dir.join(format!("generated-{optimize}.s"));
        fs::write(&assembly, keel_codegen::compile(&module, target).unwrap()).unwrap();
        let executable = assembly.with_extension("exe");
        run(Command::new("clang")
            .args(["-std=gnu2x", "-w"])
            .arg(&assembly)
            .arg(&runner)
            .arg("-o")
            .arg(&executable));
        run(&mut Command::new(&executable));
    }
}

#[test]
fn packed_enum_x86_objects_preserve_text_typed_and_chunked_parity() {
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
    let export = std::env::var_os("KEEL_PACKED_ENUM_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), format!("{HEADER}\n{SOURCE}")).unwrap();
        fs::write(dir.join("main.c"), format!("{HEADER}\n{RUNNER}")).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(
            &format!("{HEADER}\n{SOURCE}"),
            Target::X86_64LinuxGnu,
            false,
        );
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
                            dir.join(format!("enum-{optimize}-{tier:?}-{allocator:?}-{name}.o")),
                            object,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
