use keel_ir::{InstData, Module};

fn source() -> Module {
    keel_ir::parse(
        r#"
func @body() alwaysinline {
block0:
 return
}
func @caller() {
block0:
 call @body()
 return
}
"#,
    )
    .unwrap()
}

#[test]
fn only_explicit_c_ownership_admits_public_mandatory_bodies() {
    let mut ordinary = source();
    let error = keel_opt::required_inline_asm(&mut ordinary).unwrap_err();
    assert!(error.contains("public_definition"), "{error}");
    let mut owned = source();
    keel_opt::required_inline_asm_with_c_definitions(&mut owned, &[], &["body"]).unwrap();
    keel_ir::verify_module(&owned).unwrap();
    let body = &owned.functions[0];
    assert!(!body.local && !body.attrs.noninterposable && !body.attrs.no_semantic_interposition);
    let caller = &owned.functions[1];
    assert!(
        !caller
            .layout_insts()
            .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. }))
    );
}

#[test]
fn invalid_c_ownership_is_rejected_before_module_changes() {
    for case in 0..5 {
        let mut module = source();
        match case {
            0 => module.functions[0].local = true,
            1 => module.functions[0].attrs.weak = true,
            2 => module.functions[0].attrs.alwaysinline = false,
            _ => {}
        }
        module.sync_function_attrs();
        let before = module.to_string();
        let name = if case == 3 { "missing" } else { "body" };
        let inline_only: &[&str] = if case == 4 { &["body"] } else { &[] };
        assert!(
            keel_opt::required_inline_asm_with_c_definitions(&mut module, inline_only, &[name])
                .is_err()
        );
        assert_eq!(module.to_string(), before, "case {case}");
    }
}

#[test]
fn mandatory_wrapper_can_participate_in_ordinary_recursion() {
    let mut module = keel_ir::parse(
        r#"
func @wrapper() alwaysinline local {
block0:
 call @ordinary()
 return
}
func @ordinary() {
block0:
 call @wrapper()
 return
}
"#,
    )
    .unwrap();
    keel_opt::required_inline_asm(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ordinary = module
        .functions
        .iter()
        .find(|f| f.name == "ordinary")
        .unwrap();
    let calls: Vec<_> = ordinary
        .layout_insts()
        .filter_map(|(_, inst)| match ordinary.inst_data(inst) {
            InstData::Call { func, .. } => Some(ordinary.callee(*func).name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(calls, ["ordinary"]);
}

#[test]
fn mandatory_recursive_cycles_are_rejected() {
    for recursive_body in ["call @wrapper()", "call @other()"] {
        let source = format!(
            r#"
func @wrapper() alwaysinline local {{
block0:
 {recursive_body}
 return
}}
func @other() alwaysinline local {{
block0:
 call @wrapper()
 return
}}
func @caller() {{
block0:
 call @wrapper()
 return
}}
"#
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let error = keel_opt::required_inline_asm(&mut module).unwrap_err();
        assert!(
            error.contains("recursive callee"),
            "{recursive_body}: {error}"
        );
    }
}
