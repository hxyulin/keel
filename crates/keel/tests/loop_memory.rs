//! Prefix control flow and scalar memory recurrences from the C application profiles.
use std::{fs, path::Path, process::Command};

fn run(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn source(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for (name, text) in [
        ("kernels.c", include_str!("loop_memory/kernels.c")),
        ("hand.c", include_str!("loop_memory/hand.c")),
        ("oracle.c", include_str!("loop_memory/oracle.c")),
    ] {
        fs::write(dir.join(name), text).unwrap();
    }
}
#[test]
fn native_prefix_and_aliasing_hash_oracles() {
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        eprintln!("SKIP: native C kernel oracle requires AArch64 macOS or x86-64 Linux");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-memory-native");
    source(&dir);
    for disabled in [false, true] {
        let mut cc = Command::new(env!("CARGO_BIN_EXE_keel"));
        cc.current_dir(&dir).args([
            "cc",
            "-O",
            "--link-intent",
            "executable",
            "--verify-each",
            "--emit-object",
        ]);
        if disabled {
            cc.args(["--opt-disable", "loop-rotate,loop-unroll-exact,load-reuse"]);
        }
        cc.args(["kernels.c", "-o", "kernels.o"]);
        run(&mut cc);
        run(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "-fno-strict-aliasing",
            "oracle.c",
            "hand.c",
            "kernels.o",
            "-o",
            "oracle",
        ]));
        run(&mut Command::new(dir.join("oracle")));
    }
}
#[test]
fn prefix_and_memory_recurrences_verify_for_both_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-memory-targets");
    source(&dir);
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "--target",
                target,
                "-O",
                "--verify-each",
                "--emit-object",
                "kernels.c",
                "-o",
                "kernels.o",
            ]));
    }
}

#[test]
fn ordered_float_batches_match_clang_and_emit_for_both_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ordered-float-batches");
    fs::create_dir_all(&dir).unwrap();
    let kernels = "double fabs(double); float fabsf(float); double sum_d(const double *p,unsigned long n,double s) { for(unsigned long i=0;i<n;++i) s+=p[i]; return s; } double abs_d(const double *p,unsigned long n,double s) { for(unsigned long i=0;i<n;++i) s+=fabs(p[i]); return s; } float sum_f(const float *p,unsigned long n,float s) { for(unsigned long i=0;i<n;++i) s+=p[i]; return s; } float abs_f(const float *p,unsigned long n,float s) { for(unsigned long i=0;i<n;++i) s+=fabsf(p[i]); return s; }";
    fs::write(dir.join("kernels.c"), kernels).unwrap();
    let pipeline =
        "simplify-cfg,canonicalize,stack-promote,instcombine,dce,loop-batch,simplify-cfg,dce";
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "--target",
                target,
                "--verify-each",
                "-p",
                pipeline,
                "--emit-ir",
                "kernels.c",
                "-o",
                "kernels.ir",
            ]));
        let ir = fs::read_to_string(dir.join("kernels.ir")).unwrap();
        assert!(
            ir.contains("load.f64x2") && ir.contains("load.f32x4"),
            "{target}: {ir}"
        );
        assert!(!ir.contains("vfadd"), "scalar addition order: {ir}");
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "compile",
                "--target",
                target,
                "--verify-each",
                "--emit-object",
                "kernels.ir",
                "-o",
                "kernels.o",
            ]));
    }
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        return;
    }
    fs::write(dir.join("oracle.c"), r#"
#include <math.h>
#include <stdint.h>
#include <string.h>
#define DECL(T,N) extern T N(const T *, unsigned long, T); extern T ref_##N(const T *, unsigned long, T);
DECL(double,sum_d) DECL(double,abs_d) DECL(float,sum_f) DECL(float,abs_f)
#define CHECK(T,N,A) { T x=N(A,n,seed), y=ref_##N(A,n,seed); if (!(isnan(x)&&isnan(y)) && memcmp(&x,&y,sizeof(x))) return __LINE__; }
int main(void) {
 double d[35]; float f[35];
 for(int mode=0;mode<4;++mode) {
  for(int i=0;i<35;++i) { d[i]=(i%3==0 ? 9007199254740992.0 : i%3==1 ? 1.0 : -9007199254740992.0); f[i]=(i%3==0 ? 16777216.0f : i%3==1 ? 1.0f : -16777216.0f); }
  if(mode) { d[8]=mode==1 ? INFINITY : mode==2 ? NAN : -0.0; f[8]=(float)d[8]; }
  unsigned long sizes[]={0,1,7,8,9,15,16,17,31,32,33};
  for(int k=0;k<11;++k) for(int offset=0;offset<2;++offset) for(int s=0;s<3;++s) {
    unsigned long n=sizes[k];
    double seed=s==0 ? -0.0 : s==1 ? 0.0 : 3.5;
    CHECK(double,sum_d,d+offset) CHECK(double,abs_d,d+offset)
    CHECK(float,sum_f,f+offset) CHECK(float,abs_f,f+offset)
  }
 }
 return 0;
}
"#).unwrap();
    let mut cc = Command::new("clang");
    cc.current_dir(&dir)
        .args(["-O3", "-c", "kernels.c", "-o", "reference.o"]);
    for name in ["sum_d", "abs_d", "sum_f", "abs_f"] {
        cc.arg(format!("-D{name}=ref_{name}"));
    }
    run(&mut cc);
    for pass in ["loop-batch", "loop-batch-scalar"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "--verify-each",
                "-p",
                &pipeline.replace("loop-batch", pass),
                "--emit-object",
                "kernels.c",
                "-o",
                "kernels.o",
            ]));
        run(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "oracle.c",
            "reference.o",
            "kernels.o",
            "-o",
            "oracle",
        ]));
        run(&mut Command::new(dir.join("oracle")));
    }
}

