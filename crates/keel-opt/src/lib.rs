//! Optimization passes over keel IR.
//!
//! A [`Pass`] returns whether it changed anything. Most passes work on one
//! function at a time; module passes such as inlining use `run_module`.
//! Function passes compute the analyses they need
//! (CFG, dominators) themselves, through a cache that reuses them while the
//! CFG is unchanged; see `analysis`.
//!
//! Passes must preserve valid IR regardless of verification policy. Repeated
//! checks after passes and canonical restoration default on with debug assertions
//! or the `verify-each` feature. Scoped options can override that default.
//! Callers must validate input modules before optimization; the CLI always does.

use keel_ir::{Function, Module, VerifierError, verify};
use std::fmt;

mod adjacent_abs;
mod adjacent_diff;
mod analysis;
mod assembly_symbols;
mod branch_ranges;
mod call_contracts;
mod call_load_reuse;
mod canonicalize;
mod const_fold;
mod constant_args;
mod wasm32;
pub use wasm32::optimize_wasm32_basic;
mod dce;
mod fast;
mod function_policy;
mod gcm;
mod gvn;
mod if_convert;
mod inline;
mod report;
pub use inline::{InlineReport, InlineSkip, run_with_report as inline_with_report};
pub use inline::{
    required_inline_asm, required_inline_asm_with_c_bodies, required_inline_asm_with_c_definitions,
};
pub use report::{InlineSite, IrCounts, OptimizationReport, PassObservation};
mod instcombine;
mod internal_fast_calls;
mod known_bits;
mod load_narrow;
mod load_reuse;
mod loop_memory;
mod loop_rotate;
mod loop_strength_reduce;
mod loop_unroll;
mod loop_vectorize;
pub mod private_aggregate;
mod tail_calls;
pub use loop_vectorize::VectorTarget;

/// Per-invocation immutable target context. Legacy entrypoints retain 128-bit policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptimizationContext {
    pub target: VectorTarget,
    pub capabilities: keel_ir::VectorCapabilities,
}
impl From<VectorTarget> for OptimizationContext {
    fn from(target: VectorTarget) -> Self {
        Self {
            target,
            capabilities: keel_ir::VectorCapabilities::BASELINE,
        }
    }
}

mod memory_alias;
mod memory_forward;
mod module_cleanup;
mod phi_simplify;
mod run_balance_predicate;
mod sccp;
mod scratch;
mod sign_extend_idiom;
mod simplify_cfg;
mod small_memory;
mod stack_address;
mod stack_cache;
mod stack_forward;
mod stack_promote;

