#[test]
fn entry_contracts_cannot_fall_through_to_the_ordinary_abi() {
    for policy in ["naked", "interrupt"] {
        let module = keel_ir::parse(&format!("decl @entry() {policy}\n")).unwrap();
        let error = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap_err();
        assert!(error.contains(policy) && error.contains("not implemented"));
    }
}
