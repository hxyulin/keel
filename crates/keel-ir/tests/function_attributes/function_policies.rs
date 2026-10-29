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

#[test]
fn ordered_policy_resolution_preserves_switches_and_optnone_wins() {
    use keel_ir::{FunctionOptLevel, FunctionPolicy};
    let module = parse("decl @a() cold optimize(1, \"no-unroll-loops,O3\", \"-ftree-vectorize\", \"no-tree-loop-vectorize\")\ndecl @b() optnone optimize(3, \"inline-functions,unroll-loops\")\n").unwrap();
    let a = FunctionPolicy::resolve(&module.decls[0].attrs).unwrap();
    assert_eq!(a.level, Some(FunctionOptLevel::Full));
    assert_eq!(a.unroll, Some(false));
    assert_eq!(a.vectorize, Some(false));
    let b = FunctionPolicy::resolve(&module.decls[1].attrs).unwrap();
    assert!(b.disabled() && !b.permits_inlining());
    assert_eq!(b.unroll, Some(false));
    for options in ["O4", "Ofast", "no-unknown-pass", "", "tree-slp-vectorize"] {
        let attrs = keel_ir::FunctionAttrs {
            optimize: keel_ir::FunctionOptimization::new([keel_ir::OptimizationOption::Options(
                options.into(),
            )]),
            ..Default::default()
        };
        assert!(FunctionPolicy::resolve(&attrs).is_err(), "{options}");
    }
    assert!(verify_module(&parse("decl @bad() optnone alwaysinline\n").unwrap()).is_err());
}
