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

use keel_ir::{DataLayout, Function, Module, VerifierError, verify};
use std::fmt;

mod adjacent_abs;
mod analysis;
mod assembly_symbols;
mod call_contracts;
mod call_load_reuse;
mod canonicalize;
mod const_fold;
mod constant_args;
mod counted_loop;
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
pub use inline::InlineBudget;
pub use inline::{InlineReport, InlineSkip, run_with_report as inline_with_report};
pub use inline::{
    required_inline_asm, required_inline_asm_with_c_bodies, required_inline_asm_with_c_definitions,
};
pub use report::{InlineSite, IrCounts, OptimizationReport, PassObservation};
mod instcombine;
mod internal_fast_calls;
mod ir_match;
mod known_bits;
mod load_reuse;
mod loop_dot;
mod loop_rotate;
mod loop_strength_reduce;
mod loop_unroll;
mod loop_unroll_exact;
mod loop_vectorize;
pub mod private_aggregate;
mod tail_calls;
pub use loop_vectorize::VectorTarget;

/// Per-invocation immutable target context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptimizationContext {
    pub target: VectorTarget,
    pub capabilities: keel_ir::VectorCapabilities,
    /// Pointer storage model. `None` turns off rewrites that depend on it.
    pub layout: Option<DataLayout>,
    /// Every instruction is canonical on entry. The pass manager sets this to
    /// [`Pass::requires_canonical_form`] for each run, whatever the caller passed.
    pub canonical: bool,
}
impl OptimizationContext {
    /// The native vector targets have eight-byte integral pointers. Portable
    /// policy, which Wasm also uses, does not establish a pointer layout.
    pub const fn new(target: VectorTarget, capabilities: keel_ir::VectorCapabilities) -> Self {
        let layout = match target {
            VectorTarget::X86Sse2 | VectorTarget::Aarch64Neon => Some(DataLayout::NATIVE64),
            VectorTarget::Portable => None,
        };
        Self {
            target,
            capabilities,
            layout,
            canonical: false,
        }
    }
}
impl From<VectorTarget> for OptimizationContext {
    fn from(target: VectorTarget) -> Self {
        Self::new(target, keel_ir::VectorCapabilities::BASELINE)
    }
}

mod memory_alias;
mod memory_forward;
mod module_cleanup;
mod phi_simplify;
mod scratch;
mod simplify_cfg;
mod small_memory;
mod stack_address;
mod stack_cache;
mod stack_forward;
mod stack_notrap;
mod stack_promote;

/// A replaceable optimization pass. Function passes implement `run`; module
/// passes implement `run_module` and return false from `run`.
pub trait Pass: Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Runs on one function. Must report true whenever it mutates the
    /// function, including aliases.
    fn run(&self, func: &mut Function, context: &OptimizationContext) -> bool;

    /// Module passes return `Some(changed)`. They must leave functions that
    /// `context` does not enable unchanged, along with their call and ABI
    /// contracts. `None` declines: the manager then runs `run` on each
    /// enabled function.
    fn run_module(&self, _module: &mut Module, _context: &mut ModuleContext<'_>) -> Option<bool> {
        None
    }

    /// Whether the function is scheduled for this invocation. Checked before
    /// canonical restoration so protected bodies remain unchanged.
    fn enabled_for(&self, _function: &Function) -> bool {
        true
    }

    /// Whether this function pass depends only on the function and target.
    /// The manager then skips a run on a function that its previous run left
    /// unchanged and that nothing has changed since: by determinism and the
    /// `run` contract, that run would change nothing either. Cache entries belong
    /// to a pass instance; names do not identify configured instances.
    fn deterministic(&self) -> bool {
        false
    }

    /// Request the structural form documented in `docs/compiler/optimizer.md`.
    /// The manager restores it on entry when an earlier pass changed the IR,
    /// and then sets [`OptimizationContext::canonical`].
    fn requires_canonical_form(&self) -> bool {
        false
    }

    /// Only return true if every instruction is canonical on exit.
    fn produces_canonical_form(&self) -> bool {
        false
    }

    /// Whether the pass charges the pipeline-wide Full inlining allowance. The
    /// manager sizes [`ModuleContext`]'s budget from the input module, and
    /// reports it, only when some pass in the pipeline returns true.
    fn uses_inline_budget(&self) -> bool {
        false
    }

    /// Which function policy removes this pass from a schedule.
    fn family(&self) -> PassFamily {
        PassFamily::Other
    }
}

/// The function policy (`FunctionPolicy::inline`, `unroll` or `vectorize`)
/// that controls a pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassFamily {
    #[default]
    Other,
    Inline,
    Unroll,
    Vectorize,
}

