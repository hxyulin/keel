//! Floating intrinsic wrappers preserve rounding, status flags and lane layout.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, path::PathBuf, process::Command};

const SOURCE: &str = include_str!("fixtures/x86_float_intrinsics.c");
const HARNESS: &str = include_str!("fixtures/x86_float_intrinsics_runner.c");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn frontend(source: &str) -> Result<keel_ir::Module, String> {
    let source = keel_c::preprocess(
        source,
        "float-intrinsics.c",
        &keel_c::PreprocessOptions {
            target: Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &source,
        "float-intrinsics.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}
fn features() -> Features {
    Features::parse("sse3,sse41,fma,f16c").unwrap()
}
fn pipeline<'a>(
    selector: &'a FeatureSelector<'a>,
    allocator: &'a AllocationTier,
) -> CodegenPipeline<'a> {
    CodegenPipeline {
        selector,
        allocator,
    }
}
fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn vendor_float_wrappers_keep_effects_and_validate_controls() {
    for source in [
        "#include <immintrin.h>\n__m128i f(__m128 x,int c){return _mm_cvtps_ph(x,c);}",
        "#include <immintrin.h>\n__m128i f(__m128 x){return _mm_cvtps_ph(x,256);}",
        "#include <immintrin.h>\n__m128i f(__m128 x){return _mm_cvtps_ph(x,-1);}",
    ] {
        assert!(
            frontend(source)
                .unwrap_err()
                .contains("integer constant expression in 0..255")
        );
    }
    for mode in [0, 1, 2, 3, 4, 7, 8, 255] {
        let mut module = frontend(&format!(
            "#include <immintrin.h>\nvoid f(__m128 x){{(void)_mm_cvtps_ph(x,{mode});}}"
        ))
        .unwrap();
        keel_opt::optimize(&mut module).unwrap();
        let text = module.to_string();
        assert!(text.contains("x86_f16c_ps_to_ph128"), "{text}");
        assert!(text.contains(&format!("iconst.i32 {}", mode & 7)), "{text}");
    }
    for control in [
        "_MM_FROUND_TO_NEAREST_INT",
        "_MM_FROUND_TO_NEG_INF | _MM_FROUND_NO_EXC",
        "_MM_FROUND_TO_POS_INF | _MM_FROUND_RAISE_EXC",
        "_MM_FROUND_TO_ZERO",
        "_MM_FROUND_CUR_DIRECTION | _MM_FROUND_NO_EXC",
        "_MM_FROUND_NINT",
        "_MM_FROUND_FLOOR",
        "_MM_FROUND_CEIL",
        "_MM_FROUND_TRUNC",
        "_MM_FROUND_RINT",
        "_MM_FROUND_NEARBYINT",
    ] {
        let module = frontend(&format!(
            "#include <immintrin.h>\n__m128i f(__m128 x){{return _mm_cvtps_ph(x,{control});}}"
        ))
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
    let mut dead_fma = frontend(
        "#include <immintrin.h>\nvoid f(__m128 a,__m128 b,__m128 c){(void)_mm_fmadd_ps(a,b,c);}",
    )
    .unwrap();
    keel_opt::optimize(&mut dead_fma).unwrap();
    assert!(dead_fma.to_string().contains("x86_fma_vfmadd213ps128"));
    for (source, enabled, expected) in [
        (
            "#include <immintrin.h>\n__m128 f(__m128 a,__m128 b,__m128 c){return _mm_fmadd_ps(a,b,c);}",
            "avx,f16c",
            "fma",
        ),
        (
            "#include <immintrin.h>\n__m128 f(__m128i a){return _mm_cvtph_ps(a);}",
            "avx,fma",
            "f16c",
        ),
    ] {
        let module = frontend(source).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features: Features::parse(enabled).unwrap(),
        };
        let error = keel_codegen::compile_program_with_pipeline(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline(&selector, &AllocationTier::Full),
        )
        .unwrap_err();
        assert!(
            error.to_string().to_lowercase().contains(expected),
            "{error}"
        );
    }
    // Merely including declarations does not grant or require optional ISA bits.
    let module = frontend("#include <immintrin.h>\nint f(void){return 1;}").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::default(),
    };
    keel_codegen::compile_program_with_pipeline(
        &module,
        Target::X86_64LinuxGnu,
        &pipeline(&selector, &AllocationTier::Full),
    )
    .unwrap();
}

