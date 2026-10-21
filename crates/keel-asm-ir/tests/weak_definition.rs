use keel_asm_ir::Directive;

#[test]
fn weak_definition_is_a_validated_symbol_directive() {
    let directive = Directive::WeakDefinition("_odr".into());
    directive.validate().unwrap();
    assert!(directive.contains("_odr"));
    assert_eq!(directive.to_string(), ".weak_definition _odr");
    for symbol in ["", "Ltemporary", "_bad\n.text", "_bad,other"] {
        assert!(Directive::WeakDefinition(symbol.into()).validate().is_err());
    }
}
