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
