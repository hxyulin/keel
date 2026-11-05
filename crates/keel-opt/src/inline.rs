//! Bounded direct-call inlining of definitions with proven binding. Newly
//! cloned calls are not reconsidered, so growth and recursion cannot
//! recursively reconsider cloned sites. The binding-aware policies first
//! inline bounded tiny alwaysinline leaves (in Full, also tiny hinted leaves),
//! then the remaining alwaysinline sites with callees before callers, so a
//! cloned wrapper already contains its requested callees. Templates are frozen
//! when the ordinary sites start. Each priority and ordinary phase takes turns
//! across callers. Other policies consume budgets in layout order. ABI
//! by-value arguments get a fresh snapshot.
//! Instruction growth bounds the increase in layout instructions before cleanup;
//! the removed call pays for the jump entering the cloned body.
mod clone;
mod cost;
mod eligibility;
mod required;
mod schedule;

pub(crate) use cost::CHEAP_MODULE_GROWTH;
pub use required::{
    required_inline_asm, required_inline_asm_with_c_bodies, required_inline_asm_with_c_definitions,
};

use crate::{ModuleContext, OptimizationContext, Pass, PassFamily};
use cost::{FULL_GROWTH_INPUT_DIVISOR, FULL_MIN_MODULE_GROWTH};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{Function, Inst, InstData, Module};
use schedule::{run_observed, run_observed_policy_budgeted, run_observed_policy_scoped};

/// Conservative Full expansion allowance shared by one optimization pipeline.
/// Charges cloned IR before cleanup; it is never replenished by simplification.
#[derive(Debug, Clone, Default)]
pub struct InlineBudget {
    /// Ordinary allowance fixed from pipeline-entry IR.
    pub limit: usize,
    /// Ordinary instructions charged so far.
    pub used: usize,
    /// Cheap-helper reserve instructions charged so far.
    pub reserve_used: usize,
    reserve_sites: usize,
}
impl InlineBudget {
    pub(crate) fn new(module: &Module) -> Self {
        let instructions = module
            .functions
            .iter()
            .map(|f| f.layout_insts().count())
            .fold(0usize, usize::saturating_add);
        Self {
            limit: FULL_MIN_MODULE_GROWTH.max(instructions / FULL_GROWTH_INPUT_DIVISOR),
            used: 0,
            reserve_used: 0,
            reserve_sites: 0,
        }
    }
}

pub struct Inline;
impl Pass for Inline {
    fn name(&self) -> &'static str {
        "inline"
    }
    fn description(&self) -> &'static str {
        "inline bounded private direct-call functions"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        run_scheduled(module, context, InlinePolicy::DEFAULT)
    }
    fn family(&self) -> PassFamily {
        PassFamily::Inline
    }
}
/// Inline bounded helpers to expose constants or eliminate a single local call.
#[derive(Debug)]
pub struct InlineSize;
impl Pass for InlineSize {
    fn name(&self) -> &'static str {
        "inline-size"
    }
    fn description(&self) -> &'static str {
        "inline bounded hinted, constant-argument or single-use local helpers"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        run_scheduled(
            module,
            context,
            InlinePolicy {
                cost: InlineCost::Size,
                ..InlinePolicy::NONINTERPOSABLE
            },
        )
    }
    fn family(&self) -> PassFamily {
        PassFamily::Inline
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineCost {
    Default,
    Size,
}

#[derive(Clone, Copy, Debug)]
struct InlinePolicy {
    admit_noninterposable: bool,
    cheap_reserve: bool,
    cost: InlineCost,
}
impl InlinePolicy {
    const DEFAULT: Self = Self {
        admit_noninterposable: false,
        cheap_reserve: false,
        cost: InlineCost::Default,
    };
    const NONINTERPOSABLE: Self = Self {
        admit_noninterposable: true,
        ..Self::DEFAULT
    };
}

/// Inlines into the functions `context` enables and records the callers it changed.
fn run_scheduled(
    module: &mut Module,
    context: &mut ModuleContext<'_>,
    policy: InlinePolicy,
) -> Option<bool> {
    let callers = enabled_callers(module, context);
    let report =
        run_observed_policy_scoped(module, context.sites.as_deref_mut(), policy, Some(&callers));
    Some(report.changed_callers(module, context.changed))
}

fn enabled_callers(module: &Module, context: &ModuleContext<'_>) -> FxHashSet<String> {
    module
        .functions
        .iter()
        .filter(|f| context.enabled(f))
        .map(|f| f.name.clone())
        .collect()
}

/// Binding-aware Full integration. Shares ordinary legality, growth budgets
/// and cloning, with bounded read-only leaves and one-level SCC wrappers.
/// Exported definitions require an exact-binding or equivalent-body promise.
pub struct InlineNoninterposable;
impl Pass for InlineNoninterposable {
    fn name(&self) -> &'static str {
        "inline-noninterposable"
    }
    fn description(&self) -> &'static str {
        "inline bounded direct-call functions with exact binding or equivalent bodies"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        run_scheduled(module, context, InlinePolicy::NONINTERPOSABLE)
    }
    fn family(&self) -> PassFamily {
        PassFamily::Inline
    }
}

