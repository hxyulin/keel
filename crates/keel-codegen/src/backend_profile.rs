//! Opt-in, thread-local backend timing. Enable `backend-profile` and wrap a
//! compilation in [`capture`]. Other threads and compilations are independent.
//! Inclusive times nest; self times subtract instrumented child scopes.
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Stage {
    Module,
    Function,
    Selection,
    Allocation,
    Liveness,
    Intervals,
    Scan,
    SpillSlots,
    Folding,
    Lowering,
    Relaxation,
    Data,
    Debug,
    FoldValues,
    CompareFusion,
    LoweringEntry,
    AllocationAnnotations,
    LoweringBody,
    LoweringFinish,
}
impl Stage {
    pub const ALL: [Self; 19] = [
        Self::Module,
        Self::Function,
        Self::Selection,
        Self::Allocation,
        Self::Liveness,
        Self::Intervals,
        Self::Scan,
        Self::SpillSlots,
        Self::Folding,
        Self::Lowering,
        Self::Relaxation,
        Self::Data,
        Self::Debug,
        Self::FoldValues,
        Self::CompareFusion,
        Self::LoweringEntry,
        Self::AllocationAnnotations,
        Self::LoweringBody,
        Self::LoweringFinish,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Module => "module_overhead",
            Self::Function => "function_preparation_cleanup",
            Self::Selection => "selection",
            Self::Allocation => "allocation_overhead",
            Self::Liveness => "liveness",
            Self::Intervals => "live_intervals",
            Self::Scan => "linear_scan",
            Self::SpillSlots => "spill_slots",
            Self::Folding => "fold_analysis",
            Self::Lowering => "instruction_lowering",
            Self::Relaxation => "branch_relaxation",
            Self::Data => "data_emission",
            Self::Debug => "debug_emission",
            Self::FoldValues => "fold_values",
            Self::CompareFusion => "compare_fusion",
            Self::LoweringEntry => "lowering_entry",
            Self::AllocationAnnotations => "allocation_annotations",
            Self::LoweringBody => "lowering_body",
            Self::LoweringFinish => "lowering_finish",
        }
    }
}
/// Static emission-helper counts, not dynamic execution counts or total memory traffic.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Counter {
    SpillHelperLoads,
    SpillHelperStores,
    RematConstants,
    RematStackAddresses,
}
impl Counter {
    pub const ALL: [Self; 4] = [
        Self::SpillHelperLoads,
        Self::SpillHelperStores,
        Self::RematConstants,
        Self::RematStackAddresses,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::SpillHelperLoads => "spill_helper_loads",
            Self::SpillHelperStores => "spill_helper_stores",
            Self::RematConstants => "rematerialized_constants",
            Self::RematStackAddresses => "rematerialized_stack_addresses",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub inclusive: Duration,
    pub self_time: Duration,
    pub calls: u64,
}
impl Timing {
    fn add(&mut self, elapsed: Duration, children: Duration) {
        self.inclusive += elapsed;
        self.self_time += elapsed.saturating_sub(children);
        self.calls += 1;
    }
}
#[derive(Debug)]
pub struct FunctionProfile {
    pub name: String,
    pub stages: [Timing; Stage::ALL.len()],
    pub counters: [u64; Counter::ALL.len()],
}
#[derive(Debug, Default)]
pub struct Report {
    pub stages: [Timing; Stage::ALL.len()],
    pub counters: [u64; Counter::ALL.len()],
    pub functions: Vec<FunctionProfile>,
}
struct Active {
    start: Instant,
    children: Duration,
    stage: Stage,
    function: Option<usize>,
}
#[derive(Default)]
struct Recorder {
    report: Report,
    stack: Vec<Active>,
    function: Option<usize>,
}
thread_local! { static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) }; }

/// Collect one synchronous compilation. Nested captures on the same thread
/// are rejected; an unwinding compilation clears the collector for reuse.
pub fn capture<T>(compile: impl FnOnce() -> T) -> (T, Report) {
    RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        assert!(
            slot.is_none(),
            "nested backend profile captures are unsupported"
        );
        *slot = Some(Recorder::default());
    });
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RECORDER.with(|cell| {
                cell.borrow_mut().take();
            });
        }
    }
    let _reset = Reset;
    let result = compile();
    let recorder = RECORDER.with(|cell| cell.borrow_mut().take().unwrap());
    assert!(recorder.stack.is_empty());
    (result, recorder.report)
}

/// Record one helper invocation only while capture is active.
pub(crate) fn count(counter: Counter) {
    RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(recorder) = slot.as_mut() else {
            return;
        };
        recorder.report.counters[counter as usize] += 1;
        if let Some(index) = recorder.function {
            recorder.report.functions[index].counters[counter as usize] += 1;
        }
    });
}

