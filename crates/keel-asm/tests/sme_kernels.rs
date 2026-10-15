use keel_asm::TargetFeatures;
mod common;
const SOURCES: [&str; 3] = [
    include_str!("fixtures/sme-kernels/fp32.s"),
    include_str!("fixtures/sme-kernels/bf16.s"),
    include_str!("fixtures/sme-kernels/int8.s"),
];
#[test]
fn tail_aware_matrix_kernels_match_clang_with_just_sme() {
    for source in SOURCES {
        common::compare_clang_with_features(
            source,
            TargetFeatures::default().with_sme(),
            &["-march=armv8-a+sme"],
        );
        assert!(keel_asm::assemble(source).is_err());
    }
    common::compare_clang("nop");
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn matrix_kernels_match_scalar_references_for_irregular_dimensions() {
    use common::{Temp, execute, run};
    use std::process::Command;
    let flag = Command::new("sysctl")
        .args(["-n", "hw.optional.arm.FEAT_SME"])
        .output()
        .unwrap();
    if !flag.status.success() || flag.stdout != b"1\n" {
        eprintln!("skipping matrix execution: SME not advertised");
        return;
    }
    let probe = ".text\n.globl _streaming_lanes\n_streaming_lanes:\nsub sp,sp,#64\nstp d8,d9,[sp]\nstp d10,d11,[sp,#16]\nstp d12,d13,[sp,#32]\nstp d14,d15,[sp,#48]\nsmstart sm\ncntw x0\nsmstop sm\nldp d8,d9,[sp]\nldp d10,d11,[sp,#16]\nldp d12,d13,[sp,#32]\nldp d14,d15,[sp,#48]\nadd sp,sp,#64\nret\n";
    let source = SOURCES
        .into_iter()
        .chain([probe])
        .collect::<Vec<_>>()
        .join("\n");
    let temp = Temp::new();
    let object = temp.0.join("kernels.o");
    let driver = temp.0.join("driver.c");
    let executable = temp.0.join("kernels");
    std::fs::write(
        &object,
        keel_asm::assemble_object_with_features(&source, TargetFeatures::default().with_sme())
            .unwrap(),
    )
    .unwrap();
    std::fs::write(&driver, include_str!("fixtures/sme-kernels/driver.c")).unwrap();
    run(Command::new("clang")
        .arg("-O2")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg(&driver)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