#[test]
fn floating_vendor_oracles_cover_rounding_status_and_lane_layouts() {
    let directory = std::env::var_os("KEEL_FLOAT_INTRINSICS_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("x86-float-intrinsics"));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("kernels.c"), SOURCE).unwrap();
    fs::write(directory.join("runner.c"), HARNESS).unwrap();
    // Reference compilation is independent of Keel's embedded headers/lowering.
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-O2",
                "-mfma",
                "-mf16c",
                "-msse3",
                "-msse4.1",
                "-DREFERENCE",
                "-c",
            ])
            .arg(directory.join("kernels.c"))
            .arg("-o")
            .arg(directory.join("reference.o")),
    );
    let raw = frontend(SOURCE).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let native = native_supported();
    for (selection, module) in [
        (SelectionTier::Baseline, &raw),
        (SelectionTier::Full, &optimized),
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let selector = FeatureSelector {
                    inner: &selection,
                    features: features(),
                };
                let pipeline = pipeline(&selector, &allocation);
                let pools = if spilled {
                    &EMPTY
                } else {
                    &keel_codegen::x86_64::REGISTERS
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline)
                        .unwrap();
                let mut chunks = keel_asm::x86::ProgramAssembler::new(features());
                keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                    module,
                    pools,
                    &pipeline,
                    &mut |chunk| chunks.push(chunk),
                )
                .unwrap();
                let bytes =
                    keel_asm::x86::assemble_program_object_with_features(&program, features())
                        .unwrap();
                assert_eq!(bytes, chunks.finish().unwrap());
                assert_eq!(
                    bytes,
                    keel_asm::x86::assemble_object_with_features(&text, features()).unwrap()
                );
                let path = directory.join(format!("{selection:?}-{allocation:?}-{spilled}.o"));
                fs::write(&path, bytes).unwrap();
                if native {
                    let runner = directory.join("run");
                    command(
                        Command::new("clang")
                            .arg(directory.join("runner.c"))
                            .arg(directory.join("reference.o"))
                            .arg(&path)
                            .arg("-o")
                            .arg(&runner),
                    );
                    command(&mut Command::new(runner));
                }
            }
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn native_supported() -> bool {
    std::is_x86_feature_detected!("sse3")
        && std::is_x86_feature_detected!("sse4.1")
        && std::is_x86_feature_detected!("avx")
        && std::is_x86_feature_detected!("fma")
        && std::is_x86_feature_detected!("f16c")
}
#[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
fn native_supported() -> bool {
    false
}

#[test]
fn packed_float_arithmetic_preserves_status_and_unpack_keeps_bits() {
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        for suffix in ["ps", "pd"] {
            let ty = format!("__m{width}{}", if suffix == "pd" { "d" } else { "" });
            for op in ["hadd", "hsub", "unpacklo", "unpackhi", "addsub", "sqrt"] {
                let call = format!("{prefix}_{op}_{suffix}");
                let args = if op == "sqrt" { "a" } else { "a,b" };
                let mut module=frontend(&format!("#include <immintrin.h>\n{ty} f({ty} a,{ty} b){{return {call}({args});}}\nvoid dead({ty} a,{ty} b){{(void){call}({args});}}")).unwrap();
                keel_ir::verify_module(&module).unwrap();
                let effectful = !op.starts_with("unpack");
                let sse3 = op.starts_with('h') || op == "addsub";
                let ir = if effectful {
                    if width == 128 {
                        if op == "sqrt" {
                            format!("x86_sse_sqrt{suffix}128")
                        } else {
                            format!("x86_sse3_{op}{suffix}")
                        }
                    } else {
                        format!("x86_avx_v{op}{suffix}256")
                    }
                } else {
                    "vshuffle".to_string()
                };
                assert!(module.to_string().contains(&ir), "{call}");
                keel_opt::optimize(&mut module).unwrap();
                if effectful {
                    let dead = module
                        .functions
                        .iter()
                        .find(|function| function.name == "dead")
                        .unwrap();
                    let text = dead.to_string();
                    assert!(
                        text.contains("call @") || text.contains(&ir),
                        "{call}: {text}"
                    );
                }
                let allowed = if width == 128 {
                    Features::parse(if sse3 { "sse3" } else { "none" }).unwrap()
                } else {
                    Features::parse("avx").unwrap()
                };
                let selector = FeatureSelector {
                    inner: &SelectionTier::Baseline,
                    features: allowed,
                };
                let program = keel_codegen::compile_program_with_pipeline(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline(&selector, &AllocationTier::Full),
                )
                .unwrap();
                keel_asm::x86::assemble_program_object_with_features(&program, allowed).unwrap();
                if effectful && (width == 256 || sse3) {
                    let mut missing = allowed;
                    if width == 128 {
                        missing.sse3 = false;
                    } else {
                        missing.avx = false;
                    }
                    let selector = FeatureSelector {
                        inner: &SelectionTier::Baseline,
                        features: missing,
                    };
                    assert!(
                        keel_codegen::compile_program_with_pipeline(
                            &module,
                            Target::X86_64LinuxGnu,
                            &pipeline(&selector, &AllocationTier::Full)
                        )
                        .is_err(),
                        "{call}"
                    );
                }
                let inputs = if op == "sqrt" {
                    "next()"
                } else {
                    "next(),next()"
                };
                let once=frontend(&format!("#include <immintrin.h>\nextern {ty} next(void);{ty} f(void){{return {call}({inputs});}}")).unwrap();
                assert_eq!(
                    once.to_string().matches("call @next()").count(),
                    if op == "sqrt" { 1 } else { 2 },
                    "{call}"
                );
            }
        }
    }
    for source in [
        "#include <pmmintrin.h>\n__m128 f(__m128d a){return _mm_hadd_ps(a,a);}",
        "typedef float V __attribute__((vector_size(16)));V f(V a){return __builtin_keel_sse3_haddps(a);}",
        "typedef float V __attribute__((vector_size(16)));V f(V a){return __builtin_keel_avx_vhaddps256(a,a);}",
    ] {
        assert!(frontend(source).is_err(), "{source}");
    }
    let module = frontend("#include <pmmintrin.h>\nint f(void){return 0;}").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::NONE,
    };
    keel_codegen::compile_program_with_pipeline(
        &module,
        Target::X86_64LinuxGnu,
        &pipeline(&selector, &AllocationTier::Full),
    )
    .unwrap();
}

