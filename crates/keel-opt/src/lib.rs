//! Optimization passes over keel IR.
//!
//! A [`Pass`] returns whether it changed anything. Most passes work on one
//! function at a time; module passes such as inlining use `run_module`.
//! Function passes compute the analyses they need
//! (CFG, dominators) themselves; there is no analysis cache until one is
//! needed.
//!
//! Passes must preserve valid IR regardless of verification policy. Repeated
//! checks after passes and canonical restoration default on with debug assertions
//! or the `verify-each` feature. Scoped options can override that default.
//! Callers must validate input modules before optimization; the CLI always does.

use keel_ir::{Function, Module, VerifierError, verify};
use std::fmt;

mod adjacent_abs;
mod adjacent_diff;
mod branch_ranges;
mod call_contracts;
mod call_load_reuse;
mod canonicalize;
mod const_fold;
mod dce;
mod fast;
mod gcm;
mod gvn;
mod if_convert;
mod inline;
mod report;
pub use inline::{InlineReport, InlineSkip, run_with_report as inline_with_report};
pub use report::{InlineSite, IrCounts, OptimizationReport, PassObservation};
mod instcombine;
mod known_bits;
mod load_narrow;
mod load_reuse;
mod loop_memory;
mod loop_rotate;
mod loop_strength_reduce;
mod loop_unroll;
mod loop_vectorize;
pub use loop_vectorize::VectorTarget;
mod memory_alias;
mod memory_forward;
mod phi_simplify;
mod run_balance_predicate;
mod sccp;
mod sign_extend_idiom;
mod simplify_cfg;
mod small_memory;
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

    /// Target-aware passes may override this; ordinary passes ignore it.
    fn run_for_target(&self, func: &mut Function, _target: VectorTarget) -> bool {
        self.run(func)
    }

    /// Request the structural form documented in `docs/simplification.md`.
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
    fn produces_canonical_form(&self) -> bool {
        self.name == "canonicalize"
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
    check: impl FnOnce() -> Result<(), OptError>,
) -> (Result<(), OptError>, u128) {
    if !enabled {
        return (Ok(()), 0);
    }
    let start = std::time::Instant::now();
    let result = check();
    (result, start.elapsed().as_nanos())
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
            Self::Full => DEFAULT_PIPELINE,
        }
    }
}

pub fn optimize_with_tier(module: &mut Module, tier: OptimizationTier) -> Result<(), OptError> {
    run_passes(module, tier.pipeline())
}

const FULL_ROUND: [&str; 16] = [
    "inline-noninterposable",
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

const FULL_EPILOGUE: [&str; 14] = [
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
const fn full_pipeline() -> [&'static str; FULL_ROUND.len() * FULL_ROUNDS + FULL_EPILOGUE.len()] {
    let mut passes = [""; FULL_ROUND.len() * FULL_ROUNDS + FULL_EPILOGUE.len()];
    let mut i = 0;
    while i < FULL_ROUND.len() * FULL_ROUNDS {
        passes[i] = FULL_ROUND[i % FULL_ROUND.len()];
        i += 1;
    }
    let mut j = 0;
    while j < FULL_EPILOGUE.len() {
        passes[i + j] = FULL_EPILOGUE[j];
        j += 1;
    }
    passes
}

/// The passes `keel compile -O` runs: three bounded rounds, then memory/loop cleanup.
pub const DEFAULT_PIPELINE: &[&str] = &full_pipeline();

pub fn find_pass(name: &str) -> Option<&'static dyn Pass> {
    PASSES.iter().copied().find(|p| p.name() == name)
}

#[derive(Debug)]
pub enum OptError {
    UnknownPass(String),
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

/// Custom pipeline with scoped options; canonical restoration always runs.
pub fn run_pipeline_for_target_with_options(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
    options: OptimizeOptions,
) -> Result<(), OptError> {
    let verification_enabled = options.verification.enabled();
    profiling::scope!("optimization");
    let mut canonical = vec![false; module.functions.len()];
    for pass in passes {
        profiling::scope!("pass", pass.name());
        if pass.run_module(module).is_some() {
            canonical.resize(module.functions.len(), false);
            canonical.fill(false);
            for func in &module.functions {
                verify_after(func, pass.name(), verification_enabled)?;
            }
            continue;
        }
        for (index, func) in module.functions.iter_mut().enumerate() {
            if pass.requires_canonical_form() && !canonical[index] {
                canonicalize::run(func);
                verify_after(func, "canonicalize", verification_enabled)?;
                canonical[index] = true;
            }
            let changed = pass.run_for_target(func, target);
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            verify_after(func, pass.name(), verification_enabled)?;
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
    let verification_enabled = options.verification.enabled();
    use std::time::Instant;
    profiling::scope!("optimization");
    let mut canonical = vec![false; module.functions.len()];
    for (pipeline_index, pass) in passes.iter().enumerate() {
        profiling::scope!("pass", pass.name());
        let before = IrCounts::module(module);
        let mut inline_sites = Vec::new();
        let start = Instant::now();
        let module_changed = pass.run_module_with_report(module, &mut inline_sites);
        let pass_ns = start.elapsed().as_nanos();
        if let Some(changed) = module_changed {
            canonical.resize(module.functions.len(), false);
            canonical.fill(false);
            let (result, verification_ns) = observe_verification(verification_enabled, || {
                module
                    .functions
                    .iter()
                    .try_for_each(|func| verify_after(func, pass.name(), verification_enabled))
            });
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
                before,
                after: IrCounts::module(module),
                inline_sites,
            });
            result?;
            continue;
        }
        for (index, func) in module.functions.iter_mut().enumerate() {
            let before = IrCounts::function(func);
            let mut restoration_ns = 0;
            let mut restoration_verification_ns = 0;
            let mut restoration_changed = None;
            if pass.requires_canonical_form() && !canonical[index] {
                let start = Instant::now();
                restoration_changed = Some(canonicalize::run(func));
                restoration_ns = start.elapsed().as_nanos();
                let (result, ns) = observe_verification(verification_enabled, || {
                    verify_after(func, "canonicalize", verification_enabled)
                });
                restoration_verification_ns = ns;
                // As in the ordinary runner, a failed restoration stops before
                // the requested pass. No completed invocation is fabricated.
                result?;
                canonical[index] = true;
            }
            let start = Instant::now();
            let changed = pass.run_for_target(func, target);
            let pass_ns = start.elapsed().as_nanos();
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            let (result, verification_ns) = observe_verification(verification_enabled, || {
                verify_after(func, pass.name(), verification_enabled)
            });
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
                before,
                after: IrCounts::function(func),
                inline_sites: Vec::new(),
            });
            result?;
        }
    }
    Ok(())
}