/// A replaceable optimization pass over one function.
/// `run` must report true whenever it mutates the function, including aliases.
pub trait Pass: Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn run(&self, func: &mut Function) -> bool;

    /// Module passes return Some after running; function passes use the default.
    fn run_module(&self, _module: &mut Module) -> Option<bool> {
        None
    }

    /// Optional module observation hook; default preserves custom-pass dispatch.
    fn run_module_with_report(
        &self,
        module: &mut Module,
        _sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        self.run_module(module)
    }

    /// Runs a module pass (observed when `sites` is given) and sets
    /// `changed[i]` for each function it may have changed, indexed after the
    /// pass. The default marks every function whenever the module changed.
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let result = match sites {
            Some(sites) => self.run_module_with_report(module, sites),
            None => self.run_module(module),
        }?;
        changed.clear();
        changed.resize(module.functions.len(), result);
        Some(result)
    }

    /// Whether the function is scheduled for this invocation. Checked before
    /// canonical restoration so protected bodies remain unchanged.
    fn enabled_for(&self, _function: &Function) -> bool {
        true
    }

    /// Run a module pass for scheduled functions while inspecting the complete
    /// module. The default admits unscoped hooks only when every body is enabled.
    /// Overrides must preserve unscheduled bodies and any call/ABI contracts.
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        if module.functions.iter().all(enabled) {
            self.run_module_tracked(module, sites, changed)
        } else {
            None
        }
    }

    /// Whether this function pass depends only on the function and target.
    /// The manager then skips a run on a function that its previous run left
    /// unchanged and that nothing has changed since: by determinism and the
    /// `run` contract, that run would change nothing either. Cache entries belong
    /// to a pass instance; names do not identify configured instances.
    fn deterministic(&self) -> bool {
        false
    }

    /// Target-aware passes may override this; ordinary passes ignore it.
    fn run_for_target(&self, func: &mut Function, _target: VectorTarget) -> bool {
        self.run(func)
    }

    /// The manager runs a pass that requires canonical form with this, once
    /// every instruction is canonical. Passes may rely on that to skip checks.
    fn run_canonical(&self, func: &mut Function, target: VectorTarget) -> bool {
        self.run_for_target(func, target)
    }

    /// Additive context dispatch preserves existing target-aware overrides.
    fn run_with_context(&self, func: &mut Function, context: OptimizationContext) -> bool {
        self.run_for_target(func, context.target)
    }
    /// Preserve canonical overrides independently of ordinary target dispatch.
    /// A new context-aware pass that requires canonical form MUST also override
    /// this hook, usually by forwarding to its `run_with_context`. Overriding only
    /// `run_with_context` affects noncanonical dispatch only: the default here
    /// deliberately calls the legacy `run_canonical` override for compatibility.
    fn run_canonical_with_context(
        &self,
        func: &mut Function,
        context: OptimizationContext,
    ) -> bool {
        self.run_canonical(func, context.target)
    }

    /// Request the structural form documented in `docs/compiler/simplification.md`.
    /// The manager restores it on entry when an earlier pass changed the IR.
    fn requires_canonical_form(&self) -> bool {
        false
    }

    /// Only return true if every instruction is canonical on exit.
    fn produces_canonical_form(&self) -> bool {
        false
    }
}

struct BuiltinPass {
    pub name: &'static str,
    pub description: &'static str,
    pub run: fn(&mut Function) -> bool,
}

const SMALL_MEMORY: BuiltinPass = BuiltinPass {
    name: "small-memory",
    description: "expand ordinary constant memory intrinsics of at most 32 bytes",
    run: small_memory::run,
};

const BUILTINS: &[BuiltinPass] = &[
    BuiltinPass {
        name: "const-fold",
        description: "evaluate instructions whose operands are constants",
        run: const_fold::run,
    },
    BuiltinPass {
        name: "simplify-cfg",
        description: "fold constant branches, skip empty blocks, merge straight-line blocks, remove unreachable blocks",
        run: simplify_cfg::run,
    },
    BuiltinPass {
        name: "gvn",
        description: "replace equivalent pure instructions with a dominating computation",
        run: gvn::run,
    },
    BuiltinPass {
        name: "dce",
        description: "remove unused pure instructions and block parameters",
        run: dce::run,
    },
    BuiltinPass {
        name: "gcm",
        description: "move pure instructions out of loops and into the branches that need them; copy constants to each block using them",
        run: gcm::run,
    },
    BuiltinPass {
        name: "sccp",
        description: "propagate constants through executable edges and block parameters, then evaluate instructions",
        run: sccp::run,
    },
    BuiltinPass {
        name: "instcombine",
        description: "combine integer identities, fold constants, and reduce power-of-two arithmetic",
        run: instcombine::run,
    },
    BuiltinPass {
        name: "phi-simplify",
        description: "replace trivial block parameters with their dominating incoming value",
        run: phi_simplify::run,
    },
    BuiltinPass {
        name: "memory-forward",
        description: "forward exact-address ordinary loads and stores within a block",
        run: memory_forward::run,
    },
    BuiltinPass {
        name: "known-bits",
        description: "prove integer bits and ranges to remove redundant masks and fold comparisons",
        run: known_bits::run,
    },
    BuiltinPass {
        name: "canonicalize",
        description: "resolve operand aliases and normalize integer operand order and comparisons",
        run: canonicalize::run,
    },
    BuiltinPass {
        name: "stack-forward",
        description: "forward bytes in private fixed stack objects exposed by inlining",
        run: stack_forward::run,
    },
    BuiltinPass {
        name: "stack-forward-cfg",
        description: "forward private stack bytes and clean CFG when a conditional becomes constant (opt-in)",
        run: stack_forward::run_with_cfg,
    },
    BuiltinPass {
        name: "branch-ranges",
        description: "fold integer comparisons using ranges from dominating conditional edges",
        run: branch_ranges::run,
    },
    BuiltinPass {
        name: "call-contracts",
        description: "trim direct noreturn call continuations and lower direct nounwind invokes",
        run: call_contracts::run,
    },
    BuiltinPass {
        name: "stack-promote",
        description: "promote initialized disjoint private stack fields to loop SSA (opt-in)",
        run: stack_promote::run,
    },
    BuiltinPass {
        name: "if-convert",
        description: "convert bounded nontrapping scalar diamonds to selects (opt-in)",
        run: if_convert::run,
    },
    BuiltinPass {
        name: "adjacent-abs",
        description: "convert a private scalar absolute-value triangle without moving memory",
        run: adjacent_abs::run,
    },
    BuiltinPass {
        name: "adjacent-diff",
        description: "ordered adjacent i32 absolute-difference SIMD reduction (opt-in)",
        run: adjacent_diff::run,
    },
    BuiltinPass {
        name: "run-balance-predicate",
        description: "predicate a bounded wrapping running-state recurrence (opt-in)",
        run: run_balance_predicate::run,
    },
    BuiltinPass {
        name: "adjacent-diff-narrow",
        description: "ordered adjacent i32 reduction with narrow history (opt-in)",
        run: adjacent_diff::run_narrow,
    },
    BuiltinPass {
        name: "stack-promote-large",
        description: "bounded private field promotion with pruned SSA for large functions (opt-in)",
        run: stack_promote::run_large,
    },
];