/// One module pass invocation's scheduling and observation state.
pub struct ModuleContext<'a> {
    enabled: &'a dyn Fn(&Function) -> bool,
    /// Inline decisions, collected only when a report was requested.
    pub sites: Option<&'a mut Vec<InlineSite>>,
    /// After the pass, per function, whether the pass may have changed it.
    /// The manager clears it first. Entries the pass leaves out take the
    /// pass's overall result.
    pub changed: &'a mut Vec<bool>,
    pub(crate) budget: &'a mut InlineBudget,
}
impl fmt::Debug for ModuleContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleContext")
            .field("sites", &self.sites)
            .field("changed", &self.changed)
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

impl ModuleContext<'_> {
    /// Whether this invocation may change `function`.
    pub fn enabled(&self, function: &Function) -> bool {
        (self.enabled)(function)
    }

    /// Runs a transform that cannot limit itself to enabled functions, only
    /// when every function is enabled. Otherwise it declines.
    pub fn whole_module(
        &self,
        module: &mut Module,
        run: impl FnOnce(&mut Module) -> bool,
    ) -> Option<bool> {
        module
            .functions
            .iter()
            .all(|f| self.enabled(f))
            .then(|| run(module))
    }
}

/// A function pass with a plain kernel and its canonical-form properties.
struct BuiltinPass {
    name: &'static str,
    description: &'static str,
    run: fn(&mut Function, &OptimizationContext) -> bool,
    requires_canonical: bool,
    produces_canonical: bool,
}
impl BuiltinPass {
    const fn new(
        name: &'static str,
        description: &'static str,
        run: fn(&mut Function, &OptimizationContext) -> bool,
    ) -> Self {
        Self {
            name,
            description,
            run,
            requires_canonical: false,
            produces_canonical: false,
        }
    }
}

impl Pass for BuiltinPass {
    fn name(&self) -> &'static str {
        self.name
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn run(&self, func: &mut Function, context: &OptimizationContext) -> bool {
        (self.run)(func, context)
    }
    fn requires_canonical_form(&self) -> bool {
        self.requires_canonical
    }
    fn produces_canonical_form(&self) -> bool {
        self.produces_canonical
    }
    fn deterministic(&self) -> bool {
        true
    }
}

