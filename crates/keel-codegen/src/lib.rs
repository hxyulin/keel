//! Machine code generation: arm64 macOS ([`aarch64`]) and x86-64 Linux
//! ([`x86_64`]).

pub mod aarch64;
mod abi;
mod data;
mod dwarf;
mod fold;
mod parallel_copy;
mod regalloc;
mod sysv;
pub mod x86_64;

pub use regalloc::Pools;

use keel_ir::Module;

/// A machine and operating system to generate code for.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Target {
    /// arm64 macOS: Apple's variant of AAPCS64, Mach-O.
    Aarch64AppleDarwin,
    /// x86-64 Linux: the System V psABI, ELF.
    X86_64LinuxGnu,
}

impl Target {
    pub const ALL: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];

    /// The machine this program runs on, if keel supports it.
    pub fn host() -> Option<Target> {
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Some(Target::Aarch64AppleDarwin)
        } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            Some(Target::X86_64LinuxGnu)
        } else {
            None
        }
    }

    /// The target triple, as clang and rustc spell it.
    pub fn triple(self) -> &'static str {
        match self {
            Target::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        }
    }

    pub fn from_triple(triple: &str) -> Option<Target> {
        Target::ALL.into_iter().find(|t| t.triple() == triple)
    }
}

/// Compiles a module into one assembly file for `target`.
pub fn compile(module: &Module, target: Target) -> Result<String, CodegenError> {
    match target {
        Target::Aarch64AppleDarwin => aarch64::compile(module),
        Target::X86_64LinuxGnu => x86_64::compile(module),
    }
}

use std::fmt;

/// An object file format's conventions for names and sections, for the
/// code shared by the targets.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    MachO,
    Elf,
}

impl Format {
    /// The prefix of labels that stay in the assembly file and do not
    /// become symbols.
    pub(crate) fn local(self) -> &'static str {
        match self {
            Format::MachO => "L",
            Format::Elf => ".L",
        }
    }

    /// The assembler name of symbol `name`: Mach-O prefixes C names with
    /// an underscore, ELF does not.
    pub(crate) fn symbol(self, name: &str) -> String {
        match self {
            Format::MachO => format!("_{name}"),
            Format::Elf => name.to_string(),
        }
    }

    /// The section directive's operands for DWARF section `.debug_<name>`.
    pub(crate) fn debug_section(self, name: &str) -> String {
        match self {
            Format::MachO => format!("__DWARF,__debug_{name},regular,debug"),
            Format::Elf => format!(".debug_{name},\"\",@progbits"),
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