impl Pass for BuiltinPass {
    fn name(&self) -> &'static str {
        self.name
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn run(&self, func: &mut Function) -> bool {
        (self.run)(func)
    }
    fn requires_canonical_form(&self) -> bool {
        self.name == "instcombine"
    }
    fn run_canonical(&self, func: &mut Function, target: VectorTarget) -> bool {
        if self.name == "instcombine" {
            instcombine::run_canonical(func)
        } else {
            self.run_for_target(func, target)
        }
    }
    fn produces_canonical_form(&self) -> bool {
        self.name == "canonicalize"
    }
    fn deterministic(&self) -> bool {
        true
    }
}

pub const PASSES: &[&dyn Pass] = &[
    &fast::Cfg,
    &fast::Local,
    &SMALL_MEMORY,
    &sign_extend_idiom::SignExtendIdiom,
    &BUILTINS[0],
    &BUILTINS[1],
    &BUILTINS[2],
    &BUILTINS[3],
    &BUILTINS[4],
    &BUILTINS[5],
    &BUILTINS[6],
    &BUILTINS[7],
    &BUILTINS[8],
    &BUILTINS[9],
    &BUILTINS[10],
    &BUILTINS[11],
    &BUILTINS[12],
    &BUILTINS[13],
    &BUILTINS[14],
    &BUILTINS[15],
    &BUILTINS[16],
    &stack_cache::StackCache,
    &load_reuse::LoadReuse,
    &load_narrow::LoadNarrow,
    &loop_memory::LoopMemory,
    &loop_rotate::LoopRotate,
    &loop_strength_reduce::LoopStrengthReduce,
    &loop_unroll::LoopUnroll,
    &loop_vectorize::LoopVectorize,
    &loop_vectorize::LoopReduce,
    &loop_vectorize::AffineLoopReduce,
    &loop_vectorize::CastLoopReduce,
    &loop_vectorize::MultiLoopReduce,
    &loop_vectorize::FastLoopReduce,
    &inline::Inline,
    &inline::InlineAggregateReserve,
    &inline::InlineNoninterposable,
    &BUILTINS[17],
    &BUILTINS[18],
    &BUILTINS[19],
    &BUILTINS[20],
    &inline::InlineCheapReserve,
    &BUILTINS[21],
    &call_load_reuse::CallLoadReuse,
    &internal_fast_calls::InternalFastCalls,
    &tail_calls::TailCalls,
    &constant_args::ConstantArgs,
    &module_cleanup::EmptyCalls,
    &module_cleanup::UnusedFunctions,
    &private_aggregate::PrivateAggregate,
    &private_aggregate::PrivateAggregateCleanup,
    &inline::InlineSize,
];

