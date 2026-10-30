#[path = "common/x86_packet.rs"]
mod packet;
#[path = "x86_intrinsics/pclmul.rs"]
mod pclmul;
#[path = "x86_intrinsics/x86_aes.rs"]
mod x86_aes;
#[path = "x86_intrinsics/x86_comparisons.rs"]
mod x86_comparisons;
#[path = "x86_intrinsics/x86_construct.rs"]
mod x86_construct;
#[path = "x86_intrinsics/x86_conversions.rs"]
mod x86_conversions;
#[path = "x86_intrinsics/x86_cpu_queries.rs"]
mod x86_cpu_queries;
#[path = "x86_intrinsics/x86_float_intrinsics.rs"]
mod x86_float_intrinsics;
#[path = "x86_intrinsics/x86_lane_intrinsics.rs"]
mod x86_lane_intrinsics;
#[path = "x86_intrinsics/x86_mask_memory.rs"]
mod x86_mask_memory;
#[path = "x86_intrinsics/x86_numeric_conversions.rs"]
mod x86_numeric_conversions;
#[path = "x86_intrinsics/x86_queries.rs"]
mod x86_queries;
#[path = "x86_intrinsics/x86_saturation.rs"]
mod x86_saturation;
#[path = "x86_intrinsics/x86_shifts.rs"]
mod x86_shifts;
#[path = "x86_intrinsics/x86_state_intrinsics.rs"]
mod x86_state_intrinsics;
#[path = "x86_intrinsics/x86_strings.rs"]
mod x86_strings;
#[path = "x86_intrinsics/x86_target_features.rs"]
mod x86_target_features;
#[path = "x86_intrinsics/x86_widening.rs"]
mod x86_widening;

use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::Target;

