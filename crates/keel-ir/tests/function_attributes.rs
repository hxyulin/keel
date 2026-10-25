use keel_ir::{FunctionAttrs, parse, verify, verify_module};

#[test]
fn contracts_roundtrip_on_declarations_definitions_and_direct_callees() {
    let text = "decl @stop() noreturn nounwind noinline\nfunc @id(i32) -> i32 inlinehint alwaysinline nounwind local {\nblock0(v0: i32):\n return v0\n}\nfunc @f(i32) -> i32 {\nblock0(v0: i32):\n v1 = call @id(v0)\n call @stop()\n trap\n}\n";
    let m = parse(text).unwrap();
    verify_module(&m).unwrap();
    let printed = m.to_string();
    let again = parse(&printed).unwrap();
    verify_module(&again).unwrap();
    assert_eq!(printed, again.to_string());
    let caller = &again.functions[1];
    for (_, decl) in caller.callees() {
        assert_eq!(Some(decl.attrs), again.function_attrs(&decl.name));
    }
    assert!(again.decls[0].attrs.noreturn);
    assert!(again.decls[0].attrs.nounwind);
    assert_eq!(again.functions[0].sig, again.functions[1].sig);
}

#[test]
fn malformed_duplicate_and_conflicting_policies() {
    for suffix in [
        "inlinehint inlinehint",
        "alwaysinline(1)",
        "nounwind bogus",
        "noinline = 1",
    ] {
        assert!(parse(&format!("decl @f() {suffix}\n")).is_err(), "{suffix}");
    }
    for suffix in ["alwaysinline noinline", "returns_twice noreturn"] {
        let m = parse(&format!("decl @f() {suffix}\n")).unwrap();
        assert!(verify_module(&m).is_err(), "{suffix}");
    }
    let m = parse("decl @f() inlinehint noinline\n").unwrap();
    verify_module(&m).unwrap();
}

#[test]
fn independent_promises_and_structural_verification() {
    for suffix in [
        "noreturn",
        "nounwind",
        "noreturn nounwind",
        "inlinehint noinline",
    ] {
        // Verifier checks representation, not arbitrary semantic promises.
        let m = parse(&format!("func @f() {suffix} {{\nblock0:\n return\n}}\n")).unwrap();
        verify_module(&m).unwrap();
    }
    let mut m =
        parse("decl @g() nounwind\nfunc @f() {\nblock0:\n call @g()\n return\n}\n").unwrap();
    let f = &mut m.functions[0];
    let r = f.callees().next().unwrap().0;
    f.callee_mut(r).attrs = FunctionAttrs::default();
    assert!(
        verify_module(&m)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("attributes disagree"))
    );
    m.functions[0].attrs.alwaysinline = true;
    m.functions[0].attrs.noinline = true;
    assert!(verify(&m.functions[0]).is_err());
}

#[test]
fn noninterposable_is_independent_of_linkage_abi_and_inline_policy() {
    for policy in ["", "alwaysinline", "noinline", "inlinehint", "local"] {
        let text = format!(
            "decl @external(i8 sext) -> i16 uext noninterposable\nfunc @helper(i8 sext) -> i16 uext noninterposable {policy} {{\nblock0(v0: i8):\n v1 = uext.i16 v0\n return v1\n}}\nfunc @caller(i8 sext) -> i16 uext {{\nblock0(v0: i8):\n v1 = call @helper(v0)\n return v1\n}}\n"
        );
        let mut m = parse(&text).unwrap();
        verify_module(&m).unwrap();
        assert_eq!(m.functions[0].local, policy == "local");
        assert!(m.decls[0].attrs.noninterposable);
        let printed = m.to_string();
        assert_eq!(parse(&printed).unwrap().to_string(), printed);
        let r = m.functions[1].callees().next().unwrap().0;
        assert!(m.functions[1].callee(r).attrs.noninterposable);
        m.functions[1].callee_mut(r).attrs.noninterposable = false;
        assert!(
            verify_module(&m)
                .unwrap_err()
                .iter()
                .any(|e| e.message.contains("attributes disagree"))
        );
        m.sync_function_attrs();
        verify_module(&m).unwrap();
    }
    for suffix in ["noninterposable noninterposable", "noninterposable(1)"] {
        assert!(parse(&format!("decl @f() {suffix}\n")).is_err());
    }
    let mut attrs = FunctionAttrs::default();
    attrs.merge(FunctionAttrs {
        noninterposable: true,
        ..Default::default()
    });
    attrs.merge(FunctionAttrs::default());
    assert!(attrs.noninterposable);
}

#[test]
fn semantic_interposition_contract_roundtrips_without_exact_binding() {
    let text = "decl @external(i32) -> i32 no_semantic_interposition\nfunc @helper(i32) -> i32 no_semantic_interposition {\nblock0(v0: i32):\n return v0\n}\nfunc @caller(i32) -> i32 {\nblock0(v0: i32):\n v1 = call @helper(v0)\n return v1\n}\n";
    let m = parse(text).unwrap();
    verify_module(&m).unwrap();
    let printed = m.to_string();
    let again = parse(&printed).unwrap();
    verify_module(&again).unwrap();
    assert_eq!(printed, again.to_string());
    assert!(again.decls[0].attrs.no_semantic_interposition);
    assert!(again.functions[0].attrs.no_semantic_interposition);
    assert!(!again.functions[0].attrs.noninterposable);
    assert!(!again.functions[0].local);
    assert!(!again.functions[0].hidden);
    assert!(
        again.functions[1]
            .callees()
            .next()
            .unwrap()
            .1
            .attrs
            .no_semantic_interposition
    );
    let mut attrs = FunctionAttrs::default();
    attrs.merge(again.functions[0].attrs);
    assert_eq!(attrs, again.functions[0].attrs);
    assert!(parse("decl @f() no_semantic_interposition no_semantic_interposition\n").is_err());
}
