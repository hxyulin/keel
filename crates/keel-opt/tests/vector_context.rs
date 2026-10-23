use keel_ir::{Function, VectorCapabilities, parse};
use keel_opt::{OptimizationContext, OptimizationReport, OptimizeOptions, Pass, VectorTarget};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Custom<'a> {
    canonical: bool,
    calls: &'a AtomicUsize,
}
impl Pass for Custom<'_> {
    fn name(&self) -> &'static str {
        "custom"
    }
    fn description(&self) -> &'static str {
        "dispatch test"
    }
    fn run(&self, _: &mut Function) -> bool {
        panic!("target override lost")
    }
    fn run_for_target(&self, _: &mut Function, target: VectorTarget) -> bool {
        assert!(!self.canonical);
        assert_eq!(target, VectorTarget::X86Sse2);
        self.calls.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn run_canonical(&self, _: &mut Function, target: VectorTarget) -> bool {
        assert!(self.canonical);
        assert_eq!(target, VectorTarget::X86Sse2);
        self.calls.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn requires_canonical_form(&self) -> bool {
        self.canonical
    }
}
#[test]
fn context_preserves_custom_dispatch_and_observation() {
    let context = OptimizationContext {
        target: VectorTarget::X86Sse2,
        capabilities: VectorCapabilities::new(true, true, true, 123),
    };
    for canonical in [false, true] {
        let calls = AtomicUsize::new(0);
        let pass = Custom {
            canonical,
            calls: &calls,
        };
        let mut ordinary = parse("func @f() {\nblock0:\n return\n}").unwrap();
        let mut observed = ordinary.clone();
        let mut legacy = ordinary.clone();
        let options = OptimizeOptions::default();
        keel_opt::run_pipeline_with_context_and_options(&mut ordinary, &[&pass], context, options)
            .unwrap();
        let mut report = OptimizationReport::default();
        keel_opt::run_pipeline_with_context_and_report_and_options(
            &mut observed,
            &[&pass],
            context,
            &mut report,
            options,
        )
        .unwrap();
        keel_opt::run_pipeline_for_target(&mut legacy, &[&pass], context.target).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert_eq!(ordinary.to_string(), observed.to_string());
        assert_eq!(ordinary.to_string(), legacy.to_string());
        assert_eq!(report.invocations[0].context, context);
        let json = report.to_json();
        assert!(json.contains("\"signed_i32_sum_bits\":256"), "{json}");
        assert!(json.contains("\"effective_x86_features\":123"));
    }
}
