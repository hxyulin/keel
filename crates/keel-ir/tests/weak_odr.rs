#[test]
fn roundtrip_and_clone_preserve_odr_linkage() {
    let src = "data @table = weak_odr hidden const align 8 { ptr @f, rtti_name @strong }\ndata @strong = align 8 { i64 0 }\nfunc @f() weak_odr {\nblock0:\n return\n}\nfunc @g() {\nblock0:\n return\n}\n";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let cloned = m.clone();
    assert!(cloned.functions[0].weak_odr && cloned.data[0].weak_odr);
    assert!(cloned.data[0].hidden);
    assert!(!cloned.data[1].hidden);
    assert!(!cloned.functions[1].weak_odr && !cloned.data[1].weak_odr);
    let printed = m.to_string();
    let reparsed = keel_ir::parse(&printed).unwrap();
    keel_ir::verify_module(&reparsed).unwrap();
    assert_eq!(printed, reparsed.to_string());
}

#[test]
fn hidden_linkage_roundtrips_on_nonlocal_definitions() {
    let src = "data @d = hidden align 8 { i64 0 }\ndata @t = hidden thread_local align 4 { i32 1 }\ndata @c = hidden const align 1 { b\"x\" }\nfunc @f() hidden {\nblock0:\n return\n}\nfunc @g() weak_odr hidden {\nblock0:\n return\n}\n";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.data.iter().all(|d| d.hidden && !d.weak_odr));
    assert!(m.functions.iter().all(|f| f.hidden && !f.local));
    assert!(m.functions[1].weak_odr);
    let printed = m.to_string();
    assert!(printed.contains("func @f() cc(c) hidden {"), "{printed}");
    assert!(
        printed.contains("func @g() cc(c) weak_odr hidden {"),
        "{printed}"
    );
    let reparsed = keel_ir::parse(&printed).unwrap();
    assert_eq!(printed, reparsed.to_string());
    let mut m = keel_ir::parse("decl data @d\n").unwrap();
    m.data[0].hidden = true;
    assert!(keel_ir::verify_module(&m).is_err());
}

#[test]
fn invalid_odr_linkage_is_rejected() {
    for src in [
        "func @f() local weak_odr {\nblock0:\n return\n}\n",
        "data @d = local weak_odr align 8 { i64 0 }\n",
        "data @d = weak_odr thread_local align 8 { i64 0 }\n",
        "data @d = local hidden align 8 { i64 0 }\n",
        "func @f() local hidden {\nblock0:\n return\n}\n",
        "data @d = weak_odr hidden thread_local align 8 { i64 0 }\n",
        "data @d = align 8 { rtti_name @missing }\n",
        "func @f() {\nblock0:\n return\n}\ndata @d = align 8 { rtti_name @f }\n",
        "decl data thread_local @tls\ndata @d = align 8 { rtti_name @tls }\n",
        "data @name = align 1 { i8 0 }\ndata @d = align 1 { rtti_name @name }\n",
        "data @name = align 1 { i8 0 }\ndata @d = align 8 { i8 0, rtti_name @name }\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        assert!(keel_ir::verify_module(&m).is_err(), "{src}");
    }
    let mut m = keel_ir::parse("decl data @d\n").unwrap();
    m.data[0].weak_odr = true;
    assert!(keel_ir::verify_module(&m).is_err());
    for src in [
        "decl @f() weak_odr\n",
        "decl data weak_odr @d\n",
    ] {
        assert!(keel_ir::parse(src).is_err());
    }
}
