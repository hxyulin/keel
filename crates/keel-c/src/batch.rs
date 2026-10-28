//! Caller-owned concurrency for independent preprocessed translation units.
use crate::{Diagnostic, FunctionBinding, Options};
use rayon::prelude::*;
use std::{fmt, num::NonZeroUsize};

/// One preprocessed translation unit and its compilation policy.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    pub source: &'a str,
    pub path: &'a str,
    pub options: Options,
    pub binding: FunctionBinding,
}

/// A reusable, bounded worker pool. It never uses Rayon's global pool.
pub struct BatchCompiler {
    pool: rayon::ThreadPool,
}

impl fmt::Debug for BatchCompiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchCompiler")
            .field("workers", &self.pool.current_num_threads())
            .finish()
    }
}

/// The requested worker pool could not be created.
#[derive(Debug)]
pub struct BatchCompilerError(rayon::ThreadPoolBuildError);

impl fmt::Display for BatchCompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for BatchCompilerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl BatchCompiler {
    /// Creates a reusable pool with exactly `workers` threads.
    ///
    /// # Errors
    /// Returns an error if the operating system cannot create the pool.
    pub fn new(workers: NonZeroUsize) -> Result<Self, BatchCompilerError> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers.get())
            .stack_size(16 * 1024 * 1024)
            .build()
            .map(|pool| Self { pool })
            .map_err(BatchCompilerError)
    }

    /// Compiles every input, returning results in input order, including errors.
    /// Each job owns its parser, arenas and lowering state. A failed input does
    /// not cancel other inputs or change which diagnostic they return.
    pub fn compile(&self, inputs: &[Input<'_>]) -> Vec<Result<keel_ir::Module, Diagnostic>> {
        self.pool.install(|| {
            inputs
                .par_iter()
                .map(|input| {
                    crate::compile_with_binding(
                        input.source,
                        input.path,
                        input.options,
                        input.binding,
                    )
                })
                .collect()
        })
    }
}
