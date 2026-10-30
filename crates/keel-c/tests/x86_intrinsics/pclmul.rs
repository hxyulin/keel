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

#[test]
fn avx512_unmasked_headers_native_and_single_evaluation() {
    let mut source = String::from("#include <immintrin.h>\nint evaluations;\n");
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <string.h>\n#include <stdint.h>\nextern int evaluations;\n",
    );
    let mut cases = String::new();
    for (suffix, ty) in [("ps", "__m512"), ("pd", "__m512d")] {
        source += &format!(
            "static {ty} get_{suffix}(const void*p){{evaluations++;return _mm512_loadu_{suffix}(p);}}\n"
        );
        for op in ["add", "sub", "mul", "div", "sqrt"] {
            let name = format!("unmasked_{op}_{suffix}");
            let inputs = if op == "sqrt" {
                format!("get_{suffix}(a)")
            } else {
                format!("get_{suffix}(a),get_{suffix}(b)")
            };
            source += &format!(
                "void {name}(const void*a,const void*b,void*out){{{ty} r=_mm512_{op}_{suffix}({inputs});_mm512_storeu_{suffix}(out,r);}}\n"
            );
            harness += &format!("extern void {name}(const void*,const void*,void*);\n");
            let expected = if op == "sqrt" {
                format!("_mm512_sqrt_{suffix}(a)")
            } else {
                format!("_mm512_{op}_{suffix}(a,b)")
            };
            let count = if op == "sqrt" { 1 } else { 2 };
            cases += &format!(
                "{{{ty} a=_mm512_set1_{suffix}(9),b=_mm512_set1_{suffix}(2),want={expected},out;evaluations=0;{name}(&a,&b,&out);if(evaluations!={count}||memcmp(&want,&out,64))return 1;}}\n"
            );
        }
        source += &format!(
            "void constructor_{suffix}(void*out){{_mm512_store_{suffix}(out,_mm512_set1_{suffix}(3));}}\nvoid zero_{suffix}(void*out){{_mm512_store_{suffix}(out,_mm512_setzero_{suffix}());}}\nvoid aligned_{suffix}(const void*p,void*out){{_mm512_store_{suffix}(out,_mm512_load_{suffix}(p));}}\n"
        );
        for (name, expected) in [
            ("constructor", format!("_mm512_set1_{suffix}(3)")),
            ("zero", format!("_mm512_setzero_{suffix}()")),
        ] {
            harness += &format!("extern void {name}_{suffix}(void*);\n");
            cases += &format!(
                "{{{ty} out,want={expected};{name}_{suffix}(&out);if(memcmp(&want,&out,64))return 2;}}\n"
            );
        }
    }
    for (op, bits) in [
        ("add", 32),
        ("sub", 32),
        ("mullo", 32),
        ("add", 64),
        ("sub", 64),
    ] {
        let name = format!("integer_{op}_{bits}");
        source += &format!(
            "void {name}(const void*a,const void*b,void*out){{_mm512_storeu_si512(out,_mm512_{op}_epi{bits}(_mm512_loadu_si512(a),_mm512_loadu_si512(b)));}}\n"
        );
        harness += &format!("extern void {name}(const void*,const void*,void*);\n");
        cases += &format!(
            "{{__m512i a=_mm512_set1_epi{bits}(-123),b=_mm512_set1_epi{bits}(31),want=_mm512_{op}_epi{bits}(a,b),out;{name}(&a,&b,&out);if(memcmp(&want,&out,64))return 3;}}\n"
        );
    }
    for op in ["and", "or", "xor", "andnot"] {
        let name = format!("bits_{op}");
        source += &format!(
            "void {name}(const void*a,const void*b,void*out){{_mm512_storeu_si512(out,_mm512_{op}_si512(_mm512_loadu_si512(a),_mm512_loadu_si512(b)));}}\n"
        );
        harness += &format!("extern void {name}(const void*,const void*,void*);\n");
        cases += &format!(
            "{{__m512i a=_mm512_set1_epi64(-123),b=_mm512_set1_epi64(31),want=_mm512_{op}_si512(a,b),out;{name}(&a,&b,&out);if(memcmp(&want,&out,64))return 4;}}\n"
        );
    }
    source += "void integer_zero(void*out){_mm512_store_si512(out,_mm512_setzero_si512());}\nvoid casts(const void*p,void*out){__m512i a=_mm512_load_si512(p);__m512 b=_mm512_castsi512_ps(a);__m512d c=_mm512_castps_pd(b);a=_mm512_castpd_si512(c);c=_mm512_castsi512_pd(a);b=_mm512_castpd_ps(c);_mm512_store_si512(out,_mm512_castps_si512(b));}\n";
    harness += "extern void integer_zero(void*),casts(const void*,void*);\n";
    cases += "{__m512i a=_mm512_set1_epi64(-123),out,want=_mm512_setzero_si512();integer_zero(&out);if(memcmp(&want,&out,64))return 5;casts(&a,&out);if(memcmp(&a,&out,64))return 6;}\n";
    harness += &format!("int main(void){{{cases}return 0;}}\n");
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let directory =
        std::env::temp_dir().join(format!("keel-avx512-unmasked-c-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    for (tier, allocator) in [
        (SelectionTier::Baseline, AllocationTier::Fast),
        (SelectionTier::Full, AllocationTier::Full),
    ] {
        let mut module = original.clone();
        keel_opt::optimize(&mut module).unwrap();
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &allocator,
        };
        let program =
            keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
                .unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("code.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                eprintln!("SKIP native unmasked headers: AVX512F unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-mavx512f", "-O2"])
                .arg(directory.join("harness.c"))
                .arg(directory.join("code.o"))
                .arg("-o")
                .arg(directory.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.join("run"))
                    .status()
                    .unwrap()
                    .success(),
                "{tier:?}"
            );
        }
    }
}

