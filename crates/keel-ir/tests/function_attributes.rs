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
