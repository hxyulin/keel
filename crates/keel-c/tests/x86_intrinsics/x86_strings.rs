use keel_codegen::x86_features::{FeatureSelector, Features};
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::Target;

use crate::packet;

#[test]
fn sse42_strings_and_crc_preserve_vendor_results() {
    packet::check(
        include_str!("../fixtures/x86_strings.c"),
        include_str!("../fixtures/x86_strings_runner.c"),
        "KEEL_C_STRINGS_EXPORT_DIR",
        "strings",
        Some(&["-mavx2", "-DREFERENCE"]),
    );
}

#[test]
fn string_controls_and_sse42_permissions_are_checked() {
    for expression in [
        "_mm_cmpistri(a,a,i)",
        "_mm_cmpistrz(a,a,256)",
        "_mm_cmpestrm(a,1,a,2,-1)",
        "_mm_cmpestra(a,1,a,2,(i++,3))",
    ] {
        let source =
            format!("#include <nmmintrin.h>\nvoid f(__m128i a,int i){{(void){expression};}}");
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
    for definition in [
        "int f(__m128i a){return _mm_cmpistri(a,a,255);}",
        "int f(__m128i a,int n){return _mm_cmpestrz(a,n,a,n,255);}",
        "unsigned f(unsigned a,unsigned char b){return _mm_crc32_u8(a,b);}",
        "unsigned long long f(unsigned long long a,unsigned long long b){return _mm_crc32_u64(a,b);}",
    ] {
        let module = packet::frontend(&format!("#include <nmmintrin.h>\n{definition}"));
        for (features, allowed) in [("sse42", true), ("sse41", false)] {
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
