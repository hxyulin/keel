//! Instruction families used by unchanged optimized Clang/rustc output.
#![allow(dead_code)]
mod common;
use keel_asm::{TargetFeatures, assemble, assemble_with_features};
use std::fmt::Write;

const RCPC2: &[(&str, &str)] = &[
    ("ldapur", "w"),
    ("ldapur", "x"),
    ("ldapurb", "w"),
    ("ldapurh", "w"),
    ("ldapursb", "w"),
    ("ldapursb", "x"),
    ("ldapursh", "w"),
    ("ldapursh", "x"),
    ("ldapursw", "x"),
    ("stlur", "w"),
    ("stlur", "x"),
    ("stlurb", "w"),
    ("stlurh", "w"),
];

#[test]
fn lrcpc2_widths_registers_and_every_signed_byte_offset_match_clang() {
    let mut source = String::new();
    for &(op, width) in RCPC2 {
        for offset in -256..=255 {
            writeln!(&mut source, "{op} {width}0,[x30,#{offset}]").unwrap();
            writeln!(&mut source, "{op} {width}zr,[sp,#{offset}]").unwrap();
        }
        writeln!(&mut source, "{op} {width}30,[x0]").unwrap();
        writeln!(&mut source, "{op} {width}1,[sp,#0]").unwrap();
    }
    common::compare_clang_with_features(
        &source,
        TargetFeatures::default().with_lrcpc2(),
        &["-march=armv8.4-a"],
    );
    common::compare_clang_with_features(
        "ldapur x0,[x1,#16]\nstlur x10,[x8,#8]",
        TargetFeatures::apple_m5(),
        &["-mcpu=apple-m5"],
    );
}

#[test]
fn lrcpc2_has_an_independent_gate_and_preserves_lrcpc() {
    for &(op, width) in RCPC2 {
        let source = format!("{op} {width}0,[x1,#-1]");
        for features in [
            TargetFeatures::default(),
            TargetFeatures::default().with_lrcpc(),
        ] {
            let error = assemble_with_features(&source, features).unwrap_err();
            assert!(error.message.contains("FEAT_LRCPC2"), "{error}");
        }
        let bytes = assemble_with_features(&source, TargetFeatures::default().with_lrcpc2())
            .unwrap()
            .text;
        assert_eq!(
            bytes,
            assemble_with_features(&source, TargetFeatures::apple_m5())
                .unwrap()
                .text
        );
    }
    for source in ["ldapr x0,[x1]", "ldaprb w0,[sp]", "ldaprh w0,[x1]"] {
        assert!(assemble(source).is_err());
        assert!(assemble_with_features(source, TargetFeatures::default().with_lrcpc2()).is_ok());
    }
    // Direct and typed instruction APIs must enforce the same ISA predicate.
    use keel_asm_ir::{FieldId::*, Instruction, Opcode, Program};
    let instruction = Instruction::new(
        Opcode::LDAPUR_64_ldapstl_unscaled,
        [(Rt, 0), (Rn, 1), (imm9, 511)],
    );
    assert!(
        keel_asm::encode_instruction(&instruction, TargetFeatures::default().with_lrcpc())
            .unwrap_err()
            .contains("FEAT_LRCPC2")
    );
    let mut program = Program::new();
    program.instruction(instruction);
    assert_eq!(
        keel_asm::assemble_program(&program, TargetFeatures::default().with_lrcpc2())
            .unwrap()
            .text,
        assemble_with_features("ldapur x0,[x1,#-1]", TargetFeatures::apple_m5())
            .unwrap()
            .text,
    );
}

