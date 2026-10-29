#[path = "numeric/m5_compat.rs"]
mod m5_compat;
use keel_asm::{TargetFeatures, assemble, assemble_with_features};
mod common;
use common::compare_clang_with_features;
const M5_FLAGS: &[&str] = &["-mcpu=apple-m5"];
fn compare(source: &str) {
    compare_clang_with_features(source, TargetFeatures::apple_m5(), M5_FLAGS);
}

#[test]
fn integer_dot_matrix_rdm_bfloat_and_widening_half_match_clang() {
    let mut s = String::new();
    for q in 0..=1 {
        let ds = 2 << q;
        let nb = 8 << q;
        let nh = 4 << q;
        for op in ["sdot", "udot", "usdot", "bfdot"] {
            let src = if op == "bfdot" {
                format!("{nh}h")
            } else {
                format!("{nb}b")
            };
            s.push_str(&format!("{op} v31.{ds}s,v0.{src},v30.{src}\n"));
            for index in 0..4 {
                for rm in [0, 31] {
                    let element = if op == "bfdot" { "2h" } else { "4b" };
                    s.push_str(&format!(
                        "{op} v31.{ds}s,v0.{src},v{rm}.{element}[{index}]\n"
                    ));
                }
            }
        }
        for index in 0..4 {
            s.push_str(&format!("sudot v0.{ds}s,v31.{nb}b,v30.4b[{index}]\n"));
        }
        for e in ["h", "s"] {
            let size = if e == "h" { 1 } else { 2 };
            let lanes = (8 << q) >> size;
            for op in ["sqrdmlah", "sqrdmlsh"] {
                s.push_str(&format!(
                    "{op} v31.{lanes}{e},v0.{lanes}{e},v30.{lanes}{e}\n"
                ));
                for index in 0..16 >> size {
                    for rm in if e == "h" { vec![0, 15] } else { vec![0, 31] } {
                        s.push_str(&format!(
                            "{op} v31.{lanes}{e},v0.{lanes}{e},v{rm}.{e}[{index}]\n"
                        ));
                        s.push_str(&format!("{op} {e}31,{e}0,v{rm}.{e}[{index}]\n"));
                    }
                }
                s.push_str(&format!("{op} {e}31,{e}0,{e}30\n"));
            }
        }
        let src = 2 << q;
        for op in ["fmlal", "fmlsl", "fmlal2", "fmlsl2"] {
            s.push_str(&format!("{op} v31.{ds}s,v0.{src}h,v30.{src}h\n"));
            for index in 0..8 {
                for rm in [0, 15] {
                    s.push_str(&format!("{op} v31.{ds}s,v0.{src}h,v{rm}.h[{index}]\n"));
                }
            }
        }
    }
    for op in ["smmla", "ummla", "usmmla"] {
        s.push_str(&format!("{op} v31.4s,v0.16b,v30.16b\n"));
    }
    s.push_str(
        "bfmmla v31.4s,v0.8h,v30.8h\nbfcvt h31,s0\nbfcvtn v31.4h,v0.4s\nbfcvtn2 v31.8h,v0.4s\n",
    );
    for op in ["bfmlalb", "bfmlalt"] {
        s.push_str(&format!("{op} v31.4s,v0.8h,v30.8h\n"));
        for index in 0..8 {
            for rm in [0, 15] {
                s.push_str(&format!("{op} v31.4s,v0.8h,v{rm}.h[{index}]\n"));
            }
        }
    }
    compare(&s);
}

