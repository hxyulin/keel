use keel_ir::{DataLayout, Target};

#[test]
fn recognized_and_native_targets_are_distinct() {
    assert_eq!(
        Target::ALL,
        [
            Target::Aarch64AppleDarwin,
            Target::X86_64LinuxGnu,
            Target::Aarch64LinuxGnu,
            Target::Wasm32UnknownUnknown
        ]
    );
    assert_eq!(
        Target::NATIVE,
        [
            Target::Aarch64AppleDarwin,
            Target::X86_64LinuxGnu,
            Target::Aarch64LinuxGnu
        ]
    );
    for target in Target::ALL {
        assert_eq!(Target::from_triple(target.triple()), Some(target));
    }
    assert_eq!(
        Target::Wasm32UnknownUnknown.triple(),
        "wasm32-unknown-unknown"
    );
    for unsupported in [
        "wasm32",
        "wasm32-wasi",
        "wasm32-wasip1",
        "wasm64-unknown-unknown",
        "wasm32-unknown-unknown-unknown",
        "WASM32-unknown-unknown",
    ] {
        assert_eq!(Target::from_triple(unsupported), None, "{unsupported}");
    }
    assert_eq!(
        Target::from_triple("aarch64-linux-gnu"),
        Some(Target::Aarch64LinuxGnu)
    );
}

#[test]
fn layout_is_explicit_and_host_defaults_stay_native() {
    const WASM: DataLayout = Target::Wasm32UnknownUnknown.data_layout();
    assert_eq!(WASM, DataLayout::WASM32);
    assert_eq!((WASM.pointer_bytes(), WASM.pointer_align()), (4, 4));
    for target in Target::NATIVE {
        assert_eq!(target.data_layout(), DataLayout::NATIVE64);
        assert_eq!(
            (
                target.data_layout().pointer_bytes(),
                target.data_layout().pointer_align()
            ),
            (8, 8)
        );
    }
    if let Some(host) = Target::host() {
        assert!(Target::NATIVE.contains(&host));
        assert_eq!(Target::default(), host);
    } else {
        assert_eq!(Target::default(), Target::Aarch64AppleDarwin);
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_eq!(Target::host(), Some(Target::Aarch64AppleDarwin));
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    assert_eq!(Target::host(), Some(Target::Aarch64LinuxGnu));
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    assert_eq!(Target::host(), Some(Target::X86_64LinuxGnu));
}

#[test]
fn native_section_validation_rejects_even_an_empty_wasm_module() {
    let target = Target::Wasm32UnknownUnknown;
    let module = keel_ir::parse("").unwrap();
    let error = keel_ir::section::validate_module(&module, target).unwrap_err();
    assert!(error.contains("unsupported target wasm32-unknown-unknown"));
    let section = keel_ir::SectionName::new(".data").unwrap();
    assert!(
        section
            .validate(target, keel_ir::section::Placement::Data)
            .unwrap_err()
            .contains("unsupported target wasm32-unknown-unknown")
    );
}
