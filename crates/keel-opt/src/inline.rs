//! Bounded direct-call inlining of definitions with proven binding. Templates
//! are frozen at
//! pass entry: newly cloned calls are not reconsidered, so growth and recursion
//! cannot recursively reconsider cloned sites. The module budget is consumed in
//! layout order. ABI by-value arguments get a fresh snapshot.
//! Instruction growth bounds the increase in layout instructions before cleanup;
//! the removed call pays for the jump entering the cloned body.
use crate::{InlineSite, Pass};
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{
    BinaryOp, BlockCall, DebugScope, EntityRef, Function, InsertPoint, Inst, InstData, Intrinsic,
    Module, Purpose, Symbol, Type,
};
use std::collections::{HashMap, HashSet};

pub struct Inline;
impl Pass for Inline {
    fn name(&self) -> &'static str {
        "inline"
    }
    fn description(&self) -> &'static str {
        "inline bounded private direct-call functions"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed(module, Some(sites)).changed)
    }
}
/// Explicit research pass. Never included in an optimization tier.
/// Uses the ordinary clone implementation and legality checks; only a separate
/// small aggregate-leaf instruction reserve differs. No cleanup or alias-map
/// reconstruction is performed as part of admission.
pub struct InlineAggregateReserve;
impl Pass for InlineAggregateReserve {
    fn name(&self) -> &'static str {
        "inline-aggregate-reserve"
    }
    fn description(&self) -> &'static str {
        "experimental bounded aggregate-leaf inlining reserve"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run_observed_policy(module, None, true, false).changed)
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed_policy(module, Some(sites), true, false).changed)
    }
}

/// Explicit binding-aware integration. No tier schedules this pass. It shares
/// ordinary inlining's legality, profitability, budgets and cloning machinery;
/// only exported definitions with a producer binding promise gain admission.
pub struct InlineNoninterposable;
impl Pass for InlineNoninterposable {
    fn name(&self) -> &'static str {
        "inline-noninterposable"
    }
    fn description(&self) -> &'static str {
        "inline bounded private or explicitly noninterposable direct-call functions"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run_observed_policy(module, None, false, true).changed)
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed_policy(module, Some(sites), false, true).changed)
    }
}

const RESERVE_SITE_GROWTH: usize = 32;
const RESERVE_CALLER_SITES: usize = 8;
const RESERVE_MODULE_SITES: usize = 8;

// A bounded structural domain, not a profitability or hotness prediction.
// All ordinary eligibility/signature/recursion checks run first.
fn aggregate_leaf(f: &Function, cost: usize, stack_cost: u64) -> bool {
    cost <= RESERVE_SITE_GROWTH
        && stack_cost <= 64
        && f.blocks().count() <= 4
        && f.sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, Purpose::Sret(_)))
        && f.sig
            .params
            .iter()
            .any(|p| matches!(&p.purpose, Purpose::ByVal(l) if l.size() <= 16))
        && !f.layout_insts().any(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Intrinsic { .. }
            )
        })
}

const MAX_INSTS: usize = 64;
const MAX_BLOCKS: usize = 8;
const MAX_STACK: u64 = 512;
const MAX_GROWTH: usize = 2048;
const MAX_CALLS: usize = 128;
const MAX_MODULE_GROWTH: usize = 16_384;
const HARD_STACK: u64 = 65_536;

