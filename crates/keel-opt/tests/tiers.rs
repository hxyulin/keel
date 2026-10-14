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
fn optimizer_tiers_and_legacy_full_pipeline() {
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
