#![allow(dead_code)]
use crate::common;
use keel_asm::{TargetFeatures, assemble, assemble_with_features};

const PAC: &str = "paciasp\npacibsp\nautiasp\nautibsp\npacia1716\npacib1716\nautia1716\nautib1716\npaciaz\npacibz\nautiaz\nautibz\nxpaclri\npacia x0, sp\npacib x1, x2\npacda x3, x4\npacdb x5, x6\nautia x7, x8\nautib x9, x10\nautda x11, x12\nautdb x13, x14\npaciza x0\npacizb x1\npacdza x2\npacdzb x3\nautiza x4\nautizb x5\nautdza x6\nautdzb x7\nxpaci x8\nxpacd x9\npacga x0, x1, sp\nbraa x0, sp\nbrab x1, x2\nblraa x3, x4\nblrab x5, sp\nbraaz x0\nbrabz x1\nblraaz x2\nblrabz x3\nretaa\nretab\n";
const CRYPTO: &str = "pmull v0.1q, v1.1d, v2.1d\npmull2 v3.1q, v4.2d, v5.2d\naese v0.16b, v1.16b\naesd v31.16b, v30.16b\naesmc v2.16b, v3.16b\naesimc v4.16b, v5.16b\nsha1c q0, s1, v2.4s\nsha1p q3, s4, v5.4s\nsha1m q6, s7, v8.4s\nsha1su0 v9.4s, v10.4s, v11.4s\nsha1h s12, s13\nsha1su1 v14.4s, v15.4s\nsha256h q0, q1, v2.4s\nsha256h2 q3, q4, v5.4s\nsha256su0 v6.4s, v7.4s\nsha256su1 v8.4s, v9.4s, v10.4s\ncrc32b w0, w1, w2\ncrc32h w3, w4, w5\ncrc32w w6, w7, w8\ncrc32x w9, w10, x11\ncrc32cb w12, w13, w14\ncrc32ch w15, w16, w17\ncrc32cw w18, w19, w20\ncrc32cx wzr, w30, xzr\n";
#[test]
fn families_match_clang() {
    let features = TargetFeatures::apple_m5();
    common::compare_clang_with_features(PAC, features, &["-mcpu=apple-m5"]);
    common::compare_clang_with_features(CRYPTO, features, &["-mcpu=apple-m5"]);
    common::compare_clang_with_features(
        "bti\nbti c\nbti j\nbti jc\nmrs x0, tpidrro_el0\nmrs x1, tpidr_el0\nmrs x2, nzcv\nmsr nzcv, x3\nmrs x4, fpcr\nmsr fpsr, x5\nmrs x6, cntvct_el0\nmrs x7, cntfrq_el0\nmrs x8, ctr_el0\nmrs x9, dczid_el0\ndc cvau, x0\ndc cvac, x1\ndc civac, x2\ndc zva, x3\nic ivau, x4\nic iallu\nic ialluis\nsys #3, c7, c11, #1, x0\n",
        features,
        &["-mcpu=apple-m5"],
    );
    common::compare_clang(
        "casp x0, x1, x2, x3, [x4]\ncaspa w2, w3, w4, w5, [sp]\ncaspl x30, xzr, x28, x29, [x1]\ncaspal w0, w1, w30, wzr, [x2]\nprfm pldl1keep, [x0, #64]\nprfm pldl3strm, [sp, #32760]\nprfm pstl2keep, [x2, x3, lsl #3]\nprfm plil1keep, [x4, w5, sxtw]\nprfm #31, [x0]\nprfum pldl1strm, [x0, #-256]\nprfum #0, [sp, #255]\nprfm pldl1keep, Llit\nLlit:\nret\n",
    );
}
#[test]
fn gates_and_invalid_operands() {
    for source in PAC.lines().chain(CRYPTO.lines()).chain(["bti c"]) {
        assert!(assemble(source).is_err(), "missing gate: {source}");
    }
    for (source, features) in [
        ("paciasp", TargetFeatures::default().with_pauth()),
        ("bti c", TargetFeatures::default().with_bti()),
        ("aese v0.16b, v1.16b", TargetFeatures::default().with_aes()),
        ("sha1h s0, s1", TargetFeatures::default().with_sha1()),
        (
            "sha256h q0, q1, v2.4s",
            TargetFeatures::default().with_sha256(),
        ),
        ("crc32x w0, w1, x2", TargetFeatures::default().with_crc32()),
    ] {
        assert!(assemble_with_features(source, features).is_ok(), "{source}");
    }
    for source in [
        "paciasp x0",
        "pacia w0, x1",
        "pacia sp, x0",
        "bti cj",
        "bti c, j",
        "aese v0.8b, v1.8b",
        "sha256h q0, v1.4s, v2.4s",
        "sha1h d0, d1",
        "crc32x x0, w1, x2",
        "crc32x w0, w1, w2",
        "mrs w0, tpidrro_el0",
        "mrs x0, invented_el0",
        "dc cvau, w0",
        "ic ivau",
        "ic iallu, x0",
        "casp x1, x2, x4, x5, [x0]",
        "casp x0, x2, x4, x5, [x0]",
        "casp x0, x1, w4, w5, [x0]",
        "casp x0, x1, x4, x5, [x0, #8]",
        "prfm pldl1keep, [x0, #1]",
        "prfm pldl1keep, [x0, #32768]",
        "prfm pldl1keep, [x0, #8]!",
        "prfm pldl1keep, [x0, x1, lsl #2]",
        "prfum #0, [x0, #256]",
        "prfm #32, [x0]",
    ] {
        assert!(
            assemble_with_features(source, TargetFeatures::apple_m5()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn compiler_cssc_and_pair_sign_extension_match_clang() {
    let source = "smax w0, w1, w2\nsmin x0, x1, x2\numax x0, x1, #255\numin w0, w1, #0\nsmin w0, w1, #-128\nsmax x0, x1, #127\ncnt w0, w1\nctz x0, x1\nabs w0, w1\n";
    common::compare_clang_with_features(source, TargetFeatures::apple_m5(), &["-mcpu=apple-m5"]);
    for line in source.lines() {
        assert!(assemble(line).is_err());
        assert!(assemble_with_features(line, TargetFeatures::default().with_cssc()).is_ok());
    }
    common::compare_clang(
        "ldpsw x0, x1, [x2, #252]\nldpsw x3, x4, [sp, #-256]!\nldpsw x5, x6, [x7], #4\nubfiz x0, x1, #4, #12\nsbfiz w0, w1, #16, #8\n",
    );
    for bad in [
        "ldpsw w0, w1, [x2]",
        "ldpsw x0, x0, [x2]",
        "ldpsw x0, x1, [x2, #2]",
        "smin w0, w1, #128",
        "umax x0, x1, #-1",
        "cnt x0, w1",
    ] {
        assert!(
            assemble_with_features(bad, TargetFeatures::apple_m5()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn scalar_conversion_rounding_and_rcpc_match_clang() {
    let mut source = String::new();
    use std::fmt::Write;
    for fp in ["s", "d"] {
        for gp in ["w", "x"] {
            for op in ["scvtf", "ucvtf"] {
                for scale in [1, if gp == "w" { 32 } else { 64 }] {
                    writeln!(&mut source, "{op} {fp}0, {gp}1, #{scale}").unwrap();
                }
            }
            for op in [
                "fcvtzs", "fcvtzu", "fcvtms", "fcvtmu", "fcvtns", "fcvtnu", "fcvtps", "fcvtpu",
                "fcvtas", "fcvtau",
            ] {
                writeln!(&mut source, "{op} {gp}0, {fp}1").unwrap();
                if matches!(op, "fcvtzs" | "fcvtzu") {
                    for scale in [1, if gp == "w" { 32 } else { 64 }] {
                        writeln!(&mut source, "{op} {gp}0, {fp}1, #{scale}").unwrap();
                    }
                }
            }
        }
        for op in ["fccmp", "fccmpe"] {
            writeln!(&mut source, "{op} {fp}0, {fp}1, #15, mi").unwrap();
        }
    }
    common::compare_clang(&source);
    let source = "ldapr x0, [x1]\nldapr w2, [sp]\nldaprb w3, [x4]\nldaprh w5, [x6]\n";
    common::compare_clang_with_features(source, TargetFeatures::apple_m5(), &["-mcpu=apple-m5"]);
    for line in source.lines() {
        assert!(assemble(line).is_err());
        assert!(assemble_with_features(line, TargetFeatures::default().with_lrcpc()).is_ok());
    }
    for bad in [
        "scvtf d0, w1, #0",
        "scvtf s0, w1, #33",
        "fcvtzu w0, d1, #0",
        "fcvtzs x0, d1, #65",
        "fccmp s0, d1, #0, eq",
        "fccmp s0, s1, #16, eq",
        "ldaprb x0, [x1]",
        "ldapr w0, [x1, #8]",
    ] {
        assert!(
            assemble_with_features(bad, TargetFeatures::apple_m5()).is_err(),
            "{bad}"
        );
    }
}