fn compile(source: &str) -> keel_ir::Module {
    let target = Target::X86_64LinuxGnu;
    let source = keel_c::preprocess(
        source,
        "intrinsics.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &source,
        "intrinsics.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn program(module: &keel_ir::Module, features: Features) -> Result<(), keel_codegen::CodegenError> {
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let code = keel_codegen::compile_program_with_pipeline(
        module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )?;
    keel_asm::x86::assemble_program_object_with_features(&code, features).unwrap();
    Ok(())
}
#[test]
fn packed_headers_use_typed_ir_and_existing_feature_gates() {
    let source = r#"
#include <immintrin.h>
_Static_assert(sizeof(__m256)==32 && _Alignof(__m256)==32, "layout");
__m256 add(__m256 a,__m256 b) { return _mm256_add_ps(a,b); }
__m256i integer(__m256i a,__m256i b) { return _mm256_add_epi32(a,b); }
__m256 shuffle(__m256 a,__m256 b) { return _mm256_shuffle_ps(a,b,_MM_SHUFFLE(2,1,3,0)); }
__m256i ishuffle(__m256i a) { return _mm256_shuffle_epi32(a,27); }
__m128 half(__m256 a) { return _mm256_extractf128_ps(a,1); }
__m256 insert(__m256 a,__m128 b) { return _mm256_insertf128_ps(a,b,1); }
__m256i cast(__m256 a) { return _mm256_castps_si256(a); }
void copy(float *dst,const float *src) { _mm256_storeu_ps(dst,_mm256_loadu_ps(src)); }
__m128 compare(__m128 a,__m128 b) { return _mm_cmplt_ps(a,b); }
"#;
    let module = compile(source);
    let text = module.to_string();
    assert!(text.contains("vfadd"), "{text}");
    assert!(text.contains("viadd"), "{text}");
    assert!(text.contains("vshuffle"), "{text}");
    let features = Features::for_level(CpuLevel::V3);
    program(&module, features).unwrap();
    let error = program(&module, Features::for_level(CpuLevel::V1)).unwrap_err();
    assert!(error.message.contains("256-bit SIMD"), "{error:?}");
    let mut avx = features;
    avx.avx2 = false;
    // The existing backend may expand AVX2 arithmetic using legal AVX/scalar operations.
    program(&module, avx).unwrap();
    let floating =
        compile("#include <immintrin.h>\n__m256 f(__m256 a,__m256 b){return _mm256_mul_ps(a,b);}");
    program(&floating, avx).unwrap();
    let sse =
        compile("#include <immintrin.h>\n__m128 f(__m128 a,__m128 b){return _mm_add_ps(a,b);}");
    program(&sse, Features::for_level(CpuLevel::V1)).unwrap();
}
#[test]
fn every_header_function_can_emit_an_object() {
    let target = Target::X86_64LinuxGnu;
    for (header, level, extensions) in [
        ("smmintrin.h", CpuLevel::V2, "monitor"),
        ("immintrin.h", CpuLevel::V3, "monitor,avx512f"),
    ] {
        let source = keel_c::preprocess(
            &format!("#include <{header}>\n"),
            "headers.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        // Force otherwise unused inline bodies through the ordinary lowering path.
        let source = source.replace("static __inline__", "");
        let module = keel_c::compile(
            &source,
            "headers.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        program(
            &module,
            Features::for_level(level)
                .with_features(extensions)
                .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn wide_vector_semantics_and_diagnostics() {
    for base in [
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned",
        "long",
        "unsigned long",
        "long long",
        "float",
        "double",
    ] {
        let source = format!(
            "typedef {base} V __attribute__((vector_size(32)));\n_Static_assert(sizeof(V)==32 && _Alignof(V)==32,\"layout\");\nstruct S {{ char c; V v; char d; }};\n_Static_assert(sizeof(struct S)==96,\"aggregate\");\nV f(V a,V b,int i) {{ V c={{1,2}}; c[i]=a[i]; return c+a*b; }}\n"
        );
        let module = compile(&source);
        program(&module, Features::for_level(CpuLevel::V3)).unwrap();
    }
    for body in [
        "return __builtin_shufflevector(a,a,0,1,2);",
        "return __builtin_shufflevector(a,a,0,1,2,-1);",
        "return __builtin_shufflevector(a,a,0,1,2,8);",
        "return __builtin_shufflevector(a,a,0,1,2,i);",
        "return __builtin_shufflevector(a,a,0,1,2,(i++,3));",
    ] {
        let source =
            format!("typedef int V __attribute__((vector_size(16))); V f(V a,int i){{{body}}}");
        assert!(
            keel_c::compile(
                &source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err()
        );
    }
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        assert!(
            keel_c::compile(
                "typedef int V __attribute__((vector_size(32)));",
                "bad.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .unwrap_err()
            .message
            .contains("x86-64")
        );
    }
}

#[test]
fn comparison_controls_and_wide_casts_fail_closed() {
    for predicate in ["1", "2", "31", "-1", "i", "(i++,0)", "0.0"] {
        let source = format!(
            "typedef float V __attribute__((vector_size(32))); V f(V a,V b,int i){{return __builtin_ia32_cmpps256(a,b,{predicate});}}"
        );
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
            error.message.contains("constant quiet predicates"),
            "{error:?}"
        );
    }
    for body in ["return (N)a;", "return (__int128)a;"] {
        let source = format!(
            "typedef int V __attribute__((vector_size(32))); typedef int N __attribute__((vector_size(16))); N f(V a){{{body}}}"
        );
        assert!(
            keel_c::compile(
                &source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err()
        );
    }
}
#[test]
fn shuffle_builtin_uses_the_existing_128_bit_path_on_all_targets() {
    let source = "typedef int V __attribute__((vector_size(16))); V f(V a,V b){return __builtin_shufflevector(a,b,7,0,5,2);}";
    for target in Target::NATIVE {
        let queried = format!(
            "_Static_assert(__has_builtin(__builtin_shufflevector),\"shuffle feature\"); _Static_assert(__has_builtin(__builtin_ia32_cmpps256)=={},\"x86 feature\"); {source}",
            u8::from(target == Target::X86_64LinuxGnu)
        );
        let queried = keel_c::preprocess(
            &queried,
            "shuffle.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let module = keel_c::compile(
            &queried,
            "shuffle.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, target).unwrap();
    }
}

fn abi_sources() -> (String, String) {
    let mut source = String::from(include_str!("fixtures/x86_intrinsics.c"));
    let mut harness = String::from(include_str!("fixtures/x86_intrinsics_harness.c"));
    let mut main =
        String::from("int main(void) { int result=intrinsic_checks(); if(result)return result;\n");
    for predicate in [0, 3, 4, 7, 17, 18, 29, 30] {
        for (suffix, ty) in [("ps", "__m256"), ("pd", "__m256d")] {
            source.push_str(&format!("{ty} k_pred_{suffix}_{predicate}({ty} a,{ty} b){{return _mm256_cmp_{suffix}(a,b,{predicate});}}\n"));
            harness.push_str(&format!("extern {ty} k_pred_{suffix}_{predicate}({ty},{ty}),c_pred_{suffix}_{predicate}({ty},{ty});\n"));
            main.push_str(&format!("{{ {ty} a={{__builtin_nan(\"\"),-0.0,__builtin_inf(),-__builtin_inf()}}, b={{1.0,0.0,__builtin_inf(),1.0}}; CHECK({ty},pred_{suffix}_{predicate},a,b); }}\n"));
        }
    }
    for (id, base, lanes) in [
        ("i8", "signed char", 32),
        ("u8", "unsigned char", 32),
        ("i16", "short", 16),
        ("u16", "unsigned short", 16),
        ("i32", "int", 8),
        ("u32", "unsigned", 8),
        ("i64", "long long", 4),
        ("u64", "unsigned long long", 4),
        ("f32", "float", 8),
        ("f64", "double", 4),
    ] {
        let ty = format!("V_{id}");
        let typedef = format!("typedef {base} {ty} __attribute__((vector_size(32)));\n");
        source.push_str(&typedef);
        harness.push_str(&typedef);
        let params = (0..10)
            .map(|i| format!("{ty} a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sum = (0..10)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("+");
        let indices = (0..lanes)
            .map(|i| (lanes * 2 - i - 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        source.push_str(&format!(
            r#"
extern {ty} callback_{id}({params});
{ty} k_global_{id}={{1,2}};
{ty} k_stack_{id}({params}) {{ return {sum}; }}
{ty} k_call_{id}({params}) {{ return callback_{id}({args}); }}
{ty} k_indirect_{id}({ty} (*fn)({params}),{params}) {{ return fn({args}); }}
{ty} k_lanes_{id}({ty} a,int i) {{ {ty} b={{1,2}}; b[i]=a[i]; b[(i+1)%{lanes}]+=2; return b; }}
{ty} k_shuffle_{id}({ty} a,{ty} b) {{ return __builtin_shufflevector(a,b,{indices}); }}
"#
        ));
        harness.push_str(&format!(
            r#"
{ty} callback_{id}({params}) {{ return {sum}; }}
extern {ty} k_global_{id};
extern {ty} k_stack_{id}({params}), k_call_{id}({params});
extern {ty} k_indirect_{id}({ty} (*)({params}),{params});
extern {ty} k_lanes_{id}({ty},int), c_lanes_{id}({ty},int);
extern {ty} k_shuffle_{id}({ty},{ty}), c_shuffle_{id}({ty},{ty});
"#
        ));
        main.push_str(&format!(r#"{{
{ty} a={{1,2}}, b={{0}};if(memcmp(&a,&k_global_{id},32))return __LINE__;for(int i=0;i<{lanes};i++)b[i]=({base})(i+3);
{ty} (*volatile fn)({params})=k_stack_{id};
{ty} expected=callback_{id}(a,a,a,a,a,a,a,a,a,a),actual=fn(a,a,a,a,a,a,a,a,a,a);
if(memcmp(&actual,&expected,32))return __LINE__;
actual=k_call_{id}(a,a,a,a,a,a,a,a,a,a);if(memcmp(&actual,&expected,32))return __LINE__;
actual=k_indirect_{id}(callback_{id},a,a,a,a,a,a,a,a,a,a);if(memcmp(&actual,&expected,32))return __LINE__;
for(int i=0;i<{lanes};i++) {{ actual=k_lanes_{id}(b,i);expected=c_lanes_{id}(b,i);if(memcmp(&actual,&expected,32))return __LINE__; }}
actual=k_shuffle_{id}(a,b);expected=c_shuffle_{id}(a,b);if(memcmp(&actual,&expected,32))return __LINE__;
}}
"#));
    }
    main.push_str("return 0; }\n");
    harness.push_str(&main);
    (source, harness)
}
fn command(c: &mut std::process::Command) {
    let output = c.output().unwrap();
    assert!(
        output.status.success(),
        "{c:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn wide_abi_intrinsics_and_exact_memory_footprints() {
    use std::{fs, path::PathBuf, process::Command};
    let dir = std::env::var_os("KEEL_C_X86_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-x86-intrinsics"));
    fs::create_dir_all(&dir).unwrap();
    let (source, harness) = abi_sources();
    fs::write(dir.join("kernels.c"), &source).unwrap();
    fs::write(dir.join("oracle.c"), source.replace(" k_", " c_")).unwrap();
    fs::write(
        dir.join("harness.c"),
        harness.replace("return __LINE__;", "return (__LINE__ % 250 + 1);"),
    )
    .unwrap();
    // Cross-object emission is always tested; native execution additionally needs AVX2.
    command(Command::new("clang").current_dir(&dir).args([
        "-target",
        "x86_64-unknown-linux-gnu",
        "-ffreestanding",
        "-mavx2",
        "-O1",
        "-c",
        "oracle.c",
        "-o",
        "oracle.o",
    ]));
    command(Command::new("clang").current_dir(&dir).args([
        "-target",
        "x86_64-unknown-linux-gnu",
        "-ffreestanding",
        "-mavx2",
        "-O1",
        "-c",
        "harness.c",
        "-o",
        "harness.o",
    ]));
    for (name, selection, allocation, optimized) in [
        (
            "baseline",
            SelectionTier::Baseline,
            AllocationTier::Full,
            false,
        ),
        (
            "trees-fast",
            SelectionTier::Trees,
            AllocationTier::Fast,
            false,
        ),
        ("optimized", SelectionTier::Full, AllocationTier::Full, true),
    ] {
        let mut module = compile(&source);
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
            keel_ir::verify_module(&module).unwrap();
        }
        let features = Features::for_level(CpuLevel::V3);
        let selector = FeatureSelector {
            inner: &selection,
            features,
        };
        let code = keel_codegen::compile_program_with_pipeline(
            &module,
            Target::X86_64LinuxGnu,
            &CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            },
        )
        .unwrap();
        let object = keel_asm::x86::assemble_program_object_with_features(&code, features).unwrap();
        fs::write(dir.join(format!("keel-{name}.o")), object).unwrap();
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        if std::is_x86_feature_detected!("avx2") {
            for compiler in ["clang", "gcc"] {
                if Command::new(compiler).arg("--version").output().is_err() {
                    continue;
                }
                let exe = format!("run-{compiler}-{name}");
                command(Command::new(compiler).current_dir(&dir).args([
                    "-mavx2",
                    "-O1",
                    "oracle.c",
                    "harness.c",
                    &format!("keel-{name}.o"),
                    "-o",
                    &exe,
                ]));
                command(&mut Command::new(dir.join(exe)));
            }
        }
    }
}

fn integer_family_sources() -> (String, String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut narrow = source.clone();
    let mut declarations = String::new();
    let mut cases = String::new();
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let mut operations = Vec::new();
        for bits in [8, 16, 32] {
            for signed in [false, true] {
                for (op, code) in [("min", 0), ("max", 1)] {
                    operations.push((
                        format!("{prefix}_{op}_ep{}{bits}", if signed { 'i' } else { 'u' }),
                        bits,
                        signed,
                        code,
                    ));
                }
            }
            operations.push((format!("{prefix}_abs_epi{bits}"), bits, true, 2));
            operations.push((format!("{prefix}_sign_epi{bits}"), bits, true, 3));
        }
        operations.push((format!("{prefix}_blendv_epi8"), 8, false, 4));
        for (name, bits, signed, op) in operations {
            let bytes = width / 8;
            let load = format!("{prefix}_loadu_si{width}");
            let store = format!("{prefix}_storeu_si{width}");
            let ty = format!("__m{width}i");
            let args = match op {
                2 => "x",
                4 => "x,y,z",
                _ => "x,y",
            };
            let body = format!(
                "void k{name}(const void*a,const void*b,const void*m,void*out){{ {ty} x={load}((const {ty}*)a),y={load}((const {ty}*)b),z={load}((const {ty}*)m); (void)y;(void)z;{store}(({ty}*)out,{name}({args})); }}\n"
            );
            source += &body;
            if width == 128 {
                narrow += &body;
            }
            declarations += &format!(
                "extern void k{name}(const void*,const void*,const void*,void*);\nextern void c{name}(const void*,const void*,const void*,void*);\n"
            );
            cases += &format!(
                "{{\"{name}\",k{name},c{name},{bytes},{bits},{},{op}}},\n",
                u8::from(signed)
            );
        }
    }
    let harness = format!(
        "#include <stdint.h>\n{declarations}\n{}\nstatic const struct Case cases[]={{\n{cases}}};\n{}",
        concat!(
            include_str!("fixtures/x86_integer_lanes.h"),
            include_str!("fixtures/x86_integer_family_oracle.h")
        ),
        include_str!("fixtures/x86_integer_family_runner.c")
    );
    (source, narrow, harness)
}

#[test]
fn integer_minmax_abs_sign_and_blend_match_vendor_and_scalar_oracles() {
    use keel_codegen::regalloc::Pools;
    use std::{fs, path::PathBuf, process::Command};
    let directory = std::env::var_os("KEEL_C_INTEGER_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-integer-intrinsics"));
    fs::create_dir_all(&directory).unwrap();
    let (source, narrow, harness) = integer_family_sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(
        directory.join("oracle.c"),
        source.replace("void k_", "void c_"),
    )
    .unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    // These wrappers have exact software lowerings; narrow values remain legal
    // at baseline, while wide argument transport still requires AVX.
    program(&compile(&narrow), Features::for_level(CpuLevel::V1)).unwrap();
    let raw = compile(&source);
    assert!(
        program(&raw, Features::for_level(CpuLevel::V1))
            .unwrap_err()
            .message
            .contains("256-bit SIMD")
    );
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    for file in ["oracle", "harness"] {
        command(Command::new("clang").current_dir(&directory).args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-mavx2",
            "-O2",
            "-c",
            &format!("{file}.c"),
            "-o",
            &format!("{file}.o"),
        ]));
    }
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V3);
    for (name, selection, allocation, module) in [
        (
            "baseline",
            SelectionTier::Baseline,
            AllocationTier::Full,
            &raw,
        ),
        (
            "trees-fast",
            SelectionTier::Trees,
            AllocationTier::Fast,
            &raw,
        ),
        (
            "optimized",
            SelectionTier::Full,
            AllocationTier::Full,
            &optimized,
        ),
    ] {
        for spilled in [false, true] {
            let selector = FeatureSelector {
                inner: &selection,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let pools = if spilled {
                &empty
            } else {
                &keel_codegen::x86_64::REGISTERS
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
            let typed =
                keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&typed, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
            keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                module,
                pools,
                &pipeline,
                &mut |part| chunks.push(part),
            )
            .unwrap();
            assert_eq!(object, chunks.finish().unwrap());
            let filename = format!("keel-{name}-{spilled}.o");
            fs::write(directory.join(&filename), object).unwrap();
            #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
            if std::is_x86_feature_detected!("avx2") {
                command(Command::new("clang").current_dir(&directory).args([
                    "oracle.o",
                    "harness.o",
                    &filename,
                    "-o",
                    "run",
                ]));
                command(&mut Command::new(directory.join("run")));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PackedIntegerOperation {
    AddSigned,
    AddUnsigned,
    SubSigned,
    SubUnsigned,
    Average,
    MaddSigned,
    MaddMixed,
    MulHighSigned,
    MulHighUnsigned,
    MulRounded,
    PackSigned,
    PackUnsigned,
    Shuffle,
    SignMask,
    HorizontalAdd,
    HorizontalSub,
    HorizontalAddSaturating,
    HorizontalSubSaturating,
    UnpackLow,
    UnpackHigh,
    WidenMultiplySigned,
    WidenMultiplyUnsigned,
    ByteSad,
}

struct PackedIntegerIntrinsic {
    name: &'static str,
    ir: &'static str,
    width: u32,
    input_bits: u32,
    output_bits: u32,
    operation: PackedIntegerOperation,
    feature: &'static str,
}

const PACKED_INTEGER_INTRINSICS: &[PackedIntegerIntrinsic] = &[
    PackedIntegerIntrinsic {
        name: "_mm_avg_epu8",
        ir: "x86_sse2_pavgb128",
        width: 128,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::Average,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_avg_epu16",
        ir: "x86_sse2_pavgw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::Average,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_madd_epi16",
        ir: "x86_sse2_pmaddwd128",
        width: 128,
        input_bits: 16,
        output_bits: 32,
        operation: PackedIntegerOperation::MaddSigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_maddubs_epi16",
        ir: "x86_ssse3_pmaddubsw128",
        width: 128,
        input_bits: 8,
        output_bits: 16,
        operation: PackedIntegerOperation::MaddMixed,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_mulhi_epi16",
        ir: "x86_sse2_pmulhw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulHighSigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_mulhi_epu16",
        ir: "x86_sse2_pmulhuw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulHighUnsigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_mulhrs_epi16",
        ir: "x86_ssse3_pmulhrsw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulRounded,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_packs_epi16",
        ir: "x86_sse2_packsswb128",
        width: 128,
        input_bits: 16,
        output_bits: 8,
        operation: PackedIntegerOperation::PackSigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_packs_epi32",
        ir: "x86_sse2_packssdw128",
        width: 128,
        input_bits: 32,
        output_bits: 16,
        operation: PackedIntegerOperation::PackSigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_packus_epi16",
        ir: "x86_sse2_packuswb128",
        width: 128,
        input_bits: 16,
        output_bits: 8,
        operation: PackedIntegerOperation::PackUnsigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_packus_epi32",
        ir: "x86_sse41_packusdw128",
        width: 128,
        input_bits: 32,
        output_bits: 16,
        operation: PackedIntegerOperation::PackUnsigned,
        feature: "sse41",
    },
    PackedIntegerIntrinsic {
        name: "_mm_shuffle_epi8",
        ir: "x86_ssse3_pshufb128",
        width: 128,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::Shuffle,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_movemask_epi8",
        ir: "x86_sse_pmovmskb128",
        width: 128,
        input_bits: 8,
        output_bits: 0,
        operation: PackedIntegerOperation::SignMask,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_adds_epi8",
        ir: "x86_avx2_vpaddsb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::AddSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_adds_epu8",
        ir: "x86_avx2_vpaddusb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::AddUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_subs_epi8",
        ir: "x86_avx2_vpsubsb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::SubSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_subs_epu8",
        ir: "x86_avx2_vpsubusb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::SubUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_avg_epu8",
        ir: "x86_avx2_vpavgb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::Average,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_adds_epi16",
        ir: "x86_avx2_vpaddsw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::AddSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_adds_epu16",
        ir: "x86_avx2_vpaddusw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::AddUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_subs_epi16",
        ir: "x86_avx2_vpsubsw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::SubSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_subs_epu16",
        ir: "x86_avx2_vpsubusw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::SubUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_avg_epu16",
        ir: "x86_avx2_vpavgw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::Average,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_madd_epi16",
        ir: "x86_avx2_vpmaddwd256",
        width: 256,
        input_bits: 16,
        output_bits: 32,
        operation: PackedIntegerOperation::MaddSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_maddubs_epi16",
        ir: "x86_avx2_vpmaddubsw256",
        width: 256,
        input_bits: 8,
        output_bits: 16,
        operation: PackedIntegerOperation::MaddMixed,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_mulhi_epi16",
        ir: "x86_avx2_vpmulhw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulHighSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_mulhi_epu16",
        ir: "x86_avx2_vpmulhuw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulHighUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_mulhrs_epi16",
        ir: "x86_avx2_vpmulhrsw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::MulRounded,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_packs_epi16",
        ir: "x86_avx2_vpacksswb256",
        width: 256,
        input_bits: 16,
        output_bits: 8,
        operation: PackedIntegerOperation::PackSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_packs_epi32",
        ir: "x86_avx2_vpackssdw256",
        width: 256,
        input_bits: 32,
        output_bits: 16,
        operation: PackedIntegerOperation::PackSigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_packus_epi16",
        ir: "x86_avx2_vpackuswb256",
        width: 256,
        input_bits: 16,
        output_bits: 8,
        operation: PackedIntegerOperation::PackUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_packus_epi32",
        ir: "x86_avx2_vpackusdw256",
        width: 256,
        input_bits: 32,
        output_bits: 16,
        operation: PackedIntegerOperation::PackUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_shuffle_epi8",
        ir: "x86_avx2_vpshufb256",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::Shuffle,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_movemask_epi8",
        ir: "x86_avx2_vpmovmskb256",
        width: 256,
        input_bits: 8,
        output_bits: 0,
        operation: PackedIntegerOperation::SignMask,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hadd_epi16",
        ir: "x86_ssse3_phaddw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalAdd,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hadd_epi32",
        ir: "x86_ssse3_phaddd128",
        width: 128,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::HorizontalAdd,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hadds_epi16",
        ir: "x86_ssse3_phaddsw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalAddSaturating,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hsub_epi16",
        ir: "x86_ssse3_phsubw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalSub,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hsub_epi32",
        ir: "x86_ssse3_phsubd128",
        width: 128,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::HorizontalSub,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_hsubs_epi16",
        ir: "x86_ssse3_phsubsw128",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalSubSaturating,
        feature: "ssse3",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpacklo_epi8",
        ir: "vshuffle",
        width: 128,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpackhi_epi8",
        ir: "vshuffle",
        width: 128,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpacklo_epi16",
        ir: "vshuffle",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpackhi_epi16",
        ir: "vshuffle",
        width: 128,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpacklo_epi32",
        ir: "vshuffle",
        width: 128,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpackhi_epi32",
        ir: "vshuffle",
        width: 128,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpacklo_epi64",
        ir: "vshuffle",
        width: 128,
        input_bits: 64,
        output_bits: 64,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_unpackhi_epi64",
        ir: "vshuffle",
        width: 128,
        input_bits: 64,
        output_bits: 64,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hadd_epi16",
        ir: "x86_avx2_vphaddw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalAdd,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hadd_epi32",
        ir: "x86_avx2_vphaddd256",
        width: 256,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::HorizontalAdd,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hadds_epi16",
        ir: "x86_avx2_vphaddsw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalAddSaturating,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hsub_epi16",
        ir: "x86_avx2_vphsubw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalSub,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hsub_epi32",
        ir: "x86_avx2_vphsubd256",
        width: 256,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::HorizontalSub,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_hsubs_epi16",
        ir: "x86_avx2_vphsubsw256",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::HorizontalSubSaturating,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpacklo_epi8",
        ir: "vshuffle",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpackhi_epi8",
        ir: "vshuffle",
        width: 256,
        input_bits: 8,
        output_bits: 8,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpacklo_epi16",
        ir: "vshuffle",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpackhi_epi16",
        ir: "vshuffle",
        width: 256,
        input_bits: 16,
        output_bits: 16,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpacklo_epi32",
        ir: "vshuffle",
        width: 256,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpackhi_epi32",
        ir: "vshuffle",
        width: 256,
        input_bits: 32,
        output_bits: 32,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpacklo_epi64",
        ir: "vshuffle",
        width: 256,
        input_bits: 64,
        output_bits: 64,
        operation: PackedIntegerOperation::UnpackLow,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_unpackhi_epi64",
        ir: "vshuffle",
        width: 256,
        input_bits: 64,
        output_bits: 64,
        operation: PackedIntegerOperation::UnpackHigh,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_mul_epi32",
        ir: "x86_sse41_pmuldq128",
        width: 128,
        input_bits: 32,
        output_bits: 64,
        operation: PackedIntegerOperation::WidenMultiplySigned,
        feature: "sse41",
    },
    PackedIntegerIntrinsic {
        name: "_mm_mul_epu32",
        ir: "x86_sse2_pmuludq128",
        width: 128,
        input_bits: 32,
        output_bits: 64,
        operation: PackedIntegerOperation::WidenMultiplyUnsigned,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm_sad_epu8",
        ir: "x86_sse2_psadbw128",
        width: 128,
        input_bits: 8,
        output_bits: 64,
        operation: PackedIntegerOperation::ByteSad,
        feature: "",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_mul_epi32",
        ir: "x86_avx2_vpmuldq256",
        width: 256,
        input_bits: 32,
        output_bits: 64,
        operation: PackedIntegerOperation::WidenMultiplySigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_mul_epu32",
        ir: "x86_avx2_vpmuludq256",
        width: 256,
        input_bits: 32,
        output_bits: 64,
        operation: PackedIntegerOperation::WidenMultiplyUnsigned,
        feature: "avx2",
    },
    PackedIntegerIntrinsic {
        name: "_mm256_sad_epu8",
        ir: "x86_avx2_vpsadbw256",
        width: 256,
        input_bits: 8,
        output_bits: 64,
        operation: PackedIntegerOperation::ByteSad,
        feature: "avx2",
    },
];

fn packed_integer_kernel(i: usize, case: &PackedIntegerIntrinsic) -> String {
    let width = case.width;
    let prefix = if width == 128 { "_mm" } else { "_mm256" };
    let name = case.name;
    let (result, args, memory_args, store) = if case.operation == PackedIntegerOperation::SignMask {
        (
            "int".to_string(),
            "a",
            format!("kpacked_read{width}(a)"),
            "__builtin_memcpy(out,&result,4);".to_string(),
        )
    } else {
        (
            format!("__m{width}i"),
            "a,b",
            format!("kpacked_read{width}(a),kpacked_read{width}(b)"),
            format!("{prefix}_storeu_si{width}(out,result);"),
        )
    };
    format!(
        "{result} kpacked{i}(__m{width}i a,__m{width}i b){{(void)b;return {name}({args});}}\nvoid kpacked_memory{i}(const void*a,const void*b,void*out){{(void)b;{result} result={name}({memory_args});{store}}}\n"
    )
}

fn packed_integer_sources() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\nint kpacked_evaluations;\n");
    for width in [128, 256] {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        source += &format!(
            "static __m{width}i kpacked_read{width}(const void*p){{kpacked_evaluations++;return {prefix}_loadu_si{width}(p);}}\n"
        );
    }
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nextern int printf(const char*,...);\nextern void*memcpy(void*,const void*,__SIZE_TYPE__);\nextern void*memset(void*,int,__SIZE_TYPE__);\nextern int memcmp(const void*,const void*,__SIZE_TYPE__);\nextern int kpacked_evaluations;\n",
    );
    let mut table = String::new();
    for (i, case) in PACKED_INTEGER_INTRINSICS.iter().enumerate() {
        source += &packed_integer_kernel(i, case);
        let width = case.width;
        let result = if case.output_bits == 0 {
            "int".to_string()
        } else {
            format!("__m{width}i")
        };
        let bytes = if case.output_bits == 0 { 4 } else { width / 8 };
        harness += &format!(
            "extern {result} kpacked{i}(__m{width}i,__m{width}i);extern {result} cpacked{i}(__m{width}i,__m{width}i);extern void kpacked_memory{i}(const void*,const void*,void*);\n"
        );
        for compiler in ['k', 'c'] {
            harness += &format!(
                "static void {compiler}abi{i}(const void*a,const void*b,void*out){{__m{width}i x,y;memcpy(&x,a,{});memcpy(&y,b,{});{result} z={compiler}packed{i}(x,y);memcpy(out,&z,{bytes});}}\n",
                width / 8,
                width / 8
            );
        }
        table += &format!(
            "{{\"{}\",{},{},{},{},{},kabi{i},cabi{i},kpacked_memory{i}}},\n",
            case.name,
            width / 8,
            case.input_bits,
            case.output_bits,
            bytes,
            case.operation as u8
        );
    }
    harness += include_str!("fixtures/x86_integer_lanes.h");
    harness += "typedef void (*PackedKernel)(const void*,const void*,void*);\nstruct PackedCase {const char*name;unsigned bytes,input_bits,output_bits,result_bytes,operation;PackedKernel keel,vendor,memory;};\nstatic const struct PackedCase cases[]={\n";
    harness += &table;
    harness += "};\n";
    harness += include_str!("fixtures/x86_packed_integer_runner.c");
    (source, harness)
}

#[test]
fn packed_integer_bindings_preserve_types_features_and_operand_evaluation() {
    // Inclusion must remain legal without enabling the newer inline bodies.
    program(
        &compile("#include <immintrin.h>\nint f(void){return 0;}"),
        Features::for_level(CpuLevel::V1),
    )
    .unwrap();
    for (i, case) in PACKED_INTEGER_INTRINSICS.iter().enumerate() {
        let width = case.width;
        let kernel = packed_integer_kernel(i, case);
        let module = compile(&format!(
            "#include <immintrin.h>\n{}",
            kernel.lines().next().unwrap()
        ));
        assert!(module.to_string().contains(case.ir), "{}", case.name);
        let features = Features::for_level(if width == 256 {
            CpuLevel::V3
        } else {
            CpuLevel::V2
        });
        program(&module, features).unwrap();
        if case.feature.is_empty() && width == 256 {
            // Generic unpack shuffles may use narrower/scalar operations.
            program(&module, features.with_features("-avx2").unwrap()).unwrap();
            assert!(
                program(&module, Features::for_level(CpuLevel::V1))
                    .unwrap_err()
                    .message
                    .contains("256-bit SIMD")
            );
        } else if case.feature.is_empty() {
            program(&module, Features::for_level(CpuLevel::V1)).unwrap();
        } else {
            let missing = features
                .with_features(&format!("-{}", case.feature))
                .unwrap();
            let error = program(&module, missing).unwrap_err();
            assert!(
                error.message.contains(case.feature),
                "{}: {error:?}",
                case.name
            );
        }
        let args = if case.operation == PackedIntegerOperation::SignMask {
            "next()"
        } else {
            "next(),next()"
        };
        let result = if case.output_bits == 0 {
            "int".to_string()
        } else {
            format!("__m{width}i")
        };
        let once = compile(&format!(
            "#include <immintrin.h>\nextern __m{width}i next(void);{result} f(void){{return {}({args});}}",
            case.name
        ));
        assert_eq!(
            once.to_string().matches("call @next()").count(),
            if case.operation == PackedIntegerOperation::SignMask {
                1
            } else {
                2
            },
            "{}",
            case.name
        );
    }
    for call in [
        "__builtin_keel_avx2_vpaddsb256(a)",
        "__builtin_keel_avx2_vpaddsb256(a,1)",
        "__builtin_keel_sse2_pavgb128(a,a)",
        "__builtin_keel_ssse3_pmaddubsw128(a,a)",
    ] {
        let source =
            format!("typedef int V __attribute__((vector_size(32)));V f(V a){{return {call};}}");
        assert!(
            keel_c::compile(
                &source,
                "invalid.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "{call}"
        );
    }
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let source = "typedef signed char V __attribute__((vector_size(16)));V f(V a){return __builtin_keel_ssse3_pshufb128(a,a);}";
        assert!(
            keel_c::compile(
                source,
                "foreign.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .is_err()
        );
        let source=keel_c::preprocess("#if __has_builtin(__builtin_keel_ssse3_pshufb128) || __has_builtin(__builtin_keel_avx2_vpmovmskb256)\n#error foreign intrinsic\n#endif\nint f(void){return 0;}","foreign.c",&keel_c::PreprocessOptions {target,..Default::default()}).unwrap();
        keel_c::compile(
            &source,
            "foreign.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
    }
}

#[test]
fn packed_integer_families_match_vendor_and_scalar_oracles() {
    use keel_codegen::regalloc::Pools;
    use std::{fs, path::PathBuf, process::Command};
    let directory = std::env::var_os("KEEL_C_PACKED_INTEGER_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-packed-integer"));
    fs::create_dir_all(&directory).unwrap();
    let (source, harness) = packed_integer_sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(
        directory.join("oracle.c"),
        source.replace("kpacked", "cpacked"),
    )
    .unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    let raw = compile(&source);
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let features = Features::for_level(CpuLevel::V3);
    let empty = Pools {
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
            &raw,
        ),
        (
            "trees",
            SelectionTier::Trees,
            AllocationTier::Full,
            false,
            &raw,
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
        let typed =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&typed, features).unwrap();
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
            &mut |part| chunks.push(part),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        fs::write(directory.join(format!("{name}.o")), object).unwrap();
    }
    command(Command::new("clang").current_dir(&directory).args([
        "-target",
        "x86_64-unknown-linux-gnu",
        "-ffreestanding",
        "-O2",
        "-mavx2",
        "-c",
        "oracle.c",
        "-o",
        "oracle.o",
    ]));
    command(Command::new("clang").current_dir(&directory).args([
        "-target",
        "x86_64-unknown-linux-gnu",
        "-ffreestanding",
        "-O2",
        "-mavx2",
        "-c",
        "harness.c",
        "-o",
        "harness.o",
    ]));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("avx2") {
        for name in ["baseline", "trees", "optimized-spill"] {
            command(Command::new("clang").current_dir(&directory).args([
                "harness.o",
                "oracle.o",
                &format!("{name}.o"),
                "-o",
                "run",
            ]));
            command(&mut Command::new(directory.join("run")));
        }
    } else {
        eprintln!("native packed-integer execution skipped: host lacks AVX2");
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    eprintln!(
        "native packed-integer execution skipped: requires x86-64 Linux; artifacts: {}",
        directory.display()
    );
}

#[path = "x86_intrinsics/x86_avx512_packed.rs"]
mod x86_avx512_packed;
#[path = "x86_intrinsics/x86_avx512_permute.rs"]
mod x86_avx512_permute;

#[path = "x86_intrinsics/x86_avx512_lane.rs"]
mod x86_avx512_lane;

#[path = "x86_intrinsics/x86_avx512_scalar_move.rs"]
mod x86_avx512_scalar_move;

#[path = "x86_intrinsics/x86_avx512_construct.rs"]
mod x86_avx512_construct;

#[path = "x86_intrinsics/x86_avx512_compat.rs"]
mod x86_avx512_compat;

#[path = "x86_intrinsics/x86_avx512_reduce_integer.rs"]
mod x86_avx512_reduce_integer;

#[path = "x86_intrinsics/x86_avx512_reduce_float.rs"]
mod x86_avx512_reduce_float;

#[path = "x86_intrinsics/x86_avx512_vl.rs"]
mod x86_avx512_vl;

#[path = "x86_intrinsics/x86_avx512_vl_float.rs"]
mod x86_avx512_vl_float;

#[path = "x86_intrinsics/x86_avx512_vl_integer_move.rs"]
mod x86_avx512_vl_integer_move;

#[path = "x86_intrinsics/x86_avx512_vl_convert.rs"]
mod x86_avx512_vl_convert;

#[path = "x86_intrinsics/x86_avx512_cd.rs"]
mod x86_avx512_cd;

#[path = "x86_intrinsics/x86_avx512_dq_bits.rs"]
mod x86_avx512_dq_bits;

#[path = "x86_intrinsics/x86_avx512_bw.rs"]
mod x86_avx512_bw;

#[path = "x86_intrinsics/x86_avx512_bw_mask.rs"]
mod x86_avx512_bw_mask;
#[path = "x86_intrinsics/x86_avx512_dq_convert.rs"]
mod x86_avx512_dq_convert;
#[path = "x86_intrinsics/x86_avx512_vl_fma.rs"]
mod x86_avx512_vl_fma;