// The registry. Each pass is one named static; `PASSES` lists them in
// `keel passes` order. Code that needs a particular pass uses its static, so
// a pass's name decides nothing but its lookup.
pub(crate) static FAST_CFG: &dyn Pass = &fast::Cfg;
pub(crate) static FAST_LOCAL: &dyn Pass = &fast::Local;
pub(crate) static SMALL_MEMORY: &dyn Pass = &BuiltinPass::new(
    "small-memory",
    "expand ordinary constant memory intrinsics of at most 32 bytes",
    |f, _| small_memory::run(f, small_memory::MAX_BYTES),
);
pub(crate) static MEMORY_EXPAND: &dyn Pass = &BuiltinPass::new(
    "memory-expand",
    "expand ordinary constant memory intrinsics of at most 128 bytes, for tiers without inlining",
    |f, _| small_memory::run(f, small_memory::LATE_MAX_BYTES),
);
pub(crate) static STACK_NOTRAP: &dyn Pass = &BuiltinPass::new(
    "stack-notrap",
    "mark loads and stores inside a fixed stack slot as unable to fault",
    |f, _| stack_notrap::run(f),
);
pub(crate) static CONST_FOLD: &dyn Pass = &BuiltinPass::new(
    "const-fold",
    "evaluate instructions whose operands are constants",
    |f, _| const_fold::run(f),
);
pub(crate) static SIMPLIFY_CFG: &dyn Pass = &BuiltinPass::new(
    "simplify-cfg",
    "fold constant branches, skip empty blocks, merge straight-line blocks, remove unreachable blocks",
    |f, _| simplify_cfg::run(f),
);
pub(crate) static GVN: &dyn Pass = &BuiltinPass::new(
    "gvn",
    "replace equivalent pure instructions with a dominating computation",
    |f, _| gvn::run(f),
);
pub(crate) static DCE: &dyn Pass = &BuiltinPass::new(
    "dce",
    "remove unused pure instructions and block parameters",
    |f, _| dce::run(f),
);
pub(crate) static GCM: &dyn Pass = &BuiltinPass::new(
    "gcm",
    "move pure instructions out of loops and into the branches that need them; copy constants to each block using them",
    |f, _| gcm::run(f),
);
pub(crate) static INSTCOMBINE: &dyn Pass = &BuiltinPass {
    requires_canonical: true,
    ..BuiltinPass::new(
        "instcombine",
        "combine integer identities, fold constants, and reduce power-of-two arithmetic",
        |f, context| {
            if context.canonical {
                instcombine::run_canonical(f)
            } else {
                instcombine::run(f)
            }
        },
    )
};
pub(crate) static PHI_SIMPLIFY: &dyn Pass = &BuiltinPass::new(
    "phi-simplify",
    "replace trivial block parameters with their dominating incoming value",
    |f, _| phi_simplify::run(f),
);
pub(crate) static MEMORY_FORWARD: &dyn Pass = &BuiltinPass::new(
    "memory-forward",
    "forward exact-address ordinary loads and stores within a block",
    |f, _| memory_forward::run(f),
);
pub(crate) static KNOWN_BITS: &dyn Pass = &BuiltinPass::new(
    "known-bits",
    "prove integer bits and ranges to remove redundant masks and fold comparisons",
    |f, _| known_bits::run(f),
);
pub(crate) static CANONICALIZE: &dyn Pass = &BuiltinPass {
    produces_canonical: true,
    ..BuiltinPass::new(
        "canonicalize",
        "resolve operand aliases and normalize integer operand order and comparisons",
        |f, _| canonicalize::run(f),
    )
};
pub(crate) static STACK_FORWARD: &dyn Pass = &BuiltinPass::new(
    "stack-forward",
    "forward bytes in private fixed stack objects exposed by inlining",
    |f, context| stack_forward::run_with_layout(f, context.layout),
);
pub(crate) static CALL_CONTRACTS: &dyn Pass = &BuiltinPass::new(
    "call-contracts",
    "trim direct noreturn call continuations and lower direct nounwind invokes",
    |f, _| call_contracts::run(f),
);
pub(crate) static STACK_PROMOTE: &dyn Pass = &BuiltinPass::new(
    "stack-promote",
    "promote initialized disjoint private stack fields to loop SSA",
    |f, _| stack_promote::run(f),
);
pub(crate) static IF_CONVERT: &dyn Pass = &BuiltinPass::new(
    "if-convert",
    "convert bounded nontrapping scalar diamonds to selects (opt-in)",
    |f, _| if_convert::run(f),
);
pub(crate) static LOAD_REUSE: &dyn Pass = &load_reuse::LoadReuse;
pub(crate) static LOOP_DOT: &dyn Pass = &loop_dot::LoopDot;
pub(crate) static LOOP_ROTATE: &dyn Pass = &loop_rotate::LoopRotate;
pub(crate) static LOOP_STRENGTH_REDUCE: &dyn Pass = &loop_strength_reduce::LoopStrengthReduce;
pub(crate) static LOOP_UNROLL: &dyn Pass = &loop_unroll::LoopUnroll;
pub(crate) static LOOP_UNROLL_EXACT: &dyn Pass = &loop_unroll_exact::LoopUnrollExact;
pub(crate) static LOOP_VECTORIZE: &dyn Pass = &loop_vectorize::LoopVectorize;
pub(crate) static LOOP_REDUCE: &dyn Pass = &loop_vectorize::LoopReduce;
pub(crate) static LOOP_REDUCE_MULTI: &dyn Pass = &loop_vectorize::MultiLoopReduce;
pub(crate) static FAST_LOOP_REDUCE: &dyn Pass = &loop_vectorize::FastLoopReduce;
pub(crate) static INLINE: &dyn Pass = &inline::Inline;
pub(crate) static INLINE_NONINTERPOSABLE: &dyn Pass = &inline::InlineNoninterposable;
pub(crate) static ADJACENT_ABS: &dyn Pass = &BuiltinPass::new(
    "adjacent-abs",
    "convert a private scalar absolute-value triangle without moving memory",
    |f, _| adjacent_abs::run(f),
);
pub(crate) static INLINE_CHEAP_RESERVE: &dyn Pass = &inline::InlineCheapReserve;
pub(crate) static CALL_LOAD_REUSE: &dyn Pass = &call_load_reuse::CallLoadReuse;
pub(crate) static INTERNAL_FAST_CALLS: &dyn Pass = &internal_fast_calls::InternalFastCalls;
pub(crate) static TAIL_CALLS: &dyn Pass = &tail_calls::TailCalls;
pub(crate) static CONSTANT_ARGS: &dyn Pass = &constant_args::ConstantArgs;
pub(crate) static EMPTY_CALLS: &dyn Pass = &module_cleanup::EmptyCalls;
pub(crate) static UNUSED_FUNCTIONS: &dyn Pass = &module_cleanup::UnusedFunctions;
pub(crate) static COMPACT_UNUSED_FUNCTIONS: &dyn Pass = &module_cleanup::CompactUnusedFunctions;
pub(crate) static PRIVATE_AGGREGATE_CLEANUP: &dyn Pass =
    &private_aggregate::PrivateAggregateCleanup;