/// Controls repeated checks after passes and canonical restoration only.
/// Does not change pass correctness requirements or canonical restoration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerificationPolicy {
    /// Enabled with debug assertions or the `verify-each` Cargo feature.
    #[default]
    Default,
    Enabled,
    Disabled,
}
impl VerificationPolicy {
    pub const fn enabled(self) -> bool {
        match self {
            Self::Default => cfg!(debug_assertions) || cfg!(feature = "verify-each"),
            Self::Enabled => true,
            Self::Disabled => false,
        }
    }
}

/// Scoped optimizer configuration. Input must already be validated, in all modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptimizeOptions {
    pub verification: VerificationPolicy,
}

fn verify_after(func: &Function, pass: &'static str, enabled: bool) -> Result<(), OptError> {
    if enabled {
        verify(func).map_err(|errors| OptError::Verifier {
            pass,
            func: func.name.clone(),
            errors,
        })?;
    }
    Ok(())
}

// Disabled checks return exact zero without starting a verification timer.
fn observe_verification(
    enabled: bool,
    reporting: bool,
    check: impl FnOnce() -> Result<(), OptError>,
) -> (Result<(), OptError>, u128) {
    if !enabled {
        return (Ok(()), 0);
    }
    let start = reporting.then(std::time::Instant::now);
    let result = check();
    (result, start.map_or(0, |start| start.elapsed().as_nanos()))
}

/// Independent optimizer effort; Full runs three bounded rounds of direct-call
/// inlining, scalar/memory cleanup, code motion, and guarded loop optimization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptimizationTier {
    #[default]
    None,
    /// Fixed CFG/local cleanup plus size-capped single-stream sum reductions.
    Fast,
    Basic,
    /// Three bounded inlining rounds and scalar/memory cleanup, without loop expansion.
    Balanced,
    /// Scalar cleanup and bounded helper inlining, without unrolling or vectorization.
    Size,
    Full,
}
impl OptimizationTier {
    pub fn pipeline(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::Fast => &[
                "fast-cfg",
                "fast-local",
                "fast-cfg",
                "dce",
                "fast-loop-reduce",
            ],
            Self::Basic => &[
                "simplify-cfg",
                "canonicalize",
                "instcombine",
                "call-contracts",
                "simplify-cfg",
                "dce",
            ],
            Self::Balanced => BALANCED_PIPELINE,
            Self::Size => SIZE_PIPELINE,
            Self::Full => DEFAULT_PIPELINE,
        }
    }
}

/// Balanced release pipeline. Loop expansion remains in Full. GCM is also
/// omitted here: its placement after promotion regressed Lua dispatch in the
/// balanced schedule; Full retains its separately measured pass ordering.
pub const BALANCED_PIPELINE: &[&str] = &[
    "private-aggregate-cleanup",
    "inline-noninterposable",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "small-memory",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "inline-noninterposable",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "inline-noninterposable",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "constant-args",
    "known-bits",
    "canonicalize",
    "instcombine",
    "simplify-cfg",
    "dce",
    "empty-calls",
    "dce",
    "unused-functions",
    "private-aggregate-cleanup",
    "internal-fast-calls",
    "tail-calls",
];

/// Size policy. Bounded hinted, constant-argument and single-use helper inlining
/// exposes dead branches. Omits memory expansion, code motion, unrolling and vectorization.
/// Mandatory frontend inlining still runs to satisfy language/operand contracts.
/// This is an IR growth policy, not a guarantee about final machine-code size.
pub const SIZE_PIPELINE: &[&str] = &[
    "private-aggregate-cleanup",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "inline-size",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "inline-size",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "constant-args",
    "known-bits",
    "canonicalize",
    "instcombine",
    "simplify-cfg",
    "dce",
    "empty-calls",
    "dce",
    "unused-functions",
    "private-aggregate-cleanup",
    "internal-fast-calls",
    "tail-calls",
];

