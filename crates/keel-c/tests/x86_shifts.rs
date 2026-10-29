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
    h += include_str!("fixtures/x86_shifts_oracle.h");
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

struct Avx2Shift {
    width: u32,
    bits: u32,
    op: &'static str,
    // None uses a vector count. Some(n) uses the checked immediate spelling.
    immediate: Option<u32>,
    per_lane: bool,
}

fn avx2_shifts() -> Vec<Avx2Shift> {
    let mut cases = Vec::new();
    for (op, _, bits) in OPS {
        for immediate in [
            None,
            Some(0),
            Some(bits - 1),
            Some(bits),
            Some(bits + 1),
            Some(255),
        ] {
            cases.push(Avx2Shift {
                width: 256,
                bits,
                op,
                immediate,
                per_lane: false,
            });
        }
    }
    for width in [128, 256] {
        for (op, _, bits) in OPS {
            if bits == 16 {
                continue;
            }
            cases.push(Avx2Shift {
                width,
                bits,
                op,
                immediate: None,
                per_lane: true,
            });
        }
    }
    cases
}

// Both compilers see the same public calls. The harness also checks an unsigned
// scalar oracle, so a shared header or reference-compiler bug is not the oracle.
fn avx2_shift_sources() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\nint kshift_evaluations;\n");
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        source += &format!(
            "static __m{width}i kshift_read{width}(const void*p){{kshift_evaluations++;return {prefix}_loadu_si{width}(p);}}\n"
        );
    }
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\nextern int kshift_evaluations;\n",
    );
    let mut table = String::new();
    for (i, case) in avx2_shifts().iter().enumerate() {
        let Avx2Shift {
            width,
            bits,
            op,
            immediate,
            per_lane,
        } = case;
        let prefix = if *width == 128 { "_mm" } else { "_mm256" };
        let suffix = if immediate.is_some() {
            "i"
        } else if *per_lane {
            "v"
        } else {
            ""
        };
        let name = format!("{prefix}_{op}{suffix}_epi{bits}");
        let count_width = if *per_lane { *width } else { 128 };
        let ty = format!("__m{width}i");
        let count_ty = format!("__m{count_width}i");
        let args = immediate.map_or_else(|| "a,n".to_string(), |n| format!("a,{n}"));
        let memory_args = immediate.map_or_else(
            || format!("kshift_read{width}(a),kshift_read{count_width}(n)"),
            |n| format!("kshift_read{width}(a),{n}"),
        );
        source += &format!(
            "{ty} kshift{i}({ty} a,{count_ty} n){{(void)n;return {name}({args});}}\nvoid kshift_mem{i}(void*out,const void*a,const void*n){{(void)n;{prefix}_storeu_si{width}(out,{name}({memory_args}));}}\n"
        );
        harness += &format!(
            "extern {ty} kshift{i}({ty},{count_ty});extern {ty} cshift{i}({ty},{count_ty});extern void kshift_mem{i}(void*,const void*,const void*);\n"
        );
        for compiler in ['k', 'c'] {
            harness += &format!(
                "static void {compiler}abi{i}(void*out,const void*a,const void*n){{{ty} x,z;{count_ty} count;memcpy(&x,a,{});memcpy(&count,n,{});z={compiler}shift{i}(x,count);memcpy(out,&z,{});}}\n",
                width / 8,
                count_width / 8,
                width / 8
            );
        }
        let kind = match *op {
            "sll" => 0,
            "srl" => 1,
            "sra" => 2,
            _ => unreachable!(),
        };
        table += &format!(
            "{{\"{name}\",{},{bits},{kind},{},{},{},kabi{i},cabi{i},kshift_mem{i}}},\n",
            width / 8,
            count_width / 8,
            u8::from(*per_lane),
            immediate.map_or(-1, |n| n as i32)
        );
    }
    harness += include_str!("fixtures/x86_shifts_oracle.h");
    harness += "typedef void (*Kernel)(void*,const void*,const void*);\nstruct ShiftCase {const char*name;int bytes,bits,op,count_bytes,per_lane,immediate;Kernel keel,vendor,memory;};\nstatic const struct ShiftCase cases[]={\n";
    harness += &table;
    harness += "};\n";
    harness += include_str!("fixtures/x86_avx2_shifts_harness.c");
    (source, harness)
}

