//! Bounded C SSE2 saturation: frontend checks, strict-v1 encoding, native oracles.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, process::Command};

const OPS: [(&str, &str, &str); 8] = [
    ("adds_epi8", "paddsb", "signed char"),
    ("adds_epi16", "paddsw", "short"),
    ("adds_epu8", "paddusb", "unsigned char"),
    ("adds_epu16", "paddusw", "unsigned short"),
    ("subs_epi8", "psubsb", "signed char"),
    ("subs_epi16", "psubsw", "short"),
    ("subs_epu8", "psubusb", "unsigned char"),
    ("subs_epu16", "psubusw", "unsigned short"),
];
fn compile(source: &str, target: Target) -> Result<keel_ir::Module, String> {
    let pp = keel_c::preprocess(
        source,
        "sat.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = keel_c::compile(
        &pp,
        "sat.c",
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
fn checked_types_arity_targets_and_single_evaluation() {
    let t = Target::X86_64LinuxGnu;
    for (wrapper, op, elem) in OPS {
        let name = format!("__builtin_keel_sse2_{op}");
        let prefix = format!("typedef {elem} V __attribute__((vector_size(16)));\n");
        compile(&format!("{prefix}_Static_assert(__has_builtin({name}),\"recognized\"); V f(V a,V b){{return {name}(a,b);}}"), t).unwrap();
        for args in [
            "", "a", "a,a,a", "a,1", "1,a", "a,b", "b,a", "a,c", "a,d", "a,e",
        ] {
            let src = format!(
                "{prefix}typedef float F __attribute__((vector_size(16))); typedef long long L __attribute__((vector_size(16))); typedef int I __attribute__((vector_size(16))); typedef {elem} W __attribute__((vector_size(32))); V f(V a,F b,L c,I d,W e){{return {name}({args});}}"
            );
            assert!(compile(&src, t).is_err(), "{name}({args})");
        }
        // Equal IR width/shape does not erase C signedness or nominal char identity.
        for other in [
            "char",
            "signed char",
            "unsigned char",
            "short",
            "unsigned short",
            "int",
            "unsigned int",
        ] {
            if other == elem {
                continue;
            }
            let src = format!(
                "{prefix}typedef {other} U __attribute__((vector_size(16))); V f(U a){{return {name}(a,a);}}"
            );
            assert!(compile(&src, t).is_err(), "accepted {other} for {op}");
        }
        for target in [Target::Aarch64LinuxGnu, Target::Aarch64AppleDarwin] {
            compile(
                &format!(
                    "_Static_assert(!__has_builtin({name}),\"foreign\"); int f(void){{return 0;}}"
                ),
                target,
            )
            .unwrap();
            assert!(compile(&format!("{prefix}V f(V a){{return {name}(a,a);}}"), target).is_err());
            assert!(compile("#include <emmintrin.h>\nint f(void){return 0;}", target).is_err());
        }
        for arg in [
            "(__m128d){0}",
            "(__m128){0}",
            "(__keel_i32_128){0}",
            "(LongVector){0}",
            "1",
        ] {
            assert!(compile(&format!("#include <emmintrin.h>\ntypedef long LongVector __attribute__((vector_size(16))); __m128i f(__m128i a){{return _mm_{wrapper}({arg},a);}}"),t).is_err());
            assert!(compile(&format!("#include <emmintrin.h>\ntypedef long LongVector __attribute__((vector_size(16))); __m128i f(__m128i a){{return _mm_{wrapper}(a,{arg});}}"),t).is_err());
        }
        let m = compile(&format!("#include <immintrin.h>\nextern __m128i next(void); __m128i f(void){{return _mm_{wrapper}(next(),next());}}"),t).unwrap().to_string();
        assert_eq!(m.matches("call @next()").count(), 2, "{m}");
    }
}
#[test]
fn architectural_mapping_order_and_header_composition() {
    let target = Target::X86_64LinuxGnu;
    for (wrapper, op, _) in OPS {
        let name = format!("x86_sse2_{op}128");
        let id = keel_ir::Intrinsic::from_name(&name).unwrap();
        assert_eq!(id.x86_packed_pair(), Some((op, 0)));
        let m = compile(&format!("#include <emmintrin.h>\nextern __m128i left(void),right(void); __m128i f(void){{return _mm_{wrapper}(left(),right());}}"), target).unwrap();
        let ir = m.to_string();
        assert_eq!(ir.matches(&name).count(), 1, "{ir}");
        assert_eq!(ir.matches("call @left()").count(), 1, "{ir}");
        assert_eq!(ir.matches("call @right()").count(), 1, "{ir}");
        assert!(ir.find("call @left()") < ir.find("call @right()"), "{ir}");
    }
    for headers in [
        "#include <emmintrin.h>\n#include <wmmintrin.h>\n#include <immintrin.h>\n",
        "#include <immintrin.h>\n#include <wmmintrin.h>\n#include <emmintrin.h>\n",
    ] {
        let mut src = headers.to_owned();
        for (wrapper, _, _) in OPS {
            src +=
                &format!("__m128i {wrapper}(__m128i a,__m128i b){{return _mm_{wrapper}(a,b);}}\n");
        }
        src += "__m128i shift(__m128i a){return _mm_slli_epi16(a,3);} __m128i aes(__m128i a,__m128i b){return _mm_aesenc_si128(a,b);} __m128i clmul(__m128i a,__m128i b){return _mm_clmulepi64_si128(a,b,17);}";
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
fn source() -> String {
    let mut s = String::from(
        "#include <emmintrin.h>\nint eval_count; static __m128i read(const void*p){eval_count++;return _mm_loadu_si128((const __m128i*)p);}\nextern __m128i callback(__m128i,__m128i);\n",
    );
    for (i, (wrapper, _, _)) in OPS.iter().enumerate() {
        s += &format!(
            "__m128i sat_{i}(__m128i a,__m128i b){{return _mm_{wrapper}(a,b);}}\nvoid mem_{i}(const void*p,const void*q,void*out){{_mm_storeu_si128((__m128i*)out,_mm_{wrapper}(read(p),read(q)));}}\nvoid live_{i}(const void*p,const void*q,void*out){{__m128i a=read(p),b=read(q),c=callback(a,b),z=_mm_{wrapper}(a,b);_mm_storeu_si128((__m128i*)out,z);_mm_storeu_si128((__m128i*)((char*)out+16),a);_mm_storeu_si128((__m128i*)((char*)out+32),b);_mm_storeu_si128((__m128i*)((char*)out+48),c);}}\n__m128i alias_{i}(__m128i a){{return _mm_{wrapper}(a,a);}}\n__m128i indirect_{i}(__m128i a,__m128i b,__m128i(*f)(__m128i,__m128i)){{__m128i c=f(a,b);return _mm_{wrapper}(a,c);}}\n"
        );
    }
    s
}
#[test]
fn strict_v1_parity_and_native_oracles() {
    let module = compile(&source(), Target::X86_64LinuxGnu).unwrap();
    let dir = std::env::var_os("KEEL_C_SAT_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("keel-c-sat-{}", std::process::id())));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), source()).unwrap();
    let mut harness = String::from(
        "#include <emmintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    for (i, (wrapper, _, _)) in OPS.iter().enumerate() {
        harness += &format!(
            "extern __m128i sat_{i}(__m128i,__m128i),alias_{i}(__m128i),indirect_{i}(__m128i,__m128i,__m128i(*)(__m128i,__m128i)); extern void mem_{i}(const void*,const void*,void*),live_{i}(const void*,const void*,void*); static __m128i vendor_{i}(__m128i a,__m128i b){{return _mm_{wrapper}(a,b);}}\n"
        );
    }
    harness += "typedef __m128i(*binary)(__m128i,__m128i);\n";
    for name in ["sat", "vendor"] {
        harness += &format!("static binary {name}[8]={{");
        for i in 0..8 {
            harness += &format!("{name}_{i},");
        }
        harness += "};\n";
    }
    harness += "static __m128i(*alias[8])(__m128i)={";
    for i in 0..8 {
        harness += &format!("alias_{i},");
    }
    harness += "};\n";
    harness += "static __m128i(*indirect[8])(__m128i,__m128i,binary)={";
    for i in 0..8 {
        harness += &format!("indirect_{i},");
    }
    harness += "};\n";
    for name in ["mem", "live"] {
        harness += &format!("static void(*{name}[8])(const void*,const void*,void*)={{");
        for i in 0..8 {
            harness += &format!("{name}_{i},");
        }
        harness += "};\n";
    }
    harness += include_str!("../fixtures/x86_saturation_harness.c");
    fs::write(dir.join("harness.c"), harness).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for (label, tier, alloc, spilled) in [
        (
            "baseline",
            SelectionTier::Baseline,
            AllocationTier::Fast,
            false,
        ),
        ("trees", SelectionTier::Trees, AllocationTier::Fast, false),
        ("optimized", SelectionTier::Full, AllocationTier::Full, true),
    ] {
        let mut m = module.clone();
        if tier == SelectionTier::Full {
            keel_opt::optimize(&mut m).unwrap();
        }
        keel_ir::verify_module(&m).unwrap();
        let features = Features::for_level(CpuLevel::V1);
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let pools = if spilled {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
        assert_eq!(program.stats().text_bytes, 0);
        let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        let mut chunks = program.clone();
        chunks.items.clear();
        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
            &m,
            pools,
            &pipeline,
            &mut |p| chunks.items.extend(p.items),
        )
        .unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
        );
        for (_, op, _) in OPS {
            assert!(text.contains(op), "missing {op}: {text}");
        }
        fs::write(dir.join(format!("{label}.o")), object).unwrap();
        fs::write(dir.join(format!("{label}.s")), text).unwrap();
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            for cc in ["gcc", "clang"] {
                let result = Command::new(cc)
                    .args(["-O2", "-march=x86-64", "-mno-avx", "-fno-tree-vectorize"])
                    .arg(dir.join("harness.c"))
                    .arg(dir.join(format!("{label}.o")))
                    .arg("-o")
                    .arg(dir.join(format!("{cc}-{label}")))
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{cc}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = Command::new(dir.join(format!("{cc}-{label}")))
                    .output()
                    .unwrap();
                assert!(result.status.success(), "{cc} {label}: {result:?}");
            }
        }
    }
}
