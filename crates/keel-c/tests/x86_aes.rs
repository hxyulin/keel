use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
fn compile(source: &str, target: Target) -> Result<keel_ir::Module, String> {
    let pp = keel_c::preprocess(
        source,
        "aes.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = keel_c::compile(
        &pp,
        "aes.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    keel_ir::verify_module(&m).map_err(|e| format!("{e:?}"))?;
    Ok(m)
}
#[test]
fn headers_builtins_ice_shapes_features_and_foreign_diagnostics() {
    let t = Target::X86_64LinuxGnu;
    let src = "#include <wmmintrin.h>\n#if !__has_builtin(__builtin_ia32_aesenc128) || !__has_builtin(__builtin_ia32_aeskeygenassist128)\n#error missing frontend recognition\n#endif\n__m128i enc(__m128i a,__m128i b){return _mm_aesenc_si128(a,b);}\n__m128i enclast(__m128i a,__m128i b){return _mm_aesenclast_si128(a,b);}\n__m128i dec(__m128i a,__m128i b){return _mm_aesdec_si128(a,b);}\n__m128i declast(__m128i a,__m128i b){return _mm_aesdeclast_si128(a,b);}\n__m128i imc(__m128i a){return _mm_aesimc_si128(a);}\n__m128i kg(__m128i a){return _mm_aeskeygenassist_si128(a,(1<<7)+127);}\n";
    let m = compile(src, t).unwrap();
    for op in [
        "aesenc",
        "aesenclast",
        "aesdec",
        "aesdeclast",
        "aesimc",
        "aeskeygenassist",
    ] {
        assert!(m.to_string().contains(&format!("x86_{op}128")));
    }
    assert_eq!(m.functions.len(), 6);
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for enable in [false, true] {
            let f = Features::for_level(level)
                .with_features(if enable { "+aes" } else { "none" })
                .unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features: f,
            };
            let p = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let code = keel_codegen::compile_program_with_pipeline(&m, t, &p);
            if enable {
                keel_asm::x86::assemble_program_object_with_features(&code.unwrap(), f).unwrap();
            } else {
                assert!(code.unwrap_err().message.contains("AES"));
            }
        }
    }
    let empty = compile("#include <wmmintrin.h>\nint f(void){return 0;}\n", t).unwrap();
    assert_eq!(empty.functions.len(), 1);
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(CpuLevel::V1),
    };
    keel_codegen::compile_program_with_pipeline(
        &empty,
        t,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Fast,
        },
    )
    .unwrap();
    for expr in [
        "_mm_aesenc_si128(a, (__m128d){0})",
        "_mm_aesenc_si128((__m128d){0},a)",
        "_mm_aesimc_si128((__m128d){0})",
        "_mm_aeskeygenassist_si128(a,n)",
        "_mm_aeskeygenassist_si128(a,-1)",
        "_mm_aeskeygenassist_si128(a,256)",
        "_mm_aeskeygenassist_si128(a,1.0)",
        "_mm_aeskeygenassist_si128(a,(n++,1))",
        "_mm_aeskeygenassist_si128(a,(int)(((__int128)1<<64)/256))",
        "_mm_aeskeygenassist_si128(a,(int)(0.5+0.5))",
        "_mm_aeskeygenassist_si128(a,(long)(void*)1)",
    ] {
        assert!(
            compile(
                &format!("#include <wmmintrin.h>\n__m128i f(__m128i a,int n){{return {expr};}}\n"),
                t
            )
            .is_err(),
            "{expr}"
        );
    }
    for target in [Target::Aarch64LinuxGnu, Target::Aarch64AppleDarwin] {
        assert!(compile(src, target).is_err());
        assert!(compile("typedef long long V __attribute__((vector_size(16))); V f(V a,V b){return __builtin_ia32_aesenc128(a,b);}",target).is_err());
        compile("#if __has_builtin(__builtin_ia32_aesenc128)\n#error foreign AES\n#endif\nint f(void){return 0;}\n",target).unwrap();
    }
    // Macro expansion must evaluate the source expression once.
    let single=compile("#include <wmmintrin.h>\nextern __m128i next(void); __m128i f(void){return _mm_aeskeygenassist_si128(next(),0);}\n",t).unwrap().to_string();
    assert_eq!(single.matches("call @next()").count(), 1, "{single}");
}
