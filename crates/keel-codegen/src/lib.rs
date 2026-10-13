//! Machine code generation. Only aarch64 (Apple) for now.

pub mod aarch64;
mod parallel_copy;
mod regalloc;

pub use regalloc::Pools;

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodegenError {
    pub func: String,
    pub message: String,
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}: {}", self.func, self.message)
    }
}

impl std::error::Error for CodegenError {}
