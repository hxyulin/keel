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

#[test]
fn aes_keyassist_reuses_defined_bounded_ice_proof() {
    let target = Target::X86_64LinuxGnu;
    let source = |control: &str| {
        format!(
            "#include <wmmintrin.h>\nenum {{ HI=16 }}; __m128i f(__m128i a,__m128i b,int n){{return _mm_aeskeygenassist_si128(a,{control});}}\n"
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
        let m = compile(&source(control), target).unwrap_or_else(|e| panic!("{control}: {e}"));
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
        "(1/0)",
        "(int)(__int128)17",
        "(int)(void*)17",
        "1 ? 17 : n",
        "(n,17)",
    ] {
        assert!(
            compile(&source(control), target).is_err(),
            "accepted {control}"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_c_wrappers_independent_oracle_and_single_evaluation() {
    use std::{fs, process::Command};
    if !std::is_x86_feature_detected!("aes") || !std::is_x86_feature_detected!("sse2") {
        return;
    }
    let ops = [
        "aesenc",
        "aesenclast",
        "aesdec",
        "aesdeclast",
        "aesimc",
        "aeskeygenassist",
    ];
    let mut source = String::from(
        "#include <wmmintrin.h>\nint eval_count; static __m128i read(const void*p){__m128i a;eval_count++;__builtin_memcpy(&a,p,16);return a;}\n__m128i once(const void*p){return _mm_aeskeygenassist_si128(read(p),255);}\n",
    );
    let mut harness = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\ntypedef unsigned char u8; typedef long long V __attribute__((vector_size(16)));\n",
    );
    for (i, op) in ops.iter().enumerate() {
        for n in 0..if i == 5 { 256 } else { 1 } {
            let params = if i < 4 {
                "__m128i a,__m128i b"
            } else {
                "__m128i a"
            };
            let args = if i < 4 {
                "a,b".to_string()
            } else if i == 4 {
                "a".into()
            } else {
                format!("a,{n}")
            };
            source += &format!("__m128i aes_{i}_{n}({params}){{return _mm_{op}_si128({args});}}\n");
            harness += &format!(
                "extern V aes_{i}_{n}(V{});\n",
                if i < 4 { ",V" } else { "" }
            );
        }
        for alias in 0..2 {
            let args = if i < 4 {
                if alias == 0 { "a,b" } else { "a,a" }
            } else {
                "a"
            };
            source += &format!(
                "void live_{i}_{alias}(const void*p,const void*q,void*out){{__m128i a=read(p),b=read(q),z=aes_{i}_0({args});__builtin_memcpy(out,&z,16);__builtin_memcpy((char*)out+16,&a,16);__builtin_memcpy((char*)out+32,&b,16);}}\n"
            );
            harness += &format!("extern void live_{i}_{alias}(const void*,const void*,void*);\n");
        }
        let args = if i < 4 { "read(p),read(q)" } else { "read(p)" };
        source += &format!(
            "void aesmem_{i}(const void*p,const void*q,void*out){{__m128i z=aes_{i}_0({args});__builtin_memcpy(out,&z,16);}}\n"
        );
        harness += &format!("extern void aesmem_{i}(const void*,const void*,void*);\n");
    }
    source += "extern __m128i callback(__m128i,__m128i);\n__m128i direct(__m128i a,__m128i b){__m128i c=callback(a,b);return _mm_aesenc_si128(a,c);}\n__m128i indirect(__m128i a,__m128i b,__m128i(*f)(__m128i,__m128i)){__m128i c=f(a,b);return _mm_aesdec_si128(b,c);}\n";
    harness += "static V(*kg[256])(V)={";
    for n in 0..256 {
        harness += &format!("aes_5_{n},");
    }
    harness += "};\n";
    harness += &include_str!("../../keel-codegen/tests/fixtures/x86_aes_harness.c")
        .replace("int main(void)", "static int oracle_main(void)");
    harness += "extern int eval_count; extern V once(const void*); int main(void){int e=oracle_main();if(e)return e;u8 a[16]={0},out[16],expected[16];eval_count=0;bytes(out,once(a));ref(5,a,a,255,expected);return eval_count==1&&eq(out,expected,16)?0:99;}\n";
    let module = compile(&source, Target::X86_64LinuxGnu).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-aes-native-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let empty = keel_codegen::regalloc::Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for (tier, alloc, spilled) in [
        (SelectionTier::Baseline, AllocationTier::Fast, false),
        (SelectionTier::Full, AllocationTier::Full, true),
    ] {
        let mut m = module.clone();
        if tier == SelectionTier::Full {
            keel_opt::optimize(&mut m).unwrap();
        }
        let f = Features::for_level(CpuLevel::V1)
            .with_features("+aes")
            .unwrap();
        let sel = FeatureSelector {
            inner: &tier,
            features: f,
        };
        let p = CodegenPipeline {
            selector: &sel,
            allocator: &alloc,
        };
        let pools = if spilled {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
        let object = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
        fs::write(dir.join("aes.o"), object).unwrap();
        for cc in ["clang", "gcc"] {
            let output = Command::new(cc)
                .args(["-O2", "-march=x86-64", "-mno-avx", "-fno-tree-vectorize"])
                .arg(dir.join("harness.c"))
                .arg(dir.join("aes.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(dir.join("run")).output().unwrap();
            assert!(
                output.status.success(),
                "{tier:?} {alloc:?} {cc}: {output:?}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
