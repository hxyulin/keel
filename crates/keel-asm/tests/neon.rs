use keel_asm::assemble;
mod common;
use common::compare_clang;

#[test]
fn lane_moves_and_permutations_match_clang() {
    let mut s = String::new();
    for arr in ["8b", "16b", "4h", "8h", "2s", "4s", "2d"] {
        for op in ["zip1", "zip2", "uzp1", "uzp2", "trn1", "trn2"] {
            s.push_str(&format!("{op} v0.{arr}, v31.{arr}, v30.{arr}\n"));
        }
    }
    for arr in ["8b", "16b"] {
        let bytes = if arr == "8b" { 8 } else { 16 };
        for index in 0..bytes {
            s.push_str(&format!("ext v31.{arr}, v0.{arr}, v1.{arr}, #{index}\n"));
        }
        for op in ["tbl", "tbx"] {
            for len in 1..=4 {
                let list = (0..len)
                    .map(|i| format!("v{}.16b", (31 + i) % 32))
                    .collect::<Vec<_>>()
                    .join(", ");
                s.push_str(&format!("{op} v0.{arr}, {{{list}}}, v30.{arr}\n"));
            }
        }
    }
    for (size, element) in ["b", "h", "s", "d"].into_iter().enumerate() {
        let p = if size == 3 { "x" } else { "w" };
        let high = 16 >> size;
        for q in 0..=1 {
            let lanes = (8 << q) >> size;
            if lanes == 1 {
                continue;
            }
            s.push_str(&format!("dup v31.{lanes}{element}, {p}zr\n"));
            for lane in 0..high {
                s.push_str(&format!("dup v0.{lanes}{element}, v31.{element}[{lane}]\n"));
            }
        }
        for lane in 0..high {
            s.push_str(&format!("dup {element}0, v31.{element}[{lane}]\nins v31.{element}[{lane}], {p}zr\nmov v0.{element}[{lane}], {p}0\n"));
            for other in 0..high {
                s.push_str(&format!("ins v0.{element}[{lane}], v31.{element}[{other}]\nmov v31.{element}[{lane}], v0.{element}[{other}]\n"));
            }
            s.push_str(&format!("umov {p}0, v31.{element}[{lane}]\n"));
            if size < 3 {
                s.push_str(&format!("smov xzr, v0.{element}[{lane}]\n"));
            }
            if size < 2 {
                s.push_str(&format!("smov w0, v31.{element}[{lane}]\n"));
            }
        }
    }
    s.push_str("tbl v0.16b, {v31.16b-v1.16b}, v2.16b\nmov d0, v31.d[1]\nmov b0,v1.b[15]\nmov h0,v1.h[7]\nmov s0,v1.s[3]\nmov v0.1d,v1.1d\nmov v0.4s,v1.4s\n");
    compare_clang(&s);
}

#[test]
fn malformed_lane_and_permutation_operands_are_rejected() {
    for s in [
        "zip1 v0.1d,v1.1d,v2.1d",
        "ext v0.4s,v1.4s,v2.4s,#1",
        "ext v0.8b,v1.8b,v2.8b,#8",
        "dup v0.1d,x0",
        "dup v0.4s,x0",
        "dup v0.4s,v1.h[0]",
        "dup s0,v1.s[4]",
        "ins v0.b[16],w0",
        "ins v0.h[0],x0",
        "ins v0.s[0],v1.h[0]",
        "umov x0,v1.s[0]",
        "smov w0,v1.s[0]",
        "smov x0,v1.d[0]",
        "tbl v0.16b,{v1.8b},v2.16b",
        "tbl v0.8b,{v0.16b,v2.16b},v3.8b",
        "tbl v0.8b,{v0.16b,v1.16b,v2.16b,v3.16b,v4.16b},v5.8b",
        "tbl v0.8b,{v0.16b,v1.16b,v2.16b,v3.16b,v4.16b,v5.16b},v5.8b",
        "tbl v0.8b,{},v1.8b",
        "tbl v0.8b,{v0.16b,v1.8b},v2.8b",
        "tbl v0.8b,{v1.16b,v2.16b,v3.16b,v4.16b,v5.16b,v6.16b},v7.8b",
    ] {
        assert!(assemble(s).is_err(), "{s}");
    }
}

