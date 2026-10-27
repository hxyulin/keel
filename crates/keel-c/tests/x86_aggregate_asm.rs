//! Register operands preserve the complete representation of one-word records.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{InstData, Module, Target, Type};
use std::{fs, path::Path, process::Command};

const SOURCE: &str = include_str!("fixtures/x86_aggregate_asm.c");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const RUNNER: &str = include_str!("fixtures/x86_aggregate_asm_runner.c");
fn compile(source: &str) -> Result<Module, String> {
    keel_c::compile(
        source,
        "aggregate-asm.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn record_operands_preserve_bits_and_all_object_paths() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V1);
    let export = std::env::var_os("KEEL_X86_AGGREGATE_ASM_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        fs::create_dir_all(path).unwrap();
    }
    let reference = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aggregate-asm-reference");
    fs::create_dir_all(&reference).unwrap();
    fs::write(reference.join("source.c"), SOURCE).unwrap();
    let clang = Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-O2", "-c"])
        .arg(reference.join("source.c"))
        .arg("-o")
        .arg(reference.join("reference.o"))
        .output()
        .unwrap();
    assert!(
        clang.status.success(),
        "{}",
        String::from_utf8_lossy(&clang.stderr)
    );
    for optimize in [false, true] {
        let mut module = compile(SOURCE).unwrap();
        // Packed member access must not acquire the integer view's natural alignment.
        let packed = module
            .functions
            .iter()
            .find(|f| f.name == "aggregate_packed")
            .unwrap();
        let accesses: Vec<_> = packed
            .layout_insts()
            .filter_map(|(_, i)| match packed.inst_data(i) {
                InstData::Load {
                    ty: Type::I64,
                    flags,
                    ..
                }
                | InstData::Store { flags, .. } => Some(*flags),
                _ => None,
            })
            .collect();
        assert_eq!(accesses.len(), 2);
        assert!(accesses.iter().all(|f| f.align == 1));
        let volatile = module
            .functions
            .iter()
            .find(|f| f.name == "aggregate_volatile")
            .unwrap();
        assert!(volatile.layout_insts().any(
            |(_, i)| matches!(volatile.inst_data(i),InstData::Load{flags,..} if flags.volatile)
        ));
        assert!(volatile.layout_insts().any(
            |(_, i)| matches!(volatile.inst_data(i),InstData::Store{flags,..} if flags.volatile)
        ));
        let volatile_input = module
            .functions
            .iter()
            .find(|f| f.name == "aggregate_volatile_input")
            .unwrap();
        assert!(volatile_input.layout_insts().any(|(_, i)| matches!(volatile_input.inst_data(i),InstData::Load{flags,..} if flags.volatile)));
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let reparsed = keel_ir::parse(&module.to_string()).unwrap();
        keel_ir::verify_module(&reparsed).unwrap();
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
                        &reparsed, pools, &pipeline,
                    )
                    .unwrap();
                    let object =
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap();
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&reparsed, pools, &pipeline)
                            .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &reparsed,
                        pools,
                        &pipeline,
                        &mut |p| chunks.push(p),
                    )
                    .unwrap();
                    assert_eq!(object, chunks.finish().unwrap());
                    if let Some(path) = &export {
                        fs::write(
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
fn unsupported_record_register_forms_and_const_outputs_fail_closed() {
    for (source, expected) in [
        (
            "struct S{long x;};void f(struct S v){asm(\"\"::\"i\"(v));}",
            "immediate requires an integer or pointer",
        ),
        (
            "struct S{long x;};void f(struct S v){asm(\"\"::\"n\"(v));}",
            "immediate requires an integer or pointer",
        ),
        (
            "struct S{char x[3];};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "1, 2, 4 or 8-byte",
        ),
        (
            "struct S{long x[2];};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "1, 2, 4 or 8-byte",
        ),
        (
            "struct S{};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "1, 2, 4 or 8-byte",
        ),
        (
            "struct S{const long x;};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "nonmodifiable",
        ),
        (
            "struct S{const long x;};void f(struct S*p){asm(\"\":\"+g\"(*p));}",
            "nonmodifiable",
        ),
        (
            "struct S{const char x[3];};void f(struct S*p){asm(\"\":\"=g\"(*p));}",
            "nonmodifiable",
        ),
        (
            "struct S{const long x;};void f(struct S*p){asm(\"\":\"=m\"(*p));}",
            "nonmodifiable",
        ),
        (
            "struct S{int *const x;};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "nonmodifiable",
        ),
        (
            "struct S{_Atomic int x;};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
            "atomic",
        ),
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains(expected),
            "{source}: expected {expected}: {error}"
        );
    }
    for source in [
        "struct S{const long x;};long f(struct S*p){long r;asm(\"movq %1,%0\":\"=r\"(r):\"r\"(*p));return r;}",
        "struct S{const int *x;};void f(struct S*p){asm(\"\":\"+r\"(*p));}",
        "struct S{_Atomic int x;};void f(struct S*p){asm(\"\":\"+m\"(*p));}",
        "struct S{long x[2];};void f(struct S*p){asm(\"\":\"+g\"(*p));}",
    ] {
        compile(source).unwrap();
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_record_bits_and_single_evaluation_match_gnu_and_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aggregate-asm-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(dir.join("main.c"), RUNNER).unwrap();
    for optimize in [false, true] {
        let mut module = compile(SOURCE).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        fs::write(
            dir.join("keel.o"),
            keel_asm::x86::assemble_program_object(&program).unwrap(),
        )
        .unwrap();
        for compiler in ["gcc", "clang"] {
            for source in ["keel.o", "source.c"] {
                let result = Command::new(compiler)
                    .args(["-O2", "-no-pie"])
                    .arg(dir.join(source))
                    .arg(dir.join("main.c"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{compiler}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = Command::new(dir.join("run")).output().unwrap();
                assert!(
                    result.status.success(),
                    "{compiler} {source} opt={optimize}: status={} {}",
                    result.status,
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
}