pub fn optimize_with_tier(module: &mut Module, tier: OptimizationTier) -> Result<(), OptError> {
    run_passes(module, tier.pipeline())
}

const FULL_ROUND: [&str; 16] = [
    "inline-cheap-reserve",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "small-memory",
    "stack-forward",
    "canonicalize",
    "instcombine",
    "dce",
    "gcm",
    "adjacent-abs",
    "loop-vectorize",
    "loop-reduce",
];

const FULL_ROUNDS: usize = 3;

const FULL_EPILOGUE: [&str; 26] = [
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "simplify-cfg",
    "loop-strength-reduce",
    "canonicalize",
    "instcombine",
    "dce",
    "loop-unroll",
    "loop-reduce-multi",
    "loop-rotate",
    "phi-simplify",
    "dce",
    "constant-args",
    "simplify-cfg",
    "known-bits",
    "simplify-cfg",
    "dce",
    "empty-calls",
    "dce",
    "unused-functions",
    "dce",
    "private-aggregate-cleanup",
    "internal-fast-calls",
    "tail-calls",
];

// Inlining freezes its templates at each invocation. Cleanup exposes smaller
// helpers for subsequent rounds, while each inliner retains its growth limits.
// Keep the measured order, including code motion and loop passes between rounds;
// cheaper schedules did not preserve the same workload/code-size tradeoffs.
// Convert only private abs triangles before vectorization; this moves a single
// wrapping negation, never a memory access or a potentially trapping operation.
// The epilogue reuses memory facts before introducing loop recurrences, then
// unrolls before extended reductions and rotation. Keep phi cleanup late and
// omit an extra GCM: broader schedules regressed Lua fib in app screening.
// Finalize private call transport only after IR rewrites. Physical preservation
// summaries are built by codegen from the final selected/allocated leaf bodies.
// Scalarize bounded private pairs before finalizing calling conventions, and
// run the associated cleanup only if transport actually changed the module.
// Also do this before freezing the first inliner templates: scalar cleanup
// removes aggregate homes and avoids charging byval snapshots at every site.
// Retain the late invocation for signatures exposed by intervening rewrites.
const fn full_pipeline() -> [&'static str; 1 + FULL_ROUND.len() * FULL_ROUNDS + FULL_EPILOGUE.len()]
{
    let mut passes = [""; 1 + FULL_ROUND.len() * FULL_ROUNDS + FULL_EPILOGUE.len()];
    passes[0] = "private-aggregate-cleanup";
    let mut i = 0;
    while i < FULL_ROUND.len() * FULL_ROUNDS {
        passes[1 + i] = FULL_ROUND[i % FULL_ROUND.len()];
        i += 1;
    }
    let mut j = 0;
    while j < FULL_EPILOGUE.len() {
        passes[1 + i + j] = FULL_EPILOGUE[j];
        j += 1;
    }
    passes
}

/// The passes `keel compile -O` runs: private aggregate cleanup, three bounded
/// rounds, then memory/loop, known-bit, call and reachability cleanup.
pub const DEFAULT_PIPELINE: &[&str] = &full_pipeline();

pub fn find_pass(name: &str) -> Option<&'static dyn Pass> {
    PASSES.iter().copied().find(|p| p.name() == name)
}

