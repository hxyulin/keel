#[test]
fn entry_contracts_cannot_fall_through_to_the_ordinary_abi() {
    let options = keel_cranelift::Options {
        target: keel_cranelift::TargetProfile::AppleArm64,
        opt: keel_cranelift::OptLevel::None,
        verify_clif: true,
    };
    for policy in ["naked", "interrupt"] {
        let module = keel_ir::parse(&format!("decl @entry() {policy}\n")).unwrap();
        let error = keel_cranelift::translate_to_clif(&module, &options).unwrap_err();
        assert!(error.contains(policy) && error.contains("not implemented"));
    }
}
