//! Thread-local backend counters and timing. Wrap a compilation in
//! [`capture`] to collect them. Other threads and compilations are independent.
//! [`Counter`]s, cover firings and per-function attribution are always
//! available; stage timings other than [`Stage::Function`] need the
//! `backend-profile` feature.
//! Inclusive times nest; self times subtract instrumented child scopes.
use crate::cover::Cover;
use std::{
    cell::RefCell,
    collections::HashMap,
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
/// Static counts of instructions the backends emit through their shared
/// location helpers: operand reloads, result spills, allocation copies and
/// rematerialization. They are not dynamic execution counts. Spilled values
/// that an x86-64 instruction reads or writes in place as a memory operand
/// are not counted.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Counter {
    /// Register-to-spill-slot stores.
    SpillStores,
    /// Spill-slot-to-register loads.
    Reloads,
    /// Register-to-register copies between allocated locations.
    RegisterMoves,
    RematConstants,
    RematStackAddresses,
}
impl Counter {
    pub const ALL: [Self; 5] = [
        Self::SpillStores,
        Self::Reloads,
        Self::RegisterMoves,
        Self::RematConstants,
        Self::RematStackAddresses,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::SpillStores => "spill_stores",
            Self::Reloads => "reloads",
            Self::RegisterMoves => "register_moves",
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
/// One function's totals. A function compiled more than once (an AArch64
/// leaf-call body that was rejected and recompiled) keeps a single entry.
#[derive(Debug)]
pub struct FunctionProfile {
    pub name: String,
    pub stages: [Timing; Stage::ALL.len()],
    pub counters: [u64; Counter::ALL.len()],
    /// Roots each cover claimed, indexed by [`Cover`].
    pub covers: [u64; Cover::ALL.len()],
}
#[derive(Debug)]
pub struct Report {
    pub stages: [Timing; Stage::ALL.len()],
    pub counters: [u64; Counter::ALL.len()],
    pub covers: [u64; Cover::ALL.len()],
    pub functions: Vec<FunctionProfile>,
}
impl Default for Report {
    fn default() -> Self {
        Self {
            stages: Default::default(),
            counters: Default::default(),
            covers: [0; Cover::ALL.len()],
            functions: Vec::new(),
        }
    }
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
    names: HashMap<String, usize>,
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

/// Record the roots that covers claimed in one function's selection, only
/// while capture is active.
pub(crate) fn count_covers(selection: &crate::select::Selection) {
    RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(recorder) = slot.as_mut() else {
            return;
        };
        for (_, cover) in selection.claims() {
            recorder.report.covers[cover as usize] += 1;
            if let Some(index) = recorder.function {
                recorder.report.functions[index].covers[cover as usize] += 1;
            }
        }
    });
}

/// Drop the current function's counts, for output that is discarded and
/// compiled again. Timings stay, since the time was spent.
pub(crate) fn discard_function_counters() {
    RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(recorder) = slot.as_mut() else {
            return;
        };
        let Some(index) = recorder.function else {
            return;
        };
        let function = &mut recorder.report.functions[index];
        let counters = std::mem::take(&mut function.counters);
        let covers = std::mem::replace(&mut function.covers, [0; Cover::ALL.len()]);
        for (total, discarded) in recorder.report.counters.iter_mut().zip(counters) {
            *total -= discarded;
        }
        for (total, discarded) in recorder.report.covers.iter_mut().zip(covers) {
            *total -= discarded;
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
/// Attribute counts and timings to function `name` until the guard drops.
pub(crate) fn function(name: &str) -> Guard {
    let previous = RECORDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let recorder = slot.as_mut()?;
        let functions = &mut recorder.report.functions;
        let index = *recorder.names.entry(name.to_owned()).or_insert_with(|| {
            functions.push(FunctionProfile {
                name: name.to_owned(),
                stages: [Timing::default(); Stage::ALL.len()],
                counters: [0; Counter::ALL.len()],
                covers: [0; Cover::ALL.len()],
            });
            functions.len() - 1
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
        count(Counter::SpillStores);
        let (_, report) = capture(|| {
            count(Counter::Reloads);
            {
                let _function = function("first");
                count(Counter::Reloads);
                count(Counter::SpillStores);
            }
            let _function = function("second");
            count(Counter::SpillStores);
        });
        assert_eq!(report.counters, [2, 2, 0, 0, 0]);
        assert_eq!(report.functions[0].counters, [1, 1, 0, 0, 0]);
        assert_eq!(report.functions[1].counters, [1, 0, 0, 0, 0]);
        assert_eq!(capture(|| ()).1.counters, [0; Counter::ALL.len()]);
    }

    #[test]
    fn recompiled_functions_share_one_entry_without_discarded_counts() {
        let (_, report) = capture(|| {
            {
                let _function = function("leaf");
                count(Counter::RegisterMoves);
                discard_function_counters();
            }
            let other = function("other");
            count(Counter::Reloads);
            drop(other);
            let _function = function("leaf");
            count(Counter::RegisterMoves);
        });
        let names: Vec<_> = report.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["leaf", "other"]);
        assert_eq!(report.functions[0].counters, [0, 0, 1, 0, 0]);
        assert_eq!(report.counters, [0, 1, 1, 0, 0]);
        assert_eq!(
            report.functions[0].stages[Stage::Function as usize].calls,
            2
        );
    }

    #[cfg(feature = "backend-profile")]
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
