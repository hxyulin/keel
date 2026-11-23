//! Large-module batching preserves function order and diagnostic behavior.

use keel_ir::{Function, Module};
use keel_opt::{OptimizationContext, OptimizationReport, OptimizeOptions, Pass, Pipeline};
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::thread::{self, ThreadId};

const FUNCTIONS: usize = 1025;
const FOUR: NonZeroUsize = NonZeroUsize::new(4).unwrap();

fn large_module() -> Module {
    let mut source = String::new();
    for index in 0..FUNCTIONS {
        use std::fmt::Write;
        writeln!(
            source,
            "func @f{index}(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 0\n v2 = iadd v0, v1\n return v2\n}}"
        )
        .unwrap();
    }
    keel_ir::parse(&source).unwrap()
}

#[test]
fn preserves_function_results_across_module_pass_barriers() {
    let mut serial = large_module();
    let mut parallel = serial.clone();
    let passes = [
        "const-fold",
        "instcombine",
        "dce",
        "instcombine",
        "compact-unused-functions",
        "canonicalize",
        "gvn",
        "dce",
    ];
    let options = OptimizeOptions {
        disabled: vec!["const-fold".into()],
        ..Default::default()
    };
    Pipeline::named(&passes)
        .unwrap()
        .options(options.clone())
        .run(&mut serial)
        .unwrap();
    Pipeline::named(&passes)
        .unwrap()
        .options(options)
        .workers(FOUR)
        .run(&mut parallel)
        .unwrap();
    keel_ir::verify_module(&parallel).unwrap();
    assert_eq!(serial.to_string(), parallel.to_string());
}

#[derive(Default)]
struct RecordThreads(Mutex<HashSet<ThreadId>>);

impl Pass for RecordThreads {
    fn name(&self) -> &'static str {
        "record-threads"
    }

    fn description(&self) -> &'static str {
        "record which threads an invocation uses"
    }

    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        self.0.lock().unwrap().insert(thread::current().id());
        false
    }

    fn deterministic(&self) -> bool {
        true
    }

    fn parallel_function_pass(&self) -> bool {
        true
    }
}

#[test]
fn respects_worker_budget_and_keeps_diagnostics_serial() {
    let mut module = large_module();
    for (verification, report, trace, limit) in [
        (false, false, false, None),
        (true, false, false, None),
        (false, true, false, None),
        (false, false, true, None),
        (false, false, false, Some(2)),
    ] {
        let pass = RecordThreads::default();
        let options = OptimizeOptions {
            verification: if verification {
                keel_opt::VerificationPolicy::Enabled
            } else {
                keel_opt::VerificationPolicy::Disabled
            },
            bisect_limit: limit,
            bisect_verbose: trace,
            ..Default::default()
        };
        let mut observations = OptimizationReport::default();
        let mut trace_bytes = Vec::new();
        let mut pipeline = Pipeline::new(&[&pass]).workers(FOUR).options(options);
        if report {
            pipeline = pipeline.report(&mut observations);
        }
        if trace {
            pipeline = pipeline.trace(&mut trace_bytes);
        }
        pipeline.run(&mut module).unwrap();
        let threads = pass.0.into_inner().unwrap();
        let expect_parallel = !cfg!(any(debug_assertions, feature = "profile-with-tracy"))
            && !verification
            && !report
            && !trace
            && limit.is_none();
        assert_eq!(threads.len(), if expect_parallel { FOUR.get() } else { 1 });
        assert!(threads.contains(&thread::current().id()));
        if report {
            assert_eq!(observations.invocations.len(), FUNCTIONS);
        }
        if trace {
            let trace = String::from_utf8(trace_bytes).unwrap();
            assert_eq!(trace.lines().count(), FUNCTIONS);
            assert_eq!(trace.lines().next(), Some("bisect: 1 record-threads @f0"));
        }
    }
}