#[derive(Debug)]
pub enum OptError {
    UnknownPass(String),
    /// A function requested an optimization policy that cannot be implemented.
    FunctionPolicy {
        func: String,
        message: String,
    },
    /// A pass left a function the verifier rejects: a bug in the pass.
    Verifier {
        pass: &'static str,
        func: String,
        errors: Vec<VerifierError>,
    },
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptError::FunctionPolicy { func, message } => write!(f, "@{func}: {message}"),
            OptError::UnknownPass(name) => {
                let names: Vec<&str> = PASSES.iter().map(|p| p.name()).collect();
                write!(f, "unknown pass `{name}`; passes are: {}", names.join(", "))
            }
            OptError::Verifier { pass, func, errors } => {
                write!(f, "pass `{pass}` broke @{func}:")?;
                for e in errors {
                    write!(f, "\n  {e}")?;
                }
                Ok(())
            }
        }
    }
}

/// Runs the named passes, in order, on every function in `module`.
pub fn run_passes(module: &mut Module, names: &[&str]) -> Result<(), OptError> {
    run_passes_for_target(module, names, VectorTarget::Portable)
}

/// Runs named passes with target SIMD capabilities; ordinary passes ignore the target.
pub fn run_passes_for_target(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
) -> Result<(), OptError> {
    run_passes_for_target_with_options(module, names, target, OptimizeOptions::default())
}

/// Named pipeline with a scoped repeated-verification policy.
pub fn run_passes_for_target_with_options(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_for_target_with_options(module, &passes, target, options)
}

/// Runs custom or built-in passes, restoring declared canonical entry
/// requirements and applying the default repeated-verification policy.
pub fn run_pipeline(module: &mut Module, passes: &[&dyn Pass]) -> Result<(), OptError> {
    run_pipeline_for_target(module, passes, VectorTarget::Portable)
}

/// Runs a custom pipeline with target SIMD capabilities and default verification.
pub fn run_pipeline_for_target(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
) -> Result<(), OptError> {
    run_pipeline_for_target_with_options(module, passes, target, OptimizeOptions::default())
}

/// Per-function change tracking. A function's version is renewed whenever
/// a pass changes it; a deterministic pass that left a function unchanged
/// records that version and is skipped while it still holds.
struct Versions<'a> {
    version: Vec<u64>,
    next: u64,
    /// Per pass instance, the version each function was left clean at, or
    /// [`NOT_CLEAN`]; dense, since most passes visit every function.
    clean: Vec<(&'a dyn Pass, Vec<u64>)>,
    changed: Vec<bool>,
}

const NOT_CLEAN: u64 = u64::MAX;

impl<'a> Versions<'a> {
    fn new(functions: usize) -> Self {
        Self {
            version: (0..functions as u64).collect(),
            next: functions as u64,
            clean: Vec::new(),
            changed: Vec::new(),
        }
    }

    /// Names describe passes; configured instances have independent cache entries.
    fn slot(&mut self, pass: &'a dyn Pass) -> usize {
        match self
            .clean
            .iter()
            .position(|&(cached, _)| std::ptr::eq(cached, pass))
        {
            Some(slot) => slot,
            None => {
                self.clean.push((pass, Vec::new()));
                self.clean.len() - 1
            }
        }
    }

    fn renew(&mut self, index: usize) {
        self.version[index] = self.next;
        self.next += 1;
    }

    /// After a module pass filled `self.changed`; new functions are fresh.
    fn module_changed(&mut self, canonical: &mut Vec<bool>) {
        let functions = self.changed.len();
        canonical.resize(functions, false);
        let old = self.version.len();
        self.version.resize(functions, 0);
        for (index, is_canonical) in canonical.iter_mut().enumerate() {
            if self.changed[index] || index >= old {
                self.renew(index);
                *is_canonical = false;
            }
        }
    }

    fn is_clean(&self, pass: &dyn Pass, slot: usize, index: usize) -> bool {
        pass.deterministic() && self.clean[slot].1.get(index) == Some(&self.version[index])
    }

    fn record(&mut self, pass: &dyn Pass, slot: usize, index: usize, changed: bool) {
        if changed {
            self.renew(index);
        } else if pass.deterministic() {
            let clean = &mut self.clean[slot].1;
            if clean.len() <= index {
                clean.resize(self.version.len().max(index + 1), NOT_CLEAN);
            }
            clean[index] = self.version[index];
        }
    }
}

/// Drops cached analyses when a pipeline ends, however it ends.
struct ClearAnalyses;

