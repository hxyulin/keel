use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[path = "fixtures/reduction_casts.rs"]
mod casts;
#[path = "fixtures/reduction_multi.rs"]
mod multi;
fn source() -> String {
    format!(
        "{}{}{}{}",
        include_str!("fixtures/reduction_affine.ir"),
        casts::source(),
        multi::source(),
        include_str!("fixtures/reduction_logic.ir").replace("load.i64 v16", "load.i64 notrap v16")
    )
}
fn module(tier: keel_opt::OptimizationTier) -> keel_ir::Module {
    let mut m = keel_ir::parse(&source()).unwrap();
    // Apply the extended pass first: tier passes may recognize the scalar
    // tail, but must preserve the extended pass's guards and seed merge.
    keel_opt::run_passes(&mut m, &["loop-reduce-multi"]).unwrap();
    keel_opt::optimize_with_tier(&mut m, tier).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.functions.iter().all(|f| {
        f.layout_insts()
            .any(|(_, i)| f.inst_data(i).is_vector_instruction())
    }));
    m
}
#[test]
fn extended_reductions_compile_to_both_target_objects() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let m = module(tier);
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            for selection in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selection,
                        allocator: &allocation,
                    };
                    let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
                    let p =
                        keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
                    let evidence = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join("reduce-extended-static");
                    std::fs::create_dir_all(&evidence).unwrap();
                    let name = format!("{tier:?}-{target:?}-{selection:?}-{allocation:?}");
                    std::fs::write(evidence.join(format!("{name}.s")), &text).unwrap();
                    std::fs::write(evidence.join(format!("{name}.keel")), m.to_string()).unwrap();
                    match target {
                        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                        Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
                            assert!(text.contains(".2d"), "{text}");
                            assert_eq!(
                                keel_asm::assemble_object(&text).unwrap(),
                                keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                            );
                        }
                        Target::X86_64LinuxGnu => {
                            assert!(text.contains("paddq"), "{text}");
                            assert_eq!(
                                keel_asm::x86::assemble_object(&text).unwrap(),
                                keel_asm::x86::assemble_program_object(&p).unwrap()
                            );
                        }
                    }
                }
            }
        }
    }
}
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_affine_offsets_and_wrap_fallback_match_independent_oracle() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reduce-extended");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        format!(
            "{}\n{}\n{}\n{}",
            include_str!("fixtures/reduction_logic.c"),
            include_str!("fixtures/reduction_multi.c"),
            include_str!("fixtures/reduction_casts.c"),
            include_str!("fixtures/reduction_affine.c")
                .replace("return 0;", "return casts_oracle(region, page) || multi_oracle(region, page) || logic_oracle(region, page);")
        ),
    )
    .unwrap();
    compile_driver(&dir);
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let m = module(tier);
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selection,
                    allocator: &allocation,
                };
                let p = keel_codegen::compile_program_with_pipeline(&m, native_target(), &pipeline)
                    .unwrap();
                fs::write(
                    dir.join("test.o"),
                    if cfg!(target_arch = "x86_64") {
                        keel_asm::x86::assemble_program_object(&p).unwrap()
                    } else {
                        keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                    },
                )
                .unwrap();
                let output = Command::new("clang")
                    .arg(dir.join("driver.o"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(dir.join("test")).status().unwrap().success(),
                    "{tier:?}/{selection:?}/{allocation:?}"
                );
            }
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn extended_passes_preserve_first_fault_between_two_read_streams() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reduce-extended-faults");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("fixtures/reduction_fault_order.c"),
    )
    .unwrap();
    compile_driver(&dir);
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(include_str!("fixtures/reduction_fault_order.ir")).unwrap();
        keel_opt::run_passes(&mut m, &["loop-reduce-multi"]).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            !m.functions[0]
                .layout_insts()
                .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocation,
            };
            let program =
                keel_codegen::compile_program_with_pipeline(&m, native_target(), &pipeline)
                    .unwrap();
            fs::write(
                dir.join("test.o"),
                if cfg!(target_arch = "x86_64") {
                    keel_asm::x86::assemble_program_object(&program).unwrap()
                } else {
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                },
            )
            .unwrap();
            let output = Command::new("clang")
                .arg(dir.join("driver.o"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                Command::new(dir.join("test")).status().unwrap().code(),
                Some(20)
            );
        }
    }
}

