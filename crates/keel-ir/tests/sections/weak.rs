#[test]
fn ordinary_weak_roundtrips_without_odr_and_invalid_combinations_fail() {
    let src = "decl @optional() weak\ndecl data weak @absent\ndata @d = weak hidden align 4 { i32 0 }\nfunc @f() weak hidden {\nblock0:\n return\n}\n";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(m.decls[0].attrs.weak && m.functions[0].attrs.weak);
    assert!(m.data.iter().all(|d| d.weak && !d.weak_odr));
    assert!(!m.defines("f") && !m.defines("d"));
    let text = m.to_string();
    assert_eq!(keel_ir::parse(&text).unwrap().to_string(), text);
    for src in [
        "func @f() weak local {\nblock0:\n return\n}\n",
        "func @f() weak weak_odr {\nblock0:\n return\n}\n",
        "data @d = local weak align 4 { i32 0 }\n",
        "data @d = weak weak_odr align 4 { i32 0 }\n",
        "data @d = weak thread_local align 4 { i32 0 }\n",
        "decl data weak thread_local @d\n",
    ] {
        assert!(
            keel_ir::verify_module(&keel_ir::parse(src).unwrap()).is_err(),
            "{src}"
        );
    }
    for src in ["decl @f() weak(1)\n", "decl data weak_odr @d\n"] {
        assert!(keel_ir::parse(src).is_err());
    }
}
