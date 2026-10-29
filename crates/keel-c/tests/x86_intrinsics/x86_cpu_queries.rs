//! Baseline CPU queries check CPUID bounds and executable AVX state.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{fs, path::PathBuf, process::Command};
const SOURCE: &str = include_str!("../fixtures/x86_cpu_queries.c");
const RUNNER: &str = include_str!("../fixtures/x86_cpu_queries_runner.c");
fn frontend(source: &str, target: Target) -> Result<keel_ir::Module, String> {
    let source = keel_c::preprocess(
        source,
        "cpu-query.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &source,
        "cpu-query.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}
fn command(cmd: &mut Command) {
    let o = cmd.output().unwrap();
    assert!(
        o.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
#[test]
fn runtime_queries_reject_unknown_nonliteral_and_foreign_names() {
    for argument in [
        "name",
        "\"avx512f\"",
        "\"unknown\"",
        "\"avx\\0suffix\"",
        "L\"avx\"",
        "(name,\"avx\")",
    ] {
        assert!(
            frontend(
                &format!("int f(const char *name){{return __builtin_cpu_supports({argument});}}"),
                Target::X86_64LinuxGnu
            )
            .is_err(),
            "{argument}"
        );
    }
    for source in [
        "int f(void){return __builtin_cpu_supports();}",
        "void f(void){__builtin_cpu_init(1);}",
    ] {
        assert!(frontend(source, Target::X86_64LinuxGnu).is_err());
    }
    for target in [
        Target::Aarch64LinuxGnu,
        Target::Aarch64AppleDarwin,
        Target::Wasm32UnknownUnknown,
    ] {
        assert!(
            frontend(
                "int f(void){return __builtin_cpu_supports(\"avx\");}",
                target
            )
            .is_err()
        );
    }
    let module=frontend("#if !__has_builtin(__builtin_cpu_supports) || !__has_builtin(__builtin_cpu_init)\n#error missing runtime queries\n#endif\nvoid init(void){__builtin_cpu_init();}\nint f(void){return __builtin_cpu_supports(\"av\" \"x\");}",Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(module.to_string().contains("x86_cpu_xcr0"));
    assert!(module.to_string().contains("x86_cpuid"));
    assert!(
        !module
            .functions
            .iter()
            .find(|f| f.name == "init")
            .unwrap()
            .layout_insts()
            .any(|(_, i)| matches!(
                module
                    .functions
                    .iter()
                    .find(|f| f.name == "init")
                    .unwrap()
                    .inst_data(i),
                keel_ir::InstData::Intrinsic { .. }
            ))
    );
}
#[test]
fn safe_xcr0_queries_do_not_enable_unguarded_xgetbv() {
    let mut module = frontend(
        "int f(void){return __builtin_cpu_supports(\"avx\");}",
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let baseline = Features::for_level(CpuLevel::V1);
    let encoding = module_features(&module, baseline).unwrap();
    assert!(!baseline.xsave && !baseline.avx);
    assert!(encoding.xsave && !encoding.avx);
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: baseline,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let safe =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap();
    keel_asm::x86::assemble_program_object_with_features(&safe, encoding).unwrap();
    for function in &mut module.functions {
        let instructions: Vec<_> = function
            .layout_insts()
            .map(|(_, instruction)| instruction)
            .collect();
        for instruction in instructions {
            if let keel_ir::InstData::Intrinsic { id, .. } = function.inst_data_mut(instruction)
                && *id == keel_ir::Intrinsic::X86CpuXcr0
            {
                *id = keel_ir::Intrinsic::X86Xgetbv0;
            }
        }
    }
    keel_ir::verify_module(&module).unwrap();
    let error =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap_err();
    assert!(
        error.message.to_ascii_lowercase().contains("xsave"),
        "{error:?}"
    );
    assert!(!module_features(&module, baseline).unwrap().xsave);
}
#[test]
fn baseline_queries_and_explicit_dispatch_match_native_capabilities() {
    let directory = std::env::var_os("KEEL_C_CPU_QUERY_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-cpu-query"));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("kernels.c"), SOURCE).unwrap();
    fs::write(directory.join("runner.c"), RUNNER).unwrap();
    let raw = frontend(SOURCE, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let features = Features::for_level(CpuLevel::V1);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for (name, module, selection, spilled) in [
        ("baseline", &raw, SelectionTier::Baseline, false),
        ("trees", &raw, SelectionTier::Trees, false),
        ("optimized-spill", &optimized, SelectionTier::Full, true),
    ] {
        let selector = FeatureSelector {
            inner: &selection,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let pools = if spilled {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let encoding = module_features(module, features).unwrap();
        let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
        let typed =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&typed, encoding).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
        );
        let mut chunks = keel_asm::x86::ProgramAssembler::new(encoding);
        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
            module,
            pools,
            &pipeline,
            &mut |p| chunks.push(p),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        fs::write(directory.join(format!("{name}.o")), object).unwrap();
    }
    command(Command::new("clang").current_dir(&directory).args([
        "-target",
        "x86_64-unknown-linux-gnu",
        "-ffreestanding",
        "-O2",
        "-c",
        "runner.c",
        "-o",
        "runner.o",
    ]));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for name in ["baseline", "trees", "optimized-spill"] {
        command(Command::new("clang").current_dir(&directory).args([
            "runner.o",
            &format!("{name}.o"),
            "-o",
            "run",
        ]));
        command(&mut Command::new(directory.join("run")));
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    eprintln!(
        "native runtime CPU execution skipped: requires x86-64 Linux; artifacts: {}",
        directory.display()
    );
}
