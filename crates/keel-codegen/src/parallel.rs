//! Caller-budgeted native function codegen with ordered coordinator callbacks.
use crate::{
    CodegenError, CodegenPipeline, InstructionSelector, Request as SerialRequest,
    regalloc::RegisterAllocator,
};
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    ops::Range,
    sync::{Arc, Mutex},
};

/// One function job borrowing the prepared module for its compilation lifetime.
pub struct Job<'job>(Box<dyn FnMut() + Send + 'job>);

impl<'job> Job<'job> {
    /// Create one job, for a caller-owned executor or its tests.
    pub fn new(work: impl FnOnce() + Send + 'job) -> Self {
        let mut work = Some(work);
        Self(Box::new(move || {
            work.take().expect("job runs exactly once")();
        }))
    }

    /// Run this job exactly once, propagating any panic.
    pub fn run(mut self) {
        (self.0)();
    }
}

impl std::fmt::Debug for Job<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Job { .. }")
    }
}

/// Parallel-safe selection and allocation, without narrowing the serial traits.
#[derive(Clone, Copy)]
pub struct Pipeline<'a> {
    /// The same selector policy used for every function and module validation.
    pub selector: &'a (dyn InstructionSelector + Sync),
    /// The same allocation policy used for every function.
    pub allocator: &'a (dyn RegisterAllocator + Sync),
}
impl std::fmt::Debug for Pipeline<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.serial().fmt(f)
    }
}
impl Pipeline<'_> {
    fn serial(&self) -> CodegenPipeline<'_> {
        CodegenPipeline {
            selector: self.selector,
            allocator: self.allocator,
        }
    }
}

pub(crate) struct Execution<'exec, 'a> {
    pipeline: Pipeline<'a>,
    executor: &'exec dyn Fn(),
    jobs: &'exec RefCell<Vec<Job<'a>>>,
    pub budget: NonZeroUsize,
}

/// Immutable policies and explicit native worker budget for a whole module.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    /// Native target to validate before any job runs.
    pub target: crate::Target,
    /// Shared thread-safe selection and allocation policies.
    pub pipeline: Pipeline<'a>,
    /// Ordinary emission policies, identical to the serial request.
    pub emission: crate::EmissionOptions,
    /// Optional target register subsets, as in the serial request.
    pub pools: Option<&'a crate::Pools<'a>>,
    /// Maximum simultaneous function jobs and buffered results.
    pub budget: NonZeroUsize,
}

/// Compile one whole module using a caller-owned executor and explicit budget.
///
/// Validation, symbols and Darwin private-call proofs are prepared once. Only
/// function selection, allocation and emission run on workers. No modules are
/// cloned and private calling conventions and linkage are unchanged. The sink
/// runs on the calling thread in module order. At most `budget` function results
/// are buffered; one very large function can still require unbounded memory.
/// Data, debug metadata, lifecycle records and x86 source validation stay on the
/// coordinator. The first error in source order wins; later results are dropped.
/// Worker work may run speculatively past that error. Custom stages must have no
/// externally visible effects whose order matters.
///
/// The caller-owned executor callback must run every job exactly once and join
/// every job before returning. Each call contains at most `budget` jobs. Panics
/// must propagate. Jobs must not recursively invoke the executor. Keel creates
/// no threads or worker pool. A callback can borrow a pool for less time than
/// the jobs borrow the module.
///
/// Active backend-profile capture uses the serial route so TLS observations
/// remain complete. Disabled capture introduces no clocks. Optimizer schedules
/// are unaffected; this API consumes an already optimized, immutable module.
///
/// # Errors
/// Returns the same target, operand and IR validation errors as serial codegen.
/// # Panics
/// Propagates executor and worker panics, including an executor omitting a job.
pub fn program_chunks<'module>(
    module: &'module keel_ir::Module,
    options: Options<'module>,
    executor: &impl Fn(Vec<Job<'module>>),
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    let Options {
        target,
        pipeline,
        emission,
        pools,
        budget,
    } = options;
    let mut request = SerialRequest::new(
        target,
        CodegenPipeline {
            selector: pipeline.selector,
            allocator: pipeline.allocator,
        },
    )
    .emission(emission);
    request.pools = pools;
    if crate::backend_profile::is_active() {
        return request.program_chunks(module, sink);
    }
    let jobs = RefCell::new(Vec::new());
    let dispatch = || executor(jobs.take());
    let execution = Execution {
        pipeline,
        executor: &dispatch,
        jobs: &jobs,
        budget,
    };
    match crate::emission::compile_with_executor(
        module,
        &request,
        true,
        crate::emission::Sink::Program(sink),
        Some(&execution),
    )? {
        crate::emission::Output::Program(program) => sink(program),
        _ => unreachable!("structured emission requested"),
    }
    Ok(())
}

pub(crate) fn run<'job, T: Send + 'job>(
    execution: &Execution<'_, 'job>,
    indices: Range<usize>,
    compile: Arc<impl Fn(usize, CodegenPipeline<'_>) -> T + Sync + Send + 'job>,
) -> Vec<T> {
    let mut slots = Vec::with_capacity(indices.len());
    for _ in indices.clone() {
        slots.push(Arc::new(Mutex::new(None)));
    }
    let pipeline = execution.pipeline;
    let mut jobs = Vec::with_capacity(slots.len());
    for (i, slot) in indices.zip(slots.iter().cloned()) {
        let compile = Arc::clone(&compile);
        let job = Job::new(move || {
            let result = compile(i, pipeline.serial());
            *slot.lock().expect("function result lock poisoned") = Some(result);
        });
        jobs.push(job);
    }
    *execution.jobs.borrow_mut() = jobs;
    (execution.executor)();
    let mut results = Vec::with_capacity(slots.len());
    for slot in slots {
        results.push(
            slot.lock()
                .expect("function result lock poisoned")
                .take()
                .expect("executor must run every function job"),
        );
    }
    results
}
