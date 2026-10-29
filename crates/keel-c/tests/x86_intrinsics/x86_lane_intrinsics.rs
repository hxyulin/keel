//! Exact lane rearrangements preserve bits, lane boundaries and single evaluation.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, path::PathBuf, process::Command};

#[derive(Clone)]
struct Case {
    name: String,
    bytes: usize,
    lane: usize,
    operation: u32,
    control: u32,
    unary: bool,
    variable: bool,
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for bytes in [16, 32] {
        let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
        for (suffix, lane) in [("ps", 4), ("pd", 8), ("epi16", 2), ("epi32", 4)] {
            for control in [
                0,
                if lane == 8 { 3 } else { 10 },
                (1u32 << (bytes / lane).min(8)) - 1,
            ] {
                cases.push(Case {
                    name: format!("{prefix}_blend_{suffix}"),
                    bytes,
                    lane,
                    operation: 0,
                    control,
                    unary: false,
                    variable: false,
                });
            }
        }
        for (suffix, operation) in [("slli", 1), ("srli", 2), ("alignr", 3)] {
            for control in [0, 1, 15, 16, 17, 31, 32, 255] {
                let name = if operation == 3 {
                    format!("{prefix}_alignr_epi8")
                } else {
                    format!("{prefix}_{suffix}_si{}", bytes * 8)
                };
                cases.push(Case {
                    name,
                    bytes,
                    lane: 1,
                    operation,
                    control,
                    unary: operation != 3,
                    variable: false,
                });
            }
        }
        for (suffix, operation) in [("shufflelo_epi16", 4), ("shufflehi_epi16", 5)] {
            for control in [0, 27, 255] {
                cases.push(Case {
                    name: format!("{prefix}_{suffix}"),
                    bytes,
                    lane: 2,
                    operation,
                    control,
                    unary: true,
                    variable: false,
                });
            }
        }
        for (suffix, lane) in [("ps", 4), ("pd", 8)] {
            for control in [
                0,
                if lane == 4 { 27 } else { 1 },
                if lane == 4 {
                    255
                } else {
                    (1 << (bytes / lane)) - 1
                },
            ] {
                cases.push(Case {
                    name: format!("{prefix}_permute_{suffix}"),
                    bytes,
                    lane,
                    operation: 6,
                    control,
                    unary: true,
                    variable: false,
                });
            }
            cases.push(Case {
                name: format!("{prefix}_permutevar_{suffix}"),
                bytes,
                lane,
                operation: 7,
                control: 0,
                unary: false,
                variable: true,
            });
        }
    }
    for control in [0, 6, 15] {
        cases.push(Case {
            name: "_mm256_shuffle_pd".into(),
            bytes: 32,
            lane: 8,
            operation: 8,
            control,
            unary: false,
            variable: false,
        });
    }
    for suffix in ["ps", "pd", "si256"] {
        for control in [0, 0x21, 0x83, 0x38, 0xff] {
            cases.push(Case {
                name: format!("_mm256_permute2f128_{suffix}"),
                bytes: 32,
                lane: 16,
                operation: 9,
                control,
                unary: false,
                variable: false,
            });
        }
    }
    for control in [0, 27, 255] {
        cases.push(Case {
            name: "_mm256_permute4x64_pd".into(),
            bytes: 32,
            lane: 8,
            operation: 10,
            control,
            unary: true,
            variable: false,
        });
    }
    for suffix in ["ps", "epi32"] {
        cases.push(Case {
            name: format!("_mm256_permutevar8x32_{suffix}"),
            bytes: 32,
            lane: 4,
            operation: 11,
            control: 0,
            unary: false,
            variable: true,
        });
    }
    for bytes in [16, 32] {
        let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
        for (suffix, operation) in [("bslli", 1), ("bsrli", 2)] {
            cases.push(Case {
                name: format!(
                    "{prefix}_{suffix}_{}",
                    if bytes == 16 { "si128" } else { "epi128" }
                ),
                bytes,
                lane: 1,
                operation,
                control: 7,
                unary: true,
                variable: false,
            });
        }
    }
    cases.push(Case {
        name: "_mm256_permute2x128_si256".into(),
        bytes: 32,
        lane: 16,
        operation: 9,
        control: 0x21,
        unary: false,
        variable: false,
    });
    cases
}
fn frontend(source: &str) -> Result<keel_ir::Module, String> {
    let target = Target::X86_64LinuxGnu;
    let source = keel_c::preprocess(
        source,
        "lanes.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &source,
        "lanes.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}
fn command(cmd: &mut Command) {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) ref_##n\n#else\n#define FN(n) n\n#endif\n",
    );
    for (bytes, t) in [(16, "__m128i"), (32, "__m256i")] {
        source.push_str(&format!("static {t} take{bytes}(const void *p,int *count){{ {t} x; ++*count; __builtin_memcpy(&x,p,{bytes}); return x; }}\n"));
    }
    source.push_str(
        "long long FN(dynamic_extract)(__m256i value, int index){return value[index];}\n",
    );
    let mut runner = String::from(
        "typedef unsigned char U; typedef unsigned long long Q;\nextern int memcmp(const void*,const void*,unsigned long);\nextern void *memcpy(void*,const void*,unsigned long);\nextern int printf(const char*,...);\ntypedef int (*Kernel)(void*,const void*,const void*);\nstruct Case {Kernel actual,reference; unsigned bytes,lane,op,control;};\n",
    );
    let cases = cases();
    for (i, c) in cases.iter().enumerate() {
        let typ = if c.name.ends_with("_ps") {
            format!("__m{}", c.bytes * 8)
        } else if c.name.ends_with("_pd") {
            format!("__m{}d", c.bytes * 8)
        } else {
            format!("__m{}i", c.bytes * 8)
        };
        let a = format!("({typ})take{}(a,&ca)", c.bytes);
        let b = if c.variable {
            format!("(__m{}i)take{}(b,&cb)", c.bytes * 8, c.bytes)
        } else {
            format!("({typ})take{}(b,&cb)", c.bytes)
        };
        let args = if c.unary {
            format!("{a},{}", c.control)
        } else if c.variable {
            format!("{a},{b}")
        } else {
            format!("{a},{b},{}", c.control)
        };
        source.push_str(&format!("int FN(lane{i})(void*out,const void*a,const void*b){{int ca=0,cb=0; {typ} r={}({args}); __builtin_memcpy(out,&r,{}); return ca==1 && cb=={};}}\n",c.name,c.bytes,u32::from(!c.unary)));
        runner.push_str(&format!("extern int lane{i}(void*,const void*,const void*); extern int ref_lane{i}(void*,const void*,const void*);\n"));
    }
    runner.push_str("typedef long long V __attribute__((vector_size(32))); extern long long dynamic_extract(V,int); extern long long ref_dynamic_extract(V,int);\n");
    runner.push_str("static const struct Case cases[]={\n");
    for (i, c) in cases.iter().enumerate() {
        runner.push_str(&format!(
            "{{lane{i},ref_lane{i},{},{},{},{}}},\n",
            c.bytes, c.lane, c.operation, c.control
        ));
    }
    runner.push_str("};\n");
    runner.push_str(include_str!("../fixtures/x86_lane_intrinsics_runner.c"));
    (source, runner)
}
#[test]
fn lane_immediates_reject_nonconstant_and_out_of_range_controls() {
    frontend("#include <immintrin.h>\n__m128 f(__m128 x){return _mm_permute_ps(x,27);}\n__m256d g(__m256d x){return _mm256_permute4x64_pd(x,27);}").unwrap();
    for name in [
        "_mm_blend_ps",
        "_mm256_alignr_epi8",
        "_mm_shufflehi_epi16",
        "_mm256_permute2f128_si256",
    ] {
        for control in ["i", "-1", "256", "(i++,0)", "0.0"] {
            let unary = name.contains("shufflehi");
            let wide = name.starts_with("_mm256");
            let typ = if name.ends_with("_ps") {
                "__m128"
            } else if wide {
                "__m256i"
            } else {
                "__m128i"
            };
            let args = if unary {
                format!("a,{control}")
            } else {
                format!("a,a,{control}")
            };
            assert!(
                frontend(&format!(
                    "#include <immintrin.h>\n{typ} f({typ} a,int i){{return {name}({args});}}\n"
                ))
                .is_err(),
                "{name} accepted {control}"
            );
        }
    }
}
#[test]
fn dynamic_vector_extract_reserves_the_entire_vector() {
    for bytes in [16, 32] {
        let module=frontend(&format!("typedef long long V __attribute__((vector_size({bytes}))); long long extract(V x,int i){{return x[i];}}")).unwrap();
        assert!(
            module.functions[0]
                .stack_slots()
                .any(|(_, slot)| slot.size == bytes && slot.align == bytes),
            "dynamic extraction requires a full-width temporary with vector alignment: {module}"
        );
    }
}
#[test]
fn lane_operations_match_vendor_and_scalar_oracles() {
    let directory = std::env::var_os("KEEL_C_LANE_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("c-lanes"));
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
    for (name, tier, allocation, spill, module) in [
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
            allocator: &allocation,
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
            &mut |p| chunks.push(p),
        )
        .unwrap();
        assert_eq!(object, chunks.finish().unwrap());
        fs::write(directory.join(format!("{name}.o")), object).unwrap();
    }
    for (file, args) in [
        ("kernels.c", vec!["-DREFERENCE", "-mavx2"]),
        ("runner.c", vec!["-mavx2"]),
    ] {
        let mut cmd = Command::new("clang");
        cmd.current_dir(&directory).args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-ffreestanding",
            "-O2",
        ]);
        cmd.args(args).args([
            "-c",
            file,
            "-o",
            if file == "kernels.c" {
                "reference.o"
            } else {
                "runner.o"
            },
        ]);
        command(&mut cmd);
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
        eprintln!("native lane execution skipped: host lacks AVX2");
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    eprintln!(
        "native lane execution skipped: requires x86-64 Linux; artifacts: {}",
        directory.display()
    );
}