impl Drop for ClearAnalyses {
    fn drop(&mut self) {
        analysis::clear();
        scratch::clear();
    }
}

/// Custom pipeline with scoped options; canonical restoration always runs.
pub fn run_pipeline_for_target_with_options(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    run_pipeline_with_context_and_options(module, passes, target.into(), options)
}

pub fn run_pipeline_with_context_and_options(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    if module.special_entry_contract().is_some() {
        return Ok(());
    }
    if let Some(result) = function_policy::run(module, passes, context, None, options) {
        return result;
    }
    run_pipeline_with_context_raw(module, passes, context, options)
}

fn run_pipeline_with_context_raw(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    execute_pipeline(module, passes, context, None, options)
}

/// One mutation/verification state machine; observation only measures its boundaries.
fn execute_pipeline(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    mut report: Option<&mut OptimizationReport>,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    // Preserve opaque entry/return bodies until target-specific lowering can
    // establish their register, frame and control-flow contracts.
    if module.special_entry_contract().is_some() {
        return Ok(());
    }
    use std::time::Instant;
    let target = context.target;
    let reporting = report.is_some();
    let verification_enabled = options.verification.enabled();
    profiling::scope!("optimization");
    let _clear = ClearAnalyses;
    let mut canonical = vec![false; module.functions.len()];
    let mut versions = Versions::new(module.functions.len());
    for (pipeline_index, &pass) in passes.iter().enumerate() {
        profiling::scope!("pass", pass.name());
        let before = reporting.then(|| IrCounts::module(module));
        let mut inline_sites = reporting.then(Vec::new);
        let start = reporting.then(Instant::now);
        let module_changed =
            pass.run_module_tracked(module, inline_sites.as_mut(), &mut versions.changed);
        let pass_ns = start.map_or(0, |start| start.elapsed().as_nanos());
        if let Some(changed) = module_changed {
            versions.module_changed(&mut canonical);
            let (result, verification_ns) =
                observe_verification(verification_enabled, reporting, || {
                    module
                        .functions
                        .iter()
                        .try_for_each(|func| verify_after(func, pass.name(), verification_enabled))
                });
            if let Some(report) = report.as_deref_mut() {
                report.invocations.push(PassObservation {
                    pipeline_index,
                    pass: pass.name(),
                    target,
                    function: None,
                    changed,
                    pass_ns,
                    verification_enabled,
                    verification_ns,
                    restoration_ns: 0,
                    restoration_verification_ns: 0,
                    restoration_changed: None,
                    before: before.expect("reporting captured module counts"),
                    after: IrCounts::module(module),
                    inline_sites: inline_sites.expect("reporting enabled module observations"),
                });
            }
            result?;
            continue;
        }
        let slot = versions.slot(pass);
        for (index, func) in module.functions.iter_mut().enumerate() {
            if !pass.enabled_for(func) {
                continue;
            }
            let before = reporting.then(|| IrCounts::function(func));
            if versions.is_clean(pass, slot, index) {
                // A skipped run's input was canonical when the pass required it.
                canonical[index] |= pass.requires_canonical_form();
                canonical[index] |= pass.produces_canonical_form();
                if let Some(report) = report.as_deref_mut() {
                    let before = before.expect("reporting captured function counts");
                    report.invocations.push(PassObservation {
                        pipeline_index,
                        pass: pass.name(),
                        target,
                        function: Some(func.name.clone()),
                        changed: false,
                        pass_ns: 0,
                        verification_enabled,
                        verification_ns: 0,
                        restoration_ns: 0,
                        restoration_verification_ns: 0,
                        restoration_changed: None,
                        after: before.clone(),
                        before,
                        inline_sites: Vec::new(),
                    });
                }
                continue;
            }
            let mut restoration_ns = 0;
            let mut restoration_verification_ns = 0;
            let mut restoration_changed = None;
            if pass.requires_canonical_form() && !canonical[index] {
                let start = reporting.then(Instant::now);
                let restored = canonicalize::run(func);
                if restored {
                    versions.renew(index);
                }
                restoration_changed = Some(restored);
                restoration_ns = start.map_or(0, |start| start.elapsed().as_nanos());
                let (result, ns) = observe_verification(verification_enabled, reporting, || {
                    verify_after(func, "canonicalize", verification_enabled)
                });
                restoration_verification_ns = ns;
                // Failed restoration stops before the requested pass; it has no
                // completed invocation to append to the report.
                result?;
                canonical[index] = true;
            }
            let start = reporting.then(Instant::now);
            let changed = if pass.requires_canonical_form() {
                pass.run_canonical_with_context(func, context)
            } else {
                pass.run_with_context(func, context)
            };
            let pass_ns = start.map_or(0, |start| start.elapsed().as_nanos());
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            versions.record(pass, slot, index, changed);
            let (result, verification_ns) =
                observe_verification(verification_enabled, reporting, || {
                    verify_after(func, pass.name(), verification_enabled)
                });
            if let Some(report) = report.as_deref_mut() {
                report.invocations.push(PassObservation {
                    pipeline_index,
                    pass: pass.name(),
                    target,
                    function: Some(func.name.clone()),
                    changed,
                    pass_ns,
                    verification_enabled,
                    verification_ns,
                    restoration_ns,
                    restoration_verification_ns,
                    restoration_changed,
                    before: before.expect("reporting captured function counts"),
                    after: IrCounts::function(func),
                    inline_sites: Vec::new(),
                });
            }
            result?;
        }
    }
    Ok(())
}