#[test]
fn integer_arithmetic_and_shifts_match_clang() {
    let mut s = String::new();
    for (size, e) in ["b", "h", "s", "d"].into_iter().enumerate() {
        for q in 0..=1 {
            if size == 3 && q == 0 {
                continue;
            }
            let arr = format!("{}{e}", (8 << q) >> size);
            for op in [
                "sqadd", "uqadd", "sqsub", "uqsub", "sshl", "ushl", "sqshl", "uqshl", "srshl",
                "urshl", "sqrshl", "uqrshl", "cmtst", "addp",
            ] {
                s.push_str(&format!("{op} v0.{arr},v31.{arr},v30.{arr}\n"));
            }
            for op in [
                "shadd", "uhadd", "srhadd", "urhadd", "shsub", "uhsub", "sabd", "uabd", "saba",
                "uaba", "smaxp", "sminp", "umaxp", "uminp",
            ] {
                if size < 3 {
                    s.push_str(&format!("{op} v31.{arr},v0.{arr},v1.{arr}\n"));
                }
            }
            for op in ["sqdmulh", "sqrdmulh"] {
                if matches!(size, 1 | 2) {
                    s.push_str(&format!("{op} v31.{arr},v0.{arr},v1.{arr}\n"));
                }
            }
            for op in [
                "sqabs", "sqneg", "suqadd", "usqadd", "clz", "cls", "rev16", "rev32", "rev64",
                "rbit",
            ] {
                let max = match op {
                    "clz" | "cls" | "rev64" => 2,
                    "rev32" => 1,
                    "rev16" | "rbit" => 0,
                    _ => 3,
                };
                if size <= max {
                    s.push_str(&format!("{op} v31.{arr},v0.{arr}\n"));
                }
            }
            for op in ["cmeq", "cmgt", "cmge", "cmlt", "cmle"] {
                s.push_str(&format!(
                    "{op} v31.{arr},v0.{arr},#0\n{op} v0.{arr},v31.{arr},v30.{arr}\n"
                ));
            }
            let bits = 8 << size;
            for op in [
                "sshr", "ushr", "ssra", "usra", "srshr", "urshr", "srsra", "ursra", "sri", "shl",
                "sli", "sqshl", "uqshl", "sqshlu",
            ] {
                let left = ["shl", "sli", "sqshl", "uqshl", "sqshlu"].contains(&op);
                for amount in if left { [0, bits - 1] } else { [1, bits] } {
                    s.push_str(&format!("{op} v31.{arr},v0.{arr},#{amount}\n"));
                }
            }
        }
        let bits = 8 << size;
        for op in [
            "sqadd", "uqadd", "sqsub", "uqsub", "sqshl", "uqshl", "sqrshl", "uqrshl", "sqdmulh",
            "sqrdmulh", "add", "sub", "cmeq", "cmgt", "cmge", "cmhi", "cmhs", "cmtst", "sshl",
            "ushl", "srshl", "urshl",
        ] {
            let valid = if ["sqdmulh", "sqrdmulh"].contains(&op) {
                matches!(size, 1 | 2)
            } else {
                size == 3
                    || [
                        "sqadd", "uqadd", "sqsub", "uqsub", "sqshl", "uqshl", "sqrshl", "uqrshl",
                    ]
                    .contains(&op)
            };
            if valid {
                s.push_str(&format!("{op} {e}31,{e}0,{e}30\n"));
            }
        }
        for op in ["sqabs", "sqneg", "suqadd", "usqadd", "abs", "neg"] {
            if size == 3 || op.starts_with(['s', 'u']) {
                s.push_str(&format!("{op} {e}31,{e}0\n"));
            }
        }
        if size == 3 {
            for op in ["cmeq", "cmgt", "cmge", "cmlt", "cmle"] {
                s.push_str(&format!("{op} d31,d0,#0\n"));
            }
        }
        for op in [
            "sshr", "ushr", "ssra", "usra", "srshr", "urshr", "srsra", "ursra", "sri", "shl",
            "sli", "sqshl", "uqshl", "sqshlu",
        ] {
            if size != 3 && !["sqshl", "uqshl", "sqshlu"].contains(&op) {
                continue;
            }
            let left = ["shl", "sli", "sqshl", "uqshl", "sqshlu"].contains(&op);
            for amount in if left { [0, bits - 1] } else { [1, bits] } {
                s.push_str(&format!("{op} {e}31,{e}0,#{amount}\n"));
            }
        }
    }
    for size in 0..3 {
        let e = ["b", "h", "s", "d"][size];
        let w = ["b", "h", "s", "d"][size + 1];
        let bits = 8 << size;
        for high in [false, true] {
            let suffix = if high { "2" } else { "" };
            let n = format!("v31.{}{e}", (8 << usize::from(high)) >> size);
            let d = format!("v0.{}{w}", 16 >> (size + 1));
            for op in [
                "saddl", "uaddl", "ssubl", "usubl", "sabal", "uabal", "sabdl", "uabdl", "smlal",
                "umlal", "smlsl", "umlsl", "smull", "umull", "sqdmlal", "sqdmlsl", "sqdmull",
                "pmull",
            ] {
                if op.starts_with("sqd") && size == 0 || op == "pmull" && size != 0 {
                    continue;
                }
                s.push_str(&format!(
                    "{op}{suffix} {d},{n},v30.{}{e}\n",
                    (8 << usize::from(high)) >> size
                ));
            }
            for op in ["saddw", "uaddw", "ssubw", "usubw"] {
                s.push_str(&format!(
                    "{op}{suffix} {d},v1.{}{w},{n}\n",
                    16 >> (size + 1)
                ));
            }
            for op in ["sshll", "ushll", "sxtl", "uxtl", "shll"] {
                if ["sxtl", "uxtl"].contains(&op) {
                    s.push_str(&format!("{op}{suffix} {d},{n}\n"));
                } else {
                    for amt in if op == "shll" {
                        [bits, bits]
                    } else {
                        [0, bits - 1]
                    } {
                        s.push_str(&format!("{op}{suffix} {d},{n},#{amt}\n"));
                    }
                }
            }
            let d = format!("v0.{}{e}", (8 << usize::from(high)) >> size);
            let n = format!("v31.{}{w}", 16 >> (size + 1));
            for op in [
                "xtn", "sqxtn", "uqxtn", "sqxtun", "addhn", "raddhn", "subhn", "rsubhn",
            ] {
                let third = if op.ends_with("hn") {
                    format!(",v30.{}{w}", 16 >> (size + 1))
                } else {
                    String::new()
                };
                s.push_str(&format!("{op}{suffix} {d},{n}{third}\n"));
            }
            for op in [
                "shrn", "rshrn", "sqshrn", "sqrshrn", "sqshrun", "sqrshrun", "uqshrn", "uqrshrn",
            ] {
                for amount in [1, bits] {
                    s.push_str(&format!("{op}{suffix} {d},{n},#{amount}\n"));
                }
            }
        }
        for q in 0..=1 {
            for op in ["saddlp", "uaddlp", "sadalp", "uadalp"] {
                s.push_str(&format!(
                    "{op} v0.{}{w},v31.{}{e}\n",
                    (8 << q) >> (size + 1),
                    (8 << q) >> size
                ));
            }
        }
        for op in ["sqxtn", "uqxtn", "sqxtun"] {
            s.push_str(&format!("{op} {e}0,{w}31\n"));
        }
        for op in [
            "sqshrn", "sqrshrn", "sqshrun", "sqrshrun", "uqshrn", "uqrshrn",
        ] {
            for amt in [1, bits] {
                s.push_str(&format!("{op} {e}0,{w}31,#{amt}\n"));
            }
        }
        if size > 0 {
            for op in ["sqdmlal", "sqdmlsl", "sqdmull"] {
                s.push_str(&format!("{op} {w}0,{e}31,{e}30\n"));
            }
        }
    }
    compare_clang(&s);
}

