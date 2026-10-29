//! Apple suffixes normalize into existing, validated SIMD encoders.
#![allow(dead_code)]
use crate::common;
use keel_asm::{TargetFeatures, assemble, assemble_with_features};
use std::fmt::Write;

#[test]
fn apple_count_and_fp_vector_and_lane_forms_match_clang() {
    let mut source = String::new();
    for shape in ["8b", "16b"] {
        for (d, n) in [(0, 0), (31, 17)] {
            writeln!(&mut source, "cnt.{shape} v{d},v{n}").unwrap();
        }
    }
    for op in ["fmul", "fmla", "fmls", "fmulx"] {
        for shape in ["2s", "4s", "2d"] {
            writeln!(&mut source, "{op}.{shape} v31,v17,v0").unwrap();
            for multiplier in [0, 15, 16, 31] {
                for lane in [0, if shape == "2d" { 1 } else { 3 }] {
                    writeln!(&mut source, "{op}.{shape} v31,v17,v{multiplier}[{lane}]").unwrap();
                }
            }
        }
    }
    common::compare_clang(&source);
    assert_eq!(
        assemble("fmul.4s v2,v2,v0[0]").unwrap().text,
        assemble("fmul v2.4s,v2.4s,v0.s[0]").unwrap().text
    );
}

#[test]
fn apple_half_indexed_forms_retain_existing_fp16_feature_gate() {
    let mut source = String::new();
    for op in ["fmul", "fmla", "fmls", "fmulx"] {
        for shape in ["4h", "8h"] {
            for lane in [0, 7] {
                writeln!(&mut source, "{op}.{shape} v31,v17,v15[{lane}]").unwrap();
            }
        }
    }
    for line in source.lines() {
        let error = assemble(line).unwrap_err();
        assert!(
            error.message.contains("fp16") || error.message.contains("FP16"),
            "{error}"
        );
    }
    common::compare_clang_with_features(
        &source,
        TargetFeatures::default().with_fp16(),
        &["-march=armv8.2-a+fp16"],
    );
}

#[test]
fn apple_suffixes_reject_invalid_lane_positions_shapes_and_registers() {
    for source in [
        "cnt.4h v0,v1",
        "cnt.8b v0,v1[0]",
        "fmla.4s v0[0],v1,v2",
        "fmul.4s v0,v1[0],v2",
        "fmla.4s v0,v1,v2[4]",
        "fmla.2d v0,v1,v2[2]",
        "fmul.1d v0,v1,v2[0]",
        "fmul.4s v0,v1,v32[0]",
        "fmul.4s v0,v1,v2.s[0]",
        "fmul.4s v0,v1,v2[-1]",
        "fmul.4s v0,v1,v2[0",
        "fmul.4s v0,v1,v2[0],v3",
        "fadd.4s v0,v1,v2[0]",
        "invented.4s v0,v1,v2[0]",
        "fmla.8h v0,v1,v16[0]",
        "fmla.8h v0,v1,v15[8]",
    ] {
        let error = assemble_with_features(source, TargetFeatures::apple_m5()).unwrap_err();
        assert_eq!(error.line, 1, "{source}");
        assert!(!error.message.is_empty(), "{source}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn apple_popcount_and_indexed_fp_execute_like_native_objects() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = r#"
.text
.globl _population, _vector_math
_population:
fmov d0,x0
cnt.8b v0,v0
uaddlv.8b h0,v0
umov w0,v0.h[0]
ret
_vector_math:
ldr q1,[x1]
ldr q2,[x2]
ldr q3,[x3]
fmul.4s v1,v1,v0[0]
fmla.4s v3,v1,v2[2]
fmls.4s v3,v1,v2[1]
fmulx.4s v3,v3,v2[0]
str q3,[x0]
ret
"#;
    // compare_clang checks the bytes, so only Keel's object links and runs.
    common::compare_clang(source);
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        r#"
#include <stdint.h>
extern unsigned population(uint64_t);
extern void vector_math(float *,const float *,const float *,const float *,float);
int main(void) {
    uint64_t cases[] = {0, 1, UINT64_MAX, UINT64_C(0x0123456789abcdef)};
    for (unsigned i = 0; i < sizeof(cases)/sizeof(cases[0]); ++i) {
        unsigned count = 0;
        for (uint64_t value = cases[i]; value; value >>= 1) count += value & 1;
        if (population(cases[i]) != count) return 1;
    }
    float a[] = {1,2,-4,8}, b[] = {2,3,5,7}, acc[] = {10,20,30,40}, out[4];
    vector_math(out,a,b,acc,2);
    for (unsigned i = 0; i < 4; ++i)
        if (out[i] != (acc[i]+a[i]*2*(b[2]-b[1]))*b[0]) return 2;
    return 0;
}
"#,
    )
    .unwrap();
    let ours = temp.0.join("ours.o");
    std::fs::write(&ours, keel_asm::assemble_object(source).unwrap()).unwrap();
    let executable = temp.0.join("run");
    run(Command::new("clang")
        .arg(&driver)
        .arg(&ours)
        .arg("-o")
        .arg(&executable));
    assert!(execute(&executable).success());
}

#[test]
fn apple_widening_reduction_suffix_uses_the_source_arrangement() {
    common::compare_clang(
        "uaddlv.8b h0,v31\nsaddlv.16b h30,v0\nuaddlv.4h s0,v1\nsaddlv.8h s1,v2\nuaddlv.4s d0,v31\nsaddlv.4s d31,v0\n",
    );
    for source in [
        "uaddlv.8b b0,v0",
        "uaddlv.2s d0,v0",
        "saddlv.2d q0,v0",
        "saddlv.8h s0,v1[0]",
    ] {
        assert!(keel_asm::assemble(source).is_err(), "{source}");
    }
}
