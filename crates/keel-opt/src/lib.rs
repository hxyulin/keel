//! Optimization passes over keel IR.
//!
//! A [`Pass`] returns whether it changed anything. Most passes work on one
//! function at a time; module passes such as inlining use `run_module`.
//! Function passes compute the analyses they need
//! (CFG, dominators) themselves; there is no analysis cache until one is
//! needed.
//!
//! The pass manager runs the verifier after every pass. The IR is valid
//! between passes, always: a pass that breaks an invariant is caught at the
//! pass that broke it, not three passes later in codegen.

use keel_ir::{Function, Module, VerifierError, verify};
use std::fmt;

mod branch_ranges;
mod call_contracts;
mod canonicalize;
mod const_fold;
mod dce;
mod gcm;
mod gvn;
mod inline;
mod report;
pub use inline::{InlineReport, InlineSkip, run_with_report as inline_with_report};
pub use report::{InlineSite, IrCounts, OptimizationReport, PassObservation};
mod instcombine;
mod known_bits;
mod loop_vectorize;
pub use loop_vectorize::VectorTarget;
mod memory_forward;
mod phi_simplify;
mod sccp;
mod simplify_cfg;
mod stack_forward;

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
        name: "branch-ranges",
        description: "fold integer comparisons using ranges from dominating conditional edges",
        run: branch_ranges::run,
    },
    BuiltinPass {
        name: "call-contracts",
        description: "trim direct noreturn call continuations and lower direct nounwind invokes",
        run: call_contracts::run,
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
    &loop_vectorize::LoopVectorize,
    &inline::Inline,
];

/// Independent optimizer effort; Full adds value numbering, code motion, and
/// bounded direct-call inlining, private stack forwarding, and guarded loop vectorization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptimizationTier {
    #[default]
    None,
    Basic,
    Full,
}
impl OptimizationTier {
    pub fn pipeline(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
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

/// The passes `keel compile -O` runs, in order.
pub const DEFAULT_PIPELINE: &[&str] = &[
    "inline",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "canonicalize",
    "instcombine",
    "dce",
    "gcm",
    "loop-vectorize",
];

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
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_for_target(module, &passes, target)
}

/// Runs custom or built-in passes, restoring declared canonical entry
/// requirements and verifying the IR after each pass and restoration.
pub fn run_pipeline(module: &mut Module, passes: &[&dyn Pass]) -> Result<(), OptError> {
    run_pipeline_for_target(module, passes, VectorTarget::Portable)
}

/// Runs a custom pipeline with target SIMD capabilities and per-pass verification.
pub fn run_pipeline_for_target(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
) -> Result<(), OptError> {
    profiling::scope!("optimization");
    let mut canonical = vec![false; module.functions.len()];
    for pass in passes {
        profiling::scope!("pass", pass.name());
        if pass.run_module(module).is_some() {
            canonical.resize(module.functions.len(), false);
            canonical.fill(false);
            for func in &module.functions {
                verify(func).map_err(|errors| OptError::Verifier {
                    pass: pass.name(),
                    func: func.name.clone(),
                    errors,
                })?;
            }
            continue;
        }
        for (index, func) in module.functions.iter_mut().enumerate() {
            if pass.requires_canonical_form() && !canonical[index] {
                canonicalize::run(func);
                verify(func).map_err(|errors| OptError::Verifier {
                    pass: "canonicalize",
                    func: func.name.clone(),
                    errors,
                })?;
                canonical[index] = true;
            }
            let changed = pass.run_for_target(func, target);
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            verify(func).map_err(|errors| OptError::Verifier {
                pass: pass.name(),
                func: func.name.clone(),
                errors,
            })?;
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
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    run_pipeline_for_target_with_report(module, &passes, target, report)
}

/// Observed custom pipeline; order and verifier policy match the ordinary runner.
pub fn run_pipeline_for_target_with_report(
    module: &mut Module,
    passes: &[&dyn Pass],
    target: VectorTarget,
    report: &mut OptimizationReport,
) -> Result<(), OptError> {
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
            let start = Instant::now();
            let result = module.functions.iter().try_for_each(|func| {
                verify(func).map_err(|errors| OptError::Verifier {
                    pass: pass.name(),
                    func: func.name.clone(),
                    errors,
                })
            });
            let verification_ns = start.elapsed().as_nanos();
            report.invocations.push(PassObservation {
                pipeline_index,
                pass: pass.name(),
                target,
                function: None,
                changed,
                pass_ns,
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
                let start = Instant::now();
                let result = verify(func).map_err(|errors| OptError::Verifier {
                    pass: "canonicalize",
                    func: func.name.clone(),
                    errors,
                });
                restoration_verification_ns = start.elapsed().as_nanos();
                // As in the ordinary runner, a failed restoration stops before
                // the requested pass. No completed invocation is fabricated.
                result?;
                canonical[index] = true;
            }
            let start = Instant::now();
            let changed = pass.run_for_target(func, target);
            let pass_ns = start.elapsed().as_nanos();
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            let start = Instant::now();
            let result = verify(func).map_err(|errors| OptError::Verifier {
                pass: pass.name(),
                func: func.name.clone(),
                errors,
            });
            let verification_ns = start.elapsed().as_nanos();
            report.invocations.push(PassObservation {
                pipeline_index,
                pass: pass.name(),
                target,
                function: Some(func.name.clone()),
                changed,
                pass_ns,
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
