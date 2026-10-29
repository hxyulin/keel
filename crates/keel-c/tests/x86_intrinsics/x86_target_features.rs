use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{Module, Target};

fn compile(source: &str, target: Target) -> Result<Module, String> {
    let text = keel_c::preprocess(
        source,
        "target-features.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &text,
        "target-features.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn target_attributes_validate_ownership_names_and_round_trip() {
    for source in [
        "int x __attribute__((target(\"avx2\")));",
        "void f(void) __attribute__((target(\"\")));",
        "void f(void) __attribute__((target(\"unknown\")));",
        "void f(void) __attribute__((target(\"arch=haswell\")));",
        "void f(void) __attribute__((target(1)));",
        "typedef int T __attribute__((target(\"avx\")));",
    ] {
        assert!(compile(source, Target::X86_64LinuxGnu).is_err(), "{source}");
    }
    assert!(
        compile(
            "void f(void) __attribute__((target(\"avx2\")));",
            Target::Aarch64LinuxGnu
        )
        .is_err()
    );
    let baseline = compile(
        "void f(void) __attribute__((target(\"sse2\")));void f(void){}",
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    assert!(baseline.functions[0].attrs.target_features.is_empty());
    let module=compile("#if !__has_attribute(target)\n#error target unsupported\n#endif\nvoid f(void) __attribute__((target(\"avx2,no-fma\",\"sse4.1\")));\nvoid f(void){}",Target::X86_64LinuxGnu).unwrap();
    let attrs = &module.functions[0].attrs;
    assert_eq!(
        attrs.target_features.iter().collect::<Vec<_>>(),
        ["+avx2,-fma", "+sse41"]
    );
    let text = module.to_string();
    assert_eq!(keel_ir::parser::parse(&text).unwrap().to_string(), text);
}

#[test]
fn header_operations_use_function_permissions_without_widening_callers() {
    let source = "#include <immintrin.h>\n__attribute__((target(\"avx2\"),noinline)) void wide(const int*a,int*b){_mm256_storeu_si256((__m256i*)b,_mm256_sllv_epi32(_mm256_loadu_si256((const __m256i*)a),_mm256_set1_epi32(1)));}\nvoid baseline(const int*a,int*b){wide(a,b);}";
    let original = compile(source, Target::X86_64LinuxGnu).unwrap();
    let mut optimized = original.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    for module in [&original, &optimized] {
        keel_ir::verify_module(module).unwrap();
        let features = keel_codegen::x86_features::module_features(module, Features::NONE).unwrap();
        assert!(features.avx2);
        let program =
            keel_codegen::compile_program_with_pipeline(module, Target::X86_64LinuxGnu, &pipeline)
                .unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        let text =
            keel_codegen::compile_with_pipeline(module, Target::X86_64LinuxGnu, &pipeline).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
    }
    let illegal=compile("#include <immintrin.h>\nvoid f(const int*a,int*b){_mm256_storeu_si256((__m256i*)b,_mm256_loadu_si256((const __m256i*)a));}",Target::X86_64LinuxGnu).unwrap();
    assert!(
        keel_codegen::compile_program_with_pipeline(&illegal, Target::X86_64LinuxGnu, &pipeline)
            .is_err()
    );
}

#[test]
fn negative_feature_callers_inline_baseline_wrappers_and_reject_avx_operations() {
    let source = "#include <immintrin.h>\n__attribute__((target(\"no-avx2,no-fma,no-f16c,no-avx\"))) __m128 f(__m128 a,__m128 b){return _mm_add_ps(a,b);}";
    let module = compile(source, Target::X86_64LinuxGnu).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(keel_codegen::x86_features::CpuLevel::V3),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
        .unwrap();
    let illegal = "#include <immintrin.h>\n__attribute__((target(\"no-avx2,no-fma,no-f16c,no-avx\"))) void f(const float*a,float*b){_mm256_storeu_ps(b,_mm256_loadu_ps(a));}";
    let module = compile(illegal, Target::X86_64LinuxGnu).unwrap();
    assert!(
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .is_err()
    );
}
