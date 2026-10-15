use keel_asm::{TargetFeatures, assemble, assemble_with_features};
mod common;
fn compare(s: &str) {
    common::compare_clang_with_features(s, TargetFeatures::apple_m5(), &["-mcpu=apple-m5"]);
}
#[test]
fn streaming_named_forms_match_clang_and_require_explicit_features() {
    let mut s = String::new();
    for line in include_str!("streaming-cases.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (_, source) = line.split_once('\t').unwrap();
        assert!(assemble(source).is_err(), "{source}");
        for f in [
            TargetFeatures::default().with_sme(),
            TargetFeatures::default().with_sve(),
        ] {
            assert!(assemble_with_features(source, f).is_ok(), "{source}");
        }
        s.push_str(source);
        s.push('\n');
    }
    compare(&s);
}
#[test]
fn streaming_memory_predicates_and_helpers_match_clang_boundaries() {
    let mut s = String::new();
    for size in ["b", "h", "s", "d"] {
        let z = if size == "d" { "x" } else { "w" };
        let sp = if size == "d" { "sp" } else { "wsp" };
        s.push_str(&format!("dup z0.{size},{z}0\ndup z31.{size},{sp}\n"));
        for op in ["whilelt", "whilelo", "whilele", "whilels"] {
            for r in ["w", "x"] {
                s.push_str(&format!(
                    "{op} p0.{size},{r}zr,{r}30\n{op} p15.{size},{r}0,{r}zr\n"
                ));
            }
        }
        let size_index = ["b", "h", "s", "d"]
            .iter()
            .position(|s| *s == size)
            .unwrap();
        for (op, width, signed) in [
            ("ld1b", 0, false),
            ("ld1h", 1, false),
            ("ld1w", 2, false),
            ("ld1d", 3, false),
            ("ld1sb", 0, true),
            ("ld1sh", 1, true),
            ("ld1sw", 2, true),
            ("st1b", 0, false),
            ("st1h", 1, false),
            ("st1w", 2, false),
            ("st1d", 3, false),
        ] {
            if size_index < width || (signed && size_index == width) {
                continue;
            }
            let p = if op.starts_with("ld") { "p0/z" } else { "p0" };
            s.push_str(&format!("{op} {{z0.{size}}},{p},[sp]\n"));
            for n in -8..=7 {
                s.push_str(&format!("{op} {{z31.{size}}},{p},[x0,#{n},mul vl]\n"));
            }
            for rn in ["x0", "sp"] {
                for rm in ["x0", "x30"] {
                    s.push_str(&format!(
                        "{op} {{z31.{size}}},{p},[{rn},{rm},lsl #{width}]\n"
                    ));
                }
            }
        }
    }
    for n in -32..=31 {
        s.push_str(&format!("addvl sp,sp,#{n}\n"));
    }
    s.push_str("LD1W {Z31.S},P7/Z,[SP,#7,MUL VL]\nWHILELO P15.H,X0,XZR\n");
    compare(&s);
    common::compare_clang("ldr x0,whilelo_label\nwhilelo_label:\n.quad 0");
}
#[test]
fn invalid_streaming_operands_are_rejected() {
    for s in [
        "whilelo p16.s,x0,x1",
        "whilelo p0.s,w0,x1",
        "whilelo p0.s,sp,x0",
        "whilelo p0.s,x0,#1",
        "whilelt p0/z,x0,x1",
        "dup z0.b,x0",
        "dup z0.d,w0",
        "dup z0.s,wzr",
        "dup z0.d,xzr",
        "dup z0.s,w0,lsl #8",
        "ld1w {z32.s},p0/z,[x0]",
        "ld1w {z0.h},p0/z,[x0]",
        "ld1sb {z0.b},p0/z,[x0]",
        "ld1sh {z0.h},p0/z,[x0]",
        "ld1w {z0.s},p0/m,[x0]",
        "ld1w {z0.s},p8/z,[x0]",
        "ld1w {z0.s},p0/z,[w0]",
        "ld1w {z0.s},p0/z,[xzr]",
        "ld1w {z0.s},p0/z,[x0,#8,mul vl]",
        "ld1w {z0.s},p0/z,[x0,#-9,mul vl]",
        "ld1w {z0.s},p0/z,[x0,#0]",
        "ld1w {z0.s},p0/z,[x0,x1]",
        "ld1h {z0.s},p0/z,[x0,x1,lsl #2]",
        "ld1b {z0.s},p0/z,[x0,xzr]",
        "ld1d {z0.d},p0/z,[x0,w1,lsl #3]",
        "st1w {z0.s},p0/m,[x0]",
        "st1w {z0.s},p0,[x0],#4",
        "ld1w {z0.s,z1.s},p0/z,[x0]",
        "addvl xzr,x0,#0",
        "addvl x0,xzr,#0",
        "addvl w0,w1,#0",
        "addvl x0,x1,#32",
    ] {
        assert!(
            assemble_with_features(s, TargetFeatures::apple_m5()).is_err(),
            "{s}"
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn streaming_extending_load_and_truncating_store_execute_with_tails() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let supported = Command::new("sysctl")
        .args(["-n", "hw.optional.arm.FEAT_SME"])
        .output()
        .unwrap();
    if !supported.status.success() || supported.stdout != b"1\n" {
        return;
    }
    let mut source = String::new();
    for (name, load, store) in [("extend", "ld1sb", "st1w"), ("truncate", "ld1w", "st1b")] {
        source.push_str(&format!(".text\n.globl _{name}\n_{name}:\nsub sp,sp,#64\nstp d8,d9,[sp]\nstp d10,d11,[sp,#16]\nstp d12,d13,[sp,#32]\nstp d14,d15,[sp,#48]\nsmstart sm\nwhilelo p0.s,xzr,x2\n{load} {{z0.s}},p0/z,[x0]\n{store} {{z0.s}},p0,[x1]\ncntw x0\nsmstop sm\nldp d8,d9,[sp]\nldp d10,d11,[sp,#16]\nldp d12,d13,[sp,#32]\nldp d14,d15,[sp,#48]\nadd sp,sp,#64\nret\n"));
    }
    compare(&source);
    let temp = Temp::new();
    let object = temp.0.join("memory.o");
    let driver = temp.0.join("driver.c");
    let executable = temp.0.join("memory");
    std::fs::write(
        &object,
        keel_asm::assemble_object_with_features(&source, TargetFeatures::default().with_sme())
            .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &driver,
        r#"
#include <stdint.h>
#include <stddef.h>
extern size_t extend(const int8_t*,int32_t*,size_t);
extern size_t truncate(const int32_t*,uint8_t*,size_t);
int main(void){int8_t a[64];int32_t b[65],c[64];uint8_t d[65];
for(int i=0;i<64;i++){a[i]=(int8_t)(i*13-128);c[i]=i*17-129;}
size_t vl=extend(a,b,0);if(vl<4||vl>64)return 1;
for(size_t n=0;n<=vl;n++){for(int i=0;i<65;i++){b[i]=0x12345678;d[i]=0x55;}
extend(a,b,n);truncate(c,d,n);for(size_t i=0;i<n;i++)if(b[i]!=a[i]||d[i]!=(uint8_t)c[i])return 2;
for(size_t i=n;i<65;i++)if(b[i]!=0x12345678||d[i]!=0x55)return 3;}return 0;}
"#,
    )
    .unwrap();
    run(Command::new("clang")
        .arg("-O2")
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
