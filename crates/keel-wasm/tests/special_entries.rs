#[test]
fn entry_contracts_cannot_fall_through_to_the_ordinary_abi() {
    for policy in ["naked", "interrupt"] {
        let module = keel_ir::parse(&format!("decl @entry() {policy}\n")).unwrap();
        let error = keel_wasm::emit_module(&module, &Default::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains(policy) && message.contains("not implemented"));
    }
}
