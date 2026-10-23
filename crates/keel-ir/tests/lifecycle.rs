#[test]
fn lifecycle_signatures_and_definition_requirement_are_verified() {
    for signature in [
        "(i32)",
        "() -> i32",
        "(...)",
        "() returns_twice",
        "() cc(fast)",
        "() noreturn",
    ] {
        let text = format!("func @f{signature} constructor local {{\nblock0:\n return\n}}\n");
        let m = keel_ir::parse(&text).unwrap();
        assert!(keel_ir::verify_module(&m).is_err(), "{text}");
    }
    for attr in ["constructor", "destructor"] {
        let m = keel_ir::parse(&format!("decl @f() {attr}\n")).unwrap();
        assert!(keel_ir::verify_module(&m).is_err());
        let m = keel_ir::parse(&format!(
            "func @f() {attr} used local {{\nblock0:\n return\n}}\n"
        ))
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(
            keel_ir::parse(&m.to_string()).unwrap().to_string(),
            m.to_string()
        );
    }
}
