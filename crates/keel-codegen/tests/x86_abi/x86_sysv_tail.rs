//! Compatible aggregate tail transport, with an exportable native Linux oracle.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_test_support::native_program;
use object::{Object, ObjectSection};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[6],
    callee_saved: &[3],
    float_caller_saved: &[1],
    float_callee_saved: &[],
};
/// x86-64-v3 features beyond the baseline, for `-march=x86-64-v3` programs.
const V3_FEATURES: [&str; 7] = ["avx", "avx2", "bmi1", "bmi2", "fma", "lzcnt", "movbe"];
const BAD_TAIL: &str = "unsigned long long __wrap_tail_gp_0_0(void) { return 0; }\n";

#[test]
fn compatible_tail_packet() {
    let module = keel_ir::parse(include_str!("../sysv_tail/corpus.ir")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = keel_test_support::TempDir::new("sysv-tail");
    let oracle_c = include_str!("../sysv_tail/oracle.c");
    let depth_s = include_str!("../sysv_tail/depth.S");
    fs::write(dir.join("oracle.c"), oracle_c).unwrap();
    let mut negative_controls = 0;
    // Clang assembles each distinct text once. Every configuration still
    // compares its own object with that oracle.
    let mut oracles = std::collections::HashMap::<String, Vec<u8>>::new();
    // V2 emits the same text as V1 in every configuration.
    for level in [CpuLevel::V1, CpuLevel::V3] {
        let features = Features::for_level(level);
        let mut module = module.clone();
        if level != CpuLevel::V3 {
            module.decls.retain(|d| d.name != "c_y" && d.name != "id_y");
            module.functions.retain(|f| {
                !f.name.starts_with("tail_y_")
                    && f.name != "result_y"
                    && f.name != "result_ind_y"
                    && f.name != "only_y"
            });
        }
        let output = Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-c",
            ])
            .arg(if level == CpuLevel::V1 {
                "-march=x86-64"
            } else {
                "-march=x86-64-v3"
            })
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join(format!("primary-{level:?}.o")))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            let mut m = module.clone();
            if selector == SelectionTier::Full {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &EMPTY),
                ] {
                    let feature = FeatureSelector {
                        inner: &selector,
                        features,
                    };
                    let pipeline = CodegenPipeline {
                        selector: &feature,
                        allocator: &allocator,
                    };
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                    let program =
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                            .unwrap();
                    let object =
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    let name = format!("{level:?}-{selector:?}-{allocator:?}-{pool}");
                    let asm = dir.join(format!("{name}.s"));
                    fs::write(&asm, &text).unwrap();
                    let clang = oracles.entry(text.clone()).or_insert_with(|| {
                        let oracle = dir.join("clang.o");
                        let output = Command::new("clang")
                            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                            .arg(&asm)
                            .arg("-o")
                            .arg(&oracle)
                            .output()
                            .unwrap();
                        assert!(
                            output.status.success(),
                            "{}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        let bytes = fs::read(&oracle).unwrap();
                        let clang = object::File::parse(&*bytes).unwrap();
                        clang
                            .section_by_name(".text")
                            .unwrap()
                            .data()
                            .unwrap()
                            .to_vec()
                    });
                    let keel = object::File::parse(&*object).unwrap();
                    assert_eq!(
                        clang.as_slice(),
                        keel.section_by_name(".text").unwrap().data().unwrap(),
                        "{name}"
                    );
                    let (march, required) = if level == CpuLevel::V1 {
                        ("-march=x86-64", &[][..])
                    } else {
                        ("-march=x86-64-v3", &V3_FEATURES[..])
                    };
                    for compiler in ["gcc", "cc"] {
                        let tail = |program: &str| {
                            native_program!(program)
                                .requires(required)
                                .file("oracle.c", oracle_c)
                                .file("depth.S", depth_s)
                                .file("keel.s", &text)
                                .file("keel.o", &object)
                                .step([compiler, "-O2", march, "-c", "oracle.c", "-o", "primary.o"])
                        };
                        tail(&format!("{name}-{compiler}"))
                            .step([compiler, "primary.o", "depth.S", "keel.o", "-o", "program"])
                            .stdout("KEEL SYSV TAIL PASS\n")
                            .check();
                        // Replacing one argument-permuting tail with a constant
                        // return must fail the independent value checks.
                        if level == CpuLevel::V1 && negative_controls < 2 {
                            negative_controls += 1;
                            tail(&format!("{name}-{compiler}-bad-tail"))
                                .file("bad-tail.c", BAD_TAIL)
                                .step([
                                    compiler,
                                    "primary.o",
                                    "depth.S",
                                    "keel.o",
                                    "bad-tail.c",
                                    "-Wl,--wrap=tail_gp_0_0",
                                    "-o",
                                    "program",
                                ])
                                .exit(1)
                                .check();
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn incompatible_tails_reject() {
    let cases = [
        (
            "decl @g(ptr byval {i32x8,i64})\nfunc @f(ptr byval [8 x i64]) {\nentry(v0:ptr):\n return_call @g(v0)\n}",
            "unsupported incoming stack alignment",
        ),
        (
            "decl @g(ptr byval {i64,i64,i64})\nfunc @f(ptr) {\nentry(v0:ptr):\n return_call @g(v0)\n}",
            "outgoing stack area grows",
        ),
        (
            "decl @g(ptr sret {i64,i64,i64})\nfunc @f(ptr sret {i64,i64,i64},ptr) {\nentry(v0:ptr,v1:ptr):\n return_call @g(v1)\n}",
            "non-forwarded sret",
        ),
        (
            "decl @g()\nfunc @f(ptr sret {i64,i64,i64}) {\nentry(v0:ptr):\n return_call @g()\n}",
            "return protocol mismatch",
        ),
        (
            "decl @g(ptr sret {f64,f64})\nfunc @f(ptr sret {i64,i64}) {\nentry(v0:ptr):\n return_call @g(v0)\n}",
            "return protocol mismatch",
        ),
    ];
    for (source, expected) in cases {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features: Features::for_level(CpuLevel::V3),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Fast,
        };
        let e = keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap_err();
        assert!(e.message.contains(expected), "{e:?}");
        let e =
            keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline).unwrap_err();
        assert!(e.message.contains(expected), "{e:?}");
    }
}

#[test]
fn witness_assembly_syntax() {
    let dir = std::env::temp_dir().join(format!("keel-tail-witness-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("depth.S"), include_str!("../sysv_tail/depth.S")).unwrap();
    let output = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("depth.S"))
        .arg("-o")
        .arg(dir.join("depth.o"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn identity_tail_is_one_jump() {
    // Distinct physical boundaries: GP and stack scalars, mixed SIMD/GP,
    // register aggregate results, and memory aggregate results/byval arguments.
    for signature in [
        "(i64,i64,i64,i64,i64,i64,i64,i64)->i64",
        "(f64,i64)->f64",
        "(ptr sret {i64,i64},ptr byval {i64,i64})",
        "(ptr sret {i64,i64,i64},ptr byval {i64,i64,i64})",
        "(i8 sext)->i8 sext",
    ] {
        let params = if signature.starts_with("(i64,i64,i64") {
            "v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64"
        } else if signature.starts_with("(f64") {
            "v0:f64,v1:i64"
        } else if signature.starts_with("(ptr") {
            "v0:ptr,v1:ptr"
        } else {
            "v0:i8"
        };
        let args = params
            .split(',')
            .map(|p| p.split(':').next().unwrap())
            .collect::<Vec<_>>()
            .join(",");
        let source = format!(
            "decl @target{signature}\nfunc @forward{signature} {{\nentry({params}):\n return_call @target({args})\n}}"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            let text = keel_codegen::x86_64::compile_with(&module, pools).unwrap();
            let instructions: Vec<_> = text
                .lines()
                .filter(|line| line.starts_with('\t') && !line.trim_start().starts_with('.'))
                .collect();
            assert_eq!(instructions, ["\tjmp\t\"target\"@PLT"], "{text}");
            let program = keel_codegen::x86_64::compile_program_with_pipeline(
                &module,
                pools,
                &CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                },
            )
            .unwrap();
            let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert_eq!(typed, keel_asm::x86::assemble_object(&text).unwrap());
            let file = object::File::parse(&*typed).unwrap();
            assert_eq!(
                file.section_by_name(".text").unwrap().data().unwrap().len(),
                5
            );
        }
    }
}

#[test]
fn identity_tail_keeps_debug_frame_and_argument_permutations() {
    let source = "decl @target(i64,i64)->i64\nfunc @forward(i64,i64)->i64 {\nentry(v0:i64,v1:i64):\n return_call @target(v1,v0)\n}";
    let module = keel_ir::parse(source).unwrap();
    let text = keel_codegen::x86_64::compile(&module).unwrap();
    assert!(text.contains("\tpush\trbp"), "{text}");
    let explicit = source.replace("@target(v1,v0)", "@target(v0 : i64,v1 : i64)");
    let module = keel_ir::parse(&explicit).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = keel_codegen::x86_64::compile(&module).unwrap();
    assert!(text.contains("\tpush\trbp"), "{text}");
    let mut module = keel_ir::parse(&source.replace("@target(v1,v0)", "@target(v0,v1)")).unwrap();
    module.files.push("forward.c".into());
    let text = keel_codegen::x86_64::compile(&module).unwrap();
    assert!(text.contains("\tpush\trbp"), "{text}");
}
