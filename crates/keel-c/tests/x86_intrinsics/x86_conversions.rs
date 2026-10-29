use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
const SOURCE: &str = include_str!("../fixtures/x86_conversions.c");
const NAMES: [&str; 3] = ["cvtdq2ps", "cvttps2dq", "cvtps2dq"];
fn compile(src: &str, target: Target) -> Result<keel_ir::Module, String> {
    let pp = keel_c::preprocess(
        src,
        "conversions.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = keel_c::compile(
        &pp,
        "conversions.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    Ok(m)
}
#[test]
fn typed_bridge_diagnostics_pinning_and_v1_objects() {
    let target = Target::X86_64LinuxGnu;
    let m = compile(SOURCE, target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for name in NAMES {
        assert!(m.to_string().contains(&format!("x86_sse_{name}128")));
        let id = keel_ir::Intrinsic::from_name(&format!("x86_sse_{name}128")).unwrap();
        assert!(!id.info().pure);
        assert_eq!(id.x86_legacy_raw_fp_opcode(), Some(name));
        let input = if name == "cvtdq2ps" {
            keel_ir::VectorType::I32x4
        } else {
            keel_ir::VectorType::F32x4
        };
        let output = if name == "cvtdq2ps" {
            keel_ir::VectorType::F32x4
        } else {
            keel_ir::VectorType::I32x4
        };
        assert_eq!(id.info().params, &[keel_ir::Type::Vector(input)]);
        assert_eq!(id.info().returns, &[keel_ir::Type::Vector(output)]);
    }
    for f in &m.functions {
        for (_, inst) in f.layout_insts() {
            let data = f.inst_data(inst);
            if matches!(data, keel_ir::InstData::Intrinsic { id, .. } if id.is_x86_legacy_raw_fp_convert())
            {
                assert!(data.is_pinned());
                assert!(data.may_trap());
                assert_eq!(data.memory_effects(), keel_ir::MemoryEffects::Barrier);
            }
        }
    }
    for (index, name) in NAMES.iter().enumerate() {
        let base = format!("__builtin_keel_sse2_{name}");
        let elem = if index == 0 { "int" } else { "float" };
        let prefix = format!("typedef {elem} V __attribute__((vector_size(16))); V f(V a){{");
        for args in [
            "",
            "a,a",
            "1",
            "(double)1",
            "(U){0}",
            "(W){0}",
            "(D){0}",
            "(L){0}",
            "(S){0}",
        ] {
            let src = format!(
                "typedef unsigned int U __attribute__((vector_size(16)));typedef int W __attribute__((vector_size(32)));typedef double D __attribute__((vector_size(16)));typedef long long L __attribute__((vector_size(16)));typedef short S __attribute__((vector_size(16)));{prefix}{base}({args});return a;}}"
            );
            let error = compile(&src, target).unwrap_err();
            assert!(
                error.contains(if args.is_empty() || args == "a,a" {
                    "argument"
                } else {
                    "SSE2 packed conversion requires"
                }),
                "wrong diagnostic: {error}"
            );
        }
        // Prove the declarations themselves are valid before testing bad calls.
        compile(&format!("typedef unsigned int U __attribute__((vector_size(16)));typedef int W __attribute__((vector_size(32)));typedef double D __attribute__((vector_size(16)));typedef long long L __attribute__((vector_size(16)));typedef short S __attribute__((vector_size(16)));{prefix}return a;}}"), target).unwrap();
        let src = format!(
            "#if !__has_builtin({base})\n#error missing conversion\n#endif\n{prefix}{base}(a);return a;}}"
        );
        compile(&src, target).unwrap();
        for arm in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
            assert!(compile(&src, arm).is_err());
            assert!(
                compile(&format!("{prefix}{base}(a);return a;}}"), arm)
                    .unwrap_err()
                    .contains("x86-64")
            );
            compile(&format!("#if __has_builtin({base})\n#error foreign builtin\n#endif\nint f(void){{return 0;}}"),arm).unwrap();
            assert!(
                compile("#include <emmintrin.h>\n", arm)
                    .unwrap_err()
                    .contains("require x86-64")
            );
        }
    }
    for headers in [
        "#include <emmintrin.h>\n#include <wmmintrin.h>\n#include <immintrin.h>\n",
        "#include <immintrin.h>\n#include <wmmintrin.h>\n#include <emmintrin.h>\n",
    ] {
        compile(&format!("{headers}{SOURCE}"), target).unwrap();
    }
    // Public frontend -> backend: no verifier is needed to repair the IR.
    for features in [
        Features::for_level(CpuLevel::V1),
        Features::for_level(CpuLevel::V3)
            .with_features("-avx2,-avx,-fma,-f16c,-aes,-pclmulqdq")
            .unwrap(),
    ] {
        for tier in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let mut m = m.clone();
            if tier == SelectionTier::Full {
                keel_opt::optimize(&mut m).unwrap();
            }
            for name in NAMES {
                assert!(m.to_string().contains(&format!("x86_sse_{name}128")));
            }
            let selector = FeatureSelector {
                inner: &tier,
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
            assert_eq!(p.stats().text_bytes, 0);
            let object =
                keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let text = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            for name in NAMES {
                assert!(text.contains(name));
            }
            let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
            keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
                &mut |p| chunks.push(p),
            )
            .unwrap();
            assert_eq!(object, chunks.finish().unwrap());
        }
    }
    let once = compile("#include <emmintrin.h>\nextern __m128i next(void);__m128 f(void){return _mm_cvtepi32_ps(next());}",target).unwrap().to_string();
    assert_eq!(once.matches("call @next()").count(), 1);
    for src in [
        "__m128 f(__m128 a){return _mm_cvtepi32_ps(a);}",
        "__m128i f(__m128i a){return _mm_cvttps_epi32(a);}",
        "__m128i f(__m128d a){return _mm_cvtps_epi32(a);}",
    ] {
        assert!(compile(&format!("#include <emmintrin.h>\n{src}"), target).is_err());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_bits_flags_vendor_abi_and_exact_memory() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-c-conversions-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("vendor.c"), SOURCE.replace("k_", "v_")).unwrap();
    fs::write(
        dir.join("harness.c"),
        include_str!("../fixtures/x86_conversions_harness.c"),
    )
    .unwrap();
    let module = compile(SOURCE, Target::X86_64LinuxGnu).unwrap();
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
        let features = Features::for_level(CpuLevel::V1);
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let p = keel_codegen::x86_64::compile_program_with_pipeline(
            &m,
            if spill {
                &empty
            } else {
                &keel_codegen::x86_64::REGISTERS
            },
            &CodegenPipeline {
                selector: &selector,
                allocator: &alloc,
            },
        )
        .unwrap();
        fs::write(
            dir.join(format!("{tier:?}.o")),
            keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap(),
        )
        .unwrap();
        for cc in ["gcc", "clang"] {
            let out = Command::new(cc)
                .args([
                    "-O2",
                    "-march=x86-64",
                    "-mno-avx",
                    "-fno-tree-vectorize",
                    "-fno-lto",
                ])
                .arg(dir.join("vendor.c"))
                .arg(dir.join("harness.c"))
                .arg(dir.join(format!("{tier:?}.o")))
                .arg("-o")
                .arg(dir.join(format!("run-{cc}-{tier:?}")))
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{cc}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = Command::new(dir.join(format!("run-{cc}-{tier:?}")))
                .output()
                .unwrap();
            assert!(out.status.success(), "{cc} {tier:?}: {out:?}");
            eprintln!("{cc} {tier:?}: {}", String::from_utf8_lossy(&out.stdout));
        }
    }
    eprintln!("native artifacts: {}", dir.display());
}
