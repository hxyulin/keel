use keel_ir::{Function, Module};
use keel_opt::{OptimizationReport, OptimizeOptions, Pass, VectorTarget, VerificationPolicy};
use std::sync::atomic::{AtomicUsize, Ordering};

const IR: &str =
    "func @f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = iadd v1, v0\n return v2\n}";

struct RemoveTerminator {
    module: bool,
}
impl Pass for RemoveTerminator {
    fn name(&self) -> &'static str {
        "remove-terminator"
    }
    fn description(&self) -> &'static str {
        "invalid-pass reducer"
    }
    fn run(&self, f: &mut Function) -> bool {
        let inst = f.layout_insts().last().unwrap().1;
        f.remove_inst(inst);
        true
    }
    fn run_module(&self, m: &mut Module) -> Option<bool> {
        self.module.then(|| self.run(&mut m.functions[0]))
    }
}

struct CanonicalConsumer(AtomicUsize);
impl Pass for CanonicalConsumer {
    fn name(&self) -> &'static str {
        "canonical-consumer"
    }
    fn description(&self) -> &'static str {
        "assert restored operand ordering"
    }
    fn requires_canonical_form(&self) -> bool {
        true
    }
    fn run(&self, f: &mut Function) -> bool {
        assert!(!f.to_string().contains("iadd v1, v0"));
        self.0.fetch_add(1, Ordering::Relaxed);
        false
    }
}

#[test]
fn actual_invalid_pass_is_caught_only_when_policy_enables_checks() {
    for module_pass in [false, true] {
        for policy in [
            VerificationPolicy::Default,
            VerificationPolicy::Enabled,
            VerificationPolicy::Disabled,
        ] {
            let options = OptimizeOptions {
                verification: policy,
            };
            let broken = RemoveTerminator {
                module: module_pass,
            };
            let mut plain = keel_ir::parse(IR).unwrap();
            let mut observed = keel_ir::parse(IR).unwrap();
            keel_ir::verify_module(&plain).unwrap();
            let mut report = OptimizationReport::default();
            let a = keel_opt::run_pipeline_for_target_with_options(
                &mut plain,
                &[&broken],
                VectorTarget::Portable,
                options,
            );
            let b = keel_opt::run_pipeline_for_target_with_report_and_options(
                &mut observed,
                &[&broken],
                VectorTarget::Portable,
                &mut report,
                options,
            );
            assert_eq!(a.is_err(), policy.enabled());
            assert_eq!(b.is_err(), policy.enabled());
            if let (Err(a), Err(b)) = (a, b) {
                assert_eq!(a.to_string(), b.to_string());
                assert!(matches!(
                    a,
                    keel_opt::OptError::Verifier {
                        pass: "remove-terminator",
                        ..
                    }
                ));
            }
            // A returned Ok in disabled mode really contains broken IR.
            assert!(keel_ir::verify_module(&observed).is_err());
            assert_eq!(report.invocations.len(), 1);
            assert_eq!(report.invocations[0].verification_enabled, policy.enabled());
            if !policy.enabled() {
                assert_eq!(report.invocations[0].verification_ns, 0);
                assert_eq!(report.invocations[0].restoration_verification_ns, 0);
            }
        }
    }
}

#[test]
fn defaults_and_legacy_entrypoints_follow_the_build_configuration() {
    let expected = cfg!(debug_assertions) || cfg!(feature = "verify-each");
    assert_eq!(VerificationPolicy::Default.enabled(), expected);
    assert!(VerificationPolicy::Enabled.enabled());
    assert!(!VerificationPolicy::Disabled.enabled());
    let mut m = keel_ir::parse(IR).unwrap();
    assert_eq!(
        keel_opt::run_pipeline(&mut m, &[&RemoveTerminator { module: false }]).is_err(),
        expected
    );
    let mut m = keel_ir::parse(IR).unwrap();
    let mut report = OptimizationReport::default();
    assert_eq!(
        keel_opt::run_pipeline_for_target_with_report(
            &mut m,
            &[&RemoveTerminator { module: true }],
            VectorTarget::Portable,
            &mut report
        )
        .is_err(),
        expected
    );
    assert_eq!(report.invocations[0].verification_enabled, expected);
}

#[test]
fn disabled_verification_still_restores_canonical_form_and_resolves_api_aliases() {
    for policy in [VerificationPolicy::Enabled, VerificationPolicy::Disabled] {
        let mut m = keel_ir::parse(IR).unwrap();
        // Direct Function API alias state, preserved without a text roundtrip.
        let f = &mut m.functions[0];
        let params = f.block_params(f.entry_block().unwrap()).to_vec();
        f.replace_all_uses(params[1], params[0]);
        assert_eq!(f.resolve(params[1]), params[0]);
        keel_ir::verify_module(&m).unwrap();
        let pass = CanonicalConsumer(AtomicUsize::new(0));
        let mut report = OptimizationReport::default();
        keel_opt::run_pipeline_for_target_with_report_and_options(
            &mut m,
            &[&pass, &pass],
            VectorTarget::Portable,
            &mut report,
            OptimizeOptions {
                verification: policy,
            },
        )
        .unwrap();
        assert_eq!(pass.0.load(Ordering::Relaxed), 2);
        assert_eq!(report.invocations[0].restoration_changed, Some(true));
        assert_eq!(report.invocations[1].restoration_changed, None);
        assert!(m.to_string().contains("iadd v0, v0"));
        keel_ir::verify_module(&m).unwrap();
        if policy == VerificationPolicy::Disabled {
            for p in &report.invocations {
                assert!(!p.verification_enabled);
                assert_eq!(p.verification_ns, 0);
                assert_eq!(p.restoration_verification_ns, 0);
            }
        }
    }
}