pub(crate) struct Guard {
    active: bool,
    previous_function: Option<Option<usize>>,
}
pub(crate) fn scope(stage: Stage) -> Guard {
    let active = RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(recorder) = slot.as_mut() else {
            return false;
        };
        recorder.stack.push(Active {
            start: Instant::now(),
            children: Duration::ZERO,
            stage,
            function: recorder.function,
        });
        true
    });
    Guard {
        active,
        previous_function: None,
    }
}
pub(crate) fn function(name: &str) -> Guard {
    let previous = RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let recorder = slot.as_mut()?;
        let index = recorder.report.functions.len();
        recorder.report.functions.push(FunctionProfile {
            name: name.to_owned(),
            stages: [Timing::default(); Stage::ALL.len()],
            counters: [0; Counter::ALL.len()],
        });
        Some(recorder.function.replace(index))
    });
    let mut guard = scope(Stage::Function);
    guard.previous_function = previous;
    guard
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        RECORDER.with(|cell| {
            let mut slot = cell.borrow_mut();
            let recorder = slot.as_mut().unwrap();
            let active = recorder.stack.pop().unwrap();
            let elapsed = active.start.elapsed();
            recorder.report.stages[active.stage as usize].add(elapsed, active.children);
            if let Some(index) = active.function {
                recorder.report.functions[index].stages[active.stage as usize]
                    .add(elapsed, active.children);
            }
            if let Some(parent) = recorder.stack.last_mut() {
                parent.children += elapsed;
            }
            if let Some(previous) = self.previous_function {
                recorder.function = previous;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_follow_function_scope_and_reset_between_captures() {
        count(Counter::SpillHelperStores);
        let (_, report) = capture(|| {
            count(Counter::SpillHelperLoads);
            {
                let _function = function("first");
                count(Counter::SpillHelperLoads);
                count(Counter::SpillHelperStores);
            }
            let _function = function("second");
            count(Counter::SpillHelperStores);
        });
        assert_eq!(report.counters, [2, 2, 0, 0]);
        assert_eq!(report.functions[0].counters, [1, 1, 0, 0]);
        assert_eq!(report.functions[1].counters, [0, 1, 0, 0]);
        assert_eq!(capture(|| ()).1.counters, [0; Counter::ALL.len()]);
    }

    #[test]
    fn coarse_lowering_stages_preserve_output_and_reconcile() {
        for (source, compare_fusion_calls) in [
            ("func @f(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}", 0),
            (
                "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 0\n v2 = icmp.eq v0,v1\n brif v2,block1,block2\nblock1:\n return v0\nblock2:\n return v1\n}",
                1,
            ),
        ] {
            let module = keel_ir::parse(source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let target = crate::Target::Aarch64AppleDarwin;
            let plain = crate::compile(&module, target).unwrap();
            let (profiled, report) = capture(|| crate::compile(&module, target).unwrap());
            assert_eq!(plain, profiled);
            for stage in [
                Stage::Folding,
                Stage::FoldValues,
                Stage::Lowering,
                Stage::LoweringEntry,
                Stage::AllocationAnnotations,
                Stage::LoweringBody,
                Stage::LoweringFinish,
                Stage::Relaxation,
            ] {
                assert_eq!(report.stages[stage as usize].calls, 1, "{}", stage.name());
            }
            assert_eq!(
                report.stages[Stage::CompareFusion as usize].calls,
                compare_fusion_calls
            );
            let total: Duration = report.stages.iter().map(|s| s.self_time).sum();
            assert_eq!(total, report.stages[Stage::Module as usize].inclusive);
            let (program, structured_report) =
                capture(|| crate::compile_program(&module, target).unwrap());
            assert_eq!(program, crate::compile_program(&module, target).unwrap());
            assert_eq!(
                structured_report.stages[Stage::AllocationAnnotations as usize].calls,
                0
            );
            assert_eq!(
                structured_report.stages[Stage::CompareFusion as usize].calls,
                compare_fusion_calls
            );
            let total: Duration = structured_report.stages.iter().map(|s| s.self_time).sum();
            assert_eq!(
                total,
                structured_report.stages[Stage::Module as usize].inclusive
            );
        }
    }

    #[test]
    fn nested_self_times_reconcile_and_capture_resets() {
        let (_, report) = capture(|| {
            let _root = scope(Stage::Module);
            let _function = function("test");
            let _allocation = scope(Stage::Allocation);
            {
                let _live = scope(Stage::Liveness);
                std::hint::black_box((0..100).sum::<u64>());
            }
        });
        assert_eq!(report.functions[0].name, "test");
        assert_eq!(report.stages[Stage::Liveness as usize].calls, 1);
        let total: Duration = report.stages.iter().map(|s| s.self_time).sum();
        assert_eq!(total, report.stages[Stage::Module as usize].inclusive);
        let _ = std::panic::catch_unwind(|| {
            capture(|| {
                let _root = scope(Stage::Module);
                panic!("test unwind");
            })
        });
        assert!(capture(|| ()).1.functions.is_empty());
    }
}
