#[path = "sme/scalable.rs"]
mod scalable;
#[path = "sme/sme_kernels.rs"]
mod sme_kernels;
#[path = "sme/streaming.rs"]
mod streaming;
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
    s.push_str("zero {za0.s, za3.s}\nzero {za0.h, za1.h}\nzero {}\nzero {za0.d,za0.d}\n");
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
        "zero {za0.s,za0.d}",
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
/// The kernels of one runtime test. Kernel `k` is `_matrix{k}` and its C
/// check is `check{k}`, so one link and one run cover every kernel.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[derive(Default)]
struct Native {
    kernels: Vec<String>,
    checks: Vec<String>,
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl Native {
    /// `driver` declares `matrix` and defines `int main(void)`.
    fn add(&mut self, body: &str, driver: &str) {
        let k = self.kernels.len();
        self.kernels.push(leaf(&format!("_matrix{k}"), body));
        self.checks.push(
            driver
                .replace("matrix", &format!("matrix{k}"))
                .replace("int main(void)", &format!("static int check{k}(void)")),
        );
    }
    fn run(self) {
        use common::{Temp, execute, run};
        use std::process::Command;
        if self.kernels.is_empty() {
            return;
        }
        // One Clang run checks every kernel; only a mismatch compares each
        // kernel alone to name it.
        let source = self.kernels.concat();
        if std::panic::catch_unwind(|| compare(&source)).is_err() {
            for kernel in &self.kernels {
                if std::panic::catch_unwind(|| compare(kernel)).is_err() {
                    panic!("kernel bytes differ from Clang:\n{kernel}");
                }
            }
            panic!("kernel bytes differ from Clang only when combined");
        }
        let mut driver = String::from("#include <stdio.h>\n");
        for check in &self.checks {
            driver += check;
            driver.push('\n');
        }
        driver += "int main(void) { int status;\n";
        for k in 0..self.checks.len() {
            driver += &format!(
                "if ((status = check{k}())) {{ fprintf(stderr, \"kernel {k}: %d\\n\", status); return 1; }}\n"
            );
        }
        driver += "return 0; }\n";
        let temp = Temp::new();
        let object = temp.0.join("matrix.o");
        let c = temp.0.join("driver.c");
        let executable = temp.0.join("matrix");
        std::fs::write(
            &object,
            keel_asm::assemble_object_with_features(&source, TargetFeatures::apple_m5()).unwrap(),
        )
        .unwrap();
        std::fs::write(&c, driver).unwrap();
        run(Command::new("clang")
            .arg("-O2")
            .arg(&c)
            .arg(&object)
            .arg("-o")
            .arg(&executable));
        let status = execute(&executable);
        assert_eq!(status.code(), Some(0), "{}", self.kernels.join("\n"));
    }
}
// Ordinary PCS leaf, entered with inactive ZA. No calls in streaming mode;
// preserve the callee-saved low halves of V8..V15 across both transitions.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn leaf(name: &str, body: &str) -> String {
    format!(
        ".text\n.globl {name}\n{name}:\nsub sp,sp,#64\nstp d8,d9,[sp]\nstp d10,d11,[sp,#16]\nstp d12,d13,[sp,#32]\nstp d14,d15,[sp,#48]\nsmstart\n{body}\nsmstop\nldp d8,d9,[sp]\nldp d10,d11,[sp,#16]\nldp d12,d13,[sp,#32]\nldp d14,d15,[sp,#48]\nadd sp,sp,#64\nret\n"
    )
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_outer_products_verify_matrix_results_and_memory_round_trips() {
    let mut native = Native::default();
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
        native.add(&body, &driver);
    }
    native.run();
}
#[test]
fn baseline_bytes_remain_unchanged_under_m5() {
    let s =
        "add v0.4s,v1.4s,v2.4s\nldr x0,[sp]\nmov x0,x1\nldr x0,za_literal\nza_literal:\n.quad 0";
    common::compare_clang(s);
    assert_eq!(
        assemble(s).unwrap(),
        assemble_with_features(s, TargetFeatures::apple_m5()).unwrap()
    );
}

