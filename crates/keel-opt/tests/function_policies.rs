//! Independent function policies survive mixed optimization and observation.
use keel_opt::{OptimizationReport, OptimizationTier, VectorTarget};

const SOURCE: &str = r#"
func @plain(i64) -> i64 {
block0(v0:i64):
 v1 = iconst.i64 0
 v2 = iadd v0, v1
 return v2
}
func @off(i64) -> i64 optnone {
block0(v0:i64):
 v1 = iconst.i64 0
 v2 = iadd v0, v1
 return v2
}
func @zero(i64) -> i64 optimize("O0") {
block0(v0:i64):
 v1 = iconst.i64 0
 v2 = iadd v0, v1
 return v2
}
func @on(i64) -> i64 optimize("O2") {
block0(v0:i64):
 v1 = iconst.i64 0
 v2 = iadd v0, v1
 return v2
}
"#;
fn body(module: &keel_ir::Module, name: &str) -> String {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .to_string()
}

#[test]
fn independent_presets_override_both_full_and_empty_invocations_with_report_parity() {
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let before = keel_ir::parse(SOURCE).unwrap();
        let mut ordinary = before.clone();
        keel_opt::run_passes(&mut ordinary, tier.pipeline()).unwrap();
        assert_eq!(body(&ordinary, "off"), body(&before, "off"));
        assert_eq!(body(&ordinary, "zero"), body(&before, "zero"));
        assert_ne!(body(&ordinary, "on"), body(&before, "on"));
        assert_eq!(
            body(&ordinary, "plain") == body(&before, "plain"),
            tier == OptimizationTier::None
        );
        let mut observed = before;
        let mut report = OptimizationReport::default();
        keel_opt::run_passes_for_target_with_report(
            &mut observed,
            tier.pipeline(),
            VectorTarget::Portable,
            &mut report,
        )
        .unwrap();
        assert_eq!(ordinary.to_string(), observed.to_string());
        assert!(
            !report
                .invocations
                .iter()
                .any(|i| matches!(i.function.as_deref(), Some("off" | "zero")))
        );
        keel_ir::verify_module(&ordinary).unwrap();
    }
}

#[test]
fn unsupported_options_fail_before_any_body_changes() {
    let mut module =
        keel_ir::parse(&SOURCE.replace("optimize(\"O2\")", "optimize(\"no-made-up-pass\")"))
            .unwrap();
    let before = module.to_string();
    let error = keel_opt::optimize(&mut module).unwrap_err().to_string();
    assert!(
        error.contains("@on") && error.contains("made-up-pass"),
        "{error}"
    );
    assert_eq!(module.to_string(), before);
}

#[test]
fn pass_switches_are_ordered_and_excluded_from_reports() {
    let source = SOURCE.replace(
        "optimize(\"O2\")",
        "optimize(3, \"no-unroll-loops,no-tree-vectorize\")",
    );
    let mut module = keel_ir::parse(&source).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut module,
        &[],
        VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    assert!(
        report
            .invocations
            .iter()
            .any(|i| i.function.as_deref() == Some("on"))
    );
    assert!(
        !report
            .invocations
            .iter()
            .any(|i| i.function.as_deref() == Some("on")
                && matches!(i.pass, "loop-unroll" | "loop-vectorize"))
    );
}

#[test]
fn an_override_does_not_enable_inlining_in_o0_neighbors() {
    let mut module = keel_ir::parse("func @helper(i64) -> i64 local {\nblock0(v0:i64):\n return v0\n}\nfunc @ordinary(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @helper(v0)\n return v1\n}\nfunc @optimized(i64) -> i64 optimize(2) {\nblock0(v0:i64):\n return v0\n}\n").unwrap();
    let before = body(&module, "ordinary");
    keel_opt::run_passes(&mut module, &[]).unwrap();
    assert_eq!(body(&module, "ordinary"), before);
}

#[test]
fn cold_alone_does_not_enable_optimization_at_o0() {
    let mut module = keel_ir::parse(&SOURCE.replace(
        "func @plain(i64) -> i64 {",
        "func @plain(i64) -> i64 cold {",
    ))
    .unwrap();
    let before = body(&module, "plain");
    keel_opt::run_passes(&mut module, &[]).unwrap();
    assert_eq!(body(&module, "plain"), before);
}

#[test]
fn kernel_link_readiness_reducers_preserve_unannotated_controls_and_report_parity() {
    let cold = include_str!("fixtures/function-policies/cold.keel");
    let rounds = include_str!("fixtures/function-policies/rounds.keel");
    let dead = include_str!("fixtures/function-policies/dead-local.keel");
    let cases = [
        cold.to_owned(),
        cold.replace(" cold", ""),
        cold.replace(" cold", " optimize(3)"),
        cold.replace(" cold", " optimize(\"inline-functions\")"),
        rounds.to_owned(),
        dead.to_owned(),
    ];
    for source in cases {
        let before = keel_ir::parse(&source).unwrap();
        let protected: Vec<_> = before
            .functions
            .iter()
            .filter(|f| f.attrs.optnone)
            .map(|f| (f.name.clone(), f.to_string()))
            .collect();
        let mut ordinary = before.clone();
        keel_opt::optimize(&mut ordinary).unwrap();
        keel_ir::verify_module(&ordinary).unwrap();
        assert!(!ordinary.functions.iter().any(|f| f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Call { func, .. } if f.callee(*func).name == "absent"))), "{ordinary}");
        for (name, expected) in protected {
            assert_eq!(body(&ordinary, &name), expected);
        }
        let mut observed = before;
        let mut report = OptimizationReport::default();
        keel_opt::run_passes_for_target_with_report(
            &mut observed,
            OptimizationTier::Full.pipeline(),
            VectorTarget::Portable,
            &mut report,
        )
        .unwrap();
        assert_eq!(ordinary.to_string(), observed.to_string());
    }
}

