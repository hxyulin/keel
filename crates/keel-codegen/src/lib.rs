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
mod emission;
pub use emission::{CodegenGoal, EmissionOptions, UnwindTables};
mod fold;
mod integer_division;
mod kinds;
mod metadata;
mod packed_absolute;
mod parallel_copy;
pub mod regalloc;
pub mod select;
mod stack_frame;
mod symbols;
mod sysv;
mod text;
pub mod tuning;
mod vector;
pub mod x86_64;

pub use regalloc::Pools;

use regalloc::Loc;

use keel_ir::Module;
pub use keel_ir::Target;

pub(crate) fn require_native_target(target: Target) -> Result<(), &'static str> {
    if target == Target::Wasm32UnknownUnknown {
        return Err(
            "unsupported target wasm32-unknown-unknown: native code generation is unavailable",
        );
    }
    Ok(())
}

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

impl std::fmt::Debug for CodegenPipeline<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodegenPipeline")
            .field("selector", &self.selector.name())
            .field("allocator", &self.allocator.name())
            .finish()
    }
}

impl CodegenPipeline<'_> {
    pub(crate) fn validate_target(&self, target: Target) -> Result<(), CodegenError> {
        require_native_target(target).map_err(|message| CodegenError {
            func: "<target>".into(),
            message: message.into(),
        })?;
        crate::x86_features::validate_target_features(target, self.selector.x86_features())
            .map_err(|message| CodegenError {
                func: "<target>".into(),
                message: message.into(),
            })?;
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
    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        require_native_target(target)
    }
    fn name(&self) -> &'static str;
    fn select(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> select::Selection;
    /// [`Self::select`], given the raw IR use count of each value, at most
    /// two, which the targets count anyway.
    fn select_with_uses(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        _uses: &[u8],
    ) -> select::Selection {
        self.select(func, target, folds)
    }
}

/// Explicit opt-in to bounded AArch64 dispatch-tail replication. Selection and
/// allocation policy are unchanged; Fast allocation ignores this experiment.
/// The wrapped selector still owns target validation and instruction planning.
/// Requires the `experimental-dispatch` Cargo feature; absent from normal builds.
#[cfg(feature = "experimental-dispatch")]
pub struct ExperimentalDispatchTails<'a>(pub &'a dyn InstructionSelector);
#[cfg(feature = "experimental-dispatch")]
impl std::fmt::Debug for ExperimentalDispatchTails<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExperimentalDispatchTails")
            .field(&self.0.name())
            .finish()
    }
}
#[cfg(feature = "experimental-dispatch")]
impl InstructionSelector for ExperimentalDispatchTails<'_> {
    fn experimental_aarch64_dispatch_tails(&self) -> bool {
        true
    }
    fn x86_features(&self) -> keel_asm_ir::x86::Features {
        self.0.x86_features()
    }
    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        require_native_target(target)?;
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
    fn select_with_uses(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> select::Selection {
        self.0.select_with_uses(func, target, folds, uses)
    }
}