/// Compiles `dir/driver.c` once without optimization or LTO; each
/// configuration links the object.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn compile_driver(dir: &std::path::Path) {
    let output = std::process::Command::new("clang")
        .args(["-O0", "-fno-lto", "-ffp-contract=off", "-c"])
        .arg(dir.join("driver.c"))
        .arg("-o")
        .arg(dir.join("driver.o"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn native_target() -> Target {
    if cfg!(target_arch = "x86_64") {
        Target::X86_64LinuxGnu
    } else {
        Target::Aarch64AppleDarwin
    }
}

fn ordered_dot_module() -> keel_ir::Module {
    let mut source = String::new();
    for (ty, shift, name) in [("f32", 2, "dot32"), ("f64", 3, "dot64")] {
        source.push_str(&format!(
            "func @{name}(ptr,ptr,i64) -> {ty} cc(c) {{
block0(v0:ptr,v1:ptr,v2:i64):
 v3 = fconst.{ty} 0.0
 v4 = iconst.i64 0
 jump block1(v4,v3)
block1(v5:i64,v6:{ty}):
 v7 = icmp.ult v5,v2
 brif v7,block2,block3
block2:
 v8 = iconst.i64 {shift}
 v9 = shl v5,v8
 v10 = ptradd v0,v9
 v11 = load.{ty} trap_ub v10
 v12 = ptradd v1,v9
 v13 = load.{ty} trap_ub v12
 v14 = fmul v11,v13
 v15 = fadd v6,v14
 v16 = iconst.i64 1
 v17 = iadd v5,v16
 jump block1(v17,v15)
block3:
 return v6
}}\n"
        ));
    }
    let mut m = keel_ir::parse(&source).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-dot"], keel_opt::VectorTarget::Aarch64Neon)
        .unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.functions.iter().all(|f| {
        f.layout_insts()
            .any(|(_, i)| f.inst_data(i).is_vector_instruction())
    }));
    m
}

#[test]
fn ordered_dot_products_compile_to_native_target_objects() {
    let m = ordered_dot_module();
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let text = keel_codegen::compile(&m, target).unwrap();
        let p = keel_codegen::compile_program(&m, target).unwrap();
        if target == Target::X86_64LinuxGnu {
            assert!(text.contains("mulpd"), "{text}");
            assert_eq!(
                keel_asm::x86::assemble_object(&text).unwrap(),
                keel_asm::x86::assemble_program_object(&p).unwrap()
            );
        } else {
            assert!(text.contains("fmul"), "{text}");
            assert_eq!(
                keel_asm::assemble_object(&text).unwrap(),
                keel_asm::assemble_program_object(&p, Default::default()).unwrap()
            );
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn ordered_dot_native_cancellation_and_tails_match_scalar_oracle() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("ordered-dot");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        r#"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
extern float dot32(const float *, const float *, uint64_t);
extern double dot64(const double *, const double *, uint64_t);
int main(void) {
 float a[19], b[19]; double x[19], y[19];
 for (int mode=0; mode<2; ++mode) {
 for (int i=0; i<19; ++i) {
  a[i]= i%4==0 ? 16777216.0f : i%4==2 ? -16777216.0f : 1.0f;
  x[i]= i%4==0 ? 9007199254740992.0 : i%4==2 ? -9007199254740992.0 : 1.0;
  b[i]=1.0f; y[i]=1.0;
  if (mode) {
   a[i]=i%4==0 ? -1.0f : i%4==1 ? 1.0f+0x1p-23f : 0.0f;
   x[i]=i%4==0 ? -1.0 : i%4==1 ? 1.0+0x1p-52 : 0.0;
   if (i%4==1) { b[i]=1.0f-0x1p-23f; y[i]=1.0-0x1p-52; }
  }
 }
 for (uint64_t n=0; n<=19; ++n) {
  float f=0.0f; double d=0.0;
  for (uint64_t i=0; i<n; ++i) { f=f+a[i]*b[i]; d=d+x[i]*y[i]; }
  float got_f=dot32(a,b,n); double got_d=dot64(x,y,n);
  if (memcmp(&f,&got_f,sizeof(f)) || memcmp(&d,&got_d,sizeof(d))) {
   fprintf(stderr,"ordered dot mismatch at length %llu\n",(unsigned long long)n);return 1;
  }
 }
 }
 return 0;
}
"#,
    )
    .unwrap();
    compile_driver(&dir);
    let m = ordered_dot_module();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let p = keel_codegen::compile_program_with_pipeline(&m, native_target(), &pipeline)
                .unwrap();
            fs::write(
                dir.join("test.o"),
                if cfg!(target_arch = "x86_64") {
                    keel_asm::x86::assemble_program_object(&p).unwrap()
                } else {
                    keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                },
            )
            .unwrap();
            let output = Command::new("clang")
                .arg(dir.join("driver.o"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(dir.join("test")).status().unwrap().success(),
                "{selection:?}/{allocation:?}"
            );
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_mixed_narrow_reduction_matches_exhaustive_oracle_and_guarded_tails() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reduce-mixed-narrow");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("fixtures/reduction_narrow.c"),
    )
    .unwrap();
    compile_driver(&dir);
    let mut m = keel_ir::parse(include_str!(
        "../../keel-opt/tests/loop_vectorize/mixed_narrow.keel"
    ))
    .unwrap();
    keel_opt::run_passes_for_target(
        &mut m,
        &["loop-reduce-multi"],
        keel_opt::VectorTarget::Aarch64Neon,
    )
    .unwrap();
    keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .layout_insts()
            .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
    );
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let p = keel_codegen::compile_program_with_pipeline(&m, native_target(), &pipeline)
                .unwrap();
            fs::write(
                dir.join("test.o"),
                keel_asm::assemble_program_object(&p, Default::default()).unwrap(),
            )
            .unwrap();
            let output = Command::new("clang")
                .arg(dir.join("driver.o"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(dir.join("test")).status().unwrap().success(),
                "{selection:?}/{allocation:?}"
            );
        }
    }
}