#[test]
fn integer_reserved_forms_are_rejected() {
    for s in [
        "sqdmulh v0.16b,v1.16b,v2.16b",
        "shadd v0.2d,v1.2d,v2.2d",
        "add s0,s1,s2",
        "rev64 v0.2d,v1.2d",
        "saddl v0.4s,v1.4h,v2.8h",
        "saddl2 v0.4s,v1.4h,v2.4h",
        "saddw v0.8h,v1.8b,v2.8b",
        "addhn v0.16b,v1.8h,v2.8h",
        "xtn2 v0.8b,v1.8h",
        "saddlp v0.4s,v1.4h",
        "pmull v0.1q,v1.1d,v2.1d",
        "shl v0.4s,v1.4s,#32",
        "sshr v0.4s,v1.4s,#0",
        "ushr v0.4s,v1.4s,#33",
        "shrn v0.8b,v1.8h,#9",
        "sshll v0.8h,v1.8b,#8",
        "shll v0.8h,v1.8b,#7",
        "sqshrn2 b0,h1,#1",
        "shl s0,s1,#1",
        "sxtl v0.8h,v1.16b",
        "sqdmull h0,b1,b2",
        "rbit v0.4h,v1.4h",
    ] {
        assert!(assemble(s).is_err(), "{s}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn permutations_widening_saturation_and_shifts_execute() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = "
.text
.globl _lookup, _narrow, _widen
_lookup:
    ldr q0,[x0]
    ldr q1,[x1]
    tbl v2.16b,{v0.16b},v1.16b
    ext v2.16b,v2.16b,v2.16b,#3
    str q2,[x2]
    ret
_narrow:
    ldr q0,[x0]
    sqxtn v1.8b,v0.8h
    sqxtn2 v1.16b,v0.8h
    str q1,[x1]
    ret
_widen:
    ldr d0,[x0]
    sshll v1.8h,v0.8b,#2
    sshr v1.8h,v1.8h,#1
    str q1,[x1]
    ret
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("neon.o");
    let driver = temp.0.join("driver.c");
    let exe = temp.0.join("neon");
    std::fs::write(&object, keel_asm::assemble_object(source).unwrap()).unwrap();
    std::fs::write(
        &driver,
        "
#include <stdint.h>
extern void lookup(const uint8_t*,const uint8_t*,uint8_t*);
extern void narrow(const int16_t*,int8_t*),widen(const int8_t*,int16_t*);
int main(void){
    uint8_t a[16],idx[16]={15,0,16,255,7,8,9,10,11,12,13,14,1,2,3,4},o[16];
    for(int i=0;i<16;++i)a[i]=i+1;
    lookup(a,idx,o);
    for(int i=0;i<16;++i){unsigned j=idx[(i+3)%16];if(o[i]!=(j<16?a[j]:0))return 1;}
    int16_t b[8]={-32768,-129,-128,-1,0,127,128,32767};int8_t c[16];narrow(b,c);
    for(int i=0;i<16;++i){int v=b[i%8];if(c[i]!=(v< -128?-128:v>127?127:v))return 2;}
    int8_t d[8]={-128,-127,-1,0,1,63,126,127};int16_t e[8];widen(d,e);
    for(int i=0;i<8;++i)if(e[i]!=2*d[i])return 3;
    return 0;
}",
    )
    .unwrap();
    run(Command::new("clang")
        .args(["-O2"])
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}

#[test]
fn reductions_fp_and_modified_immediates_match_clang() {
    let mut s = String::new();
    for (size, e) in ["b", "h", "s"].into_iter().enumerate() {
        for q in 0..=1 {
            if size == 2 && q == 0 {
                continue;
            }
            let n = format!("v31.{}{e}", (8 << q) >> size);
            for op in ["addv", "smaxv", "sminv", "umaxv", "uminv"] {
                s.push_str(&format!("{op} {e}0,{n}\n"));
            }
            for op in ["saddlv", "uaddlv"] {
                s.push_str(&format!("{op} {}0,{n}\n", ["h", "s", "d"][size]));
            }
        }
    }
    s.push_str("addp d0,v31.2d\n");
    for op in ["fmaxv", "fminv", "fmaxnmv", "fminnmv"] {
        s.push_str(&format!("{op} s0,v31.4s\n"));
    }
    for (size, e) in [(2, "s"), (3, "d")] {
        for q in 0..=1 {
            if size == 3 && q == 0 {
                continue;
            }
            let arr = format!("{}{e}", (8 << q) >> size);
            for op in [
                "frintn", "frintm", "frintp", "frintz", "frinta", "frintx", "frinti", "fcvtns",
                "fcvtms", "fcvtas", "fcvtps", "fcvtzs", "fcvtnu", "fcvtmu", "fcvtau", "fcvtpu",
                "fcvtzu", "scvtf", "ucvtf", "frecpe", "frsqrte", "urecpe", "ursqrte",
            ] {
                if size == 3 && op.starts_with('u') && op != "ucvtf" {
                    continue;
                }
                s.push_str(&format!("{op} v31.{arr},v0.{arr}\n"));
            }
            for op in [
                "fmulx", "fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle", "facgt", "facge", "frecps",
                "frsqrts", "fabd", "faddp", "fmaxp", "fminp", "fmaxnmp", "fminnmp",
            ] {
                s.push_str(&format!("{op} v31.{arr},v0.{arr},v30.{arr}\n"));
            }
            for op in ["fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle"] {
                s.push_str(&format!("{op} v31.{arr},v0.{arr},#0.0\n"));
            }
            for op in ["scvtf", "ucvtf", "fcvtzs", "fcvtzu"] {
                for amt in [1, 8 << size] {
                    s.push_str(&format!("{op} v31.{arr},v0.{arr},#{amt}\n"));
                }
            }
            for value in ["0.125", "1.0", "-31.0"] {
                s.push_str(&format!("fmov v31.{arr},#{value}\n"));
            }
        }
        for op in [
            "fcvtns", "fcvtms", "fcvtas", "fcvtps", "fcvtzs", "fcvtnu", "fcvtmu", "fcvtau",
            "fcvtpu", "fcvtzu", "scvtf", "ucvtf", "frecpe", "frsqrte", "frecpx",
        ] {
            s.push_str(&format!("{op} {e}0,{e}31\n"));
        }
        for op in [
            "fmulx", "fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle", "facgt", "facge", "frecps",
            "frsqrts", "fabd",
        ] {
            s.push_str(&format!("{op} {e}31,{e}0,{e}30\n"));
        }
        for op in ["fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle"] {
            s.push_str(&format!("{op} {e}31,{e}0,#0.0\n"));
        }
        for op in ["faddp", "fmaxp", "fminp", "fmaxnmp", "fminnmp"] {
            s.push_str(&format!("{op} {e}0,v31.2{e}\n"));
        }
        for op in ["scvtf", "ucvtf", "fcvtzs", "fcvtzu"] {
            for amt in [1, 8 << size] {
                s.push_str(&format!("{op} {e}31,{e}0,#{amt}\n"));
            }
        }
    }
    for high in [false, true] {
        let suffix = if high { "2" } else { "" };
        let q = usize::from(high);
        for size in [1, 2] {
            let e = ["b", "h", "s", "d"][size];
            let w = ["b", "h", "s", "d"][size + 1];
            s.push_str(&format!(
                "fcvtn{suffix} v0.{}{e},v31.{}{w}\nfcvtl{suffix} v0.{}{w},v31.{}{e}\n",
                (8 << q) >> size,
                16 >> (size + 1),
                16 >> (size + 1),
                (8 << q) >> size
            ));
        }
        s.push_str(&format!("fcvtxn{suffix} v0.{}s,v31.2d\n", 2 << q));
    }
    s.push_str("fcvtxn s0,d31\n");
    for arr in ["4h", "8h", "2s", "4s"] {
        for op in ["mvni", "orr", "bic"] {
            for imm in [0, 255] {
                for shift in if arr.ends_with('h') {
                    vec![0, 8]
                } else {
                    vec![0, 8, 16, 24]
                } {
                    s.push_str(&format!("{op} v31.{arr},#{imm},lsl #{shift}\n"));
                }
            }
        }
        if arr.ends_with('s') {
            for amt in [8, 16] {
                s.push_str(&format!("mvni v0.{arr},#255,msl #{amt}\n"));
            }
        }
    }
    compare_clang(&s);
}

#[test]
fn indexed_arithmetic_matches_clang() {
    let mut s = String::new();
    for size in [1, 2, 3] {
        let e = ["b", "h", "s", "d"][size];
        let lanes = 16 >> size;
        for index in 0..lanes {
            for rm in if size == 1 { vec![0, 15] } else { vec![0, 31] } {
                let m = format!("v{rm}.{e}[{index}]");
                for q in 0..=1 {
                    if size == 3 && q == 0 {
                        continue;
                    }
                    let arr = format!("{}{e}", (8 << q) >> size);
                    let ops = if size < 3 {
                        vec!["mul", "mla", "mls", "sqdmulh", "sqrdmulh"]
                    } else {
                        vec![]
                    };
                    for op in ops {
                        s.push_str(&format!("{op} v31.{arr},v0.{arr},{m}\n"));
                    }
                    if size >= 2 {
                        for op in ["fmla", "fmls", "fmul", "fmulx"] {
                            s.push_str(&format!("{op} v31.{arr},v0.{arr},{m}\n"));
                        }
                    }
                }
                if size >= 2 {
                    for op in ["fmla", "fmls", "fmul", "fmulx"] {
                        s.push_str(&format!("{op} {e}31,{e}0,{m}\n"));
                    }
                }
                if size < 3 {
                    let w = ["s", "d"][size - 1];
                    for high in [false, true] {
                        let suffix = if high { "2" } else { "" };
                        let q = usize::from(high);
                        for op in [
                            "smlal", "umlal", "smlsl", "umlsl", "smull", "umull", "sqdmlal",
                            "sqdmlsl", "sqdmull",
                        ] {
                            s.push_str(&format!(
                                "{op}{suffix} v31.{}{w},v0.{}{e},{m}\n",
                                16 >> (size + 1),
                                (8 << q) >> size
                            ));
                        }
                    }
                    for op in ["sqdmlal", "sqdmlsl", "sqdmull"] {
                        s.push_str(&format!("{op} {w}31,{e}0,{m}\n"));
                    }
                    for op in ["sqdmulh", "sqrdmulh"] {
                        s.push_str(&format!("{op} {e}31,{e}0,{m}\n"));
                    }
                }
            }
        }
    }
    compare_clang(&s);
}

#[test]
fn structured_memory_forms_match_clang() {
    let mut s = String::new();
    for size in 0..4 {
        let e = ["b", "h", "s", "d"][size];
        for q in 0..=1 {
            let arr = format!("{}{e}", (8 << q) >> size);
            for len in 1..=4 {
                let list = (0..len)
                    .map(|i| format!("v{}.{arr}", (31 + i) % 32))
                    .collect::<Vec<_>>()
                    .join(",");
                for op in ["ld1", "st1"] {
                    for post in [
                        String::new(),
                        format!(",#{}", (8 << q) * len),
                        ",x30".into(),
                    ] {
                        s.push_str(&format!("{op} {{{list}}},[sp]{post}\n"));
                    }
                }
                if len >= 2 && (size != 3 || q == 1) {
                    for root in ["ld", "st"] {
                        for post in [
                            String::new(),
                            format!(",#{}", (8 << q) * len),
                            ",x30".into(),
                        ] {
                            s.push_str(&format!("{root}{len} {{{list}}},[x0]{post}\n"));
                        }
                    }
                }
                for post in [
                    String::new(),
                    format!(",#{}", (1 << size) * len),
                    ",x30".into(),
                ] {
                    s.push_str(&format!("ld{len}r {{{list}}},[sp]{post}\n"));
                }
            }
        }
        for index in 0..16 >> size {
            for len in 1..=4 {
                let list = (0..len)
                    .map(|i| format!("v{}.{e}", (31 + i) % 32))
                    .collect::<Vec<_>>()
                    .join(",");
                for root in ["ld", "st"] {
                    for post in [
                        String::new(),
                        format!(",#{}", (1 << size) * len),
                        ",x30".into(),
                    ] {
                        s.push_str(&format!("{root}{len} {{{list}}}[{index}],[x0]{post}\n"));
                    }
                }
            }
        }
    }
    s.push_str("ld1 {v0.16b-v3.16b},[sp],#64\nld4 {v31.h-v2.h}[7],[x0],x30\n");
    compare_clang(&s);
}

#[test]
fn every_baseline_neon_encoding_matches_clang() {
    let source = include_str!("neon-cases.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|line| line.split_once('\t').unwrap().1)
        .collect::<Vec<_>>()
        .join("\n");
    compare_clang(&source);
}

#[test]
fn invalid_fp_indexed_and_structure_operands_are_rejected() {
    for s in [
        "fmaxv d0,v1.2d",
        "addp s0,v1.2s",
        "saddlv h0,v1.4h",
        "fcvtxn v0.4h,v1.4s",
        "fcvtn2 v0.4h,v1.4s",
        "fcvtl v0.2d,v1.4s",
        "frecpx v0.4s,v1.4s",
        "urecpe v0.2d,v1.2d",
        "fcmeq v0.4s,v1.4s,#1.0",
        "fcmgt d0,d1,#-0.0",
        "scvtf v0.4s,v1.4s,#0",
        "fcvtzs v0.4s,v1.4s,#33",
        "fmov v0.2d,#0.0",
        "mvni v0.16b,#1",
        "orr v0.4s,#1,msl #8",
        "mul v0.8h,v1.8h,v16.h[0]",
        "fmla v0.4s,v1.4s,v2.s[4]",
        "fmla v0.2d,v1.2d,v2.s[0]",
        "smull2 v0.4s,v1.4h,v2.h[0]",
        "mul h0,h1,v2.h[0]",
        "sqdmulh2 v0.8h,v1.8h,v2.h[0]",
        "ld2 {v0.1d,v1.1d},[x0]",
        "ld3 {v0.16b,v1.16b},[x0]",
        "ld1 {v0.16b},[x0],#8",
        "ld2r {v0.4s,v1.4s},[x0],#32",
        "st1 {v0.s}[4],[x0]",
        "ld1 {v0.h}[8],[x0]",
        "ld2 {v0.s,v2.s}[0],[x0]",
        "ld1 {v0.s}[0],[x0],#16",
        "ld1 {v0.16b},[x0],xzr",
        "st2 {v0.16b,v1.16b},[x0],w1",
        "ld1 {v0.16b},[x0,#1]",
        "st1 {v0.16b},[x0]!",
        "ld1r {v0.4s}[0],[x0]",
        "ld1 {v0.s,v1.s}[0],[x0]",
        "st1 {v0.16b},[xzr]",
    ] {
        let error = assemble(&format!("nop\n{s}")).expect_err(s);
        assert_eq!(error.line, 2, "{s}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn structured_memory_fp_conversions_and_indexed_arithmetic_execute() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let source = "
.text
.globl _structures, _scale, _convert, _reduce
_structures:
    ld2 {v0.4s,v1.4s},[x0],#32
    dup v2.4s,w2
    add v0.4s,v0.4s,v2.4s
    sub v1.4s,v1.4s,v2.4s
    st2 {v0.4s,v1.4s},[x1],#32
    ld1r {v2.4s},[x3]
    st1 {v2.s}[3],[x1]
    ret
_scale:
    ldr q0,[x0]
    ldr s1,[x1]
    fmul v0.4s,v0.4s,v1.s[0]
    fmaxv s0,v0.4s
    ret
_convert:
    ldr q0,[x0]
    fcvtzs v1.4s,v0.4s
    str q1,[x1]
    scvtf v2.4s,v1.4s
    faddp s0,v2.2s
    ret
_reduce:
    ldr q0,[x0]
    uaddlv d0,v0.4s
    fmov x0,d0
    ret
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("neon-full.o");
    let driver = temp.0.join("driver.c");
    let exe = temp.0.join("neon-full");
    std::fs::write(&object, keel_asm::assemble_object(source).unwrap()).unwrap();
    std::fs::write(
        &driver,
        "
#include <stdint.h>
extern const uint32_t* structures(const uint32_t*,uint32_t*,unsigned,const uint32_t*);
extern float scale(const float*,const float*),convert(const float*,int32_t*);
extern uint64_t reduce(const uint32_t*);
int main(void){
    uint32_t a[8]={0,1,2,3,0xfffffffe,0xffffffff,9,10},b[9],v=0x12345678;
    if(structures(a,b,7,&v)!=a+8)return 1;
    for(int i=0;i<8;++i)if(b[i]!=((i&1)?a[i]-7:a[i]+7))return 2;
    if(b[8]!=v)return 3;
    float x[4]={-3.75f,2.5f,7.0f,-0.0f},factor=2;int32_t y[4];
    if(scale(x,&factor)!=14.0f || convert(x,y)!=-1.0f)return 4;
    for(int i=0;i<4;++i)if(y[i]!=(int32_t)x[i])return 5;
    uint32_t z[4]={0xffffffff,0xffffffff,1,2};if(reduce(z)!=UINT64_C(0x200000001))return 6;
    return 0;
}",
    )
    .unwrap();
    run(Command::new("clang")
        .args(["-O2"])
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}