#[test]
fn rounding_controls_scalar_lanes_and_blend_permissions() {
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        for suffix in if width == 128 {
            &["ps", "pd", "ss", "sd"][..]
        } else {
            &["ps", "pd"][..]
        } {
            let scalar = matches!(*suffix, "ss" | "sd");
            let ty = format!(
                "__m{width}{}",
                if matches!(*suffix, "pd" | "sd") {
                    "d"
                } else {
                    ""
                }
            );
            let args = if scalar { "a,b" } else { "a" };
            let name = format!("{prefix}_round_{suffix}");
            for control in [
                "n",
                "-1",
                "16",
                "256",
                "1.0",
                "(n++,1)",
                "(1/0)",
                "(1<<31)",
                "(1ULL<<64)",
            ] {
                let source = format!(
                    "#include <immintrin.h>\n{ty} f({ty} a,{ty} b,int n){{return {name}({args},{control});}}"
                );
                let error = frontend(&source).unwrap_err();
                assert!(
                    error.contains("integer constant expression in 0..15"),
                    "{name}: {control}: {error}"
                );
            }
            for control in [0, 4, 8, 15] {
                let mut module=frontend(&format!("#include <immintrin.h>\n{ty} f({ty} a,{ty} b){{return {name}({args},{control});}}\nvoid dead({ty} a,{ty} b){{(void){name}({args},{control});}}")).unwrap();
                keel_ir::verify_module(&module).unwrap();
                keel_opt::optimize(&mut module).unwrap();
                let ir = if width == 256 {
                    format!("x86_avx_vround{suffix}256")
                } else {
                    format!("x86_sse41_round{suffix}{}", if scalar { "" } else { "128" })
                };
                assert!(module.to_string().contains(&ir), "{name}");
                let allowed = Features::parse(if width == 128 { "sse41" } else { "avx" }).unwrap();
                let selector = FeatureSelector {
                    inner: &SelectionTier::Baseline,
                    features: allowed,
                };
                let program = keel_codegen::compile_program_with_pipeline(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline(&selector, &AllocationTier::Full),
                )
                .unwrap();
                keel_asm::x86::assemble_program_object_with_features(&program, allowed).unwrap();
                let selector = FeatureSelector {
                    inner: &SelectionTier::Baseline,
                    features: Features::NONE,
                };
                assert!(
                    keel_codegen::compile_program_with_pipeline(
                        &module,
                        Target::X86_64LinuxGnu,
                        &pipeline(&selector, &AllocationTier::Full)
                    )
                    .is_err(),
                    "{name}"
                );
            }
            for op in ["round", "ceil", "floor"] {
                let inputs = if scalar { "next(),next()" } else { "next()" };
                let inputs = if op == "round" {
                    format!("{inputs},_MM_FROUND_NEARBYINT")
                } else {
                    inputs.to_string()
                };
                let once=frontend(&format!("#include <immintrin.h>\nextern {ty} next(void);{ty} f(void){{return {prefix}_{op}_{suffix}({inputs});}}")).unwrap();
                assert_eq!(
                    once.to_string().matches("call @next()").count(),
                    if scalar { 2 } else { 1 },
                    "{op} {suffix}"
                );
            }
        }
        for suffix in ["ps", "pd"] {
            let ty = format!("__m{width}{}", if suffix == "pd" { "d" } else { "" });
            let source = format!(
                "#include <immintrin.h>\nextern {ty} next(void);{ty} f(void){{return {prefix}_blendv_{suffix}(next(),next(),next());}}"
            );
            let module = frontend(&source).unwrap();
            assert_eq!(module.to_string().matches("call @next()").count(), 3);
            let allowed = Features::parse(if width == 128 { "sse41" } else { "avx" }).unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features: allowed,
            };
            let program = keel_codegen::compile_program_with_pipeline(
                &module,
                Target::X86_64LinuxGnu,
                &pipeline(&selector, &AllocationTier::Full),
            )
            .unwrap();
            keel_asm::x86::assemble_program_object_with_features(&program, allowed).unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Baseline,
                features: Features::NONE,
            };
            assert!(
                keel_codegen::compile_program_with_pipeline(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline(&selector, &AllocationTier::Full)
                )
                .is_err()
            );
        }
    }
}