/// Runs [`DEFAULT_PIPELINE`].
pub fn optimize(module: &mut Module) -> Result<(), OptError> {
    run_passes(module, DEFAULT_PIPELINE)
}

/// Opt-in counterpart to `run_passes_for_target`. Partial observations remain
/// available if a verifier fails; this performs exactly the ordinary checks.
pub fn run_passes_for_target_with_report(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
    report: &mut OptimizationReport,
) -> Result<(), OptError> {
    run_passes_for_target_with_report_and_options(
        module,
        names,
        target,
        report,
        OptimizeOptions::default(),
    )
}

/// Observed named pipeline with scoped options, matching the ordinary runner.
pub fn run_passes_for_target_with_report_and_options(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
    report: &mut OptimizationReport,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_for_target_with_report_and_options(module, &passes, target, report, options)
}

/// Observed custom pipeline; order and verifier policy match the ordinary runner.
pub fn run_pipeline_for_target_with_report(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
    report: &mut OptimizationReport,
) -> Result<(), OptError> {
    run_pipeline_for_target_with_report_and_options(
        module,
        passes,
        target,
        report,
        OptimizeOptions::default(),
    )
}

/// Observed custom pipeline with scoped options. Each observation records its
/// policy so a report may contain invocations from multiple pipeline calls.
pub fn run_pipeline_for_target_with_report_and_options(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
    report: &mut OptimizationReport,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    run_pipeline_with_context_and_report_and_options(module, passes, target.into(), report, options)
}

pub fn run_pipeline_with_context_and_report_and_options(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    report: &mut OptimizationReport,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    if module.special_entry_contract().is_some() {
        return Ok(());
    }
    if let Some(result) = function_policy::run(module, passes, context, Some(report), options) {
        return result;
    }
    run_pipeline_with_context_and_report_raw(module, passes, context, report, options)
}

fn run_pipeline_with_context_and_report_raw(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    report: &mut OptimizationReport,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    execute_pipeline(module, passes, context, Some(report), options)
}

/// Named pipeline using an explicit capability context.
pub fn run_passes_with_context_and_options(
    module: &mut Module,
    names: &[&str],
    context: OptimizationContext,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_with_context_and_options(module, &passes, context, options)
}
/// Observed named pipeline using exactly the ordinary context dispatch.
pub fn run_passes_with_context_and_report_and_options(
    module: &mut Module,
    names: &[&str],
    context: OptimizationContext,
    report: &mut OptimizationReport,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_with_context_and_report_and_options(module, &passes, context, report, options)
}
