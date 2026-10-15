use keel_asm::{TargetFeatures, assemble, assemble_with_features};
mod common;
use common::compare_clang_with_features;

const SVE_FLAGS: &[&str] = &["-march=armv8-a+sve"];
const SME_FLAGS: &[&str] = &["-march=armv8-a+sme"];

fn shared_source() -> String {
    let mut s = String::new();
    for size in ["b", "h", "s", "d"] {
        for op in ["add", "sub"] {
            s.push_str(&format!("{op} z0.{size}, z31.{size}, z30.{size}\n"));
        }
        for op in ["fadd", "fsub", "fmul"] {
            if size != "b" {
                s.push_str(&format!("{op} z31.{size}, z0.{size}, z30.{size}\n"));
            }
        }
        for pattern in [
            "pow2", "vl1", "vl2", "vl3", "vl4", "vl5", "vl6", "vl7", "vl8", "vl16", "vl32", "vl64",
            "vl128", "vl256", "mul4", "mul3", "all", "#0", "#31",
        ] {
            s.push_str(&format!("ptrue p15.{size}, {pattern}\n"));
        }
        s.push_str(&format!("ptrue p0.{size}\n"));
        for value in [-128, -1, 0, 127] {
            s.push_str(&format!("dup z31.{size}, #{value}\n"));
            if size != "b" {
                s.push_str(&format!("dup z0.{size}, #{value}, lsl #8\n"));
            }
        }
        if size != "b" {
            for value in [-32768, -256, 256, 32512] {
                s.push_str(&format!("dup z0.{size}, #{value}\n"));
            }
        }
        let p = if size == "d" { "x" } else { "w" };
        s.push_str(&format!(
            "lastb {p}0, p0, z31.{size}\nlastb {p}zr, p7, z0.{size}\n"
        ));
    }
    for op in ["and", "orr", "eor"] {
        s.push_str(&format!("{op} z31.d, z0.d, z30.d\n"));
    }
    for n in -32..=31 {
        s.push_str(&format!("rdvl x30, #{n}\n"));
    }
    for op in ["cntb", "cnth", "cntw", "cntd"] {
        for pattern in ["all", "pow2", "vl3", "mul4", "#31"] {
            for n in [1, 16] {
                s.push_str(&format!("{op} xzr, {pattern}, mul #{n}\n"));
            }
        }
        s.push_str(&format!("{op} x0\n"));
    }
    s.push_str("pfalse p0.b\npfalse p15.b\nADD Z31.S, Z0.S, Z1.S\n");
    s
}

#[test]
fn shared_sve_and_streaming_sme_forms_match_clang() {
    let source = shared_source();
    compare_clang_with_features(&source, TargetFeatures::default().with_sve(), SVE_FLAGS);
    compare_clang_with_features(&source, TargetFeatures::default().with_sme(), SME_FLAGS);
}

#[test]
fn sme_state_control_and_single_precision_outer_products_match_clang() {
    let mut source = String::from("zero {za}\n");
    for op in ["smstart", "smstop"] {
        for option in ["", "sm", "za"] {
            source.push_str(&format!("{op} {option}\n"));
        }
    }
    for op in ["fmopa", "fmops"] {
        for tile in 0..4 {
            for p in 0..8 {
                source.push_str(&format!(
                    "{op} za{tile}.s, p{p}/m, p{}/m, z31.s, z0.s\n",
                    7 - p
                ));
            }
        }
    }
    compare_clang_with_features(&source, TargetFeatures::default().with_sme(), SME_FLAGS);
}

#[test]
fn optional_features_are_explicit_and_do_not_leak() {
    common::compare_clang("add v0.4s, v1.4s, v2.4s");
    let shared = "add z0.s, z1.s, z2.s";
    for features in [
        TargetFeatures::default().with_sve(),
        TargetFeatures::default().with_sme(),
    ] {
        assert!(assemble_with_features(shared, features).is_ok());
        assert_eq!(
            assemble_with_features("add v0.4s, v1.4s, v2.4s", features).unwrap(),
            assemble("add v0.4s, v1.4s, v2.4s").unwrap()
        );
    }
    for source in shared_source().lines().filter(|s| !s.is_empty()) {
        let error = assemble(source).expect_err(source);
        assert!(error.message.contains("FEAT_SVE"), "{error}");
        assert!(error.message.contains("FEAT_SME"), "{error}");
    }
    for source in [
        "smstart",
        "smstop",
        "zero {za}",
        "fmopa za0.s, p0/m, p1/m, z0.s, z1.s",
    ] {
        for features in [
            TargetFeatures::default(),
            TargetFeatures::default().with_sve(),
        ] {
            assert!(
                assemble_with_features(source, features)
                    .unwrap_err()
                    .message
                    .contains("FEAT_SME")
            );
        }
    }
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                assert!(
                    assemble_with_features(shared, TargetFeatures::default().with_sme()).is_ok()
                );
                assert!(assemble(shared).is_err());
            });
        }
    });
}

