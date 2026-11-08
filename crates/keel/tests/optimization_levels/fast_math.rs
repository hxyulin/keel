use super::{invoke, successful};
use std::{fs, path::Path};

#[test]
fn fast_math_is_opt_in_roundtrips_and_last_flag_wins() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fast-math-mode");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), "float cancel(float x) { return x-x; }\nfloat madd(float x,float y,float z) { return x*y+z; }\nfloat dot(const float *a,const float *b,unsigned long long n) { float s=0; for(unsigned long long i=0;i<n;++i) s+=a[i]*b[i]; return s; }").unwrap();
    for target in ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"] {
        let ir = |flags: &[&str]| {
            let mut args = vec!["cc", "--target", target, "-O3", "--emit-ir", "in.c"];
            args.extend_from_slice(flags);
            String::from_utf8(successful(&dir, &args)).unwrap()
        };
        let strict = ir(&[]);
        assert!(!strict.contains("fast_math"), "{strict}");
        let fast = ir(&["-ffast-math"]);
        assert!(fast.contains("fast_math"), "{fast}");
        assert!(fast.contains("vfadd"), "{fast}");
        assert!(
            fast.contains(" = fma ") == target.starts_with("aarch64"),
            "{fast}"
        );
        assert_eq!(strict, ir(&["-ffast-math", "-fno-fast-math"]));
        assert_eq!(fast, ir(&["-fno-fast-math", "-ffast-math"]));
        fs::write(dir.join("fast.keel"), &fast).unwrap();
        // Existing IR permissions survive an empty pipeline and compiler invocation.
        let roundtrip =
            String::from_utf8(successful(&dir, &["opt", "-p", "dce", "fast.keel"])).unwrap();
        assert!(roundtrip.contains("fast_math"));
        successful(&dir, &["compile", "--target", target, "fast.keel"]);
    }
    for (flags, expected) in [
        (vec![], false),
        (vec!["-ffast-math"], true),
        (vec!["-ffast-math", "-fno-fast-math"], false),
    ] {
        let mut args = vec!["cc", "-E", "-dM", "in.c"];
        args.extend(flags);
        let macros = String::from_utf8(successful(&dir, &args)).unwrap();
        assert_eq!(macros.contains("#define __FAST_MATH__ 1"), expected);
    }
    assert!(
        !invoke(
            &dir,
            &[
                "cc",
                "--target",
                "wasm32-unknown-unknown",
                "-ffast-math",
                "in.c"
            ]
        )
        .status
        .success()
    );
}

