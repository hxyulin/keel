use keel_ir::{Function, Module};
use keel_opt::{OptimizationReport, Pass};
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
    fn run(&self, _: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
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
    fn run(&self, f: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
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
    fn run(&self, _: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, _: &mut Module, _: &mut keel_opt::ModuleContext<'_>) -> Option<bool> {
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
    keel_opt::Pipeline::new(passes)
        .report(&mut report)
        .run(&mut module)
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

/// Edits a function with `edit` and reports no change.
struct Liar(fn(&mut Function));
impl Pass for Liar {
    fn name(&self) -> &'static str {
        "liar"
    }
    fn description(&self) -> &'static str {
        "edit a function and report no change"
    }
    fn run(&self, f: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
        (self.0)(f);
        false
    }
}

/// Edits every function and reports the change.
struct Editor;
impl Pass for Editor {
    fn name(&self) -> &'static str {
        "editor"
    }
    fn description(&self) -> &'static str {
        "edit a function and report it"
    }
    fn run(&self, f: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
        add_constant(f);
        true
    }
}

/// Edits the first function and reports no module change.
struct ModuleLiar;
impl Pass for ModuleLiar {
    fn name(&self) -> &'static str {
        "module-liar"
    }
    fn description(&self) -> &'static str {
        "edit a function and report no module change"
    }
    fn run(&self, _: &mut Function, _: &keel_opt::OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, _: &mut keel_opt::ModuleContext<'_>) -> Option<bool> {
        add_constant(&mut module.functions[0]);
        Some(false)
    }
}

fn add_constant(f: &mut Function) {
    let entry = f.entry_block().unwrap();
    let first = f.block_insts(entry)[0];
    f.ins(keel_ir::InsertPoint::Before(first))
        .build(keel_ir::InstData::Iconst {
            ty: keel_ir::Type::I64,
            imm: 7,
        });
}

fn run_verified(source: &str, passes: &[&dyn Pass]) {
    let mut module = keel_ir::parse(source).unwrap();
    let options = keel_opt::OptimizeOptions {
        verification: keel_opt::VerificationPolicy::Enabled,
        ..Default::default()
    };
    keel_opt::Pipeline::new(passes)
        .options(options)
        .run(&mut module)
        .unwrap();
}

#[test]
fn honest_unchanged_passes_pass_the_audit() {
    let det = Observer(AtomicUsize::new(0), true);
    run_verified(IR, &[&det, &ModuleNoop(false), &TouchG, &det]);
}

#[test]
#[should_panic(expected = "pass `liar` reported no change but modified @f")]
fn a_function_pass_that_hides_an_edit_is_named() {
    run_verified(IR, &[&Liar(add_constant)]);
}

#[test]
#[should_panic(expected = "pass `liar` reported no change but modified @f")]
fn a_hidden_alias_is_an_edit() {
    let source = "func @f() -> i64 {\nblock0:\n v0 = iconst.i64 1\n v1 = iconst.i64 2\n \
                  v2 = iadd v0, v1\n return v2\n}";
    run_verified(
        source,
        &[&Liar(|f| {
            use keel_ir::EntityRef;
            f.replace_all_uses(keel_ir::Value::new(1), keel_ir::Value::new(0));
        })],
    );
}

#[test]
#[should_panic(expected = "pass `module-liar` reported no change but modified @f")]
fn a_module_pass_that_hides_an_edit_is_named() {
    run_verified(IR, &[&ModuleLiar]);
}

// The manager reuses a function's fingerprint while passes leave it unchanged.
#[test]
fn reported_edits_refresh_the_audit_fingerprint() {
    let det = Observer(AtomicUsize::new(0), false);
    run_verified(
        IR,
        &[&det, &Editor, &det, &ModuleNoop(false), &Editor, &det],
    );
}

#[test]
#[should_panic(expected = "pass `liar` reported no change but modified @f")]
fn an_edit_hidden_after_unchanged_passes_is_named() {
    let det = Observer(AtomicUsize::new(0), false);
    run_verified(IR, &[&det, &det, &Liar(add_constant)]);
}

#[test]
#[should_panic(expected = "pass `module-liar` reported no change but modified @f")]
fn a_module_edit_hidden_after_a_reported_edit_is_named() {
    run_verified(IR, &[&Editor, &ModuleLiar]);
}