#[test]
fn avx512_indexed_and_packed_memory_headers_validate_shapes_and_controls() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (bits, suffix, index, data, mask) in [
        (32, "epi32", "__m512i", "__m512i", "__mmask16"),
        (32, "epi64", "__m256i", "__m512i", "__mmask8"),
        (64, "epi32", "__m512i", "__m256i", "__mmask8"),
        (64, "epi64", "__m512i", "__m512i", "__mmask8"),
        (32, "ps", "__m512i", "__m512", "__mmask16"),
        (32, "pd", "__m256i", "__m512d", "__mmask8"),
        (64, "ps", "__m512i", "__m256", "__mmask8"),
        (64, "pd", "__m512i", "__m512d", "__mmask8"),
    ] {
        for scale in [1, 2, 4, 8] {
            source += &format!(
                "{data} gather_{bits}_{suffix}_{scale}(void*p,{index} i,{data} s,{mask} k){{return _mm512_mask_i{bits}gather_{suffix}(s,k,i,p,{scale});}}\nvoid scatter_{bits}_{suffix}_{scale}(void*p,{index} i,{data} a,{mask} k){{_mm512_mask_i{bits}scatter_{suffix}(p,k,i,a,{scale});}}\n"
            );
        }
        source += &format!(
            "{data} allgather_{bits}_{suffix}(void*p,{index} i){{return _mm512_i{bits}gather_{suffix}(i,p,4);}}\nvoid allscatter_{bits}_{suffix}(void*p,{index} i,{data} a){{_mm512_i{bits}scatter_{suffix}(p,i,a,4);}}\n"
        );
    }
    for (suffix, data, mask) in [
        ("epi32", "__m512i", "__mmask16"),
        ("epi64", "__m512i", "__mmask8"),
        ("ps", "__m512", "__mmask16"),
        ("pd", "__m512d", "__mmask8"),
    ] {
        for op in ["compress", "expand"] {
            source += &format!(
                "{data} {op}_{suffix}({data} s,{mask} k,{data} a){{return _mm512_mask_{op}_{suffix}(s,k,a);}}\n{data} {op}zero_{suffix}({mask} k,{data} a){{return _mm512_maskz_{op}_{suffix}(k,a);}}\n"
            );
        }
        source += &format!(
            "void packedstore_{suffix}(void*p,{mask} k,{data} a){{_mm512_mask_compressstoreu_{suffix}(p,k,a);}}\n{data} packedload_{suffix}(void*p,{mask} k,{data} s){{return _mm512_mask_expandloadu_{suffix}(s,k,p);}}\n{data} packedloadzero_{suffix}(void*p,{mask} k){{return _mm512_maskz_expandloadu_{suffix}(k,p);}}\n"
        );
    }
    let target = Target::X86_64LinuxGnu;
    let mut module = parse(&source, target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    for scale in ["0", "3", "16", "-1", "n"] {
        for op in ["gather", "scatter"] {
            let body = if op == "gather" {
                format!("return _mm512_mask_i32gather_epi32(a,k,i,p,{scale});")
            } else {
                format!("_mm512_mask_i32scatter_epi32(p,k,i,a,{scale});return a;")
            };
            let invalid = format!(
                "#include <immintrin.h>\n__m512i f(void*p,__m512i i,__m512i a,__mmask16 k,int n){{{body}}}"
            );
            assert!(
                parse(&invalid, target).is_err(),
                "{op} accepted scale {scale}"
            );
        }
    }
    assert!(parse(&source, Target::Aarch64LinuxGnu).is_err());
}