#[test]
fn default_batch_selection_preserves_short_and_unknown_loops() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("automatic-float-batches");
    fs::create_dir_all(&dir).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for (ty, name) in [("float", "fabsf"), ("double", "fabs")] {
            for bound in ["1", "7", "31", "32", "33", "n", "guarded"] {
                let (guard, loop_bound) = if bound == "guarded" {
                    ("if(n<32)return 0;", "n")
                } else {
                    ("", bound)
                };
                fs::write(dir.join("kernel.c"),format!("{ty} {name}({ty}); {ty} f(const {ty} *p,unsigned long n) {{ {guard} {ty} s=0; for(unsigned long i=0;i<{loop_bound};++i) s+={name}(p[i]); return s; }}")).unwrap();
                let emit = |flags: &[&str]| {
                    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
                        .current_dir(&dir)
                        .args([
                            "cc",
                            "--target",
                            target,
                            "--emit-ir",
                            "--verify-each",
                            "kernel.c",
                        ])
                        .args(flags)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    output.stdout
                };
                let normal = emit(&["-O3"]);
                let disabled = emit(&["-O3", "--opt-disable", "auto-loop-batch"]);
                assert_eq!(
                    normal != disabled,
                    matches!(bound, "32" | "33" | "guarded"),
                    "{target} {ty} {bound}"
                );
                if matches!(bound, "32" | "guarded") {
                    assert_eq!(
                        emit(&["-O3", "-fno-vectorize"]),
                        emit(&["-O3", "-fno-vectorize", "--opt-disable", "auto-loop-batch"])
                    );
                    for level in ["-O0", "-O1", "-O2", "-Os"] {
                        assert_eq!(
                            emit(&[level]),
                            emit(&[level, "--opt-disable", "auto-loop-batch"])
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn automatic_literal_batches_match_the_native_strict_driver() {
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        eprintln!("SKIP: automatic batch oracle requires AArch64 macOS or x86-64 Linux");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("automatic-batch-native");
    fs::create_dir_all(&dir).unwrap();
    for guarded in [false, true] {
        for n in [31, 32, 33, 64, 257] {
            let n = n.to_string();
            // Fixed-bound kernels must use the same count in the guard-page oracle.
            let mut driver = include_str!("../../../suite/c/fastmath/ordered_driver.c")
                .replace("{0,1,7,8,9,15,16,17,31,32,33}", &format!("{{{n}}}"));
            if guarded {
                driver = driver
                    .replace(
                        "volatile double s = 0;",
                        "if(n<32)return 0; volatile double s = 0;",
                    )
                    .replace(
                        "volatile float s = 0;",
                        "if(n<32)return 0; volatile float s = 0;",
                    );
            }
            fs::write(dir.join("driver.c"), driver).unwrap();
            run(Command::new("clang").current_dir(&dir).args([
                "-O2",
                "-fno-fast-math",
                "-ffp-contract=off",
                "-c",
                "driver.c",
                "-o",
                "driver.o",
            ]));
            let source = include_str!("../../../suite/c/fastmath/ordered.c");
            let kernels = if guarded {
                source.replace("  for (", "  if(n<32)return s;\n  for (")
            } else {
                source.replace("i < n", &format!("i < {n}"))
            };
            fs::write(dir.join("kernels.c"), kernels).unwrap();
            for disabled in [false, true] {
                let mut cc = Command::new(env!("CARGO_BIN_EXE_keel"));
                cc.current_dir(&dir).args([
                    "cc",
                    "-O3",
                    "--verify-each",
                    "--emit-object",
                    "kernels.c",
                    "-o",
                    "kernels.o",
                ]);
                if disabled {
                    cc.args(["--opt-disable", "auto-loop-batch"]);
                }
                run(&mut cc);
                run(Command::new("clang").current_dir(&dir).args([
                    "driver.o",
                    "kernels.o",
                    "-o",
                    "oracle",
                ]));
                run(Command::new(dir.join("oracle")).arg("check"));
                for kind in ["0", "1", "2", "3"] {
                    run(Command::new(dir.join("oracle")).args([kind, &n, "1"]));
                }
            }
        }
    }
}

#[test]
fn column_tiles_preserve_native_order_footprints_and_aliasing() {
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        eprintln!("SKIP: column-reduction oracle requires AArch64 macOS or x86-64 Linux");
        return;
    }
    let pipeline = format!(
        "simplify-cfg,canonicalize,stack-promote,instcombine,dce,loop-column-reduce,{}",
        keel_opt::DEFAULT_PIPELINE.join(",")
    );
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("column-tiles");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("columns.c"),
        include_str!("../../../suite/c/fastmath/columns.c"),
    )
    .unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("../../../suite/c/fastmath/columns_driver.c"),
    )
    .unwrap();
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "-O3",
            "-p",
            &pipeline,
            "--verify-each",
            "--emit-object",
            "--opt-disable",
            "loop-column-reduce",
            "-Dcol64=baseline64",
            "-Dcol32=baseline32",
            "columns.c",
            "-o",
            "baseline.o",
        ]));
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "-O3",
                "-p",
                &pipeline,
                "--verify-each",
                "--target",
                target,
                "--emit-object",
                "columns.c",
                "-o",
                "cross.o",
            ]));
    }
    let out = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args(["cc", "-O3", "-p", &pipeline, "--emit-ir", "columns.c"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .matches("func @__keel_columns_")
            .count(),
        2,
        "transform did not reach both C kernels"
    );
    let disabled = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "-O3",
            "-p",
            &pipeline,
            "-fno-vectorize",
            "--emit-ir",
            "columns.c",
        ])
        .output()
        .unwrap();
    assert!(disabled.status.success());
    assert!(!String::from_utf8_lossy(&disabled.stdout).contains("__keel_columns_"));
    let normal = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args(["cc", "-O3", "--emit-ir", "columns.c"])
        .output()
        .unwrap();
    assert!(normal.status.success());
    assert!(
        !String::from_utf8_lossy(&normal.stdout).contains("__keel_columns_"),
        "column tiling remains opt-in"
    );
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "-O3",
            "-p",
            &pipeline,
            "--verify-each",
            "--emit-object",
            "columns.c",
            "-o",
            "fast.o",
        ]));
    let text = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args(["cc", "-O3", "-p", &pipeline, "columns.c"])
        .output()
        .unwrap();
    assert!(text.status.success());
    let assembly = std::str::from_utf8(&text.stdout).unwrap();
    let parsed = if cfg!(target_arch = "x86_64") {
        keel_asm::x86::assemble_object(assembly).unwrap()
    } else {
        keel_asm::assemble_object(assembly).unwrap()
    };
    assert_eq!(fs::read(dir.join("fast.o")).unwrap(), parsed);
    run(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-fast-math",
        "-ffp-contract=off",
        "driver.c",
        "baseline.o",
        "fast.o",
        "-o",
        "oracle",
    ]));
    run(&mut Command::new(dir.join("oracle")));
    // Exercise the helper's four vector recurrences through stack-only allocation.
    let module = keel_ir::parse(std::str::from_utf8(&out.stdout).unwrap()).unwrap();
    let pools = keel_codegen::Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &keel_codegen::SelectionTier::Full,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let target = if cfg!(target_arch = "x86_64") {
        keel_codegen::Target::X86_64LinuxGnu
    } else {
        keel_codegen::Target::Aarch64AppleDarwin
    };
    let request = keel_codegen::Request::new(target, pipeline).pools(&pools);
    let program = request.program(&module).unwrap();
    let object = if cfg!(target_arch = "x86_64") {
        keel_asm::x86::assemble_program_object(&program).unwrap()
    } else {
        keel_asm::assemble_program_object(&program, Default::default()).unwrap()
    };
    fs::write(dir.join("spilled.o"), object).unwrap();
    run(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-fast-math",
        "-ffp-contract=off",
        "driver.c",
        "baseline.o",
        "spilled.o",
        "-o",
        "spilled-oracle",
    ]));
    run(&mut Command::new(dir.join("spilled-oracle")));
}