#[test]
fn complex_and_bounded_rounding_match_clang() {
    let mut s = String::from("fjcvtzs w0,d31\nfjcvtzs wzr,d0\n");
    for arr in ["4h", "8h", "2s", "4s", "2d"] {
        for op in ["fcmla", "fcadd"] {
            for rot in if op == "fcmla" {
                vec![0, 90, 180, 270]
            } else {
                vec![90, 270]
            } {
                s.push_str(&format!("{op} v31.{arr},v0.{arr},v30.{arr},#{rot}\n"));
            }
        }
        if arr == "2s" || arr == "2d" {
            continue;
        }
        let (e, lanes) = if arr.ends_with('h') {
            ("h", if arr == "4h" { 2 } else { 4 })
        } else {
            ("s", 2)
        };
        for index in 0..lanes {
            for rm in [0, 31] {
                for rot in [0, 90, 180, 270] {
                    s.push_str(&format!(
                        "fcmla v31.{arr},v0.{arr},v{rm}.{e}[{index}],#{rot}\n"
                    ));
                }
            }
        }
    }
    for op in ["frint32z", "frint32x", "frint64z", "frint64x"] {
        for arr in ["2s", "4s", "2d"] {
            s.push_str(&format!("{op} v31.{arr},v0.{arr}\n"));
        }
        for e in ["s", "d"] {
            s.push_str(&format!("{op} {e}31,{e}0\n"));
        }
    }
    compare(&s);
}

