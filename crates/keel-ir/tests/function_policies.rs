use keel_ir::{parse, verify_module};

#[test]
fn optimization_strings_and_entry_contracts_roundtrip_without_loss() {
    for flags in [
        "hot flatten optimize(3, \"no-inline,Os\") optimize(\"x\\\"y\\\\z\") no_instrument_function no_profile_instrument_function",
        "cold optnone optimize(\"Os\", 1, \"no-unroll-loops\")",
        "naked",
        "interrupt",
    ] {
        let source =
            format!("decl @outside() {flags}\nfunc @handler() {flags} {{\nblock0:\n return\n}}\n");
        let module = parse(&source).unwrap();
        verify_module(&module).unwrap();
        let text = module.to_string();
        let again = parse(&text).unwrap();
        verify_module(&again).unwrap();
        assert_eq!(again.to_string(), text);
        assert_eq!(module.functions[0].attrs, again.functions[0].attrs);
        assert_eq!(module.decls[0].attrs, again.decls[0].attrs);
    }
}

#[test]
fn conflicting_policies_and_stale_callee_metadata_are_rejected() {
    for flags in ["hot cold", "naked interrupt", "interrupt alwaysinline"] {
        let module = parse(&format!("decl @f() {flags}\n")).unwrap();
        assert!(verify_module(&module).is_err(), "{flags}");
    }
    for flags in [
        "cold cold",
        "optimize()",
        "optimize(-1)",
        "optimize(\"Os\",)",
    ] {
        assert!(parse(&format!("decl @f() {flags}\n")).is_err(), "{flags}");
    }
    let mut module =
        parse("decl @g() cold optimize(\"Os\")\nfunc @f() {\nblock0:\n call @g()\n return\n}\n")
            .unwrap();
    let r = module.functions[0].callees().next().unwrap().0;
    module.functions[0].callee_mut(r).attrs.optimize = Default::default();
    assert!(verify_module(&module).is_err());
    module.sync_function_attrs();
    verify_module(&module).unwrap();
}
