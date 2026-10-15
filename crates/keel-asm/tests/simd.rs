use keel_asm::assemble;
mod common;
use common::compare_clang;

#[test]
fn neon_register_arithmetic_matches_clang() {
    let mut source = String::from(".text\n");
    for arrangement in ["8b", "16b", "4h", "8h", "2s", "4s", "2d"] {
        for op in [
            "add", "sub", "cmeq", "cmgt", "cmge", "cmhi", "cmhs", "mul", "mla", "mls", "smax",
            "smin", "umax", "umin", "abs", "neg",
        ] {
            if arrangement == "2d"
                && ["mul", "mla", "mls", "smax", "smin", "umax", "umin"].contains(&op)
            {
                continue;
            }
            let third = if ["abs", "neg"].contains(&op) {
                String::new()
            } else {
                format!(", v30.{arrangement}")
            };
            source.push_str(&format!(
                "{op} v0.{arrangement}, v31.{arrangement}{third}\n"
            ));
        }
    }
    for arrangement in ["8b", "16b"] {
        for op in [
            "and", "orr", "orn", "eor", "bic", "bsl", "bit", "bif", "not", "mvn", "mov",
        ] {
            let third = if ["not", "mvn", "mov"].contains(&op) {
                String::new()
            } else {
                format!(", v0.{arrangement}")
            };
            source.push_str(&format!(
                "{op} v31.{arrangement}, v1.{arrangement}{third}\n"
            ));
        }
    }
    for arrangement in ["2s", "4s", "2d"] {
        for op in [
            "fadd", "fsub", "fmul", "fdiv", "fmla", "fmls", "fmin", "fmax", "fminnm", "fmaxnm",
            "fabs", "fneg", "fsqrt",
        ] {
            let third = if ["fabs", "fneg", "fsqrt"].contains(&op) {
                String::new()
            } else {
                format!(", v0.{arrangement}")
            };
            source.push_str(&format!(
                "{op} v31.{arrangement}, v30.{arrangement}{third}\n"
            ));
        }
    }
    source.push_str("ADD V31.2D, V0.2D, V1.2D\n");
    compare_clang(&source);
}

#[test]
fn neon_reserved_sizes_and_mismatched_operands_are_rejected() {
    for bad in [
        "add v0.1d, v1.1d, v2.1d",
        "add v0.4s, v1.2s, v2.4s",
        "sub v32.4s, v0.4s, v1.4s",
        "add v0.4s, v1.4s",
        "mla v0.2d, v1.2d, v2.2d",
        "mul v0.2d, v1.2d, v2.2d",
        "smax v0.2d, v1.2d, v2.2d",
        "umax v0.2d, v1.2d, v2.2d",
        "and v0.4s, v1.4s, v2.4s",
        "mov v0.2d, v1.2d",
        "fadd v0.8h, v1.8h, v2.8h",
        "fmul v0.16b, v1.16b, v2.16b",
        "fdiv v0.1d, v1.1d, v2.1d",
        "fmla v0.2s, v1.4s, v2.2s",
        "fadd v0.4s, s1, v2.4s",
        "fsqrt v0.4s, v1.4s, v2.4s",
        "addv d0, v1.2d",
        "addv s0, v1.2s",
        "add v0.4s, v1.4s, #1",
        "add v0.4s, v1.4s, v2.s[0]",
        "mvn v0.16b, v1.8b",
    ] {
        let error = assemble(&format!("nop\n{bad}")).expect_err(bad);
        assert_eq!(error.line, 2, "{bad}: {error}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn neon_integer_and_fp_execute() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = "
.text
.globl _integer, _floating
_integer:
    ldr q0, [x0]
    ldr q1, [x1]
    add v2.4s, v0.4s, v1.4s
    mul v2.4s, v2.4s, v1.4s
    sub v2.4s, v2.4s, v0.4s
    cmhi v3.4s, v0.4s, v1.4s
    bsl v3.16b, v2.16b, v1.16b
    str q3, [x2]
    ret
_floating:
    ldr q0, [x0]
    ldr q1, [x1]
    fmul v2.4s, v0.4s, v1.4s
    fmla v2.4s, v0.4s, v0.4s
    fabs v2.4s, v2.4s
    fsqrt v2.4s, v2.4s
    str q2, [x2]
    ret
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("simd.o");
    let c = temp.0.join("driver.c");
    let executable = temp.0.join("simd");
    std::fs::write(&object, keel_asm::assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, "
#include <stdint.h>
#include <math.h>
extern void integer(const uint32_t*, const uint32_t*, uint32_t*);
extern void floating(const float*, const float*, float*);
int main(void) {
    uint32_t a[4] = {0xffffffff, 7, 0, 0x80000000}, b[4] = {2, 3, 4, 1}, out[4];
    integer(a, b, out);
    for (int i = 0; i < 4; ++i) if (out[i] != (a[i] > b[i] ? (a[i] + b[i])*b[i] - a[i] : b[i])) return 1;
    float x[4] = {-4, 3, 0, 8}, y[4] = {3, 9, 1, -6}, z[4];
    floating(x, y, z);
    for (int i = 0; i < 4; ++i) if (z[i] != sqrtf(fabsf(fmaf(x[i], x[i], x[i]*y[i])))) return 2;
    return 0;
}").unwrap();
    run(Command::new("clang")
        .args(["-O2"])
        .arg(&c)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
