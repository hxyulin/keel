use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;

const OPS: [(&str, &str, u32); 8] = [
    ("sll", "psllw", 16),
    ("sll", "pslld", 32),
    ("sll", "psllq", 64),
    ("srl", "psrlw", 16),
    ("srl", "psrld", 32),
    ("srl", "psrlq", 64),
    ("sra", "psraw", 16),
    ("sra", "psrad", 32),
];
fn compile(src: &str, target: Target) -> Result<keel_ir::Module, String> {
    let pp = keel_c::preprocess(
        src,
        "shifts.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = keel_c::compile(
        &pp,
        "shifts.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    keel_ir::verify_module(&m).map_err(|e| format!("{e:?}"))?;
    Ok(m)
}
fn source() -> String {
    let mut s = String::from(
        "#include <emmintrin.h>\nint evaluations; static __m128i read(const void*p){evaluations++;return _mm_loadu_si128(p);}\n",
    );
    for (i, (op, _, bits)) in OPS.iter().enumerate() {
        s += &format!(
            "__m128i shift{i}(__m128i a,__m128i n){{return _mm_{op}_epi{bits}(a,n);}}\nvoid memory{i}(void*out,const void*a,const void*n){{_mm_storeu_si128(out,_mm_{op}_epi{bits}(read(a),read(n)));}}\n"
        );
        for n in [0, bits - 1, *bits, bits + 1, 255] {
            s += &format!("__m128i imm{i}_{n}(__m128i a){{return _mm_{op}i_epi{bits}(a,{n});}}\n");
        }
    }
    s += "extern __m128i callback(__m128i,__m128i); __m128i direct(__m128i a,__m128i n){return _mm_sll_epi16(callback(a,n),n);}\n__m128i indirect(__m128i a,__m128i n,__m128i(*f)(__m128i,__m128i)){return _mm_srl_epi64(f(a,n),n);}\n__m128i once(const void*p){return _mm_slli_epi32(read(p),7);}\n";
    s
}
#[test]
fn checked_controls_shapes_targets_and_strict_sse2() {
    let target = Target::X86_64LinuxGnu;
    let m = compile(&source(), target).unwrap();
    for (_, op, _) in OPS {
        assert!(m.to_string().contains(&format!("x86_sse2_{op}128")));
    }
    let f = Features::for_level(CpuLevel::V1);
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: f,
    };
    let program = keel_codegen::compile_program_with_pipeline(
        &m,
        target,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    for (op, _, bits) in OPS {
        for n in [
            "n",
            "-1",
            "256",
            "1.0",
            "(n++,1)",
            "(1<<31)",
            "(1/0)",
            "(int)(__int128)1",
            "(int)(1.0+1.0)",
        ] {
            let err=compile(&format!("#include <emmintrin.h>\n__m128i f(__m128i a,int n){{return _mm_{op}i_epi{bits}(a,{n});}}"),target).unwrap_err();
            assert!(err.contains("integer constant expression"), "{n}: {err}");
        }
    }
    assert!(
        compile(
            "#include <emmintrin.h>\n__m128i f(__m128d a){return _mm_slli_epi16(a,1);}",
            target
        )
        .is_err()
    );
    for expr in [
        "__builtin_keel_sse2_count_psllw(a,1ULL)",
        "__builtin_keel_sse2_count_pslld(a,1)",
        "__builtin_keel_sse2_imm_pslld((F){0},1)",
    ] {
        assert!(compile(&format!("typedef int V __attribute__((vector_size(16)));typedef float F __attribute__((vector_size(16))); V f(V a){{return {expr};}}"),target).is_err());
    }
    compile(
        "#include <emmintrin.h>\nenum{N=15};__m128i f(__m128i a){return _mm_slli_epi16(a,1?N:0);}",
        target,
    )
    .unwrap();
    let once=compile("#include <emmintrin.h>\nextern __m128i next(void);__m128i f(void){return _mm_slli_epi16(next(),3);}",target).unwrap().to_string();
    assert_eq!(once.matches("call @next()").count(), 1);
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        assert!(
            compile("#include <emmintrin.h>\n", target)
                .unwrap_err()
                .contains("require x86-64")
        );
        assert!(compile("typedef short V __attribute__((vector_size(16)));V f(V a){return __builtin_keel_sse2_count_psllw(a,1ULL);}",target).is_err());
        compile("#if __has_builtin(__builtin_keel_sse2_count_psllw)\n#error foreign builtin\n#endif\nint f(void){return 0;}",target).unwrap();
    }
    compile("#if !__has_builtin(__builtin_keel_sse2_count_psllw)\n#error missing builtin\n#endif\nint f(void){return 0;}",Target::X86_64LinuxGnu).unwrap();
}