pub(crate) static INLINE_SIZE: &dyn Pass = &inline::InlineSize;

/// Every registered pass, in `keel passes` order.
pub static PASSES: &[&dyn Pass] = &[
    FAST_CFG,
    FAST_LOCAL,
    SMALL_MEMORY,
    MEMORY_EXPAND,
    STACK_NOTRAP,
    CONST_FOLD,
    SIMPLIFY_CFG,
    GVN,
    DCE,
    GCM,
    INSTCOMBINE,
    PHI_SIMPLIFY,
    MEMORY_FORWARD,
    KNOWN_BITS,
    CANONICALIZE,
    STACK_FORWARD,
    CALL_CONTRACTS,
    STACK_PROMOTE,
    IF_CONVERT,
    LOAD_REUSE,
    LOOP_ROTATE,
    LOOP_STRENGTH_REDUCE,
    LOOP_UNROLL,
    LOOP_UNROLL_EXACT,
    LOOP_DOT,
    LOOP_VECTORIZE,
    LOOP_REDUCE,
    LOOP_REDUCE_MULTI,
    FAST_LOOP_REDUCE,
    INLINE,
    INLINE_NONINTERPOSABLE,
    ADJACENT_ABS,
    INLINE_CHEAP_RESERVE,
    CALL_LOAD_REUSE,
    INTERNAL_FAST_CALLS,
    TAIL_CALLS,
    CONSTANT_ARGS,
    EMPTY_CALLS,
    UNUSED_FUNCTIONS,
    COMPACT_UNUSED_FUNCTIONS,
    PRIVATE_AGGREGATE_CLEANUP,
    INLINE_SIZE,
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptimizeOptions {
    pub verification: VerificationPolicy,
    /// Pass names to skip wherever they appear in the pipeline, including
    /// function-attribute schedules. Each must name a pipeline or registered pass.
    pub disabled: Vec<String>,
    /// Run only the first N pass invocations and skip the rest. An invocation
    /// is one function pass run on one function, or one module pass run.
    /// Runs that the clean cache skips are not counted.
    pub bisect_limit: Option<usize>,
    /// Trace each invocation that runs: its index, pass and function.
    pub bisect_verbose: bool,
    /// Trace each function after an invocation that changed it.
    pub print_changed: bool,
    /// Limit `print_changed` to the function with this name.
    pub print_function: Option<String>,
}

impl OptimizeOptions {
    fn traces(&self) -> bool {
        self.bisect_verbose || self.print_changed
    }
}

/// Invocation numbering, the bisect limit, the trace sink and the least
/// inlining allowance.
struct Trace<'a, 'o> {
    options: &'o OptimizeOptions,
    out: Option<&'a mut dyn std::io::Write>,
    invocations: usize,
    inline_allowance: usize,
}

impl Trace<'_, '_> {
    /// Whether the bisect limit leaves no invocation to run.
    fn exhausted(&self) -> bool {
        self.options
            .bisect_limit
            .is_some_and(|limit| self.invocations >= limit)
    }

    /// Numbers an invocation that is about to run, unless the limit is reached.
    fn admit(&mut self, pass: &str, func: Option<&str>) -> bool {
        if self.exhausted() {
            return false;
        }
        self.invocations += 1;
        if self.options.bisect_verbose {
            let n = self.invocations;
            self.line(format_args!("bisect: {n} {pass} {}", Target(func)));
        }
        true
    }

    fn changed(&mut self, pass: &str, func: &Function) {
        let options = self.options;
        if !options.print_changed
            || options
                .print_function
                .as_ref()
                .is_some_and(|name| *name != func.name)
        {
            return;
        }
        let n = self.invocations;
        self.line(format_args!("; after {pass}, invocation {n}\n{func}"));
    }

    /// Tracing is a debugging aid; a failed write does not stop optimization.
    fn line(&mut self, text: fmt::Arguments<'_>) {
        if let Some(out) = self.out.as_deref_mut() {
            let _ = writeln!(out, "{text}");
        }
    }
}

/// `@name` for a function invocation, `(module)` for a module invocation.
struct Target<'a>(Option<&'a str>);

