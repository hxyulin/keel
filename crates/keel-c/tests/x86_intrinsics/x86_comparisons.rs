use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
use keel_codegen::{CodegenPipeline, SelectionTier};
use keel_ir::{Module, Target};
use std::{fs, path::PathBuf, process::Command};

fn compile(source: &str) -> Module {
    let source = keel_c::preprocess(
        source,
        "comparisons.c",
        &keel_c::PreprocessOptions {
            target: Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &source,
        "comparisons.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const CASES: &str = include_str!("../fixtures/x86_comparison_cases.h");
const SOURCE: &str = include_str!("../fixtures/x86_comparisons.c");
const ASSEMBLY: &str = include_str!("../fixtures/x86_comparison_assembly.h");
const RUNNER: &str = include_str!("../fixtures/x86_comparison_runner.c");

fn sources() -> (String, String, String) {
    let source = SOURCE.replace("#include \"x86_comparison_cases.h\"", CASES);
    let narrow_cases = CASES
        .lines()
        .filter(|line| !line.starts_with("COMPARISON(avx"))
        .collect::<Vec<_>>()
        .join("\n");
    let narrow = SOURCE
        .split("#ifndef REFERENCE\nstatic int evaluation_count;")
        .next()
        .unwrap()
        .replace("#include \"x86_comparison_cases.h\"", &narrow_cases);
    let harness = RUNNER
        .replace("#include \"x86_comparison_assembly.h\"", ASSEMBLY)
        .replace("#include \"x86_comparison_cases.h\"", CASES);
    (source, narrow, harness)
}

#[test]
fn comparisons_preserve_predicates_mxcsr_and_upper_lanes() {
    let directory = std::env::var_os("KEEL_C_COMPARISON_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-comparisons"));
    fs::create_dir_all(&directory).unwrap();
    let (source, narrow, harness) = sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(
        directory.join("oracle.c"),
        format!("#define REFERENCE\n{source}"),
    )
    .unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    for file in ["oracle", "harness"] {
        command(Command::new("clang").current_dir(&directory).args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-mavx",
            "-frounding-math",
            "-O2",
            "-c",
            &format!("{file}.c"),
            "-o",
            &format!("{file}.o"),
        ]));
    }
    let raw = compile(&source);
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    for name in [
        "cmpps128", "cmppd128", "cmpss", "cmpsd", "comiss", "ucomisd",
    ] {
        assert!(
            optimized.to_string().contains(&format!("x86_sse_{name}")),
            "missing effectful {name}"
        );
    }
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V3);
    // Exact generic promotions remain legal at baseline for narrow values.
    let baseline = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(CpuLevel::V1),
    };
    keel_codegen::x86_64::compile_program_with_pipeline(
        &compile(&narrow),
        &keel_codegen::x86_64::REGISTERS,
        &CodegenPipeline {
            selector: &baseline,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    let error = keel_codegen::x86_64::compile_program_with_pipeline(
        &raw,
        &keel_codegen::x86_64::REGISTERS,
        &CodegenPipeline {
            selector: &baseline,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("AVX") || error.message.contains("256-bit SIMD"),
        "{error:?}"
    );
    let mut avx = features;
    avx.avx2 = false;
    let avx_selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: avx,
    };
    keel_codegen::x86_64::compile_program_with_pipeline(
        &raw,
        &keel_codegen::x86_64::REGISTERS,
        &CodegenPipeline {
            selector: &avx_selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    for (name, selection, allocation, module, pools) in [
        (
            "baseline",
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &raw,
            &keel_codegen::x86_64::REGISTERS,
        ),
        (
            "trees",
            SelectionTier::Trees,
            AllocationTier::Full,
            &raw,
            &keel_codegen::x86_64::REGISTERS,
        ),
        (
            "optimized-spill",
            SelectionTier::Full,
            AllocationTier::Full,
            &optimized,
            &empty,
        ),
    ] {
        let selector = FeatureSelector {
            inner: &selection,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &allocation,
        };
        let typed =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&typed, features).unwrap();
        let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
            module,
            pools,
            &pipeline,
            &mut |part| chunks.push(part),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        let filename = format!("keel-{name}.o");
        fs::write(directory.join(&filename), object).unwrap();
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        if std::is_x86_feature_detected!("avx2") {
            command(Command::new("clang").current_dir(&directory).args([
                "oracle.o",
                "harness.o",
                &filename,
                "-o",
                "run",
            ]));
            command(&mut Command::new(directory.join("run")));
        }
    }
}

#[test]
fn comparison_immediates_reject_runtime_and_out_of_range_controls() {
    for predicate in ["-1", "32", "i", "(i++,0)", "0.0"] {
        for (ty, suffix) in [
            ("__m128", "ps"),
            ("__m128d", "pd"),
            ("__m128", "ss"),
            ("__m128d", "sd"),
        ] {
            let source = format!(
                "#include <immintrin.h>\n{ty} f({ty} a,{ty} b,int i){{return _mm_cmp_{suffix}(a,b,{predicate});}}"
            );
            let source = keel_c::preprocess(
                &source,
                "bad.c",
                &keel_c::PreprocessOptions {
                    target: Target::X86_64LinuxGnu,
                    ..Default::default()
                },
            )
            .unwrap();
            let error = keel_c::compile(
                &source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(
                error.message.contains("AVX comparison predicate"),
                "{predicate} {suffix}: {error:?}"
            );
        }
    }
    let source = "#include <xmmintrin.h>\n__m128 f(__m128 a,__m128 b){return (__m128)__builtin_keel_sse_cmpps128(a,b,8);}";
    let source = keel_c::preprocess(
        source,
        "bad.c",
        &keel_c::PreprocessOptions {
            target: Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .unwrap();
    let error = keel_c::compile(
        &source,
        "bad.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("SSE comparison predicate"),
        "{error:?}"
    );
}
