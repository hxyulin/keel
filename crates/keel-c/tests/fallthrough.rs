use keel_ir::Target;

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "fallthrough.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

#[test]
fn annotations_preserve_switch_and_label_control_flow() {
    let annotated = r#"
int select(int n) {
    int result = 0;
    switch (n) {
    case 0: result += 1; __attribute__((fallthrough));
    case 1: result += 2;
        if (n) { __attribute__((__fallthrough__)); }
    case 2: __attribute((fallthrough,));
    default: result += 4;
    }
    return result;
}
int labels(int n) {
    switch (n) {
    case 0: goto next;
    case 1: next: ; __attribute__((fallthrough));
    default: return n + 7;
    }
}
"#;
    let plain = annotated
        .replace("__attribute__((fallthrough))", "")
        .replace("__attribute__((__fallthrough__))", "")
        .replace("__attribute((fallthrough,))", "");
    for target in Target::ALL {
        let module = compile(annotated, target).unwrap_or_else(|e| panic!("{target:?}: {e}"));
        let plain_module = compile(&plain, target).unwrap();
        assert_eq!(module.to_string(), plain_module.to_string(), "{target:?}");
        keel_ir::verify_module(&module).unwrap();
        if Target::NATIVE.contains(&target) {
            for optimize in [false, true] {
                let mut annotated = module.clone();
                let mut plain = plain_module.clone();
                if optimize {
                    keel_opt::optimize(&mut annotated).unwrap();
                    keel_opt::optimize(&mut plain).unwrap();
                }
                let annotated = keel_codegen::compile(&annotated, target).unwrap();
                let plain = keel_codegen::compile(&plain, target).unwrap();
                assert_eq!(annotated, plain, "{target:?}, optimize={optimize}");
            }
        }
    }
}

#[test]
fn malformed_or_semantic_statement_attributes_are_rejected() {
    for (statement, expected) in [
        ("__attribute__((fallthrough(1)));", "without arguments"),
        ("__attribute__((fallthrough)) return 1;", "null statement"),
        ("__attribute__((fallthrough)) int local;", "null statement"),
        (
            "__attribute__((fallthrough, __fallthrough__));",
            "only statement attribute",
        ),
        (
            "label: __attribute__((fallthrough));",
            "cannot annotate a label",
        ),
        (
            "__attribute__((fallthrough, alias(\"other\")));",
            "statement attribute",
        ),
        (
            "__attribute__((alias(\"other\"), fallthrough));",
            "requires semantics",
        ),
    ] {
        let source =
            format!("int f(int x) {{ switch (x) {{ case 0: {statement} default: return 0; }} }}");
        let error = compile(&source, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{source}: {error}");
    }
    let error = compile(
        "void f(void) { __attribute__((fallthrough)); }",
        Target::X86_64LinuxGnu,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("outside a switch"), "{error}");
}
