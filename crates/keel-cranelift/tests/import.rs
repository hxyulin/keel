//! CLIF import verification and rejection boundaries.
use keel_cranelift::{OptLevel, Options, TargetProfile, import};
fn options() -> Options {
    Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    }
}
#[test]
fn scalar_cfg_survives_both_verifiers_and_roundtrip() {
    let src = include_str!("../tools/ir-fixtures/scalar.clif");
    let text = import::to_keel(src, "probe", &options()).unwrap();
    let m = keel_ir::parse(&text).unwrap();
    let clif = keel_cranelift::translate_to_clif(&m, &options()).unwrap();
    let again = import::to_keel(&clif, "probe", &options()).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&again).unwrap()).unwrap();
}
#[test]
fn unsupported_opcode_has_context() {
    let src = "function %f(i64, i64) -> i64 apple_aarch64 {\nblock0(v0:i64,v1:i64):\n v2 = udiv v0, v1\n return v2\n}";
    let error = import::to_keel(src, "f", &options()).unwrap_err();
    assert!(
        error.contains("block0") && error.contains("udiv"),
        "{error}"
    );
}
#[test]
fn malformed_and_wrong_abi_are_rejected() {
    assert!(import::to_keel("function ???", "f", &options()).is_err());
    let src = "function %f(i64) -> i64 system_v {\nblock0(v0:i64):\n return v0\n}";
    assert!(
        import::to_keel(src, "f", &options())
            .unwrap_err()
            .contains("calling convention")
    );
    assert!(
        import::to_keel("", "f", &options())
            .unwrap_err()
            .contains("exactly one")
    );
}
