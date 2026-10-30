use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const CONTROLS: [u8; 8] = [0, 1, 16, 17, 238, 239, 254, 255];
fn parse(s: &str, target: Target) -> Result<keel_ir::Module, String> {
    let s = keel_c::preprocess(
        s,
        "pclmul.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &s,
        "pclmul.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}
fn enabled() -> Features {
    Features::for_level(CpuLevel::V1)
        .with_features("pclmulqdq")
        .unwrap()
}
#[test]
fn pclmul_header_builtin_controls_and_targets() {
    let target = Target::X86_64LinuxGnu;
    let m = parse(
        "#include <wmmintrin.h>\nint plain(int x){return x+1;}\n",
        target,
    )
    .unwrap();
    let sel = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(CpuLevel::V1),
    };
    let p = CodegenPipeline {
        selector: &sel,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::compile_with_pipeline(&m, target, &p).unwrap();
    assert!(!text.contains("pclmulqdq"));
    assert!(!text.contains("_mm_"));
    parse(
        "_Static_assert(__has_builtin(__builtin_ia32_pclmulqdq128),\"frontend support\");",
        target,
    )
    .unwrap();
    for control in ["0", "255", "(1<<4)|1", "(unsigned char)255"] {
        let s = format!(
            "#include <wmmintrin.h>\n__m128i f(__m128i a,__m128i b){{return _mm_clmulepi64_si128(a,b,{control});}}\n"
        );
        let m = parse(&s, target).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(m.to_string().contains("x86_pclmulqdq128"));
        assert!(
            keel_codegen::compile_program_with_pipeline(&m, target, &p)
                .unwrap_err()
                .message
                .contains("PCLMULQDQ")
        );
        let sel = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features: enabled(),
        };
        keel_codegen::compile_program_with_pipeline(
            &m,
            target,
            &CodegenPipeline {
                selector: &sel,
                allocator: &AllocationTier::Full,
            },
        )
        .unwrap();
    }
    let mut controls = String::from("#include <wmmintrin.h>\n");
    for n in 0..=255 {
        controls += &format!(
            "__m128i control_{n}(__m128i a,__m128i b){{return __builtin_ia32_pclmulqdq128(a,b,{n});}}\n"
        );
    }
    let m = parse(&controls, target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: enabled(),
    };
    keel_codegen::compile_program_with_pipeline(
        &m,
        target,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    for control in ["-1", "256", "n", "1.0", "(0,1)", "((__int128)1<<64)"] {
        let s = format!(
            "#include <wmmintrin.h>\n__m128i f(__m128i a,__m128i b,int n){{return _mm_clmulepi64_si128(a,b,{control});}}\n"
        );
        assert!(parse(&s, target).is_err(), "{control}");
    }
    for s in [
        "typedef int V __attribute__((vector_size(16))); V f(V a){return __builtin_ia32_pclmulqdq128(a,a,0);}",
        "typedef long long V __attribute__((vector_size(16))); V f(V a){return __builtin_ia32_pclmulqdq128(a,1,0);}",
    ] {
        assert!(parse(s, target).is_err());
    }
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        assert!(parse("#include <wmmintrin.h>", target).is_err());
        parse(
            "_Static_assert(!__has_builtin(__builtin_ia32_pclmulqdq128),\"x86 only\");",
            target,
        )
        .unwrap();
        assert!(parse("typedef long long V __attribute__((vector_size(16))); V f(V a){return __builtin_ia32_pclmulqdq128(a,a,0);}",target).is_err());
    }
}
#[test]
fn pclmul_control_requires_defined_bounded_ice() {
    let target = Target::X86_64LinuxGnu;
    let source = |control: &str| {
        format!(
            "#include <wmmintrin.h>\nenum {{ HI=16 }}; __m128i f(__m128i a,__m128i b,int n){{return _mm_clmulepi64_si128(a,b,{control});}}\n"
        )
    };
    for (control, expected) in [
        ("1 ? HI+1 : 0", 17),
        ("sizeof(long long)+9", 17),
        ("(unsigned char)273", 17),
        ("(int)17.0", 17),
        ("(0xffffffffffffffffULL + 18) & 255", 17),
        ("(1ULL << 63) >> 63", 1),
        ("1 ? 17 : (1 / 0)", 17),
        ("0 && (1 / 0)", 0),
        ("1 || (1 / 0)", 1),
    ] {
        let m = parse(&source(control), target).unwrap_or_else(|e| panic!("{control}: {e}"));
        keel_ir::verify_module(&m).unwrap();
        assert!(
            m.to_string().contains(&format!("iconst.i32 {expected}")),
            "{control}: {m}"
        );
    }
    for control in [
        "(int)(1.0+16.0)",
        "(int)(1.0<2.0)",
        "(unsigned char)1e300",
        "(int)2147483648.0",
        "(unsigned long long)18446744073709551616.0",
        "(1ULL << 64)",
        "(1 << -1)",
        "(1 << 32)",
        "(1 << 31) & 255",
        "(-1 << 1) & 255",
        "(2147483647+1) & 255",
        "(-2147483647-2) & 255",
        "(1073741824*4) & 255",
        "(-(-2147483647-1)) & 255",
        "((-2147483647-1)/-1) & 255",
        "((-2147483647-1)%-1) & 255",
        // LP64 long bounds must survive the scalar evaluator migration even
        // when a final narrowing cast would turn an invalid fold into imm8.
        "(unsigned char)(9223372036854775807L + 1L)",
        "(unsigned char)((-9223372036854775807L - 1L) / -1L)",
        "(unsigned char)(1L << 64)",
        "(unsigned char)(long)9223372036854775808.0",
        "(1/0)",
        "(int)(__int128)17",
        "(int)(void*)17",
        "1 ? 17 : n",
        "(n,17)",
    ] {
        assert!(
            parse(&source(control), target).is_err(),
            "accepted {control}"
        );
    }
}

#[test]
fn pclmul_c_single_evaluation_and_independent_values() {
    let mut s = String::from(
        "#include <wmmintrin.h>\nint eval_count;\nextern void clobber(void);\nstatic __m128i get(const void*p){__m128i v;eval_count++;__builtin_memcpy(&v,p,16);return v;}\n",
    );
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\n#define PCLMUL_C_EVAL 1\nvoid clobber(void){}\n",
    );
    for c in CONTROLS {
        for same in 0..2 {
            s += &format!(
                "void clmul_{c}_{same}(const void*pa,const void*pb,void*po){{__m128i a,b,r;__builtin_memcpy(&a,pa,16);__builtin_memcpy(&b,pb,16);r=_mm_clmulepi64_si128(get(pa),get({}),{c});clobber();__builtin_memcpy(po,&r,16);__builtin_memcpy((char*)po+16,&a,16);__builtin_memcpy((char*)po+32,&b,16);}}\n",
                if same == 1 { "pa" } else { "pb" }
            );
            harness += &format!("extern void clmul_{c}_{same}(void*,void*,void*);\n");
        }
    }
    harness +=
        "struct Case{void(*fn)(void*,void*,void*);unsigned c,same;};static struct Case cases[]={\n";
    for c in CONTROLS {
        for same in 0..2 {
            harness += &format!("{{clmul_{c}_{same},{c},{same}}},\n");
        }
    }
    harness += "};\n";
    harness += include_str!("../../../keel-codegen/tests/fixtures/pclmul_harness.c");
    let original = parse(&s, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let d = std::env::var_os("KEEL_C_PCLMUL_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-c-pclmul-{}", std::process::id()))
        });
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("harness.c"), harness).unwrap();
    fs::write(d.join("source.c"), &s).unwrap();
    for (tier, alloc, opt) in [
        (SelectionTier::Baseline, AllocationTier::Fast, false),
        (SelectionTier::Trees, AllocationTier::Full, false),
        (SelectionTier::Full, AllocationTier::Full, true),
    ] {
        let mut m = original.clone();
        if opt {
            keel_opt::optimize(&mut m).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features: enabled(),
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::compile_program_with_pipeline(&m, Target::X86_64LinuxGnu, &p).unwrap();
        let o = d.join(format!("{tier:?}.o"));
        fs::write(
            &o,
            keel_asm::x86::assemble_program_object_with_features(&program, enabled()).unwrap(),
        )
        .unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("pclmulqdq") {
                continue;
            }
            let exe = d.join(format!("run-{tier:?}"));
            let output = Command::new("cc")
                .arg("-O1")
                .arg(d.join("harness.c"))
                .arg(o)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(exe).status().unwrap().success());
        }
    }
}