// Switches spend blocks on cheap returns. Admit only a small, acyclic integer
// leaf, charging every branch destination as well as every instruction. This
// is a structural exception to MAX_BLOCKS, not to binding or growth budgets.
fn cheap_leaf_switch(f: &Function) -> bool {
    const BLOCKS: usize = 16;
    const EDGES: usize = 32;
    if f.blocks().take(BLOCKS + 1).count() > BLOCKS
        || f.layout_insts().take(MAX_INSTS + 1).count() > MAX_INSTS
        || f.stack_slots().next().is_some()
    {
        return false;
    }
    let mut weighted = 0;
    let mut edges = 0;
    let mut switch = false;
    for (_, i) in f.layout_insts() {
        let data = f.inst_data(i);
        match data {
            InstData::Iconst { .. }
            | InstData::Icmp { .. }
            | InstData::Return { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. } => {}
            InstData::Binary { op, .. }
                if matches!(
                    op,
                    BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::And | BinaryOp::Or | BinaryOp::Xor
                ) => {}
            InstData::Switch { .. } => switch = true,
            _ => return false,
        }
        let destinations = data.branch_dests().len();
        edges += destinations;
        weighted += 1 + destinations;
        if edges > EDGES || weighted > MAX_INSTS {
            return false;
        }
    }
    if !switch {
        return false;
    }
    // Check all laid-out blocks, including unreachable cycles. Use compact
    // layout indices rather than arena IDs: deleted blocks can leave holes.
    let blocks: Vec<_> = f.blocks().collect();
    let mut successors = vec![Vec::new(); blocks.len()];
    let mut indegree = vec![0; blocks.len()];
    for (index, &block) in blocks.iter().enumerate() {
        for &i in f.block_insts(block) {
            for dest in f.inst_data(i).branch_dests() {
                let j = blocks.iter().position(|&b| b == dest.block).unwrap();
                successors[index].push(j);
                indegree[j] += 1;
            }
        }
    }
    let mut ready: Vec<_> = (0..blocks.len()).filter(|&i| indegree[i] == 0).collect();
    let mut visited = 0;
    while let Some(i) = ready.pop() {
        visited += 1;
        for &j in &successors[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                ready.push(j);
            }
        }
    }
    visited == blocks.len()
}

fn byval_copy_chunks(size: u64) -> impl Iterator<Item = (u64, Type)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let remaining = size - offset;
        let (bytes, ty) = match remaining {
            0 => return None,
            1 => (1, Type::I8),
            2..=3 => (2, Type::I16),
            4..=7 => (4, Type::I32),
            _ => (8, Type::I64),
        };
        let chunk = (offset, ty);
        offset += bytes;
        Some(chunk)
    })
}

fn byval_copy_cost(size: u64) -> usize {
    // One stack_addr, a load/store per chunk, and an iconst plus two
    // ptradds for every chunk after the first. Eligibility bounds size to 128.
    1 + byval_copy_chunks(size)
        .map(|(offset, _)| if offset == 0 { 2 } else { 5 })
        .sum::<usize>()
}

/// Reasons an explicit alwaysinline request could not be honored. No stderr
/// side effects: embedders can display this report alongside their diagnostics.
#[derive(Debug, Default)]
pub struct InlineReport {
    pub changed: bool,
    pub skipped: Vec<InlineSkip>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct InlineSkip {
    pub caller: String,
    pub callee: String,
    pub site: Inst,
    pub reason: &'static str,
}

fn profitability_reason(f: &Function) -> Option<&'static str> {
    let scale = if f.attrs.inlinehint { 2 } else { 1 };
    if f.attrs.alwaysinline {
        return None;
    }
    if f.blocks().count() > MAX_BLOCKS * scale && !cheap_leaf_switch(f) {
        Some("profitability_blocks")
    } else if f.layout_insts().count() > MAX_INSTS * scale {
        Some("profitability_instructions")
    } else if f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>()
        > MAX_STACK * scale as u64
    {
        Some("profitability_stack_declaration_bytes")
    } else {
        None
    }
}

fn eligibility_reason(f: &Function, admit_noninterposable: bool) -> Option<&'static str> {
    if !f.local && !(admit_noninterposable && f.attrs.noninterposable) {
        return Some("public_definition");
    }
    if f.attrs.noinline {
        return Some("noinline");
    }
    if f.sig.variadic {
        return Some("variadic_callee");
    }
    if f.sig.returns_twice {
        return Some("returns_twice_callee");
    }
    if f.personality.is_some() {
        return Some("callee_personality");
    }
    if f.entry_block().is_none() {
        return Some("callee_without_entry");
    }
    if f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>() > HARD_STACK {
        return Some("callee_hard_stack_declaration_bytes");
    }
    if f.sig
        .params
        .iter()
        .any(|p| matches!(&p.purpose, Purpose::ByVal(l) if l.size() > 128))
    {
        return Some("byval_snapshot_size");
    }
    for (_, i) in f.layout_insts() {
        let reason = match f.inst_data(i) {
            InstData::Call { func, .. } if f.callee(*func).sig.returns_twice => {
                Some("callee_calls_returns_twice")
            }
            InstData::CallIndirect { sig, .. } if f.signature(*sig).returns_twice => {
                Some("callee_calls_returns_twice")
            }
            InstData::Intrinsic { id, .. }
                if !matches!(
                    id,
                    Intrinsic::Memcpy
                        | Intrinsic::Memmove
                        | Intrinsic::Memset
                        | Intrinsic::Debugtrap
                ) =>
            {
                Some("unsupported_callee_intrinsic")
            }
            InstData::VaArg { .. } => Some("callee_va_arg"),
            InstData::Invoke { .. } => Some("callee_invoke"),
            InstData::LandingPad { .. } | InstData::EhTypeid { .. } => {
                Some("callee_exception_instruction")
            }
            InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } => {
                Some("callee_tail_call")
            }
            InstData::InlineAsm { .. } => Some("callee_inline_asm"),
            _ => None,
        };
        if reason.is_some() {
            return reason;
        }
    }
    None
}

