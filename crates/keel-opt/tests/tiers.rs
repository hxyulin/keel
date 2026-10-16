mod support;
use keel_opt::{OptimizationTier, Pass};
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str = "func @f() -> i64 {
block0:
    v0 = iconst.i64 20
    v1 = iconst.i64 22
    v2 = iadd v0, v1
    return v2
}";

#[test]
fn optimizer_tiers_and_full_pipeline() {
    let original = keel_ir::parse(SOURCE).unwrap();
    let mut none = original.clone();
    keel_opt::optimize_with_tier(&mut none, OptimizationTier::None).unwrap();
    assert_eq!(original.to_string(), none.to_string());
    for tier in [OptimizationTier::Basic, OptimizationTier::Full] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        assert!(!module.to_string().contains("iadd"));
        assert!(module.to_string().contains("42"));
        if tier == OptimizationTier::Full {
            let mut legacy = original.clone();
            keel_opt::optimize(&mut legacy).unwrap();
            assert_eq!(legacy.to_string(), module.to_string());
        }
    }
}

struct CustomPass(AtomicUsize);
impl Pass for CustomPass {
    fn name(&self) -> &'static str {
        "custom"
    }
    fn description(&self) -> &'static str {
        "delegate constant folding"
    }
    fn run(&self, func: &mut keel_ir::Function) -> bool {
        self.0.fetch_add(1, Ordering::Relaxed);
        keel_opt::find_pass("const-fold").unwrap().run(func)
    }
}

#[test]
fn custom_pass_is_executed() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    let pass = CustomPass(AtomicUsize::new(0));
    keel_opt::run_pipeline(&mut module, &[&pass]).unwrap();
    assert_eq!(pass.0.load(Ordering::Relaxed), 1);
    assert!(module.to_string().contains("42"));
}

struct BrokenPass;
impl Pass for BrokenPass {
    fn name(&self) -> &'static str {
        "broken"
    }
    fn description(&self) -> &'static str {
        "remove the terminator"
    }
    fn run(&self, func: &mut keel_ir::Function) -> bool {
        let terminator = func.layout_insts().last().unwrap().1;
        func.remove_inst(terminator);
        true
    }
}

#[test]
fn custom_pass_failures_identify_the_pass() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    let error = keel_opt::run_pipeline(&mut module, &[&BrokenPass]).unwrap_err();
    assert!(matches!(
        error,
        keel_opt::OptError::Verifier { pass: "broken", .. }
    ));
}

#[test]
fn basic_and_full_combine_variable_identities() {
    let original = keel_ir::parse(
        "func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 1
    v2 = iconst.i64 0
    v3 = imul v0, v1
    v4 = iadd v3, v2
    return v4
}",
    )
    .unwrap();
    for tier in [OptimizationTier::Basic, OptimizationTier::Full] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        assert_eq!(module.functions[0].layout_insts().count(), 1);
        for input in [0, 1, u64::MAX, 1 << 63] {
            assert_eq!(
                support::execute(&module.functions[0], &[input]),
                support::execute(&original.functions[0], &[input])
            );
        }
    }
}

#[test]
fn matching_extensions_fold_in_basic_and_expose_full_gvn_equivalence() {
    for op in ["sext", "uext"] {
        for cond in ["eq", "ne"] {
            let source = format!(
                "func @f(i8, i8) -> i8 {{\nblock0(v0: i8, v1: i8):\n v2 = {op}.i64 v0\n v3 = {op}.i64 v1\n v4 = icmp.{cond} v2, v3\n return v4\n}}"
            );
            for tier in [OptimizationTier::Basic, OptimizationTier::Full] {
                let mut module = keel_ir::parse(&source).unwrap();
                keel_opt::optimize_with_tier(&mut module, tier).unwrap();
                assert_eq!(module.functions[0].layout_insts().count(), 2, "{module}");
                assert!(module.to_string().contains(&format!("icmp.{cond} v0, v1")));
            }
            let source = source.replace(
                "return v4",
                &format!("v5 = icmp.{cond} v1, v0\n v6 = xor v4, v5\n return v6"),
            );
            let original = keel_ir::parse(&source).unwrap();
            let mut module = original.clone();
            keel_opt::optimize(&mut module).unwrap();
            assert_eq!(module.functions[0].layout_insts().count(), 2, "{module}");
            assert!(module.to_string().contains("iconst.i8 0"), "{module}");
            for a in [0, 1, 127, 128, 255] {
                for b in [0, 1, 127, 128, 255] {
                    assert_eq!(
                        support::execute(&original.functions[0], &[a, b]),
                        support::execute(&module.functions[0], &[a, b])
                    );
                }
            }
        }
    }
}