#[test]
fn sme2_grouped_transfers_and_arithmetic_boundaries_match_clang() {
    let mut s = String::new();
    for len in [2, 4] {
        for (size, shift) in ["b", "h", "s", "d"].into_iter().zip(0..4) {
            for tile in [0, (1 << shift) - 1] {
                for off in (0..(16 >> shift).max(len)).step_by(len as usize) {
                    for orientation in ["h", "v"] {
                        for first in [0, 32 - len] {
                            let sl = format!(
                                "za{tile}{orientation}.{size}[w15,{off}:{}]",
                                off + len - 1
                            );
                            let group = format!("{{z{first}.{size}-z31.{size}}}");
                            let group = if first == 0 {
                                format!("{{z0.{size}-z{}.{size}}}", len - 1)
                            } else {
                                group
                            };
                            s.push_str(&format!("mova {sl},{group}\nmov {group},{sl}\n"));
                        }
                    }
                }
            }
        }
        for off in 0..8 {
            let ar = format!("za.d[w11,{off},vgx{len}]");
            let group = format!("{{z{}.d-z31.d}}", 32 - len);
            s.push_str(&format!("mova {ar},{group}\nmov {group},{ar}\n"));
            for size in ["h", "s", "d"] {
                let ar = format!("za.{size}[w8,{off},vgx{len}]");
                let n = format!("{{z{}.{size}-z31.{size}}}", 32 - len);
                let m = format!("{{z0.{size}-z{}.{size}}}", len - 1);
                for op in ["fmla", "fmls", "add", "sub"] {
                    if size == "h" && matches!(op, "add" | "sub") {
                        continue;
                    }
                    s.push_str(&format!("{op} {ar},{n},{m}\n{op} {ar},{n},z15.{size}\n"));
                    if matches!(op, "add" | "sub") {
                        s.push_str(&format!("{op} {ar},{n}\n"));
                    } else {
                        let max = match size {
                            "h" => 8,
                            "s" => 4,
                            _ => 2,
                        };
                        for index in 0..max {
                            s.push_str(&format!("{op} {ar},{n},z15.{size}[{index}]\n"));
                        }
                    }
                }
            }
        }
        // Scalar-vector forms allow unaligned groups and wrapping Z31 to Z0.
        for first in [1, 31] {
            let last = (first + len - 1) % 32;
            for op in ["fmla", "fmls", "add", "sub"] {
                s.push_str(&format!("{op} za.s[w8,0],{{z{first}.s-z{last}.s}},z0.s\n"));
            }
        }
    }
    s.push_str("zero {  zt0  }\nldr zt0,[x0]\nstr zt0,[sp]\n");
    compare(&s);
}
#[test]
fn sme2_feature_gates_and_invalid_groups_are_enforced() {
    for line in include_str!("sme-cases.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (name, src) = line.split_once('\t').unwrap();
        if name.starts_with("mova_") && (src.contains('{') || src.contains("za.d["))
            || name.contains("_zt_")
            || name.starts_with("fmla_")
            || name.starts_with("fmls_")
            || name.starts_with("add_za_")
            || name.starts_with("sub_za_")
        {
            assert!(
                assemble_with_features(src, TargetFeatures::default().with_sme()).is_err(),
                "{name}"
            );
        }
    }
    assert!(
        assemble_with_features(
            "fmla za.d[w8,0],{z0.d,z1.d},z0.d",
            TargetFeatures::default().with_sme2()
        )
        .is_err()
    );
    assert!(
        assemble_with_features(
            "fmla za.s[w8,0],{z0.s,z1.s},z0.s",
            TargetFeatures::default().with_sme2()
        )
        .is_ok()
    );
    for s in [
        "mova za0h.s[w12,0:1],{z1.s,z2.s}",
        "mova {z0.s,z2.s},za0h.s[w12,0:1]",
        "mova {z0.s,z1.d},za0h.s[w12,0:1]",
        "mova {z0.s,z1.s},za0h.s[w12,1:2]",
        "mova {z0.s,z1.s},za0h.s[w12,0:2]",
        "mova {z0.s,z1.s},za0h.s[w12,4:5]",
        "mova {z0.q,z1.q},za0h.q[w12,0:1]",
        "mova {z0.d-z3.d},za0h.d[w12,1:4]",
        "mova za.s[w8,0],{z0.s,z1.s}",
        "mova za.d[w12,0],{z0.d,z1.d}",
        "mova za.d[w8,8],{z0.d,z1.d}",
        "mova za.d[w8,0,vgx4],{z0.d,z1.d}",
        "fmla za.s[w8,0],{z0.s,z1.s},z16.s",
        "fmla za.s[w8,0],{z0.s,z1.s},z0.s[4]",
        "fmla za.s[w8,0],{z1.s,z2.s},z0.s[0]",
        "fmla za.s[w8,0],{z0.s,z1.s},{z1.s,z2.s}",
        "fmla za.s[w8,0],{z0.s,z1.s},{z0.s-z3.s}",
        "fmla za.h[w8,0],{z0.h,z1.h},z0.h[8]",
        "add za.h[w8,0],{z0.h,z1.h}",
        "sub za.s[w8,0],{z0.s,z1.s},z0.s[0]",
        "fmla za.s[w7,0],{z0.s,z1.s},z0.s",
        "fmla za.s[w8,0,vgx4],{z0.s,z1.s},z0.s",
        "fmla za.s[w8,0],{z0.s,z1.s},z0.d",
        "fmla za.s[w8,0],{z0.s,z1.s}",
        "ldr zt0,[sp,#0]",
        "str zt0,[xzr]",
        "zero {zt1}",
    ] {
        assert!(
            assemble_with_features(s, TargetFeatures::apple_m5()).is_err(),
            "{s}"
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_sme2_grouped_tile_and_array_transfers_preserve_values() {
    if !advertised(&["FEAT_SME2"]) {
        return;
    }
    let mut native = Native::default();
    for len in [2, 4] {
        // Test horizontal and vertical slice groups independently. Each vector
        // has a different value so exchanges and replication cannot pass.
        for orientation in ["h", "v"] {
            let mut body = String::from("mov w12,#0\nptrue p0.d\n");
            for i in 0..len {
                body.push_str(&format!("dup z{i}.d,#{}\n", i + 7));
            }
            body.push_str(&format!("mova za7{orientation}.d[w12,0:{}],{{z0.d-z{}.d}}\nmova {{z4.d-z{}.d}},za7{orientation}.d[w12,0:{}]\n",len-1,len-1,4+len-1,len-1));
            body.push_str("mov x0,#0\n");
            for i in 0..len {
                body.push_str(&format!(
                    "lastb x9,p0,z{}.d\ncmp x9,#{}\ncset x10,ne\norr x0,x0,x10\n",
                    4 + i,
                    i + 7
                ));
            }
            native.add(
                &body,
                "extern long matrix(void);int main(void){return matrix()!=0;}",
            );
        }
        let mut body = String::from("mov w8,#0\nptrue p0.d\n");
        for i in 0..len {
            body.push_str(&format!("dup z{i}.d,#{}\n", i + 17));
        }
        body.push_str(&format!("mova za.d[w8,7,vgx{len}],{{z0.d-z{}.d}}\nmova {{z4.d-z{}.d}},za.d[w8,7,vgx{len}]\nmov x0,#0\n",len-1,4+len-1));
        for i in 0..len {
            body.push_str(&format!(
                "lastb x9,p0,z{}.d\ncmp x9,#{}\ncset x10,ne\norr x0,x0,x10\n",
                4 + i,
                i + 17
            ));
        }
        native.add(
            &body,
            "extern long matrix(void);int main(void){return matrix()!=0;}",
        );
    }
    native.run();
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_sme2_za_accumulation_and_zt0_round_trip() {
    if !advertised(&["FEAT_SME2"]) {
        return;
    }
    let mut native = Native::default();
    native.add(
        "ldr zt0,[x0]\nstr zt0,[x1]\nzero {zt0}\nstr zt0,[x2]",
        "extern void matrix(const unsigned char*,unsigned char*,unsigned char*);int main(void){unsigned char in[64],out[64],zero[64];for(int i=0;i<64;i++){in[i]=i*3+1;out[i]=0;zero[i]=255;}matrix(in,out,zero);for(int i=0;i<64;i++)if(out[i]!=in[i]||zero[i]!=0)return 1;return 0;}",
    );
    for len in [2, 4] {
        let mut body = String::from(
            "ptrue p0.s\nmov w12,#0\nld1w {za0h.s[w12,0]},p0/z,[x0]\nmova z0.s,p0/m,za0h.s[w12,0]\nld1w {za0h.s[w12,0]},p0/z,[x1]\nmova z8.s,p0/m,za0h.s[w12,0]\n",
        );
        for i in 1..len {
            body.push_str(&format!("fadd z{i}.s,z{}.s,z0.s\n", i - 1));
        }
        body.push_str(&format!("zero {{za}}\nmov w8,#0\nfmla za.s[w8,0,vgx{len}],{{z0.s-z{}.s}},z8.s\nfmls za.s[w8,0,vgx{len}],{{z0.s-z{}.s}},z8.s\nfmla za.s[w8,0,vgx{len}],{{z0.s-z{}.s}},z8.s[0]\nmova {{z4.d-z{}.d}},za.d[w8,0,vgx{len}]\n",len-1,len-1,len-1,len+3));
        for i in 0..len {
            body.push_str(&format!(
                "lastb w9,p0,z{}.s\nstr w9,[x2,#{}]\n",
                i + 4,
                i * 4
            ));
        }
        let driver = format!(
            "extern void matrix(const float*,const float*,float*);int main(void){{float a[256],b[256],out[4];for(int i=0;i<256;i++){{a[i]=2;b[i]=3;}}matrix(a,b,out);for(int i=0;i<{len};i++)if(out[i]!=(i+1)*6)return 1;return 0;}}"
        );
        native.add(&body, &driver);
        let mut body = String::from("mov w8,#0\nptrue p0.s\nzero {za}\n");
        for i in 0..len {
            body.push_str(&format!("dup z{i}.s,#{}\n", i + 7));
        }
        body.push_str(&format!("add za.s[w8,0,vgx{len}],{{z0.s-z{}.s}}\nsub za.s[w8,0,vgx{len}],{{z0.s-z{}.s}}\nadd za.s[w8,0,vgx{len}],{{z0.s-z{}.s}}\nmova {{z4.d-z{}.d}},za.d[w8,0,vgx{len}]\n",len-1,len-1,len-1,len+3));
        for i in 0..len {
            body.push_str(&format!(
                "lastb w9,p0,z{}.s\nstr w9,[x0,#{}]\n",
                i + 4,
                i * 4
            ));
        }
        native.add(
            &body,
            &format!(
                "extern void matrix(int*);int main(void){{int out[4];matrix(out);for(int i=0;i<{len};i++)if(out[i]!=i+7)return 1;return 0;}}"
            ),
        );
    }
    native.run();
}

#[test]
fn outer_product_tiles_predicates_and_optional_types_match_clang() {
    let mut s = String::new();
    for (ops, dst, src) in [
        (&["fmopa", "fmops"][..], "s", "s"),
        (&["fmopa", "fmops"][..], "s", "h"),
        (&["fmopa", "fmops"][..], "h", "h"),
        (&["fmopa", "fmops"][..], "d", "d"),
        (&["bfmopa", "bfmops"][..], "s", "h"),
        (&["bfmopa", "bfmops"][..], "h", "h"),
        (
            &[
                "smopa", "smops", "umopa", "umops", "sumopa", "sumops", "usmopa", "usmops",
            ][..],
            "s",
            "b",
        ),
        (
            &[
                "smopa", "smops", "umopa", "umops", "sumopa", "sumops", "usmopa", "usmops",
            ][..],
            "d",
            "h",
        ),
        (&["smopa", "smops", "umopa", "umops"][..], "s", "h"),
    ] {
        let max = match dst {
            "s" => 4,
            "d" => 8,
            _ => 2,
        };
        for tile in 0..max {
            for p in 0..8 {
                for op in ops {
                    s.push_str(&format!(
                        "{op} za{tile}.{dst},p{p}/m,p{}/m,z0.{src},z31.{src}\n",
                        7 - p
                    ));
                }
            }
        }
    }
    for (size, max) in [("s", 4), ("d", 8)] {
        for tile in 0..max {
            for p in 0..8 {
                for op in ["addha", "addva"] {
                    s.push_str(&format!(
                        "{op} za{tile}.{size},p{p}/m,p{}/m,z31.{size}\n",
                        7 - p
                    ));
                }
            }
        }
    }
    compare(&s);
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_bf16_outer_products_verify_widening_and_non_widening_results() {
    let mut native = Native::default();
    for (flag, dst, expected, output) in [
        ("FEAT_SME", "s", "0x41400000u", "unsigned int"),
        ("FEAT_SME_B16B16", "h", "0x40c0u", "unsigned short"),
    ] {
        if !advertised(&[flag]) {
            continue;
        }
        let store = if dst == "s" { "w" } else { "h" };
        let cnt = if dst == "s" { "cntw" } else { "cnth" };
        let body = format!(
            "ptrue p0.h\nmov w12,#0\nld1h {{za0h.h[w12,0]}},p0/z,[x0]\nmova z0.h,p0/m,za0h.h[w12,0]\nld1h {{za0h.h[w12,0]}},p0/z,[x1]\nmova z1.h,p0/m,za0h.h[w12,0]\nzero {{za}}\nbfmopa za0.{dst},p0/m,p0/m,z0.h,z1.h\nbfmops za0.{dst},p0/m,p0/m,z0.h,z1.h\nbfmopa za0.{dst},p0/m,p0/m,z0.h,z1.h\nptrue p0.{dst}\nst1{store} {{za0h.{dst}[w12,0]}},p0,[x2]\n{cnt} x0"
        );
        let driver = format!(
            "extern long matrix(const unsigned short*,const unsigned short*,{output}*);int main(void){{unsigned short a[256],b[256];{output} out[256];for(int i=0;i<256;i++){{a[i]=0x4000;b[i]=0x4040;out[i]=0;}}long n=matrix(a,b,out);if(n<1||n>256)return 1;for(int i=0;i<n;i++)if(out[i]!={expected})return 2;return 0;}}"
        );
        native.add(&body, &driver);
    }
    native.run();
}