#[test]
fn individual_features_are_explicit_and_independent() {
    common::compare_clang("add v0.4s,v1.4s,v2.4s");
    let cases = [
        (
            TargetFeatures::default().with_fhm(),
            "fmlal v0.2s,v1.2h,v2.2h",
            "FEAT_FHM",
        ),
        (
            TargetFeatures::default().with_bf16(),
            "bfdot v0.4s,v1.8h,v2.8h",
            "FEAT_BF16",
        ),
        (
            TargetFeatures::default().with_dotprod(),
            "sdot v0.4s,v1.16b,v2.16b",
            "FEAT_DotProd",
        ),
        (
            TargetFeatures::default().with_i8mm(),
            "smmla v0.4s,v1.16b,v2.16b",
            "FEAT_I8MM",
        ),
        (
            TargetFeatures::default().with_rdm(),
            "sqrdmlah v0.4s,v1.4s,v2.4s",
            "FEAT_RDM",
        ),
        (
            TargetFeatures::default().with_fcma(),
            "fcmla v0.4s,v1.4s,v2.4s,#90",
            "FEAT_FCMA",
        ),
        (
            TargetFeatures::default().with_jscvt(),
            "fjcvtzs w0,d0",
            "FEAT_JSCVT",
        ),
        (
            TargetFeatures::default().with_frintts(),
            "frint32z v0.4s,v1.4s",
            "FEAT_FRINTTS",
        ),
    ];
    for (features, source, required) in cases {
        assert!(assemble_with_features(source, features).is_ok(), "{source}");
        assert!(assemble_with_features(source, TargetFeatures::apple_m5()).is_ok());
        for disabled in [
            TargetFeatures::default(),
            TargetFeatures::default().with_sme(),
            TargetFeatures::default().with_sve(),
        ] {
            assert!(
                assemble_with_features(source, disabled)
                    .unwrap_err()
                    .message
                    .contains(required),
                "{source}"
            );
        }
        for (other, other_source, _) in cases {
            if other != features {
                assert!(
                    assemble_with_features(other_source, features).is_err(),
                    "{other_source}"
                );
            }
        }
        assert!(assemble(source).is_err());
    }
    assert!(
        assemble_with_features(
            "fcmla v0.8h,v1.8h,v2.8h,#0",
            TargetFeatures::default().with_fcma()
        )
        .unwrap_err()
        .message
        .contains("FEAT_FP16")
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn advertised(features: &[&str]) -> bool {
    use std::process::Command;
    for feature in features {
        let name = format!("hw.optional.arm.{feature}");
        let out = Command::new("sysctl").args(["-n", &name]).output().unwrap();
        if !out.status.success() || out.stdout != b"1\n" {
            eprintln!("skipping native numeric test: {name} is not advertised");
            return false;
        }
    }
    true
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn execute_numeric(source: &str, driver: &str) {
    use common::{Temp, execute, run};
    use std::process::Command;
    compare(source);
    let temp = Temp::new();
    let object = temp.0.join("numeric.o");
    let c = temp.0.join("driver.c");
    let exe = temp.0.join("numeric");
    std::fs::write(
        &object,
        keel_asm::assemble_object_with_features(source, TargetFeatures::apple_m5()).unwrap(),
    )
    .unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang")
        .arg("-O2")
        .arg(&c)
        .arg(&object)
        .arg("-o")
        .arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn dot_matrix_and_rounding_doubling_execute() {
    if !advertised(&["FEAT_DotProd", "FEAT_I8MM", "FEAT_RDM"]) {
        return;
    }
    execute_numeric("
.text
.globl _dot, _matrix, _rounding
_dot:
    ldr q0,[x0]
    ldr q1,[x1]
    movi v2.4s,#0
    sdot v2.4s,v0.16b,v1.16b
    str q2,[x2]
    ret
_matrix:
    ldr q0,[x0]
    ldr q1,[x1]
    movi v2.4s,#0
    smmla v2.4s,v0.16b,v1.16b
    str q2,[x2]
    ret
_rounding:
    ldr q0,[x0]
    ldr q1,[x1]
    ldr q2,[x2]
    sqrdmlah v0.4s,v1.4s,v2.4s
    str q0,[x3]
    ret
","
#include <stdint.h>
#include <limits.h>
extern void dot(const int8_t*,const int8_t*,int32_t*),matrix(const int8_t*,const int8_t*,int32_t*);
extern void rounding(const int32_t*,const int32_t*,const int32_t*,int32_t*);
int main(void){
    int8_t a[16]={-128,127,-1,2,-3,4,5,-6,7,-8,9,10,-11,12,13,14},b[16];int32_t out[4];
    for(int i=0;i<16;++i)b[i]=i-8;dot(a,b,out);
    for(int i=0;i<4;++i){int v=0;for(int k=0;k<4;++k)v+=a[i*4+k]*b[i*4+k];if(out[i]!=v)return 1;}
    for(int i=0;i<16;++i)b[i]=1;matrix(a,b,out);
    for(int i=0;i<4;++i){int v=0;for(int k=0;k<8;++k)v+=a[(i/2)*8+k];if(out[i]!=v)return 2;}
    int32_t acc[4]={0,INT_MAX,INT_MIN,7},x[4]={0x40000000,INT_MAX,INT_MIN,3},y[4]={0x40000000,INT_MAX,INT_MIN,0x40000000};
    rounding(acc,x,y,out);
    int32_t expected[4]={0x20000000,INT_MAX,0,9};
    for(int i=0;i<4;++i)if(out[i]!=expected[i])return 3;
    return 0;
}");
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn bfloat_widening_complex_and_scalar_rounding_execute() {
    if !advertised(&[
        "FEAT_BF16",
        "FEAT_FHM",
        "FEAT_FCMA",
        "FEAT_JSCVT",
        "FEAT_FRINTTS",
    ]) {
        return;
    }
    execute_numeric("
.text
.globl _bfdot, _wide, _complex, _js, _bounded
_bfdot:
    ldr q0,[x0]
    ldr q1,[x1]
    movi v2.4s,#0
    bfdot v2.4s,v0.8h,v1.8h
    str q2,[x2]
    ret
_wide:
    ldr d0,[x0]
    ldr d1,[x1]
    movi v2.4s,#0
    fmlal v2.4s,v0.4h,v1.4h
    str q2,[x2]
    ret
_complex:
    ldr q0,[x0]
    ldr q1,[x1]
    movi v2.4s,#0
    fcmla v2.4s,v0.4s,v1.4s,#0
    fcmla v2.4s,v0.4s,v1.4s,#90
    str q2,[x2]
    ret
_js:
    fjcvtzs w0,d0
    ret
_bounded:
    frint32z d0,d0
    ret
","
#include <stdint.h>
#include <limits.h>
extern void bfdot(const uint16_t*,const uint16_t*,float*),wide(const uint16_t*,const uint16_t*,float*),complex(const float*,const float*,float*);
extern int32_t js(double);extern double bounded(double);
int main(void){
    uint16_t a[8]={0x3f80,0x4000,0x4040,0x4080,0xbf80,0xc000,0x3f00,0x4100},b[8];float out[4];
    for(int i=0;i<8;++i)b[i]=0x4000;bfdot(a,b,out);
    float expected[4]={6,14,-6,17};for(int i=0;i<4;++i)if(out[i]!=expected[i])return 1;
    uint16_t h[4]={0x3c00,0x4000,0xc200,0x3800},k[4]={0x4000,0x4200,0x4400,0x4800};wide(h,k,out);
    float w[4]={2,6,-12,4};for(int i=0;i<4;++i)if(out[i]!=w[i])return 2;
    float x[4]={2,3,-1,4},y[4]={5,7,2,-3};complex(x,y,out);
    for(int i=0;i<2;++i){if(out[2*i]!=x[2*i]*y[2*i]-x[2*i+1]*y[2*i+1]||out[2*i+1]!=x[2*i]*y[2*i+1]+x[2*i+1]*y[2*i])return 3;}
    if(js(4294967297.0)!=1||js(-2.75)!=-2||js(0.0/0.0)!=0)return 4;
    if(bounded(12.75)!=12||bounded(-2.75)!=-2)return 5;
    return 0;
}");
}

#[test]
fn every_m5_numeric_record_matches_clang() {
    let source = include_str!("numeric-cases.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|s| s.split_once('\t').unwrap().1)
        .collect::<Vec<_>>()
        .join("\n");
    compare(&source);
}

#[test]
fn half_scalar_vector_and_conversion_boundaries_match_clang() {
    let mut s = String::new();
    for d in ["h31", "v31.4h", "v31.8h"] {
        let n = if d == "h31" {
            "h0"
        } else if d.ends_with("4h") {
            "v0.4h"
        } else {
            "v0.8h"
        };
        let m = if d == "h31" {
            "h30"
        } else if d.ends_with("4h") {
            "v30.4h"
        } else {
            "v30.8h"
        };
        for op in [
            "fabs", "fneg", "fsqrt", "frintn", "frintp", "frintm", "frintz", "frinta", "frintx",
            "frinti", "frecpe", "frsqrte", "scvtf", "ucvtf", "fcvtns", "fcvtnu", "fcvtas",
            "fcvtau", "fcvtps", "fcvtpu", "fcvtms", "fcvtmu", "fcvtzs", "fcvtzu",
        ] {
            s.push_str(&format!("{op} {d},{n}\n"));
        }
        for op in [
            "fadd", "fsub", "fmul", "fdiv", "fmin", "fmax", "fminnm", "fmaxnm", "fmulx", "fcmeq",
            "fcmgt", "fcmge", "fcmlt", "fcmle", "facgt", "facge", "fabd", "frecps", "frsqrts",
        ] {
            if d == "h31" && matches!(op, "fcmlt" | "fcmle") {
                continue;
            }
            s.push_str(&format!("{op} {d},{n},{m}\n"));
        }
        if d != "h31" {
            for op in [
                "fmla", "fmls", "faddp", "fminp", "fmaxp", "fminnmp", "fmaxnmp",
            ] {
                s.push_str(&format!("{op} {d},{n},{m}\n"));
            }
        }
        for op in ["fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle"] {
            s.push_str(&format!("{op} {d},{n},#0.0\n"));
        }
        for op in ["fmla", "fmls", "fmul", "fmulx"] {
            for index in 0..8 {
                for rm in [0, 15] {
                    s.push_str(&format!("{op} {d},{n},v{rm}.h[{index}]\n"));
                }
            }
        }
        for op in ["scvtf", "ucvtf", "fcvtzs", "fcvtzu"] {
            for scale in [1, 16] {
                s.push_str(&format!("{op} {d},{n},#{scale}\n"));
            }
        }
        for sign in [-1.0, 1.0] {
            for exponent in -3..=4 {
                for fraction in 0..16 {
                    let value = sign * (1.0 + f64::from(fraction) / 16.0) * 2f64.powi(exponent);
                    s.push_str(&format!("fmov {d},#{value}\n"));
                }
            }
        }
    }
    for p in ["w", "x"] {
        let bits = if p == "w" { 32 } else { 64 };
        for op in ["scvtf", "ucvtf"] {
            s.push_str(&format!("{op} h31,{p}zr\n"));
            for fbits in [1, bits] {
                s.push_str(&format!("{op} h31,{p}0,#{fbits}\n"));
            }
        }
        for op in [
            "fcvtns", "fcvtnu", "fcvtas", "fcvtau", "fcvtps", "fcvtpu", "fcvtms", "fcvtmu",
            "fcvtzs", "fcvtzu",
        ] {
            s.push_str(&format!("{op} {p}zr,h31\n"));
        }
        for op in ["fcvtzs", "fcvtzu"] {
            for fbits in [1, bits] {
                s.push_str(&format!("{op} {p}30,h31,#{fbits}\n"));
            }
        }
        s.push_str(&format!("fmov h31,{p}zr\nfmov {p}zr,h31\n"));
    }
    for op in ["fcmp", "fcmpe"] {
        s.push_str(&format!("{op} h31,h0\n{op} h31,#0.0\n"));
    }
    for cc in [
        "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "al",
        "nv",
    ] {
        for op in ["fccmp", "fccmpe"] {
            for nzcv in [0, 15] {
                s.push_str(&format!("{op} h31,h0,#{nzcv},{cc}\n"));
            }
        }
        s.push_str(&format!("fcsel h31,h0,h30,{cc}\n"));
    }
    for op in ["fmadd", "fmsub", "fnmadd", "fnmsub"] {
        s.push_str(&format!("{op} h31,h0,h30,h1\n"));
    }
    for op in ["faddp", "fmaxp", "fminp", "fmaxnmp", "fminnmp"] {
        s.push_str(&format!("{op} h31,v0.2h\n"));
    }
    for op in ["fmaxv", "fminv", "fmaxnmv", "fminnmv"] {
        for arr in ["4h", "8h"] {
            s.push_str(&format!("{op} h31,v0.{arr}\n"));
        }
    }
    s.push_str("frecpx h31,h0\nfnmul h31,h0,h30\nfmov h31,h0\nFADD V31.8H,V0.8H,V1.8H\n");
    compare(&s);
    common::compare_clang("fcvt h31,s0\nfcvt h31,d0\nfcvt s31,h0\nfcvt d31,h0\n");
}

#[test]
fn fp16_opt_in_and_m5_profile_preserve_baseline() {
    let features = TargetFeatures::default().with_fp16();
    for source in [
        "fadd h0,h1,h2",
        "fadd v0.8h,v1.8h,v2.8h",
        "fmul v0.8h,v1.8h,v2.h[7]",
        "scvtf h0,w0,#32",
        "fcvtzs w0,h0",
        "scvtf v0.8h,v1.8h,#16",
    ] {
        assert!(assemble_with_features(source, features).is_ok(), "{source}");
        assert!(
            assemble(source).unwrap_err().message.contains("FEAT_FP16"),
            "{source}"
        );
        assert!(assemble_with_features(source, TargetFeatures::default().with_fhm()).is_err());
    }
    for source in ["add v0.4s,v1.4s,v2.4s", "scvtf d0,x0", "fcsel d0,d1,d2,hs"] {
        assert_eq!(
            assemble(source).unwrap(),
            assemble_with_features(source, TargetFeatures::apple_m5()).unwrap()
        );
    }
    assert!(assemble_with_features("ptrue p0.s", TargetFeatures::apple_m5()).is_ok());
    // The profile does not imply ordinary SVE: only SME satisfies the shared
    // instruction's OR condition; all numeric feature builders leave SME off.
    assert!(assemble_with_features("smstart", features).is_err());
}

#[test]
fn invalid_numeric_shapes_scales_and_rotations_are_rejected() {
    for source in [
        "sdot v0.4s,v1.8b,v2.8b",
        "sdot v0.8h,v1.16b,v2.16b",
        "sdot v0.4s,v1.16b,v2.4b[4]",
        "sudot v0.4s,v1.16b,v2.16b",
        "smmla v0.2s,v1.8b,v2.8b",
        "bfdot v0.4s,v1.4h,v2.4h",
        "bfmlalb v0.2s,v1.4h,v2.4h",
        "bfmlalt v0.4s,v1.8h,v16.h[0]",
        "bfcvtn2 v0.4h,v1.4s",
        "sqrdmlah v0.16b,v1.16b,v2.16b",
        "sqrdmlsh v0.8h,v1.8h,v16.h[0]",
        "fcadd v0.4s,v1.4s,v2.4s,#0",
        "fcmla v0.4s,v1.4s,v2.4s,#360",
        "fcmla v0.4h,v1.4h,v2.h[2],#0",
        "fcmla v0.2s,v1.2s,v2.s[0],#0",
        "fcmla v0.2d,v1.2d,v2.d[0],#0",
        "fmlal v0.4s,v1.8h,v2.8h",
        "fmlal v0.2s,v1.4h,v2.4h",
        "fmlal2 v0.4s,v1.4h,v16.h[0]",
        "fadd h0,s1,h2",
        "fadd v0.4h,v1.8h,v2.4h",
        "fmul h0,h1,v16.h[0]",
        "fmul v0.8h,v1.8h,v2.h[8]",
        "scvtf h0,w0,#0",
        "scvtf h0,w0,#33",
        "fcvtzu x0,h0,#65",
        "scvtf v0.8h,v1.8h,#17",
        "fcmgt h0,h1,#1.0",
        "fcmlt h0,h1,h2",
        "fccmp h0,h1,#16,eq",
        "fmov h0,#0.0",
        "fcsel h0,h1,s2,eq",
        "faddp h0,v1.4h",
        "fmaxv h0,v1.4s",
        "fjcvtzs x0,d1",
        "frint32z h0,h1",
    ] {
        let error = assemble_with_features(&format!("nop\n{source}"), TargetFeatures::apple_m5())
            .expect_err(source);
        assert_eq!(error.line, 2, "{source}: {error}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn half_arithmetic_conversions_and_bit_transfers_execute() {
    if !advertised(&["FEAT_FP16"]) {
        return;
    }
    execute_numeric(
        "
.text
.globl _halfmath, _halfvector, _halfscale, _halfbits
_halfmath:
    fcvt h0,s0
    fcvt h1,s1
    fadd h2,h0,h1
    fmul h2,h2,h1
    fcvt s0,h2
    ret
_halfvector:
    ldr q0,[x0]
    fmul v1.8h,v0.8h,v0.h[1]
    fcvtzs v2.8h,v1.8h
    str q2,[x1]
    ret
_halfscale:
    scvtf h0,w0,#1
    fcvtzs w0,h0,#1
    ret
_halfbits:
    fmov h0,w0
    fmov w0,h0
    ret
",
        "
#include <stdint.h>
extern float halfmath(float,float);extern void halfvector(const uint16_t*,int16_t*);
extern int32_t halfscale(int32_t);extern uint32_t halfbits(uint32_t);
int main(void){
    if(halfmath(1.5f,2.0f)!=7.0f||halfmath(-4.0f,0.5f)!=-1.75f)return 1;
    uint16_t a[8]={0x3c00,0x4000,0x4200,0x4400,0xbc00,0xc000,0x3800,0};int16_t out[8];
    halfvector(a,out);int16_t expected[8]={2,4,6,8,-2,-4,1,0};
    for(int i=0;i<8;++i)if(out[i]!=expected[i])return 2;
    for(int i=-42;i<=42;++i)if(halfscale(i)!=i)return 3;
    if(halfbits(0xdead3c00)!=0x3c00)return 4;
    return 0;
}",
    );
}
