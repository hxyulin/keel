//! Machine code generation: arm64 macOS ([`aarch64`]) and x86-64 Linux
//! ([`x86_64`]).

// Timing instrumentation is absent from ordinary builds.
macro_rules! backend_scope {
    ($stage:ident) => {
        #[cfg(feature = "backend-profile")]
        let _backend_scope = crate::backend_profile::scope(crate::backend_profile::Stage::$stage);
    };
}
#[cfg(feature = "backend-profile")]
pub mod backend_profile;

pub mod aarch64;
mod abi;
mod data;
mod dwarf;
mod fold;
mod integer_division;
mod metadata;
mod packed_absolute;
mod parallel_copy;
pub mod regalloc;
pub mod select;
mod stack_frame;
mod sysv;
pub mod tuning;
mod vector;
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
    let selector = OptionsSelector(options);
    compile_with_pipeline(
        module,
        target,
        &CodegenPipeline {
            selector: &selector,
            allocator: &regalloc::AllocationTier::Full,
        },
    )
}

/// Independently replaceable selection and allocation stages.
pub struct CodegenPipeline<'a> {
    pub selector: &'a dyn InstructionSelector,
    pub allocator: &'a dyn regalloc::RegisterAllocator,
}

impl CodegenPipeline<'_> {
    pub(crate) fn validate_target(&self, target: Target) -> Result<(), CodegenError> {
        self.selector
            .validate_target(target)
            .map_err(|message| CodegenError {
                func: "<target>".into(),
                message: message.into(),
            })
    }
}

/// Plan target instructions and the operands that remain live for allocation.
/// `folds` describes operands encoded directly by the target emitter.
pub mod x86_features;

pub trait InstructionSelector {
    /// EXPERIMENTAL: replicate bounded AArch64 switch tails after allocation.
    /// Disabled by default, including for all built-in selection tiers.
    #[cfg(feature = "experimental-dispatch")]
    fn experimental_aarch64_dispatch_tails(&self) -> bool {
        false
    }

    fn x86_features(&self) -> keel_asm_ir::x86::Features {
        keel_asm_ir::x86::Features::default()
    }
    fn validate_target(&self, _target: Target) -> Result<(), &'static str> {
        Ok(())
    }
    fn name(&self) -> &'static str;
    fn select(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> select::Selection;
}

/// Explicit opt-in to bounded AArch64 dispatch-tail replication. Selection and
/// allocation policy are unchanged; Fast allocation ignores this experiment.
/// The wrapped selector still owns target validation and instruction planning.
/// Requires the `experimental-dispatch` Cargo feature; absent from normal builds.
#[cfg(feature = "experimental-dispatch")]
pub struct ExperimentalDispatchTails<'a>(pub &'a dyn InstructionSelector);
#[cfg(feature = "experimental-dispatch")]
impl InstructionSelector for ExperimentalDispatchTails<'_> {
    fn experimental_aarch64_dispatch_tails(&self) -> bool {
        true
    }
    fn x86_features(&self) -> keel_asm_ir::x86::Features {
        self.0.x86_features()
    }
    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        self.0.validate_target(target)
    }
    fn name(&self) -> &'static str {
        "experimental-dispatch-tails"
    }
    fn select(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> select::Selection {
        self.0.select(func, target, folds)
    }
}

pub struct OptionsSelector(pub SelectionOptions);
impl InstructionSelector for OptionsSelector {
    fn name(&self) -> &'static str {
        "options"
    }
    fn select(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> select::Selection {
        profiling::scope!("instruction-selection");
        let mut selection = select::Selection::compute_with_options(func, target, self.0);
        if self.0.folded_operands {
            selection.fold_operands(func, folds);
        }
        selection
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionTier {
    #[default]
    Baseline,
    Trees,
    Full,
}
impl SelectionTier {
    pub fn options(self) -> SelectionOptions {
        match self {
            Self::Baseline => SelectionOptions::default(),
            Self::Trees => SelectionOptions {
                trees: true,
                ..SelectionOptions::default()
            },
            Self::Full => SelectionOptions::ALL,
        }
    }
}
impl InstructionSelector for SelectionTier {
    fn name(&self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Trees => "trees",
            Self::Full => "full",
        }
    }
    fn select(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> select::Selection {
        OptionsSelector(self.options()).select(func, target, folds)
    }
}

pub fn compile_with_pipeline(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
) -> Result<String, CodegenError> {
    profiling::scope!("codegen", target.triple());
    match target {
        Target::Aarch64AppleDarwin => {
            aarch64::compile_with_pipeline(module, &aarch64::REGISTERS, pipeline)
        }
        Target::X86_64LinuxGnu => {
            x86_64::compile_with_pipeline(module, &x86_64::REGISTERS, pipeline)
        }
    }
}

/// Produce a shared assembly program for AArch64 Mach-O or x86-64 ELF.
/// Both backends emit typed instructions and metadata directly. Explicit text
/// fragments are reserved for supported user inline assembly.
pub fn compile_program(
    module: &Module,
    target: Target,
) -> Result<keel_asm_ir::Program, CodegenError> {
    compile_program_with_pipeline(
        module,
        target,
        &CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &regalloc::AllocationTier::Full,
        },
    )
}

pub fn compile_program_with_pipeline(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
) -> Result<keel_asm_ir::Program, CodegenError> {
    match target {
        Target::Aarch64AppleDarwin => {
            aarch64::compile_program_with_pipeline(module, &aarch64::REGISTERS, pipeline)
        }
        Target::X86_64LinuxGnu => {
            x86_64::compile_program_with_pipeline(module, &x86_64::REGISTERS, pipeline)
        }
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
    /// an underscore; ELF quotes names to disambiguate Intel syntax keywords.
    pub(crate) fn symbol(self, name: &str) -> String {
        match self {
            Format::MachO => format!("_{name}"),
            Format::Elf => name.to_owned(),
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
        Loc::Stack(_) | Loc::Remat(_) => unreachable!("not a register"),
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