#[test]
fn avx512_conversion_special_and_integer_headers_check_independent_controls() {
    let mut source = String::from("#include <immintrin.h>\n");
    for (api, input, output) in [
        ("cvtepi32_ps", "__m512i", "__m512"),
        ("cvtepu32_ps", "__m512i", "__m512"),
        ("cvtepi32_pd", "__m256i", "__m512d"),
        ("cvtepu32_pd", "__m256i", "__m512d"),
        ("cvtps_epi32", "__m512", "__m512i"),
        ("cvtps_epu32", "__m512", "__m512i"),
        ("cvttps_epi32", "__m512", "__m512i"),
        ("cvttps_epu32", "__m512", "__m512i"),
        ("cvtpd_epi32", "__m512d", "__m256i"),
        ("cvtpd_epu32", "__m512d", "__m256i"),
        ("cvttpd_epi32", "__m512d", "__m256i"),
        ("cvttpd_epu32", "__m512d", "__m256i"),
        ("cvtps_pd", "__m256", "__m512d"),
        ("cvtpd_ps", "__m512d", "__m256"),
        ("cvtph_ps", "__m256i", "__m512"),
        ("cvtps_ph", "__m512", "__m256i"),
    ] {
        let extra = if api == "cvtps_ph" { ",4" } else { "" };
        source += &format!(
            "{output} {api}_all({input} a){{return _mm512_{api}(a{extra});}}\n{output} {api}_merge({output} s,{input} a){{return _mm512_mask_{api}(s,0x55,a{extra});}}\n{output} {api}_zero({input} a){{return _mm512_maskz_{api}(0x55,a{extra});}}\n"
        );
        let rounded = match api {
            "cvtepi32_ps" => Some("cvt_roundepi32_ps"),
            "cvtepu32_ps" => Some("cvt_roundepu32_ps"),
            "cvtps_epi32" => Some("cvt_roundps_epi32"),
            "cvtps_epu32" => Some("cvt_roundps_epu32"),
            "cvttps_epi32" => Some("cvtt_roundps_epi32"),
            "cvttps_epu32" => Some("cvtt_roundps_epu32"),
            "cvtpd_epi32" => Some("cvt_roundpd_epi32"),
            "cvtpd_epu32" => Some("cvt_roundpd_epu32"),
            "cvttpd_epi32" => Some("cvtt_roundpd_epi32"),
            "cvttpd_epu32" => Some("cvtt_roundpd_epu32"),
            "cvtps_pd" => Some("cvt_roundps_pd"),
            "cvtpd_ps" => Some("cvt_roundpd_ps"),
            "cvtph_ps" => Some("cvt_roundph_ps"),
            "cvtps_ph" => Some("cvt_roundps_ph"),
            _ => None,
        };
        if let Some(rounded) = rounded {
            source += &format!(
                "{output} {rounded}_merge({output} s,{input} a){{return _mm512_mask_{rounded}(s,0x55,a,8);}}\n"
            );
        }
    }
    for (suffix, ty, table) in [("ps", "__m512", "__m512i"), ("pd", "__m512d", "__m512i")] {
        for op in [
            "getexp",
            "getmant",
            "scalef",
            "rcp14",
            "rsqrt14",
            "roundscale",
            "fixupimm",
        ] {
            let operands = match op {
                "getmant" => "a,_MM_MANT_NORM_p5_2,_MM_MANT_SIGN_nan",
                "scalef" => "a,b",
                "roundscale" => "a,0x20",
                "fixupimm" => "b,c,0x80",
                _ => "a",
            };
            let merge = if op == "fixupimm" { "a" } else { "s" };
            source += &format!(
                "{ty} {op}_{suffix}({ty} s,{ty} a,{ty} b,{table} c){{return _mm512_mask_{op}_{suffix}({merge},0x55,{operands});}}\n"
            );
            if !matches!(op, "rcp14" | "rsqrt14") {
                source += &format!(
                    "{ty} {op}_round_{suffix}({ty} s,{ty} a,{ty} b,{table} c){{return _mm512_mask_{op}_round_{suffix}({merge},0x55,{operands},8);}}\n"
                );
            }
        }
    }
    for bits in [32, 64] {
        for op in ["rol", "ror", "rolv", "rorv", "sllv", "srlv", "srav", "abs"] {
            let operands = if op == "abs" {
                "a"
            } else if matches!(op, "rol" | "ror") {
                "a,255"
            } else {
                "a,b"
            };
            source += &format!(
                "__m512i {op}_{bits}(__m512i s,__m512i a,__m512i b){{return _mm512_mask_{op}_epi{bits}(s,0x55,{operands});}}\n"
            );
        }
        source += &format!(
            "__m512i tern_{bits}(__m512i a,__m512i b,__m512i c){{return _mm512_mask_ternarylogic_epi{bits}(a,0x55,b,c,0xca);}}\nunsigned test_{bits}(__m512i a,__m512i b){{return _mm512_mask_test_epi{bits}_mask(0x55,a,b)|_mm512_mask_testn_epi{bits}_mask(0x55,a,b);}}\n"
        );
    }
    source += "__m512i mul_signed(__m512i s,__m512i a,__m512i b){return _mm512_mask_mul_epi32(s,0x55,a,b);}\n__m512i mul_unsigned(__m512i s,__m512i a,__m512i b){return _mm512_mask_mul_epu32(s,0x55,a,b);}\n";
    let target = Target::X86_64LinuxGnu;
    let mut module = parse(&source, target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline(&module, target, &pipeline).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    for expression in [
        "_mm512_mask_getmant_round_ps(a,1,a,n,0,8)",
        "_mm512_mask_getmant_round_ps(a,1,a,0,0,n)",
        "_mm512_mask_roundscale_round_ps(a,1,a,n,8)",
        "_mm512_mask_roundscale_round_ps(a,1,a,0,n)",
        "_mm512_mask_roundscale_round_ps(a,1,a,256,8)",
        "_mm512_mask_roundscale_round_ps(a,1,a,0,9)",
    ] {
        let invalid =
            format!("#include <immintrin.h>\n__m512 f(__m512 a,int n){{return {expression};}}");
        assert!(parse(&invalid, target).is_err(), "accepted {expression}");
    }
}

#[test]
fn avx512_ternary_wrapper_evaluates_operands_once() {
    let source = r#"
#include <immintrin.h>
int ternary_evaluations;
static __m512i get_vector(const void*p){ternary_evaluations++;return _mm512_loadu_si512(p);}
static __mmask16 get_mask(void){ternary_evaluations++;return 0xa55a;}
void ternary_once(const void*a,const void*b,const void*c,void*out){
 _mm512_storeu_si512(out,_mm512_mask_ternarylogic_epi32(get_vector(a),get_mask(),get_vector(b),get_vector(c),0x96));
}
"#;
    let mut module = parse(source, Target::X86_64LinuxGnu).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap();
    let object = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        #[cfg(target_arch = "x86_64")]
        if !std::is_x86_feature_detected!("avx512f") {
            eprintln!("SKIP native ternary header: AVX512F unavailable");
            return;
        }
        let directory =
            std::env::temp_dir().join(format!("keel-avx512-ternary-c-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("code.o"), object).unwrap();
        fs::write(directory.join("harness.c"),r#"
#include <stdint.h>
extern int ternary_evaluations;
extern void ternary_once(const void*,const void*,const void*,void*);
int main(void){uint32_t a[16],b[16],c[16],out[16];for(unsigned i=0;i<16;i++){a[i]=i*0x1234567u;b[i]=~a[i];c[i]=i*0x9e3779b9u;}ternary_once(a,b,c,out);if(ternary_evaluations!=4)return 1;for(unsigned i=0;i<16;i++){uint32_t want=(0xa55au>>i)&1?a[i]^b[i]^c[i]:a[i];if(out[i]!=want)return 2;}return 0;}
"#).unwrap();
        let output = Command::new("cc")
            .arg(directory.join("harness.c"))
            .arg(directory.join("code.o"))
            .arg("-o")
            .arg(directory.join("run"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            Command::new(directory.join("run"))
                .status()
                .unwrap()
                .success()
        );
    }
}