// Review the private bridge independently of casts inserted by vendor wrappers.
#[test]
fn private_bridge_contract_and_header_coexistence() {
    let target = Target::X86_64LinuxGnu;
    for (_, op, bits) in OPS {
        let element = match bits {
            16 => "short",
            32 => "int",
            _ => "long long",
        };
        for mode in ["imm", "count"] {
            let name = format!("__builtin_keel_sse2_{mode}_{op}");
            let prefix = format!(
                "typedef {element} V __attribute__((vector_size(16))); V f(V a,unsigned long long n){{return "
            );
            for args in ["a", "a,1ULL,2ULL", "1,1ULL", "a,(void*)0", "a,a"] {
                let src = format!("{prefix}{name}({args});}}");
                assert!(compile(&src, target).is_err(), "accepted {src}");
            }
            let src = format!(
                "#if !__has_builtin({name})\n#error missing shift\n#endif\n{prefix}{name}(a,1ULL);}}"
            );
            compile(&src, target).unwrap();
            for foreign in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
                assert!(compile(&src, foreign).is_err());
                compile(&format!("#if __has_builtin({name})\n#error foreign shift\n#endif\nint f(void){{return 0;}}"), foreign).unwrap();
            }
        }
    }
    for control in [
        "(1ULL<<64)",
        "(1<<-1)",
        "(1<<32)",
        "(-1<<1)&255",
        "(2147483647+1)&255",
        "(-2147483647-2)&255",
        "(1073741824*4)&255",
        "(-(-2147483647-1))&255",
        "((-2147483647-1)/-1)&255",
        "((-2147483647-1)%-1)&255",
        "(unsigned char)1e300",
        "(int)2147483648.0",
        "(unsigned long long)18446744073709551616.0",
        "(int)(void*)17",
        "(int)(((__int128)1<<64)/256)",
        "1?17:n",
        "(n,17)",
    ] {
        let src = format!(
            "#include <emmintrin.h>\n__m128i f(__m128i a,int n){{return _mm_slli_epi16(a,{control});}}"
        );
        assert!(compile(&src, target).is_err(), "accepted {control}");
    }
    for control in [
        "(int)17.0",
        "sizeof(long long)+9",
        "(unsigned char)273",
        "(0xffffffffffffffffULL+18)&255",
        "(1ULL<<63)>>63",
        "1?17:(1/0)",
        "0&&(1/0)",
        "1||(1/0)",
    ] {
        compile(&format!("#include <emmintrin.h>\n__m128i f(__m128i a){{return _mm_slli_epi16(a,{control});}}"), target).unwrap();
    }
    for headers in [
        ["emmintrin.h", "wmmintrin.h", "immintrin.h"],
        ["immintrin.h", "wmmintrin.h", "emmintrin.h"],
    ] {
        let mut src = headers.map(|h| format!("#include <{h}>\n")).concat();
        src += "__m128i f(__m128i a,__m128i n){return _mm_sra_epi32(_mm_slli_epi16(a,17),n);}";
        // Exercise the public frontend result directly, without verifier repair.
        let pp = keel_c::preprocess(
            &src,
            "review.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let m = keel_c::compile(
            &pp,
            "review.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        for features in [
            Features::for_level(CpuLevel::V1),
            Features::for_level(CpuLevel::V3)
                .with_features("-avx2,-f16c,-fma,-avx,-aes,-pclmulqdq")
                .unwrap(),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features,
            };
            let p = keel_codegen::compile_program_with_pipeline(
                &m,
                target,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                },
            )
            .unwrap();
            keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
        }
        src += "__m128i aes(__m128i a,__m128i b){return _mm_aesenc_si128(a,b); } __m128i clmul(__m128i a,__m128i b){return _mm_clmulepi64_si128(a,b,17);}";
        let m = compile(&src, target).unwrap();
        for enabled in [false, true] {
            let features = Features::for_level(CpuLevel::V1)
                .with_features(if enabled { "+aes,+pclmulqdq" } else { "none" })
                .unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features,
            };
            let p = keel_codegen::compile_program_with_pipeline(
                &m,
                target,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                },
            );
            if enabled {
                keel_asm::x86::assemble_program_object_with_features(&p.unwrap(), features)
                    .unwrap();
            } else {
                assert!(p.is_err());
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_vendor_reference_abi_and_footprints() {
    use std::{fs, process::Command};
    let mut h = String::from(
        "#include <emmintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nextern int evaluations;extern __m128i once(const void*);\n",
    );
    h += "typedef __m128i V; V callback(V a,V n){return _mm_xor_si128(a,n);}extern V direct(V,V);extern V indirect(V,V,V(*)(V,V));\n";
    for (i, (op, _, bits)) in OPS.iter().enumerate() {
        h += &format!(
            "extern V shift{i}(V,V);extern void memory{i}(void*,const void*,const void*);static V vendor{i}(V a,V n){{return _mm_{op}_epi{bits}(a,n);}}\n"
        );
        for n in [0, bits - 1, *bits, bits + 1, 255] {
            h += &format!(
                "extern V imm{i}_{n}(V);static V vendor{i}_{n}(V a){{return _mm_{op}i_epi{bits}(a,{n});}}\n"
            );
        }
    }
    h += "static V(*shifts[])(V,V)={shift0,shift1,shift2,shift3,shift4,shift5,shift6,shift7};static V(*vendors[])(V,V)={vendor0,vendor1,vendor2,vendor3,vendor4,vendor5,vendor6,vendor7};static void(*memories[])(void*,const void*,const void*)={memory0,memory1,memory2,memory3,memory4,memory5,memory6,memory7};\n";
    h += include_str!("fixtures/x86_shifts_harness.c");
    h += "int main(void){int rc=run();if(rc)return rc;V a=_mm_set_epi32(-1,0x80000000,0x7fffffff,1);V z;\n";
    for (i, (_, _, bits)) in OPS.iter().enumerate() {
        for n in [0, bits - 1, *bits, bits + 1, 255] {
            h += &format!(
                "z=imm{i}_{n}(a);V e{i}_{n}=vendor{i}_{n}(a);if(memcmp(&z,&e{i}_{n},16))return 80;\n"
            );
        }
    }
    h += "evaluations=0;z=once(&a);V e=_mm_slli_epi32(a,7);if(evaluations!=1||memcmp(&z,&e,16))return 81;return 0;}\n";
    let dir = std::env::temp_dir().join(format!("keel-c-shifts-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), h).unwrap();
    let module = compile(&source(), Target::X86_64LinuxGnu).unwrap();
    let empty = keel_codegen::regalloc::Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for (tier, alloc, spill) in [
        (SelectionTier::Baseline, AllocationTier::Fast, false),
        (SelectionTier::Trees, AllocationTier::Full, false),
        (SelectionTier::Full, AllocationTier::Full, true),
    ] {
        let mut m = module.clone();
        if tier == SelectionTier::Full {
            keel_opt::optimize(&mut m).unwrap();
        }
        let f = Features::for_level(CpuLevel::V1);
        let selector = FeatureSelector {
            inner: &tier,
            features: f,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let p = keel_codegen::x86_64::compile_program_with_pipeline(
            &m,
            if spill {
                &empty
            } else {
                &keel_codegen::x86_64::REGISTERS
            },
            &pipeline,
        )
        .unwrap();
        fs::write(
            dir.join(format!("{tier:?}.o")),
            keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap(),
        )
        .unwrap();
        for cc in ["clang", "gcc"] {
            let out = Command::new(cc)
                .args(["-O2", "-march=x86-64", "-mno-avx", "-fno-tree-vectorize"])
                .arg(dir.join("harness.c"))
                .arg(dir.join(format!("{tier:?}.o")))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{cc}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = Command::new(dir.join("run")).output().unwrap();
            assert!(out.status.success(), "{cc} {tier:?}: {out:?}");
        }
    }
    eprintln!("native artifacts: {}", dir.display());
}