#[test]
fn lrcpc2_rejects_wrong_width_addressing_and_lrcpc3_forms() {
    for source in [
        "ldapur x0,[x1,#-257]",
        "ldapur x0,[x1,#256]",
        "ldapurb w0,[x1,#512]",
        "stlur x0,[x1,#2048]",
        "ldapursb x0,[x1,#-1]!",
        "stlur x0,[x1],#1",
        "ldapur x0,[x1,x2]",
        "stlur x0,[x1,w2,uxtw]",
        "ldapur x0,[x1,#8,lsl #0]",
        "ldapur x0,[w1]",
        "ldapur x0,[xzr]",
        "ldapur sp,[x1]",
        "stlur wsp,[x1]",
        "ldapurb x0,[x1]",
        "ldapurh x0,[x1]",
        "stlurb x0,[x1]",
        "stlurh x0,[x1]",
        "ldapursw w0,[x1]",
        "ldapur q0,[x1]",
        "stlur d0,[x1]",
        "ldapur x32,[x1]",
        "ldapur x0,[x32]",
        "ldapur x0,[x1,#label]",
        "ldapur x0",
        "stlur x0,[x1],x2",
        "ldapur x0,[x1,#]",
    ] {
        assert!(
            assemble_with_features(source, TargetFeatures::apple_m5()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn apple_widening_multiply_vector_and_indexed_forms_match_clang() {
    let mut source = String::new();
    for op in [
        "smull", "umull", "smlal", "umlal", "smlsl", "umlsl", "sqdmull", "sqdmlal", "sqdmlsl",
    ] {
        for high in ["", "2"] {
            for shape in ["8h", "4s", "2d"] {
                if op.starts_with("sqd") && shape == "8h" {
                    continue;
                }
                writeln!(&mut source, "{op}{high}.{shape} v31,v20,v16").unwrap();
                let (m, lanes) = match shape {
                    "4s" => (15, 8),
                    "2d" => (31, 4),
                    _ => continue,
                };
                for lane in 0..lanes {
                    writeln!(&mut source, "{op}{high}.{shape} v31,v20,v{m}[{lane}]").unwrap();
                }
            }
        }
    }
    for op in ["mul", "mla", "mls"] {
        for shape in ["4h", "8h", "2s", "4s"] {
            writeln!(&mut source, "{op}.{shape} v31,v20,v15[3]").unwrap();
        }
    }
    common::compare_clang(&source);
}

#[test]
fn apple_fp_comparisons_preserve_zero_register_and_half_feature_semantics() {
    for (shapes, features, flags) in [
        (&["2s", "4s", "2d"][..], TargetFeatures::default(), &[][..]),
        (
            &["4h", "8h"][..],
            TargetFeatures::default().with_fp16(),
            &["-march=armv8.2-a+fp16"][..],
        ),
    ] {
        let mut source = String::new();
        for shape in shapes {
            for op in ["fcmeq", "fcmgt", "fcmge", "fcmlt", "fcmle"] {
                writeln!(&mut source, "{op}.{shape} v31,v20,#0.0").unwrap();
                writeln!(&mut source, "{op}.{shape} v31,v20,v16").unwrap();
            }
            for op in ["facgt", "facge"] {
                writeln!(&mut source, "{op}.{shape} v31,v20,v16").unwrap();
            }
        }
        common::compare_clang_with_features(&source, features, flags);
        if shapes[0] == "4h" {
            for line in source.lines() {
                assert!(
                    assemble(line)
                        .unwrap_err()
                        .message
                        .to_ascii_lowercase()
                        .contains("fp16")
                );
            }
        }
    }
}

#[test]
fn apple_new_suffixes_reject_invalid_arrangements_lanes_and_immediates() {
    for source in [
        "smlal2.2s v0,v1,v2",
        "smlal.8h v0,v1,v2[0]",
        "smlal.4s v0,v1,v16[0]",
        "smlal2.4s v0,v1,v15[8]",
        "smlal.2d v0,v1,v31[4]",
        "smlal.2d v0,v1,v32[0]",
        "smlal.4s v0[0],v1,v2",
        "smlal.4s v0,v1[0],v2",
        "sqdmull.8h v0,v1,v2",
        "smlal.4s v0,v1,v2.h[0]",
        "smlal.4s v0,v1,v2[-1]",
        "smlal.4s v0,v1,v2[0",
        "fcmgt.1d v0,v1,v2",
        "fcmlt.4s v0,v1,#-0.0",
        "fcmgt.4s v0,v1,#1.0",
        "fcmgt.4s v0,v1,v2[0]",
        "facge.4s v0,v1,#0.0",
        "fcmlt.4s v0,v1.4s,v2",
    ] {
        assert!(
            assemble_with_features(source, TargetFeatures::apple_m5()).is_err(),
            "{source}"
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_pair(source: &str, driver: &str, features: TargetFeatures) {
    use common::{Temp, execute, run};
    use std::process::Command;
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let main = temp.0.join("main.c");
    let reference = temp.0.join("reference.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&main, driver).unwrap();
    run(Command::new("clang")
        .args(["-c", "-mcpu=apple-m5"])
        .arg(input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(
        &ours,
        assemble_with_features(source, features)
            .unwrap()
            .to_macho()
            .unwrap(),
    )
    .unwrap();
    for object in [&reference, &ours] {
        let executable = temp.0.join("run");
        run(Command::new("clang")
            .arg("-std=c11")
            .arg(&main)
            .arg(object)
            .arg("-o")
            .arg(&executable));
        assert!(execute(&executable).success(), "{}", object.display());
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn lrcpc2_native_width_sign_extension_and_offset_boundaries() {
    let available = std::process::Command::new("sysctl")
        .args(["-n", "hw.optional.arm.FEAT_LRCPC2"])
        .output()
        .unwrap();
    if !available.status.success() || available.stdout != b"1\n" {
        eprintln!("LRCPC2 native execution requires hardware support");
        return;
    }
    let mut source = String::from(".text\n");
    let mut driver = String::from("#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n");
    let mut checks = String::new();
    for (form, &(op, width)) in RCPC2.iter().enumerate() {
        for (position, offset) in [-256, -1, 0, 1, 255].iter().enumerate() {
            let name = format!("check_{form}_{position}");
            let store = op.starts_with("st");
            writeln!(
                &mut source,
                ".globl _{name}\n_{name}:\n{op} {width}{},[x0,#{offset}]\nret",
                if store { 1 } else { 0 }
            )
            .unwrap();
            let bytes = if op.ends_with('b') {
                1
            } else if op.ends_with('h') {
                2
            } else if op.ends_with('w') || width == "w" {
                4
            } else {
                8
            };
            if store {
                writeln!(&mut driver, "extern void {name}(void *,uint64_t);").unwrap();
                writeln!(&mut checks, "memset(buffer,0xa5,sizeof buffer); memcpy(expected,buffer,sizeof buffer); memcpy(expected+512,&value,{bytes}); {name}(p-({offset}),value); if(memcmp(buffer,expected,sizeof buffer)) {{puts(\"{name} store width/offset\"); return 1;}}").unwrap();
            } else {
                writeln!(&mut driver, "extern uint64_t {name}(const void *);").unwrap();
                let cast = match (op, width) {
                    ("ldapursb", "w") => "(uint32_t)(int32_t)(int8_t)",
                    ("ldapursb", "x") => "(uint64_t)(int64_t)(int8_t)",
                    ("ldapursh", "w") => "(uint32_t)(int32_t)(int16_t)",
                    ("ldapursh", "x") => "(uint64_t)(int64_t)(int16_t)",
                    ("ldapursw", _) => "(uint64_t)(int64_t)(int32_t)",
                    _ if bytes == 1 => "(uint8_t)",
                    _ if bytes == 2 => "(uint16_t)",
                    _ if bytes == 4 => "(uint32_t)",
                    _ => "(uint64_t)",
                };
                writeln!(&mut checks, "for(unsigned v=0;v<sizeof values/sizeof values[0];++v) {{memset(buffer,0xa5,sizeof buffer);memcpy(p,&values[v],8); if({name}(p-({offset})) != {cast}values[v]) {{puts(\"{name} load width/sign/offset\");return 2;}}}}").unwrap();
            }
        }
    }
    writeln!(&mut driver, "int main(void) {{ _Alignas(16) unsigned char buffer[1024],expected[1024]; unsigned char *p=buffer+512; uint64_t value=UINT64_C(0xfedcba988765ff80); uint64_t values[]={{0,UINT64_MAX,UINT64_C(0xfedcba988765ff80),UINT64_C(0x012345670123017f)}}; {checks} return 0; }}").unwrap();
    native_pair(&source, &driver, TargetFeatures::default().with_lrcpc2());
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn apple_widening_native_signed_unsigned_upper_half_lanes_and_saturation() {
    let mut source = String::from(".text\n");
    let mut driver = String::from(
        r#"
#include <stdint.h>
#include <limits.h>
#include <stdio.h>
static __int128 sat(__int128 x,unsigned bits) {
    __int128 lo=-((__int128)1<<(bits-1)),hi=((__int128)1<<(bits-1))-1;
    return x<lo?lo:x>hi?hi:x;
}
"#,
    );
    let mut checks = String::new();
    for bits in [16, 32] {
        for op in [
            "smull", "umull", "smlal", "umlal", "smlsl", "umlsl", "sqdmull", "sqdmlal", "sqdmlsl",
        ] {
            for high in [false, true] {
                for indexed in [false, true] {
                    let wide = bits * 2;
                    let lanes = 128 / wide;
                    let shape = if bits == 16 { "4s" } else { "2d" };
                    let lane = if bits == 16 { 7 } else { 3 };
                    let name = format!("{op}_b{bits}_h{}_i{}", u8::from(high), u8::from(indexed));
                    let multiplier = if indexed {
                        format!("v2[{lane}]")
                    } else {
                        "v2".into()
                    };
                    writeln!(&mut source, ".globl _{name}\n_{name}:\nldr q0,[x1]\nldr q1,[x2]\nldr q2,[x3]\n{op}{}.{shape} v0,v1,{multiplier}\nstr q0,[x0]\nret", if high { "2" } else { "" }).unwrap();
                    writeln!(
                        &mut driver,
                        "extern void {name}(void *,const void *,const void *,const void *);"
                    )
                    .unwrap();
                    let start = if high { lanes } else { 0 };
                    let rhs = if indexed {
                        lane.to_string()
                    } else {
                        format!("i+{start}")
                    };
                    let unsigned = op.starts_with('u');
                    let saturating = op.starts_with("sqd");
                    let accum = !matches!(op, "smull" | "umull" | "sqdmull");
                    let subtract = op.contains("mlsl");
                    let a = if unsigned {
                        format!("(uint{bits}_t)a{bits}[i+{start}]")
                    } else {
                        format!("a{bits}[i+{start}]")
                    };
                    let b = if unsigned {
                        format!("(uint{bits}_t)b{bits}[{rhs}]")
                    } else {
                        format!("b{bits}[{rhs}]")
                    };
                    let acc = if unsigned {
                        format!("(uint{wide}_t)acc{wide}[i]")
                    } else {
                        format!("acc{wide}[i]")
                    };
                    let mut expected = format!("(__int128)({a})*({b})");
                    if saturating {
                        expected = format!("sat(2*({expected}),{wide})");
                    }
                    if accum {
                        expected =
                            format!("({acc}){}({expected})", if subtract { "-" } else { "+" });
                    }
                    if saturating {
                        expected = format!("sat({expected},{wide})");
                    }
                    writeln!(&mut checks, "{name}(out{wide},acc{wide},a{bits},b{bits}); for(unsigned i=0;i<{lanes};++i) if(out{wide}[i]!=(uint{wide}_t)({expected})) {{puts(\"{name} arithmetic/lane\");return 1;}}").unwrap();
                }
            }
        }
    }
    writeln!(
        &mut driver,
        r#"
int main(void) {{
    int16_t a16[]={{INT16_MIN,-5,30000,-19,32767,INT16_MIN,213,-301}};
    int16_t b16[]={{INT16_MIN,-13,27000,-33,1234,-1234,11,INT16_MIN}};
    int32_t a32[]={{INT32_MIN,-1234567,7654321,INT32_MIN}};
    int32_t b32[]={{INT32_MIN,-7654321,-1234567,INT32_MIN}};
    int32_t acc32[]={{INT32_MAX,INT32_MIN,42,-43}};
    int64_t acc64[]={{INT64_MAX,INT64_MIN}};
    uint32_t out32[4]; uint64_t out64[2];
    {checks}
    return 0;
}}
"#
    )
    .unwrap();
    native_pair(&source, &driver, TargetFeatures::default());
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn apple_fp_comparison_native_nan_signed_zero_and_infinity() {
    let mut source = String::from(".text\n");
    let mut driver = String::from("#include <stdint.h>\n#include <math.h>\n#include <stdio.h>\n");
    let mut checks = String::new();
    for (bits, shape, lanes) in [(32, "4s", 4), (64, "2d", 2)] {
        for (op, condition) in [
            ("fcmeq", "a==b"),
            ("fcmgt", "a>b"),
            ("fcmge", "a>=b"),
            ("fcmlt", "a<b"),
            ("fcmle", "a<=b"),
            ("facgt", "fabs(a)>fabs(b)"),
            ("facge", "fabs(a)>=fabs(b)"),
        ] {
            for zero in [false, true] {
                if zero && op.starts_with("fac") {
                    continue;
                }
                let name = format!("{op}_b{bits}_z{}", u8::from(zero));
                let rhs = if zero { "#0.0" } else { "v2" };
                writeln!(&mut source, ".globl _{name}\n_{name}:\nldr q1,[x1]\nldr q2,[x2]\n{op}.{shape} v0,v1,{rhs}\nstr q0,[x0]\nret").unwrap();
                writeln!(
                    &mut driver,
                    "extern void {name}(void *,const void *,const void *);"
                )
                .unwrap();
                writeln!(&mut checks, "for(unsigned t=0;t<4;++t) {{ {name}(out{bits},n{bits}[t],m{bits}[t]); for(unsigned i=0;i<{lanes};++i) {{ double a=n{bits}[t][i], b={}; uint{bits}_t expected=({condition})?UINT{bits}_MAX:0; if(out{bits}[i]!=expected) {{puts(\"{name} compare\");return 1;}} }} }}", if zero { "0.0".into() } else { format!("m{bits}[t][i]") }).unwrap();
            }
        }
    }
    writeln!(&mut driver, r#"
int main(void) {{
    float n32[4][4]={{{{-0.0f,-3.0f,NAN,INFINITY}},{{-INFINITY,0.0f,4.0f,-4.0f}},{{1,2,3,4}},{{-1,-2,-3,-4}}}};
    float m32[4][4]={{{{0.0f,-4.0f,1,INFINITY}},{{INFINITY,NAN,-4.0f,4.0f}},{{2,2,2,2}},{{-2,-2,-2,-2}}}};
    double n64[4][2]={{{{-0.0,-3}},{{NAN,INFINITY}},{{-INFINITY,0}},{{4,-4}}}};
    double m64[4][2]={{{{0,-4}},{{1,INFINITY}},{{INFINITY,NAN}},{{-4,4}}}};
    uint32_t out32[4]; uint64_t out64[2];
    {checks}
    return 0;
}}
"#).unwrap();
    native_pair(&source, &driver, TargetFeatures::default());
}
