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

#[test]
fn ordinary_weak_reference_and_elf_binding_are_validated_directives() {
    let reference = Directive::WeakReference("_optional".into());
    reference.validate().unwrap();
    assert_eq!(reference.to_string(), ".weak_reference _optional");
    assert!(reference.contains("optional"));
    let elf = Directive::ElfWeak("optional".into());
    elf.validate().unwrap();
    assert_eq!(elf.to_string(), ".weak optional");
    assert!(elf.contains("optional"));
    for name in ["", "Ltemporary", "_bad\n.text", "_bad,other"] {
        assert!(Directive::WeakReference(name.into()).validate().is_err());
    }
}