#[test]
fn fast_sqrt_maps_reach_both_native_backends() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fast-sqrt-maps");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), "extern float sqrtf(float); extern double sqrt(double);\nvoid normf(const float *a,const float *b,float *d,unsigned long long n) { for(unsigned long long i=0;i<n;++i) d[i]=a[i]/sqrtf(a[i]*a[i]+b[i]*b[i]); }\nvoid normd(const double *a,const double *b,double *d,unsigned long long n) { for(unsigned long long i=0;i<n;++i) d[i]=a[i]/sqrt(a[i]*a[i]+b[i]*b[i]); }").unwrap();
    for target in ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"] {
        let ir = String::from_utf8(successful(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O3",
                "-ffast-math",
                "--emit-ir",
                "in.c",
            ],
        ))
        .unwrap();
        assert_eq!(ir.matches(" = vfsqrt ").count(), 2, "{ir}");
        let disabled = String::from_utf8(successful(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O3",
                "-ffast-math",
                "-fno-builtin-sqrt",
                "--emit-ir",
                "in.c",
            ],
        ))
        .unwrap();
        assert_eq!(disabled.matches(" = vfsqrt ").count(), 1, "{disabled}");
        assert!(disabled.contains("call @sqrt("), "{disabled}");
        let strict = String::from_utf8(successful(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O3",
                "-ffast-math",
                "-fno-fast-math",
                "--emit-ir",
                "in.c",
            ],
        ))
        .unwrap();
        assert!(!strict.contains("vfsqrt"), "{strict}");
        assert!(strict.contains("call @sqrt("), "{strict}");
        fs::write(dir.join("in.keel"), &ir).unwrap();
        let assembly = String::from_utf8(successful(
            &dir,
            &["compile", "--target", target, "in.keel"],
        ))
        .unwrap();
        for shape in if target.starts_with("aarch64") {
            [".4s", ".2d"]
        } else {
            ["sqrtps", "sqrtpd"]
        } {
            assert!(
                assembly
                    .lines()
                    .any(|line| line.contains(shape) && line.contains("sqrt")),
                "{assembly}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn reciprocal_root_handles_positive_f32_range_and_scalar_fallbacks() {
    use std::process::Command;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fast-rsqrt-range");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernel.c"), "extern float sqrtf(float); void invroot(const float *x,float *y,unsigned long long n) { for(unsigned long long i=0;i<n;++i)y[i]=1.0f/sqrtf(x[i]); }").unwrap();
    fs::write(dir.join("driver.c"), r#"
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
void invroot(const float *,float *,unsigned long long);
int main(void) {
    const uint32_t edges[]={1,2,0x0000ffff,0x007fffff,0x00800000,0x00800001,0x3f7fffff,0x3f800000,0x7f000000,0x7f7fffff};
    float x[4100],out[4100]; uint32_t state=17;
    for(unsigned i=0;i<4100;++i) {
        state=state*1664525u+1013904223u;
        uint32_t bits=i<10?edges[i]:(state%0x7f7fffffu)+1;
        memcpy(&x[i],&bits,4);
        out[i]=NAN;
    }
    invroot(x,out,4099);
    for(unsigned i=0;i<4099;++i) {
        long double expected=1/sqrtl((long double)x[i]);
        if(!isfinite(out[i]) || fabsl(out[i]-expected)>2e-6L*expected) {
            fprintf(stderr,"lane %u input=%a got=%a expected=%La\n",i,x[i],out[i],expected);
            return 1;
        }
    }
    if(!isnan(out[4099])) return 2;
    const unsigned sizes[]={0,1,3,4,7,8,15,16,17,33,65};
    for(unsigned k=0;k<sizeof sizes/sizeof sizes[0];++k) for(unsigned overlap=0;overlap<2;++overlap) {
        float actual[67],expected[67];
        for(unsigned i=0;i<67;++i) actual[i]=expected[i]=x[i];
        unsigned n=sizes[k];
        for(unsigned i=0;i<n;++i) expected[i+overlap]=(float)(1/sqrtl((long double)expected[i]));
        invroot(actual,actual+overlap,n);
        for(unsigned i=0;i<67;++i) {
            if(!isfinite(actual[i]) || fabsl(actual[i]-expected[i])>2e-6L*fabsl(expected[i])) return 3;
        }
    }
    return 0;
}
"#).unwrap();
    successful(
        &dir,
        &[
            "cc",
            "-O3",
            "-ffast-math",
            "--emit-object",
            "kernel.c",
            "-o",
            "kernel.o",
        ],
    );
    let out = Command::new("clang")
        .current_dir(&dir)
        .args([
            "-O2",
            "-fno-fast-math",
            "-ffp-contract=off",
            "driver.c",
            "kernel.o",
            "-lm",
            "-o",
            "test",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = Command::new(dir.join("test")).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reciprocal_root_estimates_compile_for_both_native_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fast-rsqrt-targets");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), "extern float sqrtf(float); void invroot(const float *x,float *y,unsigned long long n) { for(unsigned long long i=0;i<n;++i)y[i]=1.0f/sqrtf(x[i]); }").unwrap();
    for (target, estimate) in [
        ("aarch64-unknown-linux-gnu", "frsqrte"),
        ("x86_64-unknown-linux-gnu", "rsqrtps"),
    ] {
        let ir = successful(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O3",
                "-ffast-math",
                "--emit-ir",
                "in.c",
            ],
        );
        fs::write(dir.join("in.keel"), ir).unwrap();
        let assembly = String::from_utf8(successful(
            &dir,
            &["compile", "--target", target, "in.keel"],
        ))
        .unwrap();
        assert!(assembly.contains(estimate), "{assembly}");
        if target.starts_with("x86_64") {
            assert!(assembly.contains("pcmpgtd"), "{assembly}");
            assert!(
                !assembly.contains("comiss"),
                "subnormal guard was scalarized: {assembly}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
