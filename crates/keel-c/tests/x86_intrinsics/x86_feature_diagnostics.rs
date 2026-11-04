//! With a known x86 baseline, a builtin call whose features the enclosing
//! function does not enable is an error at the C call.
use keel_codegen::x86_features::{CpuLevel, Features};
use keel_ir::Target;

fn compile(
    source: &str,
    baseline: Option<Features>,
) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    let target = Target::X86_64LinuxGnu;
    let source = keel_c::preprocess(
        &format!("#include <immintrin.h>\n{source}"),
        "features.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    keel_c::compile(
        &source,
        "features.c",
        keel_c::Options {
            target,
            x86_features: baseline,
            ..Default::default()
        },
    )
}

#[test]
fn missing_features_are_reported_at_the_call() {
    let error = compile(
        "__m512i f(__m512i a) {\n  return _mm512_slli_epi32(a, 3);\n}\n",
        Some(Features::NONE),
    )
    .unwrap_err();
    // The macro wrapper's builtin is reported on the line of its use.
    assert_eq!((error.file.as_str(), error.line), ("features.c", 3));
    assert_eq!(
        error.message,
        "`__builtin_keel_avx512_masksllid` requires target feature `avx512f`, which `f` does not enable"
    );
    let error = compile(
        "__attribute__((target(\"avx512f\"))) __m256i f(__m256i a) { return _mm256_maskz_srli_epi32(1, a, 3); }",
        Some(Features::NONE),
    )
    .unwrap_err();
    assert!(
        error
            .message
            .ends_with("requires target feature `avx512vl`, which `f` does not enable"),
        "{error}"
    );
    let error = compile(
        "__m128i f(__m128 a) { return _mm_cvtps_ph(a, 0); }",
        Some(Features::NONE),
    )
    .unwrap_err();
    assert!(
        error.message.contains("requires target feature `f16c`"),
        "{error}"
    );
}

#[test]
fn function_and_baseline_features_permit_calls() {
    let avx512 = "__attribute__((target(\"avx512f\"))) __m512i f(__m512i a) { return _mm512_slli_epi32(a, 3); }";
    compile(avx512, Some(Features::NONE)).unwrap();
    let plain = "__m512i f(__m512i a) { return _mm512_slli_epi32(a, 3); }";
    compile(plain, Some(Features::parse("avx512f").unwrap())).unwrap();
    // A disabled baseline feature is reported again.
    let disabled = "__attribute__((target(\"no-avx512f\"))) __m512i f(__m512i a) { return _mm512_slli_epi32(a, 3); }";
    assert!(compile(disabled, Some(Features::parse("avx512f").unwrap())).is_err());
    // As in code generation, only emitted functions are checked.
    let unused = "static inline __m512i h(__m512i a) { return _mm512_slli_epi32(a, 3); }\nint f(void) { return 0; }";
    compile(unused, Some(Features::NONE)).unwrap();
    // Without a baseline the frontend leaves the check to code generation.
    compile(plain, None).unwrap();
    // The EVEX F16C conversions are an accepted alternative to F16C.
    compile(
        "__attribute__((target(\"avx512f,avx512vl,no-f16c\"))) __m128i f(__m128 a) { return _mm_cvtps_ph(a, 0); }",
        Some(Features::for_level(CpuLevel::V3)),
    )
    .unwrap();
}

#[test]
fn compiler_header_bodies_are_not_checked() {
    // Header inline functions call AVX-512 builtins; a v1 unit that never
    // calls them still compiles, and so does a call from a function that
    // enables their features.
    compile("int f(void) { return 0; }", Some(Features::NONE)).unwrap();
    compile(
        "__attribute__((target(\"avx512f\"))) float f(__m512 a) { return _mm512_reduce_add_ps(a); }",
        Some(Features::NONE),
    )
    .unwrap();
}
