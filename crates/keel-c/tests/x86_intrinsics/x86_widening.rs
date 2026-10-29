use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
use keel_codegen::{CodegenPipeline, SelectionTier};
use keel_ir::{Module, Target};
use std::{fs, path::PathBuf, process::Command};

fn compile(source: &str) -> Module {
    let source = keel_c::preprocess(
        source,
        "widening.c",
        &keel_c::PreprocessOptions {
            target: Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &source,
        "widening.c",
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

fn sources() -> (String, String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut narrow = source.clone();
    let mut harness = String::from(
        "#include <stdint.h>\n#include <immintrin.h>\nextern int printf(const char*,...);\n",
    );
    let mut cases = String::new();
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        for signed in [true, false] {
            for (input, output) in [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)] {
                let name = format!(
                    "{prefix}_cvtep{}{input}_epi{output}",
                    if signed { 'i' } else { 'u' }
                );
                // The pointer increments prove each wrapper evaluates its argument once.
                // Separate ABI entry points exercise vector argument/result transport.
                let body = format!(
                    "__m{width}i a{name}(__m128i x){{return {name}(x);}}\nvoid k{name}(const void*input,void*out){{const __m128i*p=input;__m{width}i r={name}(_mm_loadu_si128(p++));if(p!=(const __m128i*)input+1)r=(__m{width}i){{0}};{prefix}_storeu_si{width}(out,r);}}\n"
                );
                source += &body;
                if width == 128 {
                    narrow += &body;
                }
                harness += &format!(
                    "extern void k{name}(const void*,void*);\nextern void c{name}(const void*,void*);\n"
                );
                harness += &format!(
                    "extern __m{width}i a{name}(__m128i);\nextern __m{width}i reference{name}(__m128i);\nvoid abi_k{name}(const void*input,void*out){{{prefix}_storeu_si{width}(out,a{name}(_mm_loadu_si128(input)));}}\nvoid abi_c{name}(const void*input,void*out){{{prefix}_storeu_si{width}(out,reference{name}(_mm_loadu_si128(input)));}}\n"
                );
                cases += &format!(
                    "{{\"{name}\",k{name},c{name},{},{input},{output},{}}},\n{{\"{name} ABI\",abi_k{name},abi_c{name},{},{input},{output},{}}},\n",
                    width / 8,
                    u8::from(signed),
                    width / 8,
                    u8::from(signed)
                );
            }
        }
    }
    harness += "struct Case {const char *name;void (*keel)(const void*,void*);void (*vendor)(const void*,void*);unsigned bytes,input_bits,output_bits,signed_input;};\n";
    harness += &format!("static const struct Case cases[]={{\n{cases}}};\n");
    harness += include_str!("../fixtures/x86_integer_lanes.h");
    harness += include_str!("../fixtures/x86_widening_runner.c");
    (source, narrow, harness)
}

#[test]
fn widening_matches_vendor_and_scalar_oracles() {
    let directory = std::env::var_os("KEEL_C_WIDENING_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-widening"));
    fs::create_dir_all(&directory).unwrap();
    let (source, narrow, harness) = sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(
        directory.join("oracle.c"),
        source
            .replace("void k_", "void c_")
            .replace(" a_", " reference_"),
    )
    .unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    for file in ["oracle", "harness"] {
        command(Command::new("clang").current_dir(&directory).args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-mavx2",
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
    assert!(error.message.contains("256-bit SIMD"), "{error:?}");
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