#[test]
fn sha256_headers_have_typed_vector_signatures() {
    let m = parse("#include <immintrin.h>\n__m128i f(__m128i a,__m128i b,__m128i c){return _mm_sha256rnds2_epu32(_mm_sha256msg1_epu32(a,b),_mm_sha256msg2_epu32(b,a),c);}",Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let ir = m.to_string();
    for name in ["x86_sha256msg1", "x86_sha256msg2", "x86_sha256rnds2"] {
        assert!(ir.contains(name), "{ir}");
    }
    assert!(parse("typedef long long V __attribute__((vector_size(16)));V f(V a){return __builtin_ia32_sha256msg1(a,a);}",Target::X86_64LinuxGnu).is_err());
}

#[test]
fn typed_wide_pclmul_signature_and_control() {
    for (width, shape) in [(256, "i64x4"), (512, "i64x8")] {
        let source = format!(
            "#include <immintrin.h>\nvoid f(__m{width}i*a,__m{width}i*b,__m{width}i*r){{*r=_mm{width}_clmulepi64_epi128(*a,*b,17);}}\n"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(
            module
                .to_string()
                .contains(&format!("intrinsic x86_vpclmulqdq{width}"))
        );
        assert!(module.to_string().contains(shape));
        assert!(parse(&source.replace(",17)", ",256)"), Target::X86_64LinuxGnu).is_err());
    }
}

#[test]
fn floating512_c_shapes_and_header_aliases() {
    let source = "#include <immintrin.h>\nvoid floats(__m512 *out,const __m512 *a,const __m512 *b){*out=*a+*b;}\nvoid doubles(__m512d *out,const __m512d *a,const __m512d *b){*out=*a / *b;}\n";
    let module = parse(source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.to_string();
    assert!(text.contains("f32x16"), "{text}");
    assert!(text.contains("f64x8"), "{text}");
    let direct = "typedef float F __attribute__((vector_size(64)));typedef double D __attribute__((vector_size(64)));void f(F *o,F *a,F *b){*o=*a-*b;}void d(D *o,D *a,D *b){*o=*a * *b;}";
    keel_ir::verify_module(&parse(direct, Target::X86_64LinuxGnu).unwrap()).unwrap();
    assert!(parse(direct, Target::Aarch64LinuxGnu).is_err());
}

#[test]
fn avx512_scalar_mask_headers_and_predicate_controls() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (bits, mask) in [(32, "__mmask16"), (64, "__mmask8")] {
        for sign in ["i", "u"] {
            for predicate in 0..8 {
                source += &format!(
                    "{mask} cmp_{bits}_{sign}_{predicate}(__m512i a,__m512i b){{return _mm512_cmp_ep{sign}{bits}_mask(a,b,{predicate});}}\n"
                );
            }
        }
        for op in ["add", "sub"] {
            source += &format!(
                "__m512i {op}_{bits}(__m512i s,{mask} k,__m512i a,__m512i b){{return _mm512_mask_{op}_epi{bits}(s,k,a,b);}}\n"
            );
        }
        source += &format!(
            "__m512i load_{bits}(void*p,__m512i s,{mask} k){{return _mm512_mask_loadu_epi{bits}(s,k,p);}}\nvoid store_{bits}(void*p,__m512i a,{mask} k){{_mm512_mask_storeu_epi{bits}(p,k,a);}}\n"
        );
    }
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for suffix in [
        "cmpd",
        "cmpq",
        "cmpud",
        "cmpuq",
        "maskaddd",
        "maskaddq",
        "masksubd",
        "masksubq",
        "maskloadd",
        "maskloadq",
        "maskstored",
        "maskstoreq",
    ] {
        assert!(
            ir.contains(&format!("x86_avx512_{suffix}")),
            "missing {suffix}: {ir}"
        );
    }
    for predicate in ["-1", "8", "k"] {
        let bad = format!(
            "#include <immintrin.h>\n__mmask16 f(__m512i a,__m512i b,int k){{return _mm512_cmp_epi32_mask(a,b,{predicate});}}"
        );
        assert!(parse(&bad, Target::X86_64LinuxGnu).is_err(), "{predicate}");
    }
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx512f").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, selector.features).unwrap();
    assert!(parse(&source, Target::Aarch64LinuxGnu).is_err());
}

#[test]
fn avx512_floating_mask_headers_and_predicates() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (suffix, vector, mask) in [("ps", "__m512", "__mmask16"), ("pd", "__m512d", "__mmask8")] {
        for predicate in 0..32 {
            source += &format!(
                "{mask} cmp_{suffix}_{predicate}({vector} a,{vector} b,{mask} k){{return _mm512_mask_cmp_{suffix}_mask(k,a,b,{predicate});}}\n"
            );
        }
        source += &format!(
            "{mask} all_{suffix}({vector} a,{vector} b){{return _mm512_cmp_{suffix}_mask(a,b,0);}}\n"
        );
        for op in ["add", "sub", "mul", "div"] {
            source += &format!(
                "{vector} {op}_{suffix}({vector} s,{mask} k,{vector} a,{vector} b){{return _mm512_mask_{op}_{suffix}(s,k,a,b);}}\n"
            );
        }
        source += &format!(
            "{vector} sqrt_{suffix}({vector} s,{mask} k,{vector} a){{return _mm512_mask_sqrt_{suffix}(s,k,a);}}\n{vector} load_{suffix}(const void*p,{vector} s,{mask} k){{return _mm512_mask_loadu_{suffix}(s,k,p);}}\nvoid store_{suffix}(void*p,{vector} a,{mask} k){{_mm512_mask_storeu_{suffix}(p,k,a);}}\n"
        );
    }
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for suffix in ["ps", "pd"] {
        for op in [
            "cmp",
            "maskadd",
            "masksub",
            "maskmul",
            "maskdiv",
            "masksqrt",
            "maskload",
            "maskstore",
        ] {
            assert!(
                ir.contains(&format!("x86_avx512_{op}{suffix}")),
                "missing {op}{suffix}: {ir}"
            );
        }
    }
    for predicate in ["-1", "32", "k"] {
        let bad = format!(
            "#include <immintrin.h>\n__mmask16 f(__m512 a,__m512 b,int k){{return _mm512_cmp_ps_mask(a,b,{predicate});}}"
        );
        assert!(parse(&bad, Target::X86_64LinuxGnu).is_err(), "{predicate}");
    }
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx512f").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, selector.features).unwrap();
    assert!(parse(&source, Target::Aarch64LinuxGnu).is_err());
}
