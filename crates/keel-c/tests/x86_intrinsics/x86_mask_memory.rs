//! Masked memory and gathers suppress inactive accesses and retain effects.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, path::PathBuf, process::Command};
fn frontend(source: &str) -> Result<keel_ir::Module, String> {
    let target = Target::X86_64LinuxGnu;
    let source = keel_c::preprocess(
        source,
        "mask-memory.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &source,
        "mask-memory.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}
fn command(cmd: &mut Command) {
    let o = cmd.output().unwrap();
    assert!(
        o.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\n",
    );
    let mut runner = String::from(
        "typedef unsigned char U;typedef unsigned long long Q;\ntypedef void (*Memory)(void*,const void*,void*,const void*,const void*);\ntypedef void (*Gather)(void*,const void*,const void*,const void*,const void*);\nstruct MemoryCase{Memory actual,reference;unsigned bytes,lane;};\nstruct GatherCase{Gather actual,reference;unsigned bytes,lane,indexbytes,indexlane,scale,masked;};\n",
    );
    let mut memory = Vec::new();
    let mut gather = Vec::new();
    for bits in [128, 256] {
        for (suffix, lane, scalar) in [
            ("ps", 4, "float"),
            ("pd", 8, "double"),
            ("epi32", 4, "int"),
            ("epi64", 8, "long long"),
        ] {
            let i = memory.len();
            let p = if bits == 128 { "_mm" } else { "_mm256" };
            let bytes = bits / 8;
            let ty = format!(
                "__m{bits}{}",
                if suffix.starts_with("epi") {
                    "i"
                } else if suffix == "pd" {
                    "d"
                } else {
                    ""
                }
            );
            source.push_str(&format!("void FN(memory{i})(void*out,const void*base,void*dest,const void*mask,const void*data){{__m{bits}i m;{ty} d;__builtin_memcpy(&m,mask,{bytes});__builtin_memcpy(&d,data,{bytes});{ty} r={p}_maskload_{suffix}((const {scalar}*)base,m);{p}_maskstore_{suffix}(({scalar}*)dest,m,d);__builtin_memcpy(out,&r,{bytes});}}\n"));
            runner.push_str(&format!("extern void memory{i}(void*,const void*,void*,const void*,const void*);extern void reference_memory{i}(void*,const void*,void*,const void*,const void*);\n"));
            memory.push(format!("{{memory{i},reference_memory{i},{bytes},{lane}}}"));
        }
    }
    for indexlane in [4, 8] {
        for (suffix, lane, scalar) in [
            ("ps", 4, "float"),
            ("pd", 8, "double"),
            ("epi32", 4, "int"),
            ("epi64", 8, "long long"),
        ] {
            for bits in [128, 256] {
                for scale in [1, 2, 4, 8] {
                    for masked in [false, true] {
                        let i = gather.len();
                        let p = if bits == 128 { "_mm" } else { "_mm256" };
                        let outputbits = if indexlane == 8 && lane == 4 {
                            128
                        } else {
                            bits
                        };
                        let indexbits = if indexlane == 4 && lane == 8 {
                            128
                        } else {
                            bits
                        };
                        let bytes = outputbits / 8;
                        let indexbytes = indexbits / 8;
                        let ty = format!(
                            "__m{outputbits}{}",
                            if suffix.starts_with("epi") {
                                "i"
                            } else if suffix == "pd" {
                                "d"
                            } else {
                                ""
                            }
                        );
                        let maskty = if suffix.starts_with("epi") {
                            format!("__m{outputbits}i")
                        } else {
                            ty.clone()
                        };
                        let name = format!(
                            "{p}_{}i{}gather_{suffix}",
                            if masked { "mask_" } else { "" },
                            indexlane * 8
                        );
                        let args = if masked {
                            format!("s,(const {scalar}*)base,v,m,{scale}")
                        } else {
                            format!("(const {scalar}*)base,v,{scale}")
                        };
                        source.push_str(&format!("void FN(gather{i})(void*out,const void*base,const void*index,const void*src,const void*mask){{__m{indexbits}i v;{ty} s;{maskty} m;__builtin_memcpy(&v,index,{indexbytes});__builtin_memcpy(&s,src,{bytes});__builtin_memcpy(&m,mask,{bytes});{ty} r={name}({args});__builtin_memcpy(out,&r,{bytes});}}\n"));
                        runner.push_str(&format!("extern void gather{i}(void*,const void*,const void*,const void*,const void*);extern void reference_gather{i}(void*,const void*,const void*,const void*,const void*);\n"));
                        gather.push(format!("{{gather{i},reference_gather{i},{bytes},{lane},{indexbytes},{indexlane},{scale},{}}}",u32::from(masked)));
                    }
                }
            }
        }
    }
    runner.push_str(&format!("static const struct MemoryCase memory_cases[]={{{}}};\nstatic const struct GatherCase gather_cases[]={{{}}};\n",memory.join(","),gather.join(",")));
    runner.push_str(include_str!("../fixtures/x86_mask_memory_runner.c"));
    (source, runner)
}
#[test]
fn gather_scale_and_memory_features_are_checked() {
    for scale in ["0", "3", "5", "16", "i", "(i++,1)"] {
        let error=frontend(&format!("#include <immintrin.h>\n__m128 f(const float*p,__m128i v,int i){{return _mm_i32gather_ps(p,v,{scale});}}\n")).unwrap_err();
        assert!(
            error.contains(
                "gather scale must be an integer constant expression equal to 1, 2, 4 or 8"
            ),
            "{error}"
        );
    }
    let module = frontend(
        "#include <immintrin.h>\nvoid f(float*p,__m128i m,__m128 v){_mm_maskstore_ps(p,m,v);}",
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(CpuLevel::V1),
    };
    let error = keel_codegen::compile_program_with_pipeline(
        &module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("avx"), "{error:?}");
    let mut module = frontend(
        "#include <immintrin.h>\nvoid f(const float*p,__m128i m){(void)_mm_maskload_ps(p,m);}",
    )
    .unwrap();
    keel_opt::optimize(&mut module).unwrap();
    assert!(module.to_string().contains("x86_avx_vmaskmovps_load128"));
}
#[test]
fn mask_memory_and_gathers_match_guarded_scalar_and_vendor_oracles() {
    let directory = std::env::var_os("KEEL_C_MASK_MEMORY_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-mask-memory"));
    fs::create_dir_all(&directory).unwrap();
    let (source, runner) = sources();
    fs::write(directory.join("kernels.c"), &source).unwrap();
    fs::write(directory.join("runner.c"), runner).unwrap();
    let raw = frontend(&source).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
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
            &mut |p| chunks.push(p),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        fs::write(directory.join(format!("{name}.o")), object).unwrap();
    }
    for file in ["kernels.c", "runner.c"] {
        let mut cmd = Command::new("clang");
        cmd.current_dir(&directory).args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-O2",
            "-mavx2",
        ]);
        if file == "kernels.c" {
            cmd.arg("-DREFERENCE");
        }
        command(cmd.args([
            "-c",
            file,
            "-o",
            if file == "kernels.c" {
                "reference.o"
            } else {
                "runner.o"
            },
        ]));
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("avx2") {
        for name in ["baseline", "optimized-spill"] {
            command(Command::new("clang").current_dir(&directory).args([
                "runner.o",
                "reference.o",
                &format!("{name}.o"),
                "-o",
                "run",
            ]));
            command(&mut Command::new(directory.join("run")));
        }
    } else {
        eprintln!("native masked memory execution skipped: host lacks AVX2");
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    eprintln!(
        "native masked memory execution skipped: requires x86-64 Linux; artifacts: {}",
        directory.display()
    );
}
