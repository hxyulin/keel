//! Native function codegen on caller-started worker threads, with output
//! handed to the caller in module order.
use crate::{
    CodegenError, CodegenPipeline, InstructionSelector, Request as SerialRequest,
    regalloc::RegisterAllocator,
};
use std::{
    collections::VecDeque,
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

/// One worker, borrowing the prepared module while it compiles functions.
pub struct Job<'job>(Box<dyn FnOnce() + Send + 'job>);

impl<'job> Job<'job> {
    fn new(work: impl FnOnce() + Send + 'job) -> Self {
        Self(Box::new(work))
    }

    /// Run this worker until no functions are left.
    pub fn run(self) {
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
impl<'a> Pipeline<'a> {
    fn serial(self) -> CodegenPipeline<'a> {
        CodegenPipeline {
            selector: self.selector,
            allocator: self.allocator,
        }
    }
}

pub(crate) struct Execution<'exec, 'a> {
    pipeline: Pipeline<'a>,
    spawn: &'exec dyn Fn(Job<'a>),
    workers: NonZeroUsize,
    budget: NonZeroUsize,
}

/// Immutable policies and the worker count and budget for a whole module.
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
    /// Worker jobs to start, at most one per function.
    pub workers: NonZeroUsize,
    /// Maximum functions compiled ahead of the sink, including those being
    /// compiled.
    pub budget: NonZeroUsize,
}

/// Compile one whole module on worker jobs that run beside the calling thread.
///
/// Validation, symbols and Darwin private-call proofs are prepared once. Only
/// function selection, allocation and emission run on workers. No modules are
/// cloned and private calling conventions and linkage are unchanged. The sink
/// runs on the calling thread in module order while the workers compile later
/// functions, so a slow function holds back only the sink, not the workers.
/// At most `budget` functions are compiled ahead of the sink; one very large
/// function can still require unbounded memory. Data, debug metadata,
/// lifecycle records and x86 source validation stay on the calling thread.
/// The first error in source order wins; later results are dropped. Workers
/// may compile past that error, up to the budget. Custom stages must have no
/// externally visible effects whose order matters.
///
/// `spawn` receives `workers` jobs, or one per function if that is fewer. It
/// must start each job without waiting for it, and the caller must join every
/// started job after this returns, before the module is dropped: for example,
/// `spawn` calls [`std::thread::Scope::spawn`] in a scope around this call.
/// Each job compiles functions until none are left or codegen has stopped.
/// Keel creates no threads.
///
/// Active backend-profile capture uses the serial route so TLS observations
/// remain complete, and starts no jobs. Optimizer schedules are unaffected;
/// this API consumes an already optimized, immutable module.
///
/// # Errors
/// Returns the same target, operand and IR validation errors as serial codegen.
/// # Panics
/// Propagates worker panics. Panics if every job is dropped unrun while
/// functions remain.
pub fn program_chunks<'module>(
    module: &'module keel_ir::Module,
    options: Options<'module>,
    spawn: &dyn Fn(Job<'module>),
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    let request = request(options);
    if crate::backend_profile::is_active() {
        return request.program_chunks(module, sink);
    }
    match crate::emission::compile_with_executor(
        module,
        &request,
        true,
        crate::emission::Sink::Program(sink),
        Some(&execution(options, spawn)),
    )? {
        crate::emission::Output::Program(program) => sink(program),
        _ => unreachable!("structured emission requested"),
    }
    Ok(())
}

/// [`program_chunks`] for assembly text: `sink` receives the same pieces as
/// [`crate::Request::text_chunks`], in order, and they join to the same text.
///
/// # Errors
/// As [`program_chunks`]. Pieces already given to `sink` before an error
/// are not taken back.
/// # Panics
/// As [`program_chunks`].
pub fn text_chunks<'module>(
    module: &'module keel_ir::Module,
    options: Options<'module>,
    spawn: &dyn Fn(Job<'module>),
    sink: &mut dyn FnMut(&str),
) -> Result<(), CodegenError> {
    let request = request(options);
    if crate::backend_profile::is_active() {
        return request.text_chunks(module, sink);
    }
    profiling::scope!("codegen", options.target.triple());
    match crate::emission::compile_with_executor(
        module,
        &request,
        false,
        crate::emission::Sink::Text(sink),
        Some(&execution(options, spawn)),
    )? {
        crate::emission::Output::Text(text) => sink(&text),
        _ => unreachable!("text emission requested"),
    }
    Ok(())
}

