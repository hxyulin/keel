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
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "--verify-each",
            "-p",
            pipeline,
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
