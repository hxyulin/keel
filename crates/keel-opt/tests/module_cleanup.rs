use keel_ir::{Module, parse, verify_module};

fn clean(source: &str) -> Module {
    let mut module = parse(source).unwrap();
    verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["empty-calls", "dce", "unused-functions"]).unwrap();
    verify_module(&module).unwrap();
    let once = module.to_string();
    keel_opt::run_passes(&mut module, &["empty-calls", "dce", "unused-functions"]).unwrap();
    assert_eq!(once, module.to_string());
    module
}

#[test]
fn removes_dead_recursive_component_and_its_external_reference() {
    let module = clean(
        "decl @missing()\nfunc @dead() local {\nblock0:\n call @dead()\n call @missing()\n return\n}\nfunc @live() {\nblock0:\n return\n}",
    );
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "live");
}

#[test]
fn removes_empty_noinline_call_but_preserves_argument_effects() {
    let module = clean(
        "decl @effect() -> ptr\nfunc @empty(ptr) noinline local {\nblock0(v0: ptr):\n return\n}\nfunc @live() {\nblock0:\n v0 = call @effect()\n call @empty(v0)\n return\n}",
    );
    let text = module.functions[0].to_string();
    assert!(!text.contains("call @empty"));
    assert!(text.contains("call @effect"));
    assert_eq!(module.functions[0].name, "live");
}

#[test]
fn body_substitution_requires_binding_proof_and_preserves_weak_calls() {
    for attrs in ["", "weak", "weak no_semantic_interposition"] {
        let source = format!(
            "func @empty() {attrs} {{\nblock0:\n return\n}}\nfunc @live() {{\nblock0:\n call @empty()\n return\n}}"
        );
        let module = clean(&source);
        assert!(
            module.functions[1].to_string().contains("call @empty"),
            "{attrs}"
        );
    }
    for attrs in ["noninterposable", "no_semantic_interposition"] {
        let source = format!(
            "func @empty() noinline {attrs} {{\nblock0:\n return\n}}\nfunc @live() {{\nblock0:\n call @empty()\n return\n}}"
        );
        let module = clean(&source);
        assert!(
            !module.functions[1].to_string().contains("call @empty"),
            "{attrs}"
        );
    }
}

#[test]
fn keeps_data_addresses_assembly_references_and_explicit_roots() {
    for root in [
        "data @address = align 8 { ptr @helper }",
        "module_asm \".set alias, helper\"",
        "func @address() -> ptr {\nblock0:\n v0 = symbol_addr @helper\n return v0\n}",
        "func @assembly() {\nblock0:\n asm \"call helper\", \"\"()\n return\n}",
    ] {
        let module = clean(&format!(
            "{root}\nfunc @helper() local {{\nblock0:\n return\n}}"
        ));
        assert!(
            module.functions.iter().any(|f| f.name == "helper"),
            "{root}"
        );
    }
    for attrs in ["used", "retain", "constructor", "destructor"] {
        let module = clean(&format!(
            "func @helper() {attrs} local {{\nblock0:\n return\n}}"
        ));
        assert_eq!(module.functions.len(), 1, "{attrs}");
    }
}

#[test]
fn keeps_nonempty_and_nonreturning_calls() {
    let module = clean(
        "decl @effect()\nfunc @helper() local {\nblock0:\n call @effect()\n return\n}\nfunc @live() {\nblock0:\n call @helper()\n return\n}",
    );
    assert_eq!(module.functions.len(), 2);
    assert!(module.functions[1].to_string().contains("call @helper"));
}

#[test]
fn does_not_discard_bodies_when_macros_can_construct_symbol_names() {
    let module = clean(
        "module_asm \".macro target name\\n call \\\\name\\n.endm\"\nfunc @helper() local {\nblock0:\n return\n}",
    );
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn removing_a_call_keeps_potentially_trapping_argument_reads() {
    let module = clean(
        "func @empty(i64) noinline local {\nblock0(v0: i64):\n return\n}\nfunc @live(ptr) {\nblock0(v0: ptr):\n v1 = load.i64 v0\n call @empty(v1)\n return\n}",
    );
    let text = module.functions[0].to_string();
    assert!(!text.contains("call @empty"));
    assert!(text.contains("load.i64"));
}

#[test]
fn keeps_fast_definitions_required_by_stale_import_contracts() {
    let module = clean(
        "func @helper() cc(fast) local {\nblock0:\n return\n}\nfunc @live() {\nblock0:\n call @helper()\n return\n}",
    );
    assert!(module.functions.iter().any(|f| f.name == "helper"));
    assert!(!module.functions[1].to_string().contains("call @helper"));
}