impl fmt::Display for Target<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(name) => write!(f, "@{name}"),
            None => f.write_str("(module)"),
        }
    }
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
                "memory-expand",
                "stack-notrap",
                "dce",
                "fast-loop-reduce",
            ],
            Self::Basic => &[
                "simplify-cfg",
                "canonicalize",
                "instcombine",
                "call-contracts",
                "simplify-cfg",
                "memory-expand",
                "stack-notrap",
                "dce",
            ],
            Self::Balanced => BALANCED_PIPELINE,
            Self::Size => SIZE_PIPELINE,
            Self::Full => DEFAULT_PIPELINE,
        }
    }
}

/// Scalar and memory cleanup shared by the Balanced and Size rounds. The first
/// Balanced round also expands small memory intrinsics after `gvn`.
const RELEASE_ROUND_SCALAR: [&str; 6] = [
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
];
const RELEASE_ROUND_MEMORY: [&str; 6] = [
    "stack-forward",
    "stack-notrap",
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
];
/// Known-bit, call and reachability cleanup closing Balanced and Size.
const RELEASE_EPILOGUE: [&str; 12] = [
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

// `const-fold` ahead of the first inliner: frontends may leave operations on
// literals, such as `ineg 1`, and inliner admission counts instructions.
const BALANCED_PARTS: &[&[&str]] = &[
    &["private-aggregate-cleanup"],
    &["const-fold"],
    &["inline-noninterposable"],
    &RELEASE_ROUND_SCALAR,
    &["small-memory"],
    &RELEASE_ROUND_MEMORY,
    &["inline-noninterposable"],
    &RELEASE_ROUND_SCALAR,
    &RELEASE_ROUND_MEMORY,
    &["inline-noninterposable"],
    &RELEASE_ROUND_SCALAR,
    &RELEASE_ROUND_MEMORY,
    &RELEASE_EPILOGUE,
];

/// Balanced release pipeline: three rounds of binding-aware inlining and
/// scalar/memory cleanup. Loop expansion remains in Full. GCM is also
/// omitted here: its placement after promotion regressed Lua dispatch in the
/// balanced schedule; Full retains its separately measured pass ordering.
pub const BALANCED_PIPELINE: &[&str] = &concat::<{ total_len(BALANCED_PARTS) }>(BALANCED_PARTS);

// The first Size round cleans up before any inlining.
const SIZE_PARTS: &[&[&str]] = &[
    &["private-aggregate-cleanup"],
    &RELEASE_ROUND_SCALAR,
    &RELEASE_ROUND_MEMORY,
    &["inline-size"],
    &RELEASE_ROUND_SCALAR,
    &RELEASE_ROUND_MEMORY,
    &["inline-size"],
    &RELEASE_ROUND_SCALAR,
    &RELEASE_ROUND_MEMORY,
    &RELEASE_EPILOGUE,
];

/// Size policy. Bounded hinted, constant-argument and single-use helper inlining
/// exposes dead branches. Omits memory expansion, code motion, unrolling and vectorization.
/// Mandatory frontend inlining still runs to satisfy language/operand contracts.
/// This is an IR growth policy, not a guarantee about final machine-code size.
pub const SIZE_PIPELINE: &[&str] = &concat::<{ total_len(SIZE_PARTS) }>(SIZE_PARTS);

pub fn optimize_with_tier(module: &mut Module, tier: OptimizationTier) -> Result<(), OptError> {
    run_passes(module, tier.pipeline())
}

const FULL_ROUND: [&str; 17] = [
    "inline-cheap-reserve",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "small-memory",
    "stack-forward",
    "stack-notrap",
    "canonicalize",
    "instcombine",
    "dce",
    "gcm",
    "adjacent-abs",
    "loop-vectorize",
    "loop-reduce",
];

const FULL_EPILOGUE: [&str; 28] = [
    "load-reuse",
    "stack-promote",
    "phi-simplify",
    "dce",
    "simplify-cfg",
    "loop-strength-reduce",
    "canonicalize",
    "instcombine",
    "dce",
    "loop-reduce-multi",
    "loop-unroll",
    "loop-rotate",
    "phi-simplify",
    "dce",
    "constant-args",
    "simplify-cfg",
    "known-bits",
    "simplify-cfg",
    "dce",
    "loop-unroll-exact",
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
// vectorizes extended reductions before unrolling and rotation: unrolling first
// leaves only a remainder loop too short to vectorize. Keep phi cleanup late and
// omit an extra GCM: broader schedules regressed Lua fib in app screening.
// Finalize private call transport only after IR rewrites. Physical preservation
// summaries are built by codegen from the final selected/allocated leaf bodies.
// Scalarize bounded private pairs before finalizing calling conventions, and
// run the associated cleanup only if transport actually changed the module.
// Also do this before freezing the first inliner templates: scalar cleanup
// removes aggregate homes and avoids charging byval snapshots at every site.
// Retain the late invocation for signatures exposed by intervening rewrites.
// Fold operations on literals before the first inliner, as in Balanced.
const FULL_PARTS: &[&[&str]] = &[
    &["private-aggregate-cleanup"],
    &["const-fold"],
    &FULL_ROUND,
    &FULL_ROUND,
    &FULL_ROUND,
    &FULL_EPILOGUE,
];

/// The passes `keel compile -O` runs: private aggregate cleanup, constant
/// folding, three bounded rounds, then memory/loop, known-bit, call and reachability cleanup.
pub const DEFAULT_PIPELINE: &[&str] = &concat::<{ total_len(FULL_PARTS) }>(FULL_PARTS);

const fn total_len(parts: &[&[&str]]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < parts.len() {
        total += parts[i].len();
        i += 1;
    }
    total
}

/// Joins pipeline parts; `N` must be their total length.
const fn concat<const N: usize>(parts: &[&[&'static str]]) -> [&'static str; N] {
    let mut passes = [""; N];
    let mut next = 0;
    let mut i = 0;
    while i < parts.len() {
        let mut j = 0;
        while j < parts[i].len() {
            passes[next] = parts[i][j];
            next += 1;
            j += 1;
        }
        i += 1;
    }
    assert!(next == N);
    passes
}

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
impl std::error::Error for OptError {}

/// One optimizer invocation: the passes, their target context, scoped
/// options and an optional report. [`Pipeline::run`] is the single entry
/// point; the `optimize*` and `run_*` functions are shorthands for it.
pub struct Pipeline<'a> {
    passes: Vec<&'a dyn Pass>,
    context: OptimizationContext,
    options: OptimizeOptions,
    report: Option<&'a mut OptimizationReport>,
    trace: Option<&'a mut dyn std::io::Write>,
    inline_allowance: usize,
}

impl fmt::Debug for Pipeline<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let passes: Vec<_> = self.passes.iter().map(|pass| pass.name()).collect();
        f.debug_struct("Pipeline")
            .field("passes", &passes)
            .field("context", &self.context)
            .field("options", &self.options)
            .field("report", &self.report.is_some())
            .field("trace", &self.trace.is_some())
            .finish()
    }
}

impl<'a> Pipeline<'a> {
    /// Custom or built-in passes, with the portable context and default options.
    pub fn new(passes: &[&'a dyn Pass]) -> Self {
        Self {
            passes: passes.to_vec(),
            context: OptimizationContext::default(),
            options: OptimizeOptions::default(),
            report: None,
            trace: None,
            inline_allowance: 0,
        }
    }

    /// Registered passes by name.
    pub fn named(names: &[&str]) -> Result<Self, OptError> {
        let passes = names
            .iter()
            .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(&passes))
    }

    pub fn context(mut self, context: OptimizationContext) -> Self {
        self.context = context;
        self
    }

    pub fn options(mut self, options: OptimizeOptions) -> Self {
        self.options = options;
        self
    }

    /// Records each invocation. Partial observations remain available if a
    /// verifier fails; observation performs exactly the ordinary checks. Each
    /// observation records its policy, so one report may span several runs.
    pub fn report(mut self, report: &'a mut OptimizationReport) -> Self {
        self.report = Some(report);
        self
    }

    /// Where the `bisect_verbose` and `print_changed` traces go; standard
    /// error by default.
    pub fn trace(mut self, out: &'a mut dyn std::io::Write) -> Self {
        self.trace = Some(out);
        self
    }

    /// The least ordinary inlining allowance. A module merged from several
    /// inputs passes the sum of [`InlineBudget::allowance`] over them, so
    /// merging never leaves less inlining room than compiling the inputs
    /// separately.
    pub fn inline_allowance(mut self, instructions: usize) -> Self {
        self.inline_allowance = instructions;
        self
    }

    /// Runs the passes in order, restoring declared canonical entry
    /// requirements and honoring function optimization attributes.
    pub fn run(mut self, module: &mut Module) -> Result<(), OptError> {
        if module.special_entry_contract().is_some() {
            return Ok(());
        }
        if let Some(name) = self.options.disabled.iter().find(|&name| {
            find_pass(name).is_none() && !self.passes.iter().any(|pass| pass.name() == name)
        }) {
            return Err(OptError::UnknownPass(name.clone()));
        }
        let mut stderr = std::io::stderr();
        let out: Option<&mut dyn std::io::Write> = match self.trace.take() {
            Some(out) => Some(&mut *out),
            None if self.options.traces() => Some(&mut stderr),
            None => None,
        };
        let mut trace = Trace {
            options: &self.options,
            out,
            invocations: 0,
            inline_allowance: self.inline_allowance,
        };
        if let Some(result) = function_policy::run(
            module,
            &self.passes,
            self.context,
            self.report.as_deref_mut(),
            &mut trace,
        ) {
            return result;
        }
        execute_pipeline(module, &self.passes, self.context, self.report, &mut trace)
    }
}

/// Runs [`DEFAULT_PIPELINE`].
pub fn optimize(module: &mut Module) -> Result<(), OptError> {
    run_passes(module, DEFAULT_PIPELINE)
}

/// Runs the named passes, in order, on every function in `module`.
pub fn run_passes(module: &mut Module, names: &[&str]) -> Result<(), OptError> {
    Pipeline::named(names)?.run(module)
}

/// Runs named passes with target SIMD capabilities; ordinary passes ignore the target.
pub fn run_passes_for_target(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
) -> Result<(), OptError> {
    Pipeline::named(names)?.context(target.into()).run(module)
}

/// Observed counterpart to [`run_passes_for_target`].
pub fn run_passes_for_target_with_report(
    module: &mut Module,
    names: &[&str],
    target: VectorTarget,
    report: &mut OptimizationReport,
) -> Result<(), OptError> {
    Pipeline::named(names)?
        .context(target.into())
        .report(report)
        .run(module)
}

/// Runs custom or built-in passes with the portable context.
pub fn run_pipeline(module: &mut Module, passes: &[&dyn Pass]) -> Result<(), OptError> {
    Pipeline::new(passes).run(module)
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

/// Panics, naming the pass, when a pass that reported no change modified
/// `func`. The manager skips verification of such a body and the clean cache
/// skips later runs on it, so the lie would otherwise surface far away.
fn assert_unchanged(pass: &str, func: &Function, before: u64) {
    assert!(
        func.fingerprint() == before,
        "pass `{pass}` reported no change but modified @{}",
        func.name
    );
}

fn name_key(name: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = keel_ir::fxhash::FxHasher::default();
    name.hash(&mut h);
    h.finish()
}

/// Drops cached analyses when a pipeline ends, however it ends.
struct ClearAnalyses;

impl Drop for ClearAnalyses {
    fn drop(&mut self) {
        analysis::clear();
        scratch::clear();
    }
}

/// One mutation/verification state machine; observation only measures its boundaries.
fn execute_pipeline(
    module: &mut Module,
    passes: &[&dyn Pass],
    context: OptimizationContext,
    mut report: Option<&mut OptimizationReport>,
    trace: &mut Trace<'_, '_>,
) -> Result<(), OptError> {
    // Preserve opaque entry/return bodies until target-specific lowering can
    // establish their register, frame and control-flow contracts.
    if module.special_entry_contract().is_some() {
        return Ok(());
    }
    use std::time::Instant;
    let target = context.target;
    let reporting = report.is_some();
    let options = trace.options;
    let verification_enabled = options.verification.enabled();
    // Check the no-change contract wherever verification is on, and in every
    // debug build even when a caller disabled verification.
    let audit_unchanged = verification_enabled || cfg!(debug_assertions);
    // Each function's name key and fingerprint, by index, as of its last
    // check; `None` once a pass may have changed it. A function a pass leaves
    // unchanged keeps its entry, so each function is hashed about once per
    // pass run instead of once per pipeline entry.
    let mut prints: Vec<Option<(u64, u64)>> = Vec::new();
    profiling::scope!("optimization");
    let _clear = ClearAnalyses;
    let mut canonical = vec![false; module.functions.len()];
    let mut versions = Versions::new(module.functions.len());
    let uses_inline_budget = passes.iter().any(|pass| pass.uses_inline_budget());
    let mut inline_budget = if uses_inline_budget {
        let mut budget = InlineBudget::new(module);
        budget.limit = budget.limit.max(trace.inline_allowance);
        budget
    } else {
        InlineBudget::default()
    };
    for (pipeline_index, &pass) in passes.iter().enumerate() {
        if trace.exhausted() {
            break;
        }
        if options.disabled.iter().any(|name| name == pass.name()) {
            continue;
        }
        profiling::scope!("pass", pass.name());
        let before = reporting.then(|| IrCounts::module(module));
        let mut inline_sites = reporting.then(Vec::new);
        if audit_unchanged {
            prints.resize(module.functions.len(), None);
            for (print, func) in prints.iter_mut().zip(&module.functions) {
                print.get_or_insert_with(|| (name_key(&func.name), func.fingerprint()));
            }
        }
        if reporting {
            instcombine::clear_hits();
        }
        let start = reporting.then(Instant::now);
        versions.changed.clear();
        let module_changed = pass.run_module(
            module,
            &mut ModuleContext {
                enabled: &|f| pass.enabled_for(f),
                sites: inline_sites.as_mut(),
                changed: &mut versions.changed,
                budget: &mut inline_budget,
            },
        );
        let pass_ns = start.map_or(0, |start| start.elapsed().as_nanos());
        if let Some(changed) = module_changed {
            versions.changed.resize(module.functions.len(), changed);
        }
        if uses_inline_budget && let Some(report) = report.as_deref_mut() {
            report.inline_budget = Some(inline_budget.clone());
        }
        if let Some(changed) = module_changed {
            trace.admit(pass.name(), None);
            let old_len = versions.version.len();
            versions.module_changed(&mut canonical);
            #[cfg(debug_assertions)]
            module
                .functions
                .iter()
                .for_each(analysis::debug_check_stamp);
            if audit_unchanged {
                // Functions keep their names across module passes; indices
                // shift when a pass removes one.
                let old = std::mem::take(&mut prints);
                prints = module
                    .functions
                    .iter()
                    .enumerate()
                    .map(|(index, func)| {
                        if index >= old_len || versions.changed.get(index) != Some(&false) {
                            return None;
                        }
                        let key = name_key(&func.name);
                        let before = old
                            .get(index)
                            .copied()
                            .flatten()
                            .filter(|&(k, _)| k == key)
                            .or_else(|| old.iter().flatten().find(|&&(k, _)| k == key).copied());
                        if let Some((_, before)) = before {
                            assert_unchanged(pass.name(), func, before);
                        }
                        before
                    })
                    .collect();
            }
            for (index, func) in module.functions.iter().enumerate() {
                if index >= old_len || versions.changed.get(index) != Some(&false) {
                    trace.changed(pass.name(), func);
                }
            }
            // A body the pass reported unchanged is the one verified after
            // its last change, as the version cache also assumes.
            let (result, verification_ns) =
                observe_verification(verification_enabled, reporting, || {
                    module
                        .functions
                        .iter()
                        .enumerate()
                        .filter(|&(index, _)| {
                            index >= old_len || versions.changed.get(index) != Some(&false)
                        })
                        .try_for_each(|(_, func)| {
                            verify_after(func, pass.name(), verification_enabled)
                        })
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
                    rules: instcombine::take_hits(),
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
                        rules: Vec::new(),
                    });
                }
                continue;
            }
            if !trace.admit(pass.name(), Some(&func.name)) {
                break;
            }
            // No module hook ran, so the function is as fingerprinted.
            let mut before_run = prints.get(index).copied().flatten().map(|(_, print)| print);
            let mut restoration_ns = 0;
            let mut restoration_verification_ns = 0;
            let mut restoration_changed = None;
            if pass.requires_canonical_form() && !canonical[index] {
                let start = reporting.then(Instant::now);
                let restored = canonicalize::run(func);
                if restored {
                    versions.renew(index);
                    trace.changed("canonicalize", func);
                    before_run = audit_unchanged.then(|| func.fingerprint());
                } else if let Some(before) = before_run {
                    assert_unchanged("canonicalize", func, before);
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
            if reporting {
                instcombine::clear_hits();
            }
            let start = reporting.then(Instant::now);
            let function_context = OptimizationContext {
                canonical: pass.requires_canonical_form(),
                capabilities: if context.target == VectorTarget::X86Sse2 {
                    func.attrs
                        .target_features
                        .vector_capabilities(context.capabilities)
                        .map_err(|message| OptError::FunctionPolicy {
                            func: func.name.clone(),
                            message,
                        })?
                } else {
                    context.capabilities
                },
                ..context
            };
            let changed = pass.run(func, &function_context);
            let pass_ns = start.map_or(0, |start| start.elapsed().as_nanos());
            #[cfg(debug_assertions)]
            analysis::debug_check_stamp(func);
            if !changed && let Some(before) = before_run {
                assert_unchanged(pass.name(), func, before);
            }
            if let Some(print) = prints.get_mut(index) {
                *print = before_run
                    .filter(|_| !changed)
                    .map(|before| (name_key(&func.name), before));
            }
            if changed {
                trace.changed(pass.name(), func);
            }
            canonical[index] = pass.produces_canonical_form() || (canonical[index] && !changed);
            versions.record(pass, slot, index, changed);
            // An unchanged body is the one verified after its last change.
            let (result, verification_ns) =
                observe_verification(verification_enabled && changed, reporting, || {
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
                    rules: instcombine::take_hits(),
                });
            }
            result?;
        }
    }
    Ok(())
}
