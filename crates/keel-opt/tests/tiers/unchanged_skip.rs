use keel_ir::{Function, Module};
use keel_opt::{OptimizationReport, Pass, VectorTarget};
use std::sync::atomic::{AtomicUsize, Ordering};

const IR: &str = "func @f(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\n\
                  func @g(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}";

/// Counts its runs and changes nothing.
struct Observer(AtomicUsize, bool);
impl Pass for Observer {
    fn name(&self) -> &'static str {
        "observer"
    }
    fn description(&self) -> &'static str {
        "count runs"
    }
    fn run(&self, _: &mut Function) -> bool {
        self.0.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn deterministic(&self) -> bool {
        self.1
    }
}

/// Reports a change to @g only; reporting more than changed is allowed.
struct TouchG;
impl Pass for TouchG {
    fn name(&self) -> &'static str {
        "touch-g"
    }
    fn description(&self) -> &'static str {
        "report a change to @g"
    }
    fn run(&self, f: &mut Function) -> bool {
        f.name == "g"
    }
}

/// A module pass using the default tracking: every function may change.
struct ModuleNoop(bool);
impl Pass for ModuleNoop {
    fn name(&self) -> &'static str {
        "module-noop"
    }
    fn description(&self) -> &'static str {
        "report a module change"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, _: &mut Module) -> Option<bool> {
        Some(self.0)
    }
}

fn runs(passes: &[&dyn Pass], observer: &Observer) -> usize {
    observer.0.store(0, Ordering::Relaxed);
    let mut module = keel_ir::parse(IR).unwrap();
    let mut plain = module.clone();
    keel_opt::run_pipeline(&mut plain, passes).unwrap();
    let plain_runs = observer.0.swap(0, Ordering::Relaxed);
    let mut report = OptimizationReport::default();
    keel_opt::run_pipeline_for_target_with_report(
        &mut module,
        passes,
        VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    // The observed runner skips the same runs and still reports each one.
    assert_eq!(observer.0.load(Ordering::Relaxed), plain_runs);
    let functions = module.functions.len();
    assert_eq!(
        report.invocations.len(),
        passes
            .iter()
            .map(|p| if p.name() == "module-noop" {
                1
            } else {
                functions
            })
            .sum::<usize>()
    );
    plain_runs
}

#[test]
fn deterministic_passes_skip_only_functions_unchanged_since_a_clean_run() {
    let det = Observer(AtomicUsize::new(0), true);
    assert_eq!(runs(&[&det, &det, &det], &det), 2);
    // A reported change renews only that function.
    assert_eq!(runs(&[&det, &TouchG, &det], &det), 3);
    // Default module tracking renews every function only on a change.
    assert_eq!(runs(&[&det, &ModuleNoop(true), &det], &det), 4);
    assert_eq!(runs(&[&det, &ModuleNoop(false), &det], &det), 2);
    let plain = Observer(AtomicUsize::new(0), false);
    assert_eq!(runs(&[&plain, &plain, &plain], &plain), 6);
}
