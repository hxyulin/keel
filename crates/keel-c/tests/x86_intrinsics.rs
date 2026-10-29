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
    for (header, level) in [("smmintrin.h", CpuLevel::V1), ("immintrin.h", CpuLevel::V3)] {
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
        program(&module, Features::for_level(level)).unwrap();
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
        include_str!("fixtures/x86_integer_family_oracle.h"),
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
