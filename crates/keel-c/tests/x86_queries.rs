use keel_codegen::x86_features::{FeatureSelector, Features};
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::Target;

#[path = "common/x86_packet.rs"]
mod packet;

#[test]
fn queries_and_specialized_operations_use_architectural_semantics() {
    packet::check(
        include_str!("fixtures/x86_queries.c"),
        include_str!("fixtures/x86_queries_runner.c"),
        "KEEL_C_QUERIES_EXPORT_DIR",
        "queries",
        Some(&["-mavx2", "-DREFERENCE"]),
    );
}

#[test]
fn specialized_controls_and_feature_permissions_are_checked() {
    for expression in [
        "_mm_dp_ps(a,a,i)",
        "_mm_dp_pd(b,b,256)",
        "_mm_mpsadbw_epu8(c,c,-1)",
        "_mm256_mpsadbw_epu8(d,d,i)",
        "_mm256_dp_ps(e,e,(i++,3))",
    ] {
        let source = format!(
            "#include <immintrin.h>\nvoid f(__m128 a,__m128d b,__m128i c,__m256i d,__m256 e,int i){{(void){expression};}}"
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
            error
                .to_string()
                .contains("integer constant expression in 0..255"),
            "{expression}: {error}"
        );
    }
    for (definition, enabled, missing) in [
        (
            "int f(__m128i a,__m128i b){return _mm_testnzc_si128(a,b);}",
            "sse41",
            "none",
        ),
        (
            "__m128i f(__m128i a){return _mm_minpos_epu16(a);}",
            "sse41",
            "none",
        ),
        (
            "__m128 f(__m128 a){return _mm_dp_ps(a,a,255);}",
            "sse41",
            "none",
        ),
        ("int f(__m128 a){return _mm_testz_ps(a,a);}", "avx", "sse41"),
        (
            "int f(__m256i a){return _mm256_testc_si256(a,a);}",
            "avx",
            "sse41",
        ),
        (
            "__m256i f(__m256i a){return _mm256_mpsadbw_epu8(a,a,255);}",
            "avx2",
            "avx",
        ),
    ] {
        let module = packet::frontend(&format!("#include <immintrin.h>\n{definition}"));
        for (features, allowed) in [(enabled, true), (missing, false)] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features: Features::parse(features).unwrap(),
            };
            let result = keel_codegen::compile_program_with_pipeline(
                &module,
                Target::X86_64LinuxGnu,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                },
            );
            assert_eq!(
                result.is_ok(),
                allowed,
                "{definition} features {features}: {result:?}"
            );
        }
    }
}