#[test]
fn invalid_or_out_of_scope_scalable_forms_are_diagnosed() {
    let features = TargetFeatures::default().with_sme().with_sve();
    for bad in [
        "add z32.s, z0.s, z1.s",
        "add z0.s, z1.d, z2.s",
        "add z0.4s, z1.4s, z2.4s",
        "add z0.s, z1.s, #1",
        "fadd z0.b, z1.b, z2.b",
        "and z0.s, z1.s, z2.s",
        "ptrue p16.s",
        "ptrue p0.s, vl9",
        "ptrue p0.s, #32",
        "pfalse p0.s",
        "rdvl w0, #1",
        "rdvl x0, #32",
        "rdvl x0, #-33",
        "rdvl sp, #1",
        "cntb w0",
        "cntb x0, all, mul #0",
        "cntw x0, all, mul #17",
        "cntb x0, mul #2",
        "dup z0.b, #1, lsl #8",
        "dup z0.s, #128",
        "dup z0.s, #-129",
        "dup z0.s, #128, lsl #8",
        "dup z0.s, #1, lsl #16",
        "lastb w0, p8, z0.s",
        "lastb x0, p0, z0.s",
        "lastb w0, p0/m, z0.s",
        "smstart all",
        "smstop #1",
        "zero {za0.s}",
        "fmopa za4.s, p0/m, p0/m, z0.s, z1.s",
        "fmopa za0.d, p0/m, p0/m, z0.d, z1.d",
        "fmops za0.s, p8/m, p0/m, z0.s, z1.s",
        "fmopa za0.s, p0/z, p1/m, z0.s, z1.s",
        "ptrue pn8.s",
    ] {
        let error = assemble_with_features(&format!("nop\n{bad}"), features).expect_err(bad);
        assert_eq!(error.line, 2, "{bad}: {error}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn streaming_sme_executes_only_when_advertised_by_os() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let supported = Command::new("sysctl")
        .args(["-n", "hw.optional.arm.FEAT_SME"])
        .output()
        .unwrap();
    if !supported.status.success() || supported.stdout != b"1\n" {
        eprintln!("skipping SME execution: OS does not advertise FEAT_SME");
        return;
    }
    // Ordinary PCS leaf: preserve D8-D15 across mode transitions, enter with
    // inactive ZA, make no calls while streaming, leave with SM/ZA cleared.
    let source = "
.text
.globl _streaming
_streaming:
    sub sp, sp, #64
    stp d8, d9, [sp]
    stp d10, d11, [sp, #16]
    stp d12, d13, [sp, #32]
    stp d14, d15, [sp, #48]
    smstart
    zero {za}
    ptrue p0.s
    dup z0.s, #7
    dup z1.s, #6
    add z2.s, z0.s, z1.s
    lastb w0, p0, z2.s
    rdvl x1, #1
    cntw x2, all, mul #4
    cmp x1, x2
    csel x0, x0, xzr, eq
    fmopa za0.s, p0/m, p0/m, z0.s, z1.s
    fmops za0.s, p0/m, p0/m, z0.s, z1.s
    smstop
    ldp d8, d9, [sp]
    ldp d10, d11, [sp, #16]
    ldp d12, d13, [sp, #32]
    ldp d14, d15, [sp, #48]
    add sp, sp, #64
    ret
";
    let features = TargetFeatures::default().with_sme();
    compare_clang_with_features(source, features, SME_FLAGS);
    let temp = Temp::new();
    let object = temp.0.join("streaming.o");
    let driver = temp.0.join("driver.c");
    let executable = temp.0.join("streaming");
    std::fs::write(
        &object,
        keel_asm::assemble_object_with_features(source, features).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &driver,
        "extern long streaming(void); int main(void) { return streaming() != 13; }",
    )
    .unwrap();
    run(Command::new("clang")
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
