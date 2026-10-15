mod common;
use common::{Temp, run};
use std::process::Command;

fn build_command(bytes: &[u8]) -> &[u8] {
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut at = 32;
    for _ in 0..word(16) {
        let size = word(at + 4) as usize;
        if word(at) == 0x32 {
            return &bytes[at..at + size];
        }
        at += size;
    }
    panic!("missing LC_BUILD_VERSION");
}

#[test]
fn deployment_metadata_matches_clang() {
    let temp = Temp::new();
    for directive in [
        ".build_version macos, 13, 2, 1 sdk_version 27, 0",
        ".build_version ios, 17, 0 sdk_version 18, 1, 2",
    ] {
        let source = format!("{directive}\n.text\nret\n");
        let input = temp.0.join("input.s");
        let output = temp.0.join("clang.o");
        std::fs::write(&input, &source).unwrap();
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(input)
            .arg("-o")
            .arg(&output));
        let reference = std::fs::read(output).unwrap();
        let actual = keel_asm::assemble_object(&source).unwrap();
        assert_eq!(build_command(&actual), build_command(&reference));
    }
    for source in [
        ".build_version macos, 1",
        ".build_version macos, 1, 256",
        ".build_version macos, 65536, 0",
        ".build_version nonsense, 1, 0",
        ".build_version macos, 11, 0\n.build_version macos, 12, 0",
        ".build_version macos, 11, 0 sdk_version 27",
    ] {
        assert!(keel_asm::assemble(source).is_err(), "{source}");
    }
}

#[test]
fn unchanged_clang_simple_source() {
    let temp = Temp::new();
    let input = temp.0.join("input.c");
    let assembly = temp.0.join("input.s");
    std::fs::write(&input, "long f(long x) { return x * 17 + 3; }\n").unwrap();
    run(Command::new("clang")
        .args([
            "-S",
            "-O2",
            "-target",
            "arm64-apple-macosx11.0.0",
            "-mcpu=apple-m5",
        ])
        .arg(input)
        .arg("-o")
        .arg(&assembly));
    let source = std::fs::read_to_string(assembly).unwrap();
    common::compare_clang_with_features(
        &source,
        keel_asm::TargetFeatures::apple_m5(),
        &["-mcpu=apple-m5"],
    );
    keel_asm::assemble_object_with_features(&source, keel_asm::TargetFeatures::apple_m5()).unwrap();
}