#[test]
fn protected_noncanonical_bodies_and_their_outgoing_calls_survive_module_passes() {
    let source = "func @noop() local {\nblock0:\n return\n}\nfunc @protected(i64) -> i64 optnone local {\nblock0(v0:i64):\n call @noop()\n jump block1(v0)\nblock1(v1:i64):\n v2 = iconst.i64 0\n v3 = iadd v1, v2\n return v3\n}\nfunc @root() -> i64 {\nblock0:\n v0 = iconst.i64 9\n v1 = call @protected(v0)\n return v1\n}\n";
    for policy in ["optnone", "optimize(0)"] {
        let mut module = keel_ir::parse(&source.replace("optnone", policy)).unwrap();
        let expected = body(&module, "protected");
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(body(&module, "protected"), expected);
        assert!(module.functions.iter().any(|f| f.name == "noop"));
    }
}

#[test]
fn scoped_constant_arguments_inspect_protected_callers_without_rewriting_them() {
    use keel_ir::{InstData, ValueDef};
    for other in [1, 2] {
        let source = format!(
            "func @helper(i64) -> i64 noinline local {{\nblock0(v0:i64):\n return v0\n}}\nfunc @protected() -> i64 optnone {{\nblock0:\n v0 = iconst.i64 1\n v1 = call @helper(v0)\n return v1\n}}\nfunc @ordinary() -> i64 {{\nblock0:\n v0 = iconst.i64 {other}\n v1 = call @helper(v0)\n return v1\n}}\n"
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let expected = body(&module, "protected");
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(body(&module, "protected"), expected);
        let helper = module
            .functions
            .iter()
            .find(|f| f.name == "helper")
            .unwrap();
        let returned = helper
            .layout_insts()
            .find_map(|(_, i)| match helper.inst_data(i) {
                InstData::Return { args } => Some(helper.resolve(args[0])),
                _ => None,
            })
            .unwrap();
        if other == 1 {
            assert!(
                matches!(helper.value_def(returned), ValueDef::Result(i, _) if matches!(helper.inst_data(i), InstData::Iconst { imm: 1, .. }))
            );
        } else {
            assert!(
                matches!(helper.value_def(returned), ValueDef::Param(_, _)),
                "{helper}"
            );
        }
    }
}

#[test]
fn size_and_balanced_presets_remove_constant_guard_calls() {
    let source = include_str!("fixtures/function-policies/cold.keel");
    for tier in [OptimizationTier::Balanced, OptimizationTier::Size] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        assert!(!module.functions.iter().any(|f| f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Call { func, .. } if f.callee(*func).name == "absent"))), "{tier:?}: {module}");
    }
}

#[test]
fn size_inlining_keeps_unhinted_shared_helpers_with_variable_arguments() {
    let source = "func @helper(i64) -> i64 local {\nblock0(v0:i64):\n v1 = iconst.i64 3\n v2 = imul v0, v1\n return v2\n}\nfunc @root(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @helper(v0)\n v2 = call @helper(v1)\n v3 = iadd v1, v2\n return v3\n}\n";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Size).unwrap();
    let root = module.functions.iter().find(|f| f.name == "root").unwrap();
    assert_eq!(root.layout_insts().filter(|(_, i)| matches!(root.inst_data(*i), keel_ir::InstData::Call { func, .. } if root.callee(*func).name == "helper")).count(), 2);
}

#[test]
fn scopes_do_not_hide_recursion_edges_from_inlining_legality() {
    let source = "func @a(i64) -> i64 optimize(1) local {\nblock0(v0:i64):\n v1 = call @b(v0)\n v2 = call @b(v1)\n v3 = iadd v1, v2\n return v3\n}\nfunc @b(i64) -> i64 local {\nblock0(v0:i64):\n v1 = call @a(v0)\n return v1\n}\nfunc @root(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @b(v0)\n return v1\n}\n";
    let mut module = keel_ir::parse(source).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let b = module.functions.iter().find(|f| f.name == "b").unwrap();
    assert!(b.layout_insts().any(|(_, i)| matches!(b.inst_data(i), keel_ir::InstData::Call { func, .. } if b.callee(*func).name == "a")), "{module}");
}

#[test]
fn empty_call_cleanup_removes_unused_symbol_addresses_before_emission() {
    let source = "decl @absent()\nfunc @empty(ptr) noinline local {\nblock0(v0:ptr):\n return\n}\nfunc @root() {\nblock0:\n v0 = symbol_addr @absent\n call @empty(v0)\n return\n}\n";
    for tier in [
        OptimizationTier::Balanced,
        OptimizationTier::Size,
        OptimizationTier::Full,
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(!module.functions.iter().any(|f| f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::SymbolAddr { symbol } if f.symbol_name(*symbol) == "absent"))), "{tier:?}: {module}");
    }
}
