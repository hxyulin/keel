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
    s.push_str("tbl v0.16b, {v31.16b-v1.16b}, v2.16b\nmov d0, v31.d[1]\n");
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