pub fn run(module: &mut Module) -> bool {
    run_with_report(module).changed
}

pub fn run_with_report(module: &mut Module) -> InlineReport {
    run_observed(module, None)
}

// Build the graph from every definition, including definitions that cannot
// become templates. A cycle can pass through a noinline, public, tail-calling,
// or exception-handling function. External and indirect calls add no edges.
fn recursive_definitions(module: &Module) -> Vec<bool> {
    let definitions: HashMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, f)| (f.name.as_str(), index))
        .collect();
    let graph: Vec<Vec<usize>> = module
        .functions
        .iter()
        .map(|f| {
            f.layout_insts()
                .filter_map(|(_, i)| {
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
fn recursive_members(graph: &[Vec<usize>]) -> Vec<bool> {
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
    for root in order.into_iter().rev() {
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
    recursive
}

fn run_observed(module: &mut Module, sites: Option<&mut Vec<InlineSite>>) -> InlineReport {
    run_observed_policy(module, sites, false, false)
}

fn run_observed_policy(
    module: &mut Module,
    mut sites: Option<&mut Vec<InlineSite>>,
    experimental: bool,
    admit_noninterposable: bool,
) -> InlineReport {
    let recursive = recursive_definitions(module);
    let explicit_definitions = sites.as_ref().map(|_| {
        module
            .functions
            .iter()
            .filter(|f| f.attrs.alwaysinline)
            .map(|f| f.name.clone())
            .collect::<HashSet<_>>()
    });
    // Freeze both templates and their rejection decisions at pass entry.
    // Keep the existing diagnostic precedence (noinline, recursion, legality,
    // profitability); the event stream uses these decisions, not a later probe.
    let mut templates = HashMap::new();
    let mut rejected = HashMap::new();
    for (index, f) in module.functions.iter().enumerate() {
        let legality = eligibility_reason(f, admit_noninterposable);
        let profitability = if legality.is_none() {
            profitability_reason(f)
        } else {
            None
        };
        let recursion = !f.attrs.noinline && recursive[index];
        let reason = if f.attrs.noinline {
            Some("noinline")
        } else if recursion {
            Some("recursive_callee")
        } else {
            legality.or(profitability)
        };
        if let Some(reason) = reason {
            rejected.insert(f.name.clone(), reason);
        } else {
            templates.insert(f.name.clone(), f.clone());
        }
    }
    let mut report = InlineReport::default();
    let mut module_growth = 0;
    let mut module_reserve_sites = 0;
    for caller in &mut module.functions {
        let calls: Vec<_> = caller
            .layout_insts()
            .filter_map(|(_, i)| {
                matches!(
                    caller.inst_data(i),
                    InstData::Call { .. } | InstData::Invoke { .. } | InstData::ReturnCall { .. }
                )
                .then_some(i)
            })
            .collect();
        let mut growth = 0;
        let mut reserve_sites = 0;
        let mut count = 0;
        let mut stack_growth = 0;
        for call in calls {
            let reference = match caller.inst_data(call) {
                InstData::Call { func, .. }
                | InstData::Invoke { func, .. }
                | InstData::ReturnCall { func, .. } => *func,
                _ => unreachable!(),
            };
            let decl = caller.callee(reference);
            let requested = decl.attrs.alwaysinline
                || templates
                    .get(&decl.name)
                    .is_some_and(|f| f.attrs.alwaysinline);
            let reason = if caller.sig.returns_twice || caller.personality.is_some() {
                Some((
                    "unsupported caller",
                    if caller.sig.returns_twice {
                        "returns_twice_caller"
                    } else {
                        "caller_personality"
                    },
                ))
            } else if !matches!(caller.inst_data(call), InstData::Call { .. }) {
                Some((
                    "unsupported call instruction",
                    "unsupported_call_instruction",
                ))
            } else if decl.attrs.noinline {
                Some(("noinline policy", "noinline"))
            } else if let Some(callee) = templates.get(&decl.name) {
                if decl.sig != callee.sig {
                    Some(("ABI signature mismatch", "abi_signature_mismatch"))
                } else if callee.name == caller.name {
                    Some(("recursive callee", "recursive_callee"))
                } else {
                    None
                }
            } else {
                Some({
                    let detail = rejected.get(&decl.name).copied().unwrap_or("no_definition");
                    let legacy = match detail {
                        "noinline" => "noinline policy",
                        "recursive_callee" => "recursive callee",
                        "no_definition" => "no private definition",
                        r if r.starts_with("profitability_") => "profitability threshold",
                        _ => "unsupported callee or hard stack bound",
                    };
                    (legacy, detail)
                })
            };
            if let Some((reason, detail)) = reason {
                if let Some(sites) = sites.as_deref_mut() {
                    sites.push(InlineSite::new(
                        caller,
                        &decl.name,
                        call,
                        requested
                            || explicit_definitions
                                .as_ref()
                                .is_some_and(|names| names.contains(&decl.name)),
                        detail,
                    ));
                }
                if requested {
                    report.skipped.push(InlineSkip {
                        caller: caller.name.clone(),
                        callee: decl.name.clone(),
                        site: call,
                        reason,
                    });
                }
                continue;
            }
            let callee = &templates[&decl.name];
            // Bound net layout growth before cleanup. The entry jump replaces
            // the removed call; each cloned return becomes one jump. The
            // continuation only receives moved instructions and parameters.
            // Counting all callee instructions also covers unreachable blocks
            // that the dominator traversal may omit. Snapshot IR is additional.
            let cost = callee.layout_insts().count()
                + callee
                    .sig
                    .params
                    .iter()
                    .map(|p| match &p.purpose {
                        Purpose::ByVal(l) => byval_copy_cost(l.size()),
                        _ => 0,
                    })
                    .sum::<usize>();
            let stack_cost = callee
                .stack_slots()
                .map(|(_, s)| u64::from(s.size))
                .sum::<u64>()
                + callee
                    .sig
                    .params
                    .iter()
                    .map(|p| match &p.purpose {
                        Purpose::ByVal(l) => l.size(),
                        _ => 0,
                    })
                    .sum::<u64>();
            let budget = if count == MAX_CALLS {
                Some("caller_call_count_budget")
            } else if growth + cost > MAX_GROWTH {
                Some("caller_instruction_growth_budget")
            } else if module_growth + cost > MAX_MODULE_GROWTH {
                Some("module_instruction_growth_budget")
            } else if stack_growth + stack_cost > HARD_STACK {
                Some("caller_stack_growth_budget")
            } else {
                None
            };
            // Reserve cannot bypass call-count, stack, legality or profitability.
            // Ordinary growth ledgers retain their exact original limits. Reserve
            // sites have separate worst-case 8*32/caller and 8*32/module bounds.
            let use_reserve = experimental
                && matches!(
                    budget,
                    Some("caller_instruction_growth_budget" | "module_instruction_growth_budget")
                )
                && count < MAX_CALLS
                && stack_growth + stack_cost <= HARD_STACK
                && reserve_sites < RESERVE_CALLER_SITES
                && module_reserve_sites < RESERVE_MODULE_SITES
                && aggregate_leaf(callee, cost, stack_cost);
            if let Some(detail) = budget.filter(|_| !use_reserve) {
                if let Some(sites) = sites.as_deref_mut() {
                    sites.push(InlineSite::new(
                        caller,
                        &callee.name,
                        call,
                        requested
                            || explicit_definitions
                                .as_ref()
                                .is_some_and(|names| names.contains(&decl.name)),
                        detail,
                    ));
                }
                if requested {
                    report.skipped.push(InlineSkip {
                        caller: caller.name.clone(),
                        callee: callee.name.clone(),
                        site: call,
                        reason: "hard growth safeguard",
                    });
                }
                continue;
            }
            if let Some(sites) = sites.as_deref_mut() {
                sites.push(InlineSite::new(
                    caller,
                    &callee.name,
                    call,
                    requested,
                    if use_reserve {
                        "experimental_reserve_inlined"
                    } else {
                        "inlined"
                    },
                ));
            }
            let before = use_reserve.then(|| caller.layout_insts().count());
            inline_call(caller, call, callee);
            if let Some(before) = before {
                // Fail compilation before codegen on an accounting violation.
                // No recoverable post-mutation fallback or artifact is allowed.
                let actual_net_growth = caller
                    .layout_insts()
                    .count()
                    .checked_sub(before)
                    .expect("reserve inlining unexpectedly shrank layout");
                assert!(
                    actual_net_growth <= cost && cost <= RESERVE_SITE_GROWTH,
                    "reserve emitted net growth exceeds conservative preflight"
                );
                if let Some(site) = sites.as_deref_mut().and_then(|s| s.last_mut()) {
                    site.experimental_net_growth = Some(actual_net_growth);
                }
                reserve_sites += 1;
                module_reserve_sites += 1;
            } else {
                growth += cost;
                module_growth += cost;
            }
            count += 1;
            stack_growth += stack_cost;
            report.changed = true;
        }
    }
    report
}

fn inline_call(caller: &mut Function, call: Inst, callee: &Function) {
    let InstData::Call { args, .. } = caller.inst_data(call).clone() else {
        unreachable!()
    };
    let origin = caller.inst_loc(call);
    let call_block = caller.inst_block(call);
    let continuation = caller.create_block();
    let results = caller.inst_results(call).to_vec();
    for old in results {
        let new = caller.append_block_param(continuation, caller.value_type(old));
        caller.replace_all_uses(old, new);
    }
    let tail: Vec<_> = caller
        .block_insts(call_block)
        .iter()
        .copied()
        .skip_while(|&i| i != call)
        .skip(1)
        .collect();
    for inst in tail {
        caller.move_inst(inst, InsertPoint::End(continuation));
    }
    let mut args: Vec<_> = args.into_iter().map(|v| caller.resolve(v)).collect();
    // Copy every byte, including padding. All snapshots precede callee effects.
    for (arg, param) in args.iter_mut().zip(&callee.sig.params) {
        if let Purpose::ByVal(layout) = &param.purpose {
            let slot = caller.create_stack_slot(layout.size() as u32, layout.align() as u32);
            let dst = caller.ins(InsertPoint::Before(call)).stack_addr(slot);
            for (offset, ty) in byval_copy_chunks(layout.size()) {
                let mut src_at = *arg;
                let mut dst_at = dst;
                if offset != 0 {
                    let n = caller
                        .ins(InsertPoint::Before(call))
                        .iconst(Type::I64, offset as i64);
                    src_at = caller.ins(InsertPoint::Before(call)).ptradd(*arg, n);
                    dst_at = caller.ins(InsertPoint::Before(call)).ptradd(dst, n);
                }
                let value = caller.ins(InsertPoint::Before(call)).load(ty, src_at);
                caller.ins(InsertPoint::Before(call)).store(value, dst_at);
            }
            *arg = dst;
        }
    }
    let slots: HashMap<_, _> = callee
        .stack_slots()
        .map(|(slot, data)| (slot, caller.create_stack_slot(data.size, data.align)))
        .collect();
    let refs: HashMap<_, _> = callee
        .callees()
        .map(|(r, decl)| {
            let imported = caller.import_function(&decl.name, decl.sig.clone());
            caller.callee_mut(imported).attrs.merge(decl.attrs);
            (r, imported)
        })
        .collect();
    let data: HashMap<_, _> = callee
        .data_symbols()
        .map(|(r, name)| (r, caller.import_data(name)))
        .collect();
    let sigs: HashMap<_, _> = callee
        .signatures()
        .map(|(r, sig)| (r, caller.import_signature(sig.clone())))
        .collect();
    // IR has lexical scopes but no inline-subroutine metadata. Keep callee
    // source lines in a fresh lexical subtree beneath the caller's call site.
    let root = DebugScope::new(caller.debug_scopes.len());
    caller.debug_scopes.push(origin.and_then(|l| l.scope));
    let mut scopes = Vec::new();
    for parent in &callee.debug_scopes {
        let scope = DebugScope::new(caller.debug_scopes.len());
        caller
            .debug_scopes
            .push(Some(parent.map_or(root, |p| scopes[p.index()])));
        scopes.push(scope);
    }
    for var in &callee.debug_vars {
        let mut var = var.clone();
        var.slot = slots[&var.slot];
        var.scope = Some(var.scope.map_or(root, |s| scopes[s.index()]));
        // A callee parameter is a local variable in the caller, not a formal.
        var.param = false;
        caller.debug_vars.push(var);
    }
    let cfg = ControlFlowGraph::compute(callee);
    let dom = DominatorTree::compute(callee, &cfg);
    let mut blocks = HashMap::new();
    let mut values = HashMap::new();
    for &old in dom.rpo() {
        let new = caller.create_block();
        blocks.insert(old, new);
        for &v in callee.block_params(old) {
            values.insert(v, caller.append_block_param(new, callee.value_type(v)));
        }
    }
    for &block in dom.rpo() {
        let dest = blocks[&block];
        for &old in callee.block_insts(block) {
            let mut inst = callee.inst_data(old).clone();
            inst.for_each_operand_mut(|v| *v = values[&callee.resolve(*v)]);
            for edge in inst.branch_dests_mut() {
                edge.block = blocks[&edge.block];
            }
            match &mut inst {
                InstData::StackAddr { slot } => *slot = slots[slot],
                InstData::SymbolAddr { symbol } => {
                    *symbol = match *symbol {
                        Symbol::Func(r) => Symbol::Func(refs[&r]),
                        Symbol::Data(r) => Symbol::Data(data[&r]),
                    }
                }
                InstData::Call { func, .. } => *func = refs[func],
                InstData::CallIndirect { sig, .. } => *sig = sigs[sig],
                InstData::Return { args } => {
                    inst = InstData::Jump {
                        dest: BlockCall {
                            block: continuation,
                            args: args.clone(),
                        },
                    }
                }
                _ => {}
            }
            let new = caller.append_inst(dest, inst);
            for (&old_v, &new_v) in callee
                .inst_results(old)
                .iter()
                .zip(caller.inst_results(new))
            {
                values.insert(old_v, new_v);
            }
            let loc = callee
                .inst_loc(old)
                .map(|mut l| {
                    l.scope = Some(l.scope.map_or(root, |s| scopes[s.index()]));
                    l
                })
                .or(origin);
            caller.set_inst_loc(new, loc);
        }
    }
    caller.remove_inst(call);
    let jump = caller.append_inst(
        call_block,
        InstData::Jump {
            dest: BlockCall {
                block: blocks[&callee.entry_block().unwrap()],
                args,
            },
        },
    );
    caller.set_inst_loc(jump, origin);
}

#[cfg(test)]
mod byval_tests {
    use super::*;

    #[test]
    fn copy_cost_matches_real_emitted_ir_for_every_supported_size() {
        for size in 1..=128 {
            // i8 fields give every byte size without aggregate tail padding.
            let fields = vec!["i8"; size].join(", ");
            let mut module = keel_ir::parse(&format!(
                "func @h(ptr byval {{{fields}}}) local {{\nblock0(v0: ptr):\n return\n}}\nfunc @f(ptr) {{\nblock0(v0: ptr):\n call @h(v0)\n return\n}}"
            ))
            .unwrap();
            keel_ir::verify_module(&module).unwrap();
            let before = module.functions[1].layout_insts().count();
            assert!(run(&mut module));
            keel_ir::verify_module(&module).unwrap();
            let caller = &module.functions[1];
            assert_eq!(
                caller.layout_insts().count() - before,
                1 + byval_copy_cost(size as u64),
                "net growth for size {size}",
            );
            // This block contains only the snapshot and the jump replacing
            // the call. Count real IR, independently of the chunk iterator.
            let insts = caller.block_insts(caller.entry_block().unwrap());
            assert!(matches!(
                caller.inst_data(*insts.last().unwrap()),
                InstData::Jump { .. }
            ));
            assert_eq!(byval_copy_cost(size as u64), insts.len() - 1, "size {size}");
            let copied_bytes: u64 = insts
                .iter()
                .filter_map(|&inst| match caller.inst_data(inst) {
                    InstData::Load { .. } => Some(
                        caller
                            .value_type(caller.inst_results(inst)[0])
                            .bytes()
                            .unwrap() as u64,
                    ),
                    _ => None,
                })
                .sum();
            assert_eq!(copied_bytes, size as u64);
        }
    }

    #[test]
    fn copy_chunks_keep_wide_loads_and_odd_tails() {
        for (size, expected) in [
            (1, vec![Type::I8]),
            (7, vec![Type::I32, Type::I16, Type::I8]),
            (8, vec![Type::I64]),
            (9, vec![Type::I64, Type::I8]),
            (15, vec![Type::I64, Type::I32, Type::I16, Type::I8]),
            (16, vec![Type::I64, Type::I64]),
            (128, vec![Type::I64; 16]),
        ] {
            assert_eq!(
                byval_copy_chunks(size)
                    .map(|(_, ty)| ty)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        // Empty aggregates are rejected by IR verification; preserve the
        // emitter's stack_addr-only behavior if invoked on an empty layout.
        assert_eq!(byval_copy_chunks(0).count(), 0);
        assert_eq!(byval_copy_cost(0), 1);
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;
    // Isolated guard fixtures may be structurally invalid whole functions: these
    // test exact decisions without implying the verifier accepts such inputs.
    #[test]
    fn eligibility_reports_each_unsupported_construct() {
        let source = "func @h(ptr) local {\nblock0(v0: ptr):\n return\n}";
        for (body, expected) in [
            ("v1 = va_arg.i64 v0\n return", "callee_va_arg"),
            (
                "v1, v2 = landingpad cleanup\n return",
                "callee_exception_instruction",
            ),
            (
                "v1 = eh_typeid @type\n return",
                "callee_exception_instruction",
            ),
            ("return_call @external(v0)", "callee_tail_call"),
            ("return_call_indirect v0(v0) : (ptr)", "callee_tail_call"),
            (
                "invoke @external(v0), block1, block2\nblock1:\n return\nblock2:\n trap",
                "callee_invoke",
            ),
        ] {
            let text = format!(
                "decl @external(ptr)\ndecl data @type\n{}",
                source.replace(" return", body)
            );
            let module = keel_ir::parse(&text).unwrap();
            assert_eq!(
                eligibility_reason(&module.functions[0], false),
                Some(expected),
                "{text}"
            );
        }
        for (attrs, expected) in [
            ("returns_twice", "returns_twice_callee"),
            ("personality @p", "callee_personality"),
        ] {
            let text = if attrs == "returns_twice" {
                source.replace("local {", "returns_twice local {")
            } else {
                source.replace("local {", &format!("local {attrs} {{"))
            };
            let module = keel_ir::parse(&text).unwrap();
            assert_eq!(
                eligibility_reason(&module.functions[0], false),
                Some(expected)
            );
        }
        let mut f = Function::new("empty", keel_ir::Signature::new(&[], &[]));
        f.local = true;
        assert_eq!(eligibility_reason(&f, false), Some("callee_without_entry"));
        let fields = vec!["i64"; 17].join(", ");
        let module = keel_ir::parse(&format!(
            "func @h(ptr byval {{{fields}}}) local {{\nblock0(v0: ptr):\n return\n}}"
        ))
        .unwrap();
        assert_eq!(
            eligibility_reason(&module.functions[0], false),
            Some("byval_snapshot_size")
        );
        let mut module = keel_ir::parse(source).unwrap();
        let f = &mut module.functions[0];
        let inst = f.layout_insts().next().unwrap().1;
        *f.inst_data_mut(inst) = InstData::InlineAsm {
            asm: Box::new(keel_ir::InlineAsm {
                template: String::new(),
                outputs: vec![],
                inputs: vec![],
                clobbers: vec![],
            }),
            args: vec![],
        };
        assert_eq!(eligibility_reason(f, false), Some("callee_inline_asm"));
        for indirect in [false, true] {
            let mut module = keel_ir::parse(source).unwrap();
            let f = &mut module.functions[0];
            let mut sig = keel_ir::Signature::new(&[], &[]);
            sig.returns_twice = true;
            let call = if indirect {
                let sig = f.import_signature(sig);
                InstData::CallIndirect {
                    sig,
                    callee: keel_ir::Value::new(0),
                    args: vec![],
                }
            } else {
                let func = f.import_function("external", sig);
                InstData::Call { func, args: vec![] }
            };
            let inst = f.layout_insts().next().unwrap().1;
            *f.inst_data_mut(inst) = call;
            assert_eq!(
                eligibility_reason(f, false),
                Some("callee_calls_returns_twice")
            );
        }
    }
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
        assert!(recursive_members(&graph).iter().all(|&r| !r));
        graph[n - 1].push(1);
        let recursive = recursive_members(&graph);
        assert!(!recursive[0]);
        assert!(recursive[1..].iter().all(|&r| r));
    }
}