#[derive(Debug)]
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
    fn select_with_uses(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> select::Selection {
        profiling::scope!("instruction-selection");
        let mut selection = select::Selection::compute_with_uses(func, target, self.0, Some(uses));
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
    fn select_with_uses(
        &self,
        func: &keel_ir::Function,
        target: select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> select::Selection {
        OptionsSelector(self.options()).select_with_uses(func, target, folds, uses)
    }
}

pub(crate) fn validate_odr(module: &Module, target: Target) -> Result<(), CodegenError> {
    if module
        .functions
        .iter()
        .any(|f| f.attrs.weak && (f.local || f.weak_odr))
        || module.data.iter().any(|d| {
            d.weak && (d.local || d.thread_local || d.weak_odr || (d.hidden && d.init.is_none()))
        })
    {
        return Err(CodegenError { func: "linkage".into(), message: "weak requires nonlocal non-TLS storage, no weak_odr, and default visibility for undefined references".into() });
    }
    for data in &module.data {
        let tagged = data.init.as_ref().is_some_and(|init| {
            init.items
                .iter()
                .any(|i| matches!(i, keel_ir::DataItem::RttiName { .. }))
        });
        if data.hidden && (data.local || data.init.is_none()) {
            return Err(CodegenError {
                func: data.name.clone(),
                message: "hidden requires a nonlocal definition".into(),
            });
        }
        if tagged && target != Target::Aarch64AppleDarwin {
            return Err(CodegenError {
                func: data.name.clone(),
                message: "rtti_name requires AArch64 Mach-O definitions".into(),
            });
        }
    }
    if let Some(f) = module.functions.iter().find(|f| f.hidden && f.local) {
        return Err(CodegenError {
            func: f.name.clone(),
            message: "hidden requires a nonlocal definition".into(),
        });
    }
    for (name, invalid) in module
        .functions
        .iter()
        .filter(|f| f.weak_odr)
        .map(|f| (&f.name, f.local))
        .chain(
            module
                .data
                .iter()
                .filter(|d| d.weak_odr)
                .map(|d| (&d.name, d.local || d.thread_local || d.init.is_none())),
        )
    {
        let message = if invalid {
            "weak_odr requires a nonlocal, non-thread-local definition"
        } else if !matches!(target, Target::Aarch64AppleDarwin | Target::X86_64LinuxGnu) {
            "weak_odr requires the AArch64 Mach-O or x86-64 ELF backend"
        } else {
            continue;
        };
        return Err(CodegenError {
            func: name.clone(),
            message: message.into(),
        });
    }
    Ok(())
}

pub fn compile_with_pipeline(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
) -> Result<String, CodegenError> {
    compile_with_pipeline_and_emission(module, target, pipeline, EmissionOptions::default())
}

/// Compile source assembly with explicit address and unwind emission policies.
///
/// # Errors
/// Returns target, operand or IR validation errors.
pub fn compile_with_pipeline_and_emission(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    options: EmissionOptions,
) -> Result<String, CodegenError> {
    profiling::scope!("codegen", target.triple());
    match emission::compile(module, target, pipeline, options, false, None)? {
        emission::Output::Text(text) => Ok(text),
        _ => unreachable!("text emission requested"),
    }
}

/// Produce a shared assembly program for AArch64 Mach-O or AArch64/x86-64 ELF.
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
    compile_program_with_pipeline_and_emission(module, target, pipeline, EmissionOptions::default())
}

/// Compile typed assembly with explicit address and unwind emission policies.
///
/// # Errors
/// Returns target, operand or IR validation errors.
pub fn compile_program_with_pipeline_and_emission(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    options: EmissionOptions,
) -> Result<keel_asm_ir::Program, CodegenError> {
    match emission::compile(module, target, pipeline, options, true, None)? {
        emission::Output::Program(program) => Ok(program),
        _ => unreachable!("structured emission requested"),
    }
}

/// [`compile_program_with_pipeline`] in pieces: `sink` receives consecutive
/// parts of the same program, at least one per function, so an incremental
/// assembler need not hold the whole module's program at once.
pub fn compile_program_chunks_with_pipeline(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    compile_program_chunks_with_pipeline_and_emission(
        module,
        target,
        pipeline,
        EmissionOptions::default(),
        sink,
    )
}

/// Stream typed assembly with explicit address and unwind emission policies.
/// Compiler-created CFI is controlled before chunks reach the assembler.
///
/// # Errors
/// Returns target, operand or IR validation errors.
pub fn compile_program_chunks_with_pipeline_and_emission(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    options: EmissionOptions,
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    match emission::compile(module, target, pipeline, options, true, Some(&mut *sink))? {
        emission::Output::Program(program) => sink(program),
        _ => unreachable!("structured emission requested"),
    }
    Ok(())
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

/// Targets must not silently truncate a newly introduced SIMD value width or
/// widen a narrow load/store to a 16-byte memory access.
pub(crate) fn validate_vector_width(
    module: &Module,
    minimum: u32,
    maximum: u32,
) -> Result<(), CodegenError> {
    validate_vector_width_with_memory_policy(module, minimum, maximum, |_, _, _| false)
}

/// A target may omit ABI layout leaves only after proving byte-copy MEMORY
/// transport. Ordinary scalar/SSA widths and every other target stay unchanged.
pub(crate) fn validate_vector_width_with_memory_policy(
    module: &Module,
    minimum: u32,
    maximum: u32,
    byte_copy: impl Fn(Option<keel_ir::X86VectorAbi>, &keel_ir::Layout, bool) -> bool,
) -> Result<(), CodegenError> {
    use keel_ir::InstData;
    let unsupported = |ty: keel_ir::Type| {
        ty.vector()
            .is_some_and(|v| v.bytes() < minimum || v.bytes() > maximum)
    };
    let check_signature = |name: &str, sig: &keel_ir::Signature| -> Result<(), CodegenError> {
        for param in sig.params.iter().chain(&sig.returns) {
            let ty = std::iter::once(param.ty).chain(
                param
                    .layout()
                    .into_iter()
                    .filter(|layout| !byte_copy(sig.x86_vector_abi, layout, false))
                    .flat_map(|layout| layout.leaves().into_iter().map(|(_, ty)| ty)),
            );
            for ty in ty {
                if unsupported(ty) {
                    return Err(CodegenError {
                        func: name.into(),
                        message: format!(
                            "{ty} requires a backend with {}-bit SIMD value support and exact-width memory transport",
                            ty.bytes().unwrap() * 8
                        ),
                    });
                }
            }
        }
        Ok(())
    };
    for decl in &module.decls {
        check_signature(&decl.name, &decl.sig)?;
    }
    for f in &module.functions {
        check_signature(&f.name, &f.sig)?;
        for (_, callee) in f.callees() {
            check_signature(&f.name, &callee.sig)?;
        }
        for (_, sig) in f.signatures() {
            check_signature(&f.name, sig)?;
        }
        for (_, i) in f.layout_insts() {
            let mut bad = None;
            let data = f.inst_data(i);
            let sig = match data {
                InstData::Call { func, .. }
                | InstData::ReturnCall { func, .. }
                | InstData::Invoke { func, .. } => Some(&f.callee(*func).sig),
                InstData::CallIndirect { sig, .. } | InstData::ReturnCallIndirect { sig, .. } => {
                    Some(f.signature(*sig))
                }
                _ => None,
            };
            let producer = match data {
                InstData::AggregateVaArg { x86_vector_abi, .. } => *x86_vector_abi,
                _ => sig.and_then(|s| s.x86_vector_abi),
            };
            data.for_each_abi_type_filtered(
                |ty| {
                    if unsupported(ty) {
                        bad = Some(ty);
                    }
                },
                |layout, index| {
                    !byte_copy(
                        producer,
                        layout,
                        index.is_none_or(|n| sig.is_some_and(|s| n >= s.params.len())),
                    )
                },
            );
            if let Some(ty) = bad {
                return Err(CodegenError {
                    func: f.name.clone(),
                    message: format!(
                        "{ty} in aggregate ABI requires a backend with {}-bit SIMD support",
                        ty.bytes().unwrap() * 8
                    ),
                });
            }
        }
        let mut bad = None;
        for_each_current_value_type(f, |ty| {
            if unsupported(ty) {
                bad = Some(ty);
            }
        });
        if let Some(ty) = bad {
            return Err(CodegenError {
                func: f.name.clone(),
                message: format!(
                    "{ty} requires a backend with {}-bit SIMD value support and exact-width memory transport",
                    ty.bytes().unwrap() * 8
                ),
            });
        }
    }
    Ok(())
}

/// Removed instructions retain their value-table entries. Only values still
/// present in the function constrain its machine representation.
fn for_each_current_value_type(f: &keel_ir::Function, mut visit: impl FnMut(keel_ir::Type)) {
    let mut check_value = |value| visit(f.value_type(f.resolve(value)));
    for block in f.blocks() {
        f.block_params(block)
            .iter()
            .copied()
            .for_each(&mut check_value);
    }
    for (_, inst) in f.layout_insts() {
        f.inst_data(inst).for_each_operand(&mut check_value);
        f.inst_results(inst)
            .iter()
            .copied()
            .for_each(&mut check_value);
    }
}

/// Until the matching target transport is implemented, never silently discard
/// an explicit producer ABI, including declarations and indirect signatures.
pub(crate) fn reject_x86_vector_abi(module: &Module, target: &str) -> Result<(), CodegenError> {
    for (name, sig) in module
        .decls
        .iter()
        .map(|d| (d.name.as_str(), &d.sig))
        .chain(module.functions.iter().flat_map(|f| {
            std::iter::once((f.name.as_str(), &f.sig))
                .chain(f.callees().map(move |(_, d)| (f.name.as_str(), &d.sig)))
                .chain(f.signatures().map(move |(_, s)| (f.name.as_str(), s)))
        }))
    {
        if sig.x86_vector_abi.is_some() {
            return Err(CodegenError {
                func: name.into(),
                message: format!(
                    "explicit x86 vector ABI transport is not implemented on {target}"
                ),
            });
        }
    }
    for f in &module.functions {
        if f.layout_insts().any(|(_, i)| {
            matches!(
                f.inst_data(i),
                keel_ir::InstData::VaArg {
                    x86_vector_abi: Some(_),
                    ..
                } | keel_ir::InstData::AggregateVaArg {
                    x86_vector_abi: Some(_),
                    ..
                }
            )
        }) {
            return Err(CodegenError {
                func: f.name.clone(),
                message: format!(
                    "explicit x86 vector ABI transport is not implemented on {target}"
                ),
            });
        }
    }
    Ok(())
}

/// Reject extended values before reaching scalar SIMD lowering on unsupported targets.
pub(crate) fn reject_f80(module: &Module, target: &str) -> Result<(), CodegenError> {
    use keel_ir::Type;
    let signature_has_f80 = |sig: &keel_ir::Signature| {
        sig.params.iter().chain(&sig.returns).any(|p| {
            p.ty == Type::F80
                || p.layout()
                    .is_some_and(|l| l.leaves().iter().any(|(_, t)| *t == Type::F80))
        })
    };
    let error = |name: &str| CodegenError {
        func: name.into(),
        message: format!("f80 values and foreign ABI are unsupported on {target}"),
    };
    for d in &module.decls {
        if signature_has_f80(&d.sig) {
            return Err(error(&d.name));
        }
    }
    for f in &module.functions {
        let mut value_has_f80 = false;
        for_each_current_value_type(f, |ty| value_has_f80 |= ty == Type::F80);
        if signature_has_f80(&f.sig)
            || f.callees().any(|(_, d)| signature_has_f80(&d.sig))
            || f.signatures().any(|(_, s)| signature_has_f80(s))
            || value_has_f80
            || f.layout_insts().any(|(_, i)| {
                let mut found = false;
                f.inst_data(i)
                    .for_each_abi_type(|t| found |= t == Type::F80);
                found
            })
        {
            return Err(error(&f.name));
        }
    }
    Ok(())
}

/// Entry contracts remain representable before a target can lower them.
fn validate_function_entries(module: &keel_ir::Module) -> Result<(), CodegenError> {
    if let Some((name, contract)) = module.special_entry_contract() {
        return Err(CodegenError {
            func: name.into(),
            message: format!("{contract} function entry/return lowering is not implemented"),
        });
    }
    for (name, attrs) in module
        .functions
        .iter()
        .map(|f| (&f.name, &f.attrs))
        .chain(module.decls.iter().map(|d| (&d.name, &d.attrs)))
        .chain(
            module
                .functions
                .iter()
                .flat_map(|f| f.callees().map(|(_, d)| (&d.name, &d.attrs))),
        )
    {
        keel_ir::FunctionPolicy::resolve(attrs).map_err(|message| CodegenError {
            func: name.clone(),
            message,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod vector_owned_rounds;