/// Full policy: share an input-scaled module allowance, then fairly retry cheap
/// helper sites denied only by module growth. Caller and legality limits match
/// the ordinary policy.
pub struct InlineCheapReserve;
impl Pass for InlineCheapReserve {
    fn name(&self) -> &'static str {
        "inline-cheap-reserve"
    }
    fn description(&self) -> &'static str {
        "fair bounded cheap-helper reserve"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        let callers = enabled_callers(module, context);
        let report = run_observed_policy_budgeted(
            module,
            context.sites.as_deref_mut(),
            InlinePolicy {
                cheap_reserve: true,
                ..InlinePolicy::NONINTERPOSABLE
            },
            Some(&callers),
            context.budget,
        );
        Some(report.changed_callers(module, context.changed))
    }
    fn uses_inline_budget(&self) -> bool {
        true
    }
    fn family(&self) -> PassFamily {
        PassFamily::Inline
    }
}

/// Reasons an explicit alwaysinline request could not be honored. No stderr
/// side effects: embedders can display this report alongside their diagnostics.
#[derive(Debug, Default)]
pub struct InlineReport {
    pub changed: bool,
    pub skipped: Vec<InlineSkip>,
    /// Indices of the functions that received an inlined body.
    callers: Vec<usize>,
}

impl InlineReport {
    /// Marks the callers that changed; nothing else in the module does.
    fn changed_callers(&self, module: &Module, changed: &mut Vec<bool>) -> bool {
        changed.clear();
        changed.resize(module.functions.len(), false);
        for &index in &self.callers {
            changed[index] = true;
        }
        self.changed
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct InlineSkip {
    pub caller: String,
    pub callee: String,
    pub site: Inst,
    pub reason: &'static str,
}

pub fn run_with_report(module: &mut Module) -> InlineReport {
    run_observed(module, None)
}

// Build the graph from every definition, including definitions that cannot
// become templates. A cycle can pass through a noinline, public, tail-calling,
// or exception-handling function. External and indirect calls add no edges.
// `sites` holds each function's direct call instructions in layout order.
// Also returns every definition with callees before callers, except within a
// cycle.
fn recursive_definitions(module: &Module, sites: &[Vec<Inst>]) -> (Vec<bool>, Vec<usize>) {
    let definitions: FxHashMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, f)| (f.name.as_str(), index))
        .collect();
    let graph: Vec<Vec<usize>> = module
        .functions
        .iter()
        .zip(sites)
        .map(|(f, calls)| {
            calls
                .iter()
                .filter_map(|&i| {
                    let reference = match f.inst_data(i) {
                        InstData::Call { func, .. }
                        | InstData::ReturnCall { func, .. }
                        | InstData::Invoke { func, .. } => *func,
                        _ => return None,
                    };
                    definitions.get(f.callee(reference).name.as_str()).copied()
                })
                .collect()
        })
        .collect();
    recursive_members(&graph)
}

// Iterative Kosaraju traversal: no host recursion, even on deep call chains.
// Only SCC members are recursive; an acyclic caller reaching a cycle is not.
// The second result is the first traversal's finish order: callees first.
fn recursive_members(graph: &[Vec<usize>]) -> (Vec<bool>, Vec<usize>) {
    let n = graph.len();
    let mut reverse = vec![Vec::new(); n];
    for (from, tos) in graph.iter().enumerate() {
        for &to in tos {
            reverse[to].push(from);
        }
    }
    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut pending = Vec::new();
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        pending.push((root, 0));
        while let Some((node, next)) = pending.last_mut() {
            if let Some(&to) = graph[*node].get(*next) {
                *next += 1;
                if !seen[to] {
                    seen[to] = true;
                    pending.push((to, 0));
                }
            } else {
                order.push(*node);
                pending.pop();
            }
        }
    }
    seen.fill(false);
    let mut recursive = vec![false; n];
    let mut pending = Vec::new();
    let mut component = Vec::new();
    for &root in order.iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        pending.push(root);
        component.clear();
        while let Some(node) = pending.pop() {
            component.push(node);
            for &to in &reverse[node] {
                if !seen[to] {
                    seen[to] = true;
                    pending.push(to);
                }
            }
        }
        if component.len() > 1 || graph[root].contains(&root) {
            for &node in &component {
                recursive[node] = true;
            }
        }
    }
    (recursive, order)
}

#[cfg(test)]
mod recursion_tests {
    use super::recursive_members;

    #[test]
    fn very_deep_graph_does_not_use_the_host_stack() {
        let n = 100_000;
        let mut graph: Vec<Vec<_>> = (0..n)
            .map(|i| if i + 1 < n { vec![i + 1] } else { vec![] })
            .collect();
        let (recursive, order) = recursive_members(&graph);
        assert!(recursive.iter().all(|&r| !r));
        assert!(order.iter().copied().eq((0..n).rev()));
        graph[n - 1].push(1);
        let (recursive, _) = recursive_members(&graph);
        assert!(!recursive[0]);
        assert!(recursive[1..].iter().all(|&r| r));
    }
}
