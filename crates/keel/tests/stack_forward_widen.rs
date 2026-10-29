//! C construction -> Full recognition -> strict feature objects -> native oracle.
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use std::{fs, process::Command};
const SOURCE: &str = include_str!("stack_forward_widen/kernels.c");
const ORACLE: &str = include_str!("stack_forward_widen/oracle.c");
const FOOTPRINT: &str = "decl @later()
func @footprint(ptr, ptr) {
 ss0 = stack_slot 16, align 16
block0(v0: ptr, v1: ptr):
 v2 = stack_addr ss0
 v3 = iconst.i64 8
 v4 = ptradd v2, v3
 v5 = load.i32x4 align 1 v1
 v6 = vextract v5, 0
 v7 = sext.i64 v6
 v8 = vextract v5, 1
 v9 = sext.i64 v8
 store v7, v2
 store v9, v4
 v10 = load.i64x2 v2
 store align 1 v10, v0
 call @later()
 return
}";
fn x86_native(profile: usize) -> bool {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) || profile == 4 {
        return false;
    }
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap();
    let flags: Vec<_> = cpuinfo
        .lines()
        .filter_map(|line| line.strip_prefix("flags\t\t: "))
        .map(|line| {
            line.split_whitespace()
                .collect::<std::collections::HashSet<_>>()
        })
        .collect();
    let required = match profile {
        0 => &["sse2"][..],
        1 | 3 => &[
            "cx16", "lahf_lm", "popcnt", "pni", "ssse3", "sse4_1", "sse4_2",
        ][..],
        2 => &[
            "cx16", "lahf_lm", "popcnt", "pni", "ssse3", "sse4_1", "sse4_2", "avx", "avx2",
            "xsave", "bmi1", "bmi2", "f16c", "fma", "abm", "movbe",
        ][..],
        _ => return false,
    };
    !flags.is_empty()
        && flags
            .iter()
            .all(|f| required.iter().all(|name| f.contains(name)))
}

fn command(c: &mut Command) {
    let o = c.output().unwrap();
    assert!(o.status.success(), "{c:?}: {o:?}");
}

#[test]
fn private_signed_widen_c_objects_and_native_footprints() {
    let root = std::env::var_os("KEEL_C_STACK_WIDEN_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from("/tmp")
                .join(format!("keel-c-stack-widen-{}", std::process::id()))
        });
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("oracle.c"), ORACLE).unwrap();
    fs::write(root.join("kernels.c"), SOURCE).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    // The frontend and optimizer depend on the target, not selector, CPU profile
    // or register pressure. Prepare each target once across those configurations.
    let mut prepared: [Option<Vec<keel_ir::Module>>; 2] = [None, None];
    let native = cfg!(any(
        all(target_arch = "aarch64", target_os = "macos"),
        all(target_arch = "x86_64", target_os = "linux")
    ));
    if native {
        command(
            Command::new("clang")
                .args(["-O2", "-c"])
                .arg(root.join("oracle.c"))
                .arg("-o")
                .arg(root.join("oracle.o")),
        );
    }
    // V3 with AVX/AVX2 disabled exercises the existing legal SSE2 recipe.
    for profile in 0..5 {
        let arm = profile == 4;
        let target = if arm {
            Target::Aarch64AppleDarwin
        } else {
            Target::X86_64LinuxGnu
        };
        let mut features = Features::for_level(match profile {
            0 => CpuLevel::V1,
            1 => CpuLevel::V2,
            _ => CpuLevel::V3,
        });
        if profile == 3 {
            features.sse41 = false;
            features.avx = false;
            features.avx2 = false;
            features.f16c = false;
            features.fma = false;
        }
        let modules = prepared[usize::from(arm)].get_or_insert_with(|| {
            let mut modules = Vec::new();
            for (label, optimize) in [("before_", false), ("after_", true)] {
                let mut m = keel_c::compile(
                    &SOURCE.replace("PREFIX", label),
                    "widen.c",
                    keel_c::Options {
                        target,
                        debug_info: false,
                    },
                )
                .unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                for f in &m.functions {
                    assert_eq!(f.to_string().contains("swiden_low.i64x2"), optimize, "{f}");
                    if optimize {
                        assert_eq!(
                            f.layout_insts()
                                .filter(|&(_, i)| matches!(
                                    f.inst_data(i),
                                    keel_ir::InstData::Load {
                                        ty: keel_ir::Type::Vector(keel_ir::VectorType::I32x4),
                                        ..
                                    }
                                ))
                                .count(),
                            1,
                            "{f}"
                        );
                        assert!(f.to_string().contains("store"));
                    }
                }
                fs::write(root.join(format!("{target:?}-{label}.ir")), m.to_string()).unwrap();
                modules.push(m);
            }
            // Explicit unaligned IR has the same private construction and exercises
            // the matcher itself, while preserving the entire external input load.
            let mut witness = keel_ir::parse(FOOTPRINT).unwrap();
            keel_opt::optimize(&mut witness).unwrap();
            keel_ir::verify_module(&witness).unwrap();
            assert!(witness.to_string().contains("swiden_low.i64x2"));
            fs::write(
                root.join(format!("{target:?}-footprint.ir")),
                witness.to_string(),
            )
            .unwrap();
            modules.push(witness);
            modules
        });
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for spills in [false, true] {
                let selector = FeatureSelector {
                    inner: &sel,
                    features: if arm { Features::default() } else { features },
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                };
                // keel-codegen vector_widen_x86 checks text, program and chunk
                // parity for swiden_low; this test compiles the program route.
                let mut objects = Vec::new();
                for (i, m) in modules.iter().enumerate() {
                    let obj = if arm {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let p = keel_codegen::aarch64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                    } else {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let p = keel_codegen::x86_64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap()
                    };
                    let name = root.join(format!("{profile}-{sel:?}-{spills}-{i}.o"));
                    fs::write(&name, obj).unwrap();
                    objects.push(name);
                }
                // Execute only profiles supported by this host, with the Full
                // selector. Strict assembly feature checking remains active for
                // every selector independently of host CPUID.
                if sel == SelectionTier::Full
                    && (x86_native(profile)
                        || arm && cfg!(all(target_arch = "aarch64", target_os = "macos")))
                {
                    let exe = root.join(format!("native-{profile}-{sel:?}-{spills}"));
                    command(
                        Command::new("clang")
                            .arg(root.join("oracle.o"))
                            .args(&objects)
                            .arg("-o")
                            .arg(&exe),
                    );
                    command(Command::new(exe).current_dir(&root));
                }
            }
        }
    }
}