#[test]
fn avx2_shift_controls_shapes_and_feature_permissions() {
    let target = Target::X86_64LinuxGnu;
    let (source, _) = avx2_shift_sources();
    let module = compile(&source, target).unwrap();
    let text = module.to_string();
    for (_, op, _) in OPS {
        assert!(text.contains(&format!("x86_avx2_v{op}256")), "{op}");
    }
    let features = Features::for_level(CpuLevel::V3);
    for allowed in [true, false] {
        let mut features = features;
        features.avx2 = allowed;
        let selector = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features,
        };
        let result = keel_codegen::compile_program_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            },
        );
        if allowed {
            keel_asm::x86::assemble_program_object_with_features(&result.unwrap(), features)
                .unwrap();
        } else {
            let error = result.unwrap_err();
            assert!(error.message.contains("avx2"), "{error:?}");
        }
    }
    // Even the narrow per-lane form needs AVX2, independently of wide transport.
    let narrow = compile(
        "#include <immintrin.h>\n__m128i f(__m128i a,__m128i n){return _mm_sllv_epi32(a,n);}",
        target,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: features.with_features("-avx2").unwrap(),
    };
    assert!(
        keel_codegen::compile_program_with_pipeline(
            &narrow,
            target,
            &CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full
            }
        )
        .unwrap_err()
        .message
        .contains("avx2")
    );
    for (op, _, bits) in OPS {
        for control in ["n", "-1", "256", "(n++,1)", "(1/0)", "(1ULL<<64)"] {
            let error = compile(&format!("#include <immintrin.h>\n__m256i f(__m256i a,int n){{return _mm256_{op}i_epi{bits}(a,{control});}}"),target).unwrap_err();
            assert!(
                error.contains("integer constant expression"),
                "{control}: {error}"
            );
        }
    }
    for call in [
        "__builtin_keel_avx2_count_psllw(a,1ULL)", // Wrong lane shape.
        "__builtin_keel_avx2_imm_pslld(a,1,2)",
        "__builtin_keel_avx2_count_pslld(a,1)", // Requires unsigned 64-bit count.
        "__builtin_keel_avx2_psllvd256(a,1)",
        "__builtin_keel_avx2_psllvd128(a,a)", // Wrong width.
    ] {
        assert!(
            compile(
                &format!(
                    "typedef int V __attribute__((vector_size(32)));V f(V a){{return {call};}}"
                ),
                target
            )
            .is_err(),
            "{call}"
        );
    }
    for foreign in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        assert!(compile("typedef int V __attribute__((vector_size(16)));V f(V a){return __builtin_keel_avx2_psllvd128(a,a);}",foreign).is_err());
        compile("#if __has_builtin(__builtin_keel_avx2_count_pslld) || __has_builtin(__builtin_keel_avx2_psllvd128)\n#error foreign builtin\n#endif\nint f(void){return 0;}",foreign).unwrap();
    }
    compile("#if !__has_builtin(__builtin_keel_avx2_count_pslld) || !__has_builtin(__builtin_keel_avx2_psllvd128)\n#error missing builtin\n#endif\nint f(void){return 0;}",target).unwrap();
}

#[test]
fn avx2_shifts_match_vendor_scalar_and_object_routes() {
    use std::{fs, path::PathBuf, process::Command};
    let directory = std::env::var_os("KEEL_C_AVX2_SHIFT_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-avx2-shifts"));
    fs::create_dir_all(&directory).unwrap();
    let (source, harness) = avx2_shift_sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(
        directory.join("oracle.c"),
        source.replace("kshift", "cshift"),
    )
    .unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    let module = compile(&source, Target::X86_64LinuxGnu).unwrap();
    let mut optimized = module.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let features = Features::for_level(CpuLevel::V3);
    let empty = keel_codegen::regalloc::Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for (name, tier, allocator, spill, module) in [
        (
            "baseline",
            SelectionTier::Baseline,
            AllocationTier::Fast,
            false,
            &module,
        ),
        (
            "trees",
            SelectionTier::Trees,
            AllocationTier::Full,
            false,
            &module,
        ),
        (
            "optimized-spill",
            SelectionTier::Full,
            AllocationTier::Full,
            true,
            &optimized,
        ),
    ] {
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &allocator,
        };
        let pools = if spill {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
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
            &mut |chunk| chunks.push(chunk),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        fs::write(directory.join(format!("{name}.o")), object).unwrap();
    }
    // The oracle is freestanding; the protected-page harness needs Linux libc
    // and is compiled natively below or by the remote native runner.
    let output = Command::new("clang")
        .current_dir(&directory)
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-mavx2",
            "-O2",
            "-c",
            "oracle.c",
            "-o",
            "oracle.o",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("avx2") {
        for compiler in ["clang", "gcc"] {
            let output = Command::new(compiler)
                .current_dir(&directory)
                .args(["-mavx2", "-O2", "-c", "harness.c", "-o", "harness.o"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(compiler)
                .current_dir(&directory)
                .args(["-mavx2", "-O2", "-c", "oracle.c", "-o", "native-oracle.o"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            for name in ["baseline", "trees", "optimized-spill"] {
                let output = Command::new(compiler)
                    .current_dir(&directory)
                    .args([
                        "harness.o",
                        "native-oracle.o",
                        &format!("{name}.o"),
                        "-o",
                        "run",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let output = Command::new(directory.join("run")).output().unwrap();
                assert!(output.status.success(), "{compiler} {name}: {output:?}");
            }
        }
    } else {
        eprintln!("native AVX2 execution skipped: host lacks AVX2");
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    eprintln!("native AVX2 execution skipped: requires an x86-64 Linux host");
    eprintln!("AVX2 shift artifacts: {}", directory.display());
}
