//! Machine code generation: arm64 macOS ([`aarch64`]) and x86-64 Linux
//! ([`x86_64`]).

pub mod aarch64;
mod abi;
mod data;
mod dwarf;
mod fold;
mod parallel_copy;
mod regalloc;
mod select;
mod sysv;
pub mod x86_64;

pub use regalloc::Pools;

use regalloc::Loc;

use keel_ir::Module;
pub use keel_ir::Target;

/// Optional target instruction-selection passes. They preserve IR semantics,
/// but may trade instruction count against register pressure and scheduling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionOptions {
    /// Fuse eligible single-use binary trees into target instructions.
    pub trees: bool,
    /// Omit operands already encoded as immediates/addresses from liveness.
    pub folded_operands: bool,
    /// Prefer generated operand affinities among free permitted registers.
    pub register_hints: bool,
}
impl SelectionOptions {
    pub const ALL: Self = Self {
        trees: true,
        folded_operands: true,
        register_hints: true,
    };
}

/// Compiles a module into one assembly file for `target`.
pub fn compile(module: &Module, target: Target) -> Result<String, CodegenError> {
    compile_with_options(module, target, SelectionOptions::default())
}

/// Compile with explicit optional instruction-selection passes.
pub fn compile_with_options(
    module: &Module,
    target: Target,
    options: SelectionOptions,
) -> Result<String, CodegenError> {
    match target {
        Target::Aarch64AppleDarwin => {
            aarch64::compile_with_options(module, &aarch64::REGISTERS, options)
        }
        Target::X86_64LinuxGnu => x86_64::compile_with_options(module, &x86_64::REGISTERS, options),
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

/// The register of a register location.
pub(crate) fn loc_reg(loc: Loc) -> u8 {
    match loc {
        Loc::Reg(r) | Loc::FReg(r) => r,
        Loc::Stack(_) => unreachable!("not a register"),
    }
}

/// Splits `n` bytes into naturally aligned loads: 8-byte pieces, then at
/// most one each of 4, 2 and 1. Returns (offset, size) pairs.
pub(crate) fn pieces(n: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while n - at >= 8 {
        out.push((at, 8));
        at += 8;
    }
    for size in [4, 2, 1] {
        if n - at >= size {
            out.push((at, size));
            at += size;
        }
    }
    out
}
