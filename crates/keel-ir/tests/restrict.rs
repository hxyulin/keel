use keel_ir::{parse, verify_module};

#[test]
fn restrict_roundtrip_and_clone() {
    let source =
        "func @f(ptr, ptr) restrict_params(1, 0) {\nblock0(v0: ptr, v1: ptr):\n    return\n}\n";
    let module = parse(source).unwrap();
    verify_module(&module).unwrap();
    assert_eq!(module.functions[0].restrict_params, [1, 0]);
    let printed = module.to_string();
    assert_eq!(parse(&printed).unwrap().to_string(), printed);
    assert_eq!(module.functions[0].clone().restrict_params, [1, 0]);
}

#[test]
fn invalid_restrict_indices_are_diagnosed() {
    for indices in ["0, 0", "2", "1"] {
        let source = format!(
            "func @f(ptr, i64) restrict_params({indices}) {{\nblock0(v0: ptr, v1: i64):\n return\n}}\n"
        );
        let module = parse(&source).unwrap();
        let errors = verify_module(&module).unwrap_err();
        assert!(format!("{errors:?}").contains("restrict parameter index"));
    }
    for indices in ["-1", "4294967296"] {
        assert!(
            parse(&format!(
                "func @f(ptr) restrict_params({indices}) {{\nblock0(v0: ptr):\nreturn\n}}\n"
            ))
            .is_err()
        );
    }
    let module =
        parse("func @f(ptr byval {i32}) restrict_params(0) {\nblock0(v0: ptr):\nreturn\n}\n")
            .unwrap();
    assert!(verify_module(&module).is_err());
}
