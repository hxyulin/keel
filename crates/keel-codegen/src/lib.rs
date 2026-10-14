//! Machine code generation. Only aarch64 (Apple) for now.

pub mod aarch64;
mod abi;
mod data;
mod dwarf;
mod parallel_copy;
mod regalloc;

pub use regalloc::Pools;

use std::fmt;

/// An object file format's conventions for names and sections, for the
/// code shared by the targets.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    MachO,
}

impl Format {
    /// The prefix of labels that stay in the assembly file and do not
    /// become symbols.
    pub(crate) fn local(self) -> &'static str {
        match self {
            Format::MachO => "L",
        }
    }

    /// The assembler name of symbol `name`: Mach-O prefixes C names with
    /// an underscore.
    pub(crate) fn symbol(self, name: &str) -> String {
        match self {
            Format::MachO => format!("_{name}"),
        }
    }

    /// The section directive's operands for DWARF section `.debug_<name>`.
    pub(crate) fn debug_section(self, name: &str) -> String {
        match self {
            Format::MachO => format!("__DWARF,__debug_{name},regular,debug"),
        }
    }
}

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