fn request(options: Options<'_>) -> SerialRequest<'_> {
    let mut request =
        SerialRequest::new(options.target, options.pipeline.serial()).emission(options.emission);
    request.pools = options.pools;
    request
}

fn execution<'exec, 'a>(
    options: Options<'a>,
    spawn: &'exec dyn Fn(Job<'a>),
) -> Execution<'exec, 'a> {
    Execution {
        pipeline: options.pipeline,
        spawn,
        workers: options.workers,
        budget: options.budget,
    }
}

/// Compiles functions `0..count` with `compile` on the workers and hands
/// each result to `consume` on the calling thread, in order. Stops at the
/// first error `consume` returns.
pub(crate) fn ordered<'job, T: Send + 'job>(
    execution: &Execution<'_, 'job>,
    count: usize,
    compile: impl Fn(usize, CodegenPipeline<'_>) -> T + Sync + Send + 'job,
    mut consume: impl FnMut(usize, T) -> Result<(), CodegenError>,
) -> Result<(), CodegenError> {
    let workers = execution.workers.get().min(count);
    let budget = execution.budget.get();
    let window = Arc::new(Window {
        state: Mutex::new(WindowState {
            next: 0,
            base: 0,
            results: VecDeque::new(),
            workers,
            stop: false,
        }),
        ready: Condvar::new(),
        room: Condvar::new(),
    });
    // Stops the workers however the consumer leaves, including by a panic.
    let _stop = Stop(&window);
    let compile = Arc::new(compile);
    let pipeline = execution.pipeline;
    for _ in 0..workers {
        let window = Arc::clone(&window);
        let compile = Arc::clone(&compile);
        // Dropped with the job, run or not, so the consumer never waits
        // for a worker that will not start.
        let alive = Alive(Arc::clone(&window));
        (execution.spawn)(Job::new(move || {
            let _alive = alive;
            while let Some(index) = window.claim(count, budget) {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    compile(index, pipeline.serial())
                }));
                window.finish(index, result);
            }
        }));
    }
    for index in 0..count {
        let result = match window.take() {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        consume(index, result)?;
    }
    Ok(())
}

/// The functions claimed but not yet consumed.
struct Window<T> {
    state: Mutex<WindowState<T>>,
    /// The consumer waits here for the next result.
    ready: Condvar,
    /// Workers wait here for room in the budget.
    room: Condvar,
}

struct WindowState<T> {
    /// The next function to claim.
    next: usize,
    /// The next function to consume; `results[0]` is its result.
    base: usize,
    results: VecDeque<Option<std::thread::Result<T>>>,
    /// Worker jobs not yet dropped.
    workers: usize,
    stop: bool,
}

impl<T> Window<T> {
    fn lock(&self) -> MutexGuard<'_, WindowState<T>> {
        // Workers catch compile panics, so nothing panics holding the lock.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The next function to compile, once it is within `budget` of the
    /// consumer, or `None` when none are left.
    fn claim(&self, count: usize, budget: usize) -> Option<usize> {
        let mut state = self.lock();
        loop {
            if state.stop || state.next >= count {
                return None;
            }
            if state.next - state.base < budget {
                break;
            }
            state = self.room.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        let index = state.next;
        state.next += 1;
        state.results.push_back(None);
        Some(index)
    }

    fn finish(&self, index: usize, result: std::thread::Result<T>) {
        let mut state = self.lock();
        let slot = index - state.base;
        state.results[slot] = Some(result);
        if slot == 0 {
            self.ready.notify_one();
        }
    }

    /// The next result in order, once it is ready.
    fn take(&self) -> std::thread::Result<T> {
        let mut state = self.lock();
        loop {
            match state.results.front() {
                Some(Some(_)) => break,
                // A claimed function is always finished.
                Some(None) => {}
                None => assert!(state.workers > 0, "every codegen job must run"),
            }
            state = self.ready.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        let result = state
            .results
            .pop_front()
            .flatten()
            .expect("result is ready");
        state.base += 1;
        self.room.notify_one();
        result
    }
}

struct Alive<T>(Arc<Window<T>>);
impl<T> Drop for Alive<T> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.workers -= 1;
        self.0.ready.notify_one();
    }
}

struct Stop<'w, T>(&'w Window<T>);
impl<T> Drop for Stop<'_, T> {
    fn drop(&mut self) {
        self.0.lock().stop = true;
        self.0.room.notify_all();
    }
}
