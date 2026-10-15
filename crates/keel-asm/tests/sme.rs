use keel_asm::{TargetFeatures, assemble, assemble_with_features};
mod common;
const M5: &[&str] = &["-mcpu=apple-m5"];
fn compare(source: &str) {
    common::compare_clang_with_features(source, TargetFeatures::apple_m5(), M5);
}
#[test]
fn named_sme_forms_match_clang() {
    let s = include_str!("sme-cases.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split_once('\t').unwrap().1)
        .collect::<Vec<_>>()
        .join("\n");
    compare(&s);
}
#[test]
fn tiles_slices_and_addresses_match_clang() {
    let mut s = String::new();
    for (size, width) in ["b", "h", "s", "d", "q"]
        .into_iter()
        .zip(["b", "h", "w", "d", "q"])
    {
        let shift = ["b", "h", "s", "d", "q"]
            .iter()
            .position(|x| *x == size)
            .unwrap();
        for tile in [0, (1 << shift) - 1] {
            for off in 0..(16 >> shift) {
                for orientation in ["h", "v"] {
                    for selector in [12, 15] {
                        let sl = format!("za{tile}{orientation}.{size}[w{selector}, {off}]");
                        s.push_str(&format!(
                            "mova {sl}, p0/m, z31.{size}\nmov z0.{size}, p7/m, {sl}\n"
                        ));
                        for (op, p) in [("ld", "p0/z"), ("st", "p7")] {
                            s.push_str(&format!("{op}1{width} {{{sl}}}, {p}, [sp]\n{op}1{width} {{{sl}}}, {p}, [x0, x30, lsl #{shift}]\n"));
                        }
                    }
                }
            }
        }
    }
    for off in 0..16 {
        for op in ["ldr", "str"] {
            s.push_str(&format!("{op} za[w12, {off}], [sp, #{off}, mul vl]\n"));
        }
    }
    for n in -32..=31 {
        s.push_str(&format!("rdsvl xzr, #{n}\naddsvl sp, sp, #{n}\n"));
    }
    for size in ["b", "h", "s", "d"] {
        let max = 1
            << ["b", "h", "s", "d"]
                .iter()
                .position(|x| *x == size)
                .unwrap();
        for tile in 0..max {
            s.push_str(&format!("zero {{za{tile}.{size}}}\n"));
        }
    }
    s.push_str("zero {za0.s, za3.s}\nzero {za0.h, za1.h}\n");
    compare(&s);
}
#[test]
fn matrix_features_are_independent_and_default_is_preserved() {
    for (src, feature) in [
        (
            "fmopa za7.d, p7/m, p0/m, z31.d, z0.d",
            TargetFeatures::default().with_sme_f64f64(),
        ),
        (
            "smopa za7.d, p7/m, p0/m, z31.h, z0.h",
            TargetFeatures::default().with_sme_i16i64(),
        ),
        (
            "fmopa za1.h, p7/m, p0/m, z31.h, z0.h",
            TargetFeatures::default().with_sme_f16f16(),
        ),
        (
            "bfmopa za1.h, p7/m, p0/m, z31.h, z0.h",
            TargetFeatures::default().with_sme_b16b16(),
        ),
        (
            "smopa za3.s, p7/m, p0/m, z31.h, z0.h",
            TargetFeatures::default().with_sme2(),
        ),
    ] {
        assert!(assemble(src).is_err());
        assert!(assemble_with_features(src, TargetFeatures::default().with_sme()).is_err());
        assert!(assemble_with_features(src, feature).is_ok(), "{src}");
        compare(src);
    }
    assert!(
        assemble_with_features(
            "fmopa za0.d, p0/m, p0/m, z0.d, z1.d",
            TargetFeatures::default().with_sme2()
        )
        .is_err()
    );
    assert!(
        assemble_with_features(
            "smopa za0.d, p0/m, p0/m, z0.h, z1.h",
            TargetFeatures::default().with_sme_f64f64()
        )
        .is_err()
    );
}
#[test]
fn invalid_matrix_operands_are_rejected() {
    for s in [
        "mova za4h.s[w12,0], p0/m, z0.s",
        "mova za0h.s[w11,0], p0/m, z0.s",
        "mova za0h.s[w16,0], p0/m, z0.s",
        "mova za0h.s[w12,4], p0/m, z0.s",
        "mova za0h.s[w12,0], p0/z, z0.s",
        "mova za0h.s[w12,0], p0/m, z0.d",
        "mova za0.s[w12,0], p0/m, z0.s",
        "mova z32.s, p0/m, za0h.s[w12,0]",
        "ld1w {za0h.s[w12,0]}, p0/m, [x0]",
        "st1w {za0h.s[w12,0]}, p0/z, [x0]",
        "ld1w {za0h.d[w12,0]}, p0/z, [x0]",
        "ld1w {za0h.s[w12,0]}, p0/z, [x0,x1]",
        "ld1w {za0h.s[w12,0]}, p0/z, [x0,x1,lsl #3]",
        "ld1w {za0h.s[w12,0]}, p0/z, [xzr]",
        "ld1w {za0h.s[w12,0]}, p0/z, [x0], #4",
        "ldr za[w12,1], [x0]",
        "ldr za[w12,1], [x0,#2,mul vl]",
        "ldr za[w12,16], [x0,#16,mul vl]",
        "str za[w11,0], [sp]",
        "ldr za[w12,0], [w0]",
        "str za[w12,0], [sp]!",
        "fmopa za2.h, p0/m, p0/m, z0.h, z1.h",
        "smopa za0.s, p0/m, p0/m, z0.h, z1.b",
        "sumopa za0.s, p0/m, p0/m, z0.h, z1.h",
        "addha za0.h, p0/m, p0/m, z0.h",
        "rdsvl sp,#1",
        "addsvl xzr,x0,#1",
        "addsvl x0,xzr,#1",
        "rdsvl x0,#32",
        "zero {za8.d}",
        "zero {za0.q}",
    ] {
        assert!(
            assemble_with_features(s, TargetFeatures::apple_m5()).is_err(),
            "{s}"
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn advertised(features: &[&str]) -> bool {
    features.iter().all(|f| {
        let output = std::process::Command::new("sysctl")
            .args(["-n", &format!("hw.optional.arm.{f}")])
            .output()
            .unwrap();
        output.status.success() && output.stdout == b"1\n"
    })
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native(source: &str, driver: &str) {
    use common::{Temp, execute, run};
    use std::process::Command;
    compare(source);
    let temp = Temp::new();
    let object = temp.0.join("matrix.o");
    let c = temp.0.join("driver.c");
    let executable = temp.0.join("matrix");
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
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
// Ordinary PCS leaf, entered with inactive ZA. No calls in streaming mode;
// preserve the callee-saved low halves of V8..V15 across both transitions.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn leaf(body: &str) -> String {
    format!(
        ".text\n.globl _matrix\n_matrix:\nsub sp,sp,#64\nstp d8,d9,[sp]\nstp d10,d11,[sp,#16]\nstp d12,d13,[sp,#32]\nstp d14,d15,[sp,#48]\nsmstart\n{body}\nsmstop\nldp d8,d9,[sp]\nldp d10,d11,[sp,#16]\nldp d12,d13,[sp,#32]\nldp d14,d15,[sp,#48]\nadd sp,sp,#64\nret\n"
    )
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_outer_products_verify_matrix_results_and_memory_round_trips() {
    for (features, op, dst, src, input, output, expected) in [
        (vec!["FEAT_SME"], "fmopa", "s", "s", "float", "float", 6),
        (
            vec!["FEAT_SME"],
            "smopa",
            "s",
            "b",
            "signed char",
            "int",
            24,
        ),
        (vec!["FEAT_SME2"], "smopa", "s", "h", "short", "int", 12),
        (
            vec!["FEAT_SME_F64F64"],
            "fmopa",
            "d",
            "d",
            "double",
            "double",
            6,
        ),
        (
            vec!["FEAT_SME_I16I64"],
            "smopa",
            "d",
            "h",
            "short",
            "long long",
            24,
        ),
        (
            vec!["FEAT_SME_F16F16"],
            "fmopa",
            "h",
            "h",
            "_Float16",
            "_Float16",
            6,
        ),
    ] {
        if !advertised(&features) {
            eprintln!("skipping {features:?}");
            continue;
        }
        let load = if src == "s" { "w" } else { src };
        let store = if dst == "s" { "w" } else { dst };
        let count = match dst {
            "s" => "cntw",
            "h" => "cnth",
            _ => "cntd",
        };
        let body = format!(
            "ptrue p0.{src}\nmov w12,#0\nld1{load} {{za0h.{src}[w12,0]}},p0/z,[x0]\nmova z0.{src},p0/m,za0h.{src}[w12,0]\nld1{load} {{za0h.{src}[w12,0]}},p0/z,[x1]\nmova z1.{src},p0/m,za0h.{src}[w12,0]\nzero {{za}}\n{op} za0.{dst},p0/m,p0/m,z0.{src},z1.{src}\nstr za[w12,0],[x2]\nzero {{za}}\nldr za[w12,0],[x2]\nptrue p0.{dst}\nst1{store} {{za0h.{dst}[w12,0]}},p0,[x2]\n{count} x0"
        );
        let driver = format!(
            "extern long matrix(const {input}*,const {input}*,{output}*); int main(void) {{ {input} a[256],b[256]; {output} out[256]; for(int i=0;i<256;i++){{a[i]=2;b[i]=3;out[i]=0;}} long n=matrix(a,b,out); if(n<1||n>256)return 1; for(int i=0;i<n;i++)if(out[i]!={expected})return 2; return 0; }}"
        );
        native(&leaf(&body), &driver);
    }
}
#[test]
fn baseline_bytes_remain_unchanged_under_m5() {
    let s = "add v0.4s,v1.4s,v2.4s\nldr x0,[sp]\nmov x0,x1";
    common::compare_clang(s);
    assert_eq!(
        assemble(s).unwrap(),
        assemble_with_features(s, TargetFeatures::apple_m5()).unwrap()
    );
}
