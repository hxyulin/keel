//! Bounded direct-call inlining of definitions with proven binding. Templates
//! are frozen at
//! pass entry: newly cloned calls are not reconsidered, so growth and recursion
//! cannot recursively reconsider cloned sites. Full schedules bounded tiny
//! alwaysinline and guarded, writing hinted leaves first. Each phase otherwise
//! takes turns across callers. Other
//! policies consume budgets in layout order. ABI by-value arguments get a fresh snapshot.
//! Instruction growth bounds the increase in layout instructions before cleanup;
//! the removed call pays for the jump entering the cloned body.
use crate::{InlineSite, Pass};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp, BlockCall, DebugScope, EntityRef, Function, InsertPoint, Inst, InstData, Intrinsic,
    Module, Purpose, Symbol, Type,
};

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
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let report = run_observed(module, sites);
        Some(report.changed_callers(module, changed))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: false,
                cheap_reserve: false,
                cost: InlineCost::Default,
            },
            Some(&callers),
        );
        Some(report.changed_callers(module, changed))
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
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: true,
                cheap_reserve: false,
                cost: InlineCost::Size,
            },
            None,
        );
        Some(report.changed_callers(module, changed))
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(
            run_observed_policy_scoped(
                module,
                None,
                InlinePolicy {
                    experimental: false,
                    admit_noninterposable: true,
                    cheap_reserve: false,
                    cost: InlineCost::Size,
                },
                None,
            )
            .changed,
        )
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: true,
                cheap_reserve: false,
                cost: InlineCost::Size,
            },
            Some(&callers),
        );
        Some(report.changed_callers(module, changed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineCost {
    Default,
    Size,
}

#[derive(Clone, Copy, Debug)]
struct InlinePolicy {
    experimental: bool,
    admit_noninterposable: bool,
    cheap_reserve: bool,
    cost: InlineCost,
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
        Some(run_observed_policy(module, None, true, false, false).changed)
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed_policy(module, Some(sites), true, false, false).changed)
    }
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let report = run_observed_policy(module, sites, true, false, false);
        Some(report.changed_callers(module, changed))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: true,
                admit_noninterposable: false,
                cheap_reserve: false,
                cost: InlineCost::Default,
            },
            Some(&callers),
        );
        Some(report.changed_callers(module, changed))
    }
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
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run_observed_policy(module, None, false, true, false).changed)
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed_policy(module, Some(sites), false, true, false).changed)
    }
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let report = run_observed_policy(module, sites, false, true, false);
        Some(report.changed_callers(module, changed))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: true,
                cheap_reserve: false,
                cost: InlineCost::Default,
            },
            Some(&callers),
        );
        Some(report.changed_callers(module, changed))
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
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module_with_inline_budget(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
        budget: &mut InlineBudget,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_budgeted(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: true,
                cheap_reserve: true,
                cost: InlineCost::Default,
            },
            Some(&callers),
            budget,
        );
        Some(report.changed_callers(module, changed))
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run_observed_policy(module, None, false, true, true).changed)
    }
    fn run_module_with_report(
        &self,
        module: &mut Module,
        sites: &mut Vec<InlineSite>,
    ) -> Option<bool> {
        Some(run_observed_policy(module, Some(sites), false, true, true).changed)
    }
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let report = run_observed_policy(module, sites, false, true, true);
        Some(report.changed_callers(module, changed))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let callers = module
            .functions
            .iter()
            .filter(|f| enabled(f))
            .map(|f| f.name.clone())
            .collect();
        let report = run_observed_policy_scoped(
            module,
            sites,
            InlinePolicy {
                experimental: false,
                admit_noninterposable: true,
                cheap_reserve: true,
                cost: InlineCost::Default,
            },
            Some(&callers),
        );
        Some(report.changed_callers(module, changed))
    }
}

const CHEAP_SITE_GROWTH: usize = 48;
pub(crate) const CHEAP_MODULE_GROWTH: usize = 4096;
const CHEAP_MODULE_SITES: usize = 256;

// A cheap predicate or guarded single slow-call wrapper, independent of names
// and inline attributes. Charge all layout instructions and ABI snapshots;
// there is no speculative cleanup credit. Cloning preserves memory/trap order.
fn cheap_helper(f: &Function, cost: usize, stack: u64) -> bool {
    if cost > CHEAP_SITE_GROWTH || stack > 64 || f.blocks().count() > 8 {
        return false;
    }
    let mut calls = 0;
    let mut predicate = false;
    for (_, i) in f.layout_insts() {
        match f.inst_data(i) {
            InstData::Icmp { .. } => predicate = true,
            InstData::Call { .. } => calls += 1,
            InstData::CallIndirect { .. } | InstData::Intrinsic { .. } => return false,
            _ => {}
        }
    }
    predicate && calls <= 1 && acyclic_cfg(f)
}

const RESERVE_SITE_GROWTH: usize = 32;
const RESERVE_CALLER_SITES: usize = 8;
const RESERVE_MODULE_SITES: usize = 8;

// Contracting a recursive wrapper still adds branches, stack snapshots and
// live values to its caller. Keep that exception out of very large functions;
// ordinary profitable inlining and read-only leaf admission remain available.
const MAX_SCC_CALLER_INSTRUCTIONS: usize = 8192;

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
// A call costs about this many instructions before its arguments and
// results: LLVM's call penalty of 25 at 5 per instruction. A callee that
// costs no more than its call does not count against MAX_CALLS.
const CALL_SITE_COST: usize = 5;
const MAX_MODULE_GROWTH: usize = 16_384;
// Preserve the old three Full rounds' aggregate allowance for small modules.
const FULL_MIN_MODULE_GROWTH: usize = 196_608;
// Charge at most 50% of entry IR above the small-module floor, across all rounds.
const FULL_GROWTH_INPUT_DIVISOR: usize = 2;
const HARD_STACK: u64 = 65_536;
// Every inlined site gets its own slots, and each recursion level pays for
// all of them. Cap the whole frame of a caller in a call cycle, measured
// live so later rounds see slots from earlier ones (LLVM uses 1024 too).
const RECURSIVE_CALLER_STACK: u64 = 1024;

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
            InstData::Binary {
                op: BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::And | BinaryOp::Or | BinaryOp::Xor,
                ..
            } => {}
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
    acyclic_cfg(f)
}

fn acyclic_cfg(f: &Function) -> bool {
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

// Full-only allowance for scalar address/index helpers. This does not infer
// memory safety or speculate loads: ordinary cloning retains flags and order.
// Count unreachable blocks and duplicate edges as well as executable layout.
fn readonly_leaf(f: &Function) -> bool {
    if f.blocks().count() > 16
        || f.layout_insts().count() > 96
        || f.stack_slots().next().is_some()
        || f.sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, Purpose::ByVal(_)))
    {
        return false;
    }
    let mut edges = 0;
    let mut loads = 0;
    for (_, i) in f.layout_insts() {
        let data = f.inst_data(i);
        match data {
            InstData::Iconst { .. }
            | InstData::Icmp { .. }
            | InstData::Convert { .. }
            | InstData::SymbolAddr { .. }
            | InstData::Load { .. }
            | InstData::Select { .. }
            | InstData::Return { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. } => {}
            InstData::Binary {
                op:
                    BinaryOp::Iadd
                    | BinaryOp::Isub
                    | BinaryOp::Imul
                    | BinaryOp::Shl
                    | BinaryOp::Ushr
                    | BinaryOp::Sshr
                    | BinaryOp::Ptradd
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Xor,
                ..
            } => {}
            _ => return false,
        }
        loads += usize::from(matches!(data, InstData::Load { .. }));
        edges += data.branch_dests().len();
        if edges > 32 {
            return false;
        }
    }
    loads > 0 && acyclic_cfg(f)
}

// Full-only one-edge SCC wrappers. An inline hint or aggregate result identifies
// the narrow wrapper domain; neither overrides legality or ordinary budgets.
// Candidate bodies stay frozen as callers. Admit only edges whose sole slow
// target is outside that domain, so contracting an incoming wrapper edge leaves
// a recursive slow edge, never another admitted wrapper site. In particular a
// wrapper cycle has no admissible member. Templates and sites are pass-frozen;
// each of Full's three rounds has the same finite call/module growth limits.
fn small_scc_wrapper(f: &Function) -> bool {
    if !(f.attrs.inlinehint
        || f.sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, Purpose::Sret(_))))
        || f.blocks().count() > MAX_BLOCKS
        || f.layout_insts().count() > MAX_INSTS
        || f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>() > MAX_STACK
    {
        return false;
    }
    let mut calls = 0;
    let mut edges = 0;
    for (_, i) in f.layout_insts() {
        let data = f.inst_data(i);
        edges += data.branch_dests().len();
        match data {
            InstData::Call { func, .. } => {
                let decl = f.callee(*func);
                if decl.name == f.name {
                    return false;
                }
                calls += 1;
            }
            InstData::CallIndirect { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::Invoke { .. } => return false,
            _ => {}
        }
    }
    calls == 1 && edges <= 32 && acyclic_cfg(f)
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

// Include ABI snapshots in scheduling as well as admission. Full also gives
// guarded, writing hinted leaves this priority, so wrappers cannot crowd out
// cheap value helpers. This is a small structural priority class, not an
// alwaysinline budget exemption. Calls and
// cycles are excluded so a tiny wrapper cannot compete as a constructor leaf.
fn tiny_priority_leaf(f: &Function) -> bool {
    let (cost, stack) = inline_cost(f);
    cost <= 32
        && stack <= 64
        && f.blocks().count() <= 4
        && !f.layout_insts().any(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Intrinsic { .. }
            )
        })
        && acyclic_cfg(f)
}

// Promotion leaves declarations in the slot arena. Cost and cloning must
// agree on surviving storage, including implicit ABI and debugger homes.
fn inline_stack_slots(callee: &Function) -> Vec<bool> {
    let mut used = vec![false; callee.num_stack_slots()];
    for var in &callee.debug_vars {
        used[var.slot.index()] = true;
    }
    for (_, inst) in callee.layout_insts() {
        match *callee.inst_data(inst) {
            InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                used[slot.index()] = true;
            }
            _ => {}
        }
    }
    used
}

fn inline_cost(callee: &Function) -> (usize, u64) {
    let used = inline_stack_slots(callee);
    let mut cost = callee.layout_insts().count();
    let mut stack = callee
        .stack_slots()
        .filter(|(slot, _)| used[slot.index()])
        .map(|(_, s)| u64::from(s.size))
        .sum::<u64>();
    for p in &callee.sig.params {
        if let Purpose::ByVal(l) = &p.purpose {
            cost += byval_copy_cost(l.size());
            stack += l.size();
        }
    }
    (cost, stack)
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

fn profitability_reason(f: &Function, full: bool) -> Option<&'static str> {
    let scale = if f.attrs.inlinehint || f.attrs.hot {
        2
    } else {
        1
    };
    if f.attrs.alwaysinline || (full && readonly_leaf(f)) {
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
    eligibility_reason_with_frames(f, admit_noninterposable, FrameSemantics::Preserve)
}

#[derive(Clone, Copy)]
enum FrameSemantics {
    Preserve,
    CRequiredInline,
}

fn eligibility_reason_with_frames(
    f: &Function,
    admit_noninterposable: bool,
    frames: FrameSemantics,
) -> Option<&'static str> {
    // Every copy of a weak-ODR definition has an equivalent body, so with a
    // binding promise it is admitted like a strong one. Only the caller
    // changes; the definition keeps its linkage, signature and body.
    if f.attrs.weak
        || (!f.local
            && !(admit_noninterposable
                && (f.attrs.noninterposable || f.attrs.no_semantic_interposition)))
    {
        return Some("public_definition");
    }
    body_eligibility_reason(f, frames)
}

// These operations refer only to SSA operands and architectural state. Cloning
// their arguments preserves their existing pinning/effects. Frame observers,
// nonlocal control flow and owned regions keep their separate restrictions.
fn inlineable_target_intrinsic(id: Intrinsic) -> bool {
    id.x86_packed_search().is_some()
        || id.x86_sign_blend().is_some()
        || id.x86_packed_pair().is_some()
        || id.x86_packed_shift().is_some()
        || id.x86_byte_shift().is_some()
        || id.is_x86_byte_shuffle()
        || id.is_x86_cache_stream()
        || id.is_x86_raw_fma()
        || id.is_x86_f16c()
        || id.is_x86_bmi()
        || id.is_x86_gather()
        || id.is_x86_masked_mem()
        || id.is_x86_raw_fp_convert()
        || id.is_x86_fp_compare()
        || id.is_x86_sha256()
        || id.x86_avx512_bw_core().is_some()
        || id.x86_avx512_bw_mask().is_some()
        || id.x86_avx512_vl_shift().is_some()
        || id.x86_avx512_bw_shift().is_some()
        || id.x86_avx512_bw_pack().is_some()
        || id.x86_avx512_bw_convert().is_some()
        || id.x86_avx512_bw_mul().is_some()
        || id.is_x86_avx512_mask()
        || id.is_x86_avx512_indexed()
        || id.x86_avx512_special().is_some()
        || id.x86_avx512_bw_mask_alu().is_some()
        || id.x86_avx512_dq_mask_alu().is_some()
        || id.x86_avx512_dq_special().is_some()
        || id.x86_avx512_dq_lane().is_some()
        || id.x86_avx512_dq_bridge().is_some()
        || id.x86_avx512_dq_convert().is_some()
        || id.x86_avx512_vl_convert().is_some()
        || id.is_x86_avx512_convert()
        || id.is_x86_avx512_scalar_move()
        || id.x86_avx512_packed_opcode().is_some()
        || id.x86_avx512_scalar_convert().is_some()
        || id.x86_avx512_scalar().is_some()
        || id.x86_avx512_fma_opcode().is_some()
        || id.x86_avx512_scalar_fma_opcode().is_some()
        || id.x86_avx512_permute().is_some()
        || id.is_x86_avx512_pack()
        || id.is_x86_avx512_integer()
        || id.is_x86_avx512_mask_alu()
        || id.is_x86_avx512_integer_convert()
        || id.is_x86_avx512_lane()
        || id.x86_aes_opcode().is_some()
        || id.is_x86_pure_bits()
        || id.is_x86_fp_misc()
        || id.is_x86_legacy_raw_fp_convert()
        || id.is_x86_legacy_fp_misc()
        || id.is_x86_legacy_fp_compare()
        || id.is_x86_legacy_sign_mask()
        || id.is_x86_legacy_fp_v2()
        || id.is_x86_crc32()
        || id.is_x86_legacy_ptest()
        || id.is_x86_string()
        || id.is_x86_fp_horizontal_dot()
        || matches!(
            id,
            Intrinsic::X86Pclmulqdq128
                | Intrinsic::X86AvxVzeroall
                | Intrinsic::X86AvxVmaskmovdqu
                | Intrinsic::X86SseMaskmovdqu
                | Intrinsic::X86SseReadMxcsr
                | Intrinsic::X86SseSwapMxcsr
                | Intrinsic::X86CpuXcr0
                | Intrinsic::X86Cpuid
                | Intrinsic::X86Lfence
                | Intrinsic::X86Sfence
                | Intrinsic::X86Mfence
                | Intrinsic::X86Pause
        )
}

fn body_eligibility_reason(f: &Function, frames: FrameSemantics) -> Option<&'static str> {
    if f.attrs.noinline || f.attrs.optnone {
        return Some("noinline");
    }
    if f.attrs.naked || f.attrs.interrupt {
        return Some("special_entry");
    }
    if !f.attrs.alwaysinline
        && keel_ir::FunctionPolicy::resolve(&f.attrs)
            .is_ok_and(|p| p.disabled() || p.inline == Some(false))
    {
        return Some("function_optimization_policy");
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
                ) && !inlineable_target_intrinsic(*id)
                    && !(matches!(frames, FrameSemantics::CRequiredInline)
                        && matches!(id, Intrinsic::FrameAddress | Intrinsic::StackSave)) =>
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
            InstData::X86RflagsRegion { .. } => Some("callee_owned_flags_region"),
            InstData::InlineAsm { asm, .. } if !f.attrs.alwaysinline && !asm_is_duplicable(asm) => {
                Some("callee_inline_asm")
            }
            _ => None,
        };
        if reason.is_some() {
            return reason;
        }
    }
    None
}

/// Whether a copy of `asm` in another function assembles and allocates as it
/// does in its own. Every copy repeats the labels its template defines, so
/// only numeric local labels and labels made unique with `%=` are allowed.
/// Asm goto reaches the IR as a selector output whose labels use `%=`.
fn asm_is_duplicable(asm: &keel_ir::InlineAsm) -> bool {
    use keel_ir::{AsmArch, AsmClobber, AsmOperand, PhysReg};
    // A caller's frame can reserve a register the callee's did not: x19 is
    // the AArch64 base pointer of a function that allocates stack, and r15
    // the x86 base of an over-aligned frame, which leaves 13 x86 GPRs.
    let frame_base = |r: &PhysReg| {
        !r.float
            && match r.arch {
                AsmArch::Aarch64 => r.num == 19,
                AsmArch::X86 => r.num == 15,
            }
    };
    let operands = asm.outputs.iter().map(|(op, _)| op).chain(&asm.inputs);
    let registers = asm.outputs.len()
        + asm.inputs.len()
        + asm
            .clobbers
            .iter()
            .filter(|c| matches!(c, AsmClobber::Reg(_)))
            .count();
    registers <= 13
        && !asm
            .clobbers
            .iter()
            .any(|c| matches!(c, AsmClobber::Reg(r) if frame_base(r)))
        && !operands.into_iter().any(|op| match op {
            AsmOperand::Fixed(r) => frame_base(r),
            // A snapshot of the callee's stack pointer.
            AsmOperand::StackPointer => true,
            _ => false,
        })
        && asm_template_is_duplicable(&asm.template)
}

/// Rejects named label definitions and directives that define a symbol or
/// macro. Splitting at `;` as well as newlines can only reject more.
fn asm_template_is_duplicable(template: &str) -> bool {
    // The length of a leading symbol name, including `%` substitutions.
    let name_len = |text: &str| {
        let bytes = text.as_bytes();
        let mut end = 0;
        while end < bytes.len() {
            if bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'.' | b'$') {
                end += 1;
            } else if bytes[end] == b'%' {
                end += 1;
                while end < bytes.len() && matches!(bytes[end], b'=' | b'[' | b']') {
                    end += 1;
                }
            } else {
                break;
            }
        }
        end
    };
    template.split(['\n', ';']).all(|statement| {
        let mut rest = statement.trim_start();
        // Each leading `name:` defines a label.
        loop {
            let (name, after) = rest.split_at(name_len(rest));
            let after = after.trim_start();
            if !name.is_empty() && after.starts_with(':') {
                if !(name.bytes().all(|b| b.is_ascii_digit()) || name.contains("%=")) {
                    return false;
                }
                rest = after[1..].trim_start();
                continue;
            }
            // `name = value` assigns a symbol; `==` is a comparison.
            if !name.is_empty() && after.starts_with('=') && !after.starts_with("==") {
                return false;
            }
            let directive = name.to_ascii_lowercase();
            return !matches!(
                directive.as_str(),
                ".set" | ".equ" | ".equiv" | ".eqv" | ".macro" | ".comm" | ".lcomm" | ".symver"
            );
        }
    })
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
// `sites` holds each function's direct call instructions in layout order.
fn recursive_definitions(module: &Module, sites: &[Vec<Inst>]) -> Vec<bool> {
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
    run_observed_policy(module, sites, false, false, false)
}

fn run_observed_policy(
    module: &mut Module,
    sites: Option<&mut Vec<InlineSite>>,
    experimental: bool,
    admit_noninterposable: bool,
    cheap_reserve: bool,
) -> InlineReport {
    run_observed_policy_scoped(
        module,
        sites,
        InlinePolicy {
            experimental,
            admit_noninterposable,
            cheap_reserve,
            cost: InlineCost::Default,
        },
        None,
    )
}

fn run_observed_policy_scoped(
    module: &mut Module,
    sites: Option<&mut Vec<InlineSite>>,
    policy: InlinePolicy,
    callers: Option<&FxHashSet<String>>,
) -> InlineReport {
    let mut budget = if policy.cheap_reserve {
        InlineBudget::new(module)
    } else {
        InlineBudget::default()
    };
    run_observed_policy_budgeted(module, sites, policy, callers, &mut budget)
}

fn run_observed_policy_budgeted(
    module: &mut Module,
    mut sites: Option<&mut Vec<InlineSite>>,
    policy: InlinePolicy,
    callers: Option<&FxHashSet<String>>,
    shared: &mut InlineBudget,
) -> InlineReport {
    let InlinePolicy {
        experimental,
        admit_noninterposable,
        cheap_reserve,
        cost: cost_policy,
    } = policy;
    let module_growth_limit = if cheap_reserve {
        shared.limit
    } else {
        MAX_MODULE_GROWTH
    };
    let flatten = module.functions.iter().any(|f| f.attrs.flatten);
    let roots = if cost_policy == InlineCost::Size {
        crate::assembly_symbols::roots(module)
    } else {
        FxHashSet::default()
    };
    let mut calls = FxHashMap::<String, usize>::default();
    if cost_policy == InlineCost::Size {
        for function in &module.functions {
            for (_, instruction) in function.layout_insts() {
                let reference = match function.inst_data(instruction) {
                    InstData::Call { func, .. }
                    | InstData::Invoke { func, .. }
                    | InstData::ReturnCall { func, .. } => *func,
                    _ => continue,
                };
                *calls
                    .entry(function.callee(reference).name.clone())
                    .or_default() += 1;
            }
        }
    }
    // Freeze original sites and caller-size decisions before either phase.
    // The ledgers span both phases: priority changes order, never capacity.
    let original_sites: Vec<Vec<_>> = module
        .functions
        .iter()
        .map(|caller| {
            caller
                .layout_insts()
                .filter_map(|(_, i)| {
                    matches!(
                        caller.inst_data(i),
                        InstData::Call { .. }
                            | InstData::Invoke { .. }
                            | InstData::ReturnCall { .. }
                    )
                    .then_some(i)
                })
                .collect()
        })
        .collect();
    let recursive = recursive_definitions(module, &original_sites);
    // Use the entire structural candidate set for edge checks, including
    // candidates outside an SCC or without legal binding. Binding changes must
    // not turn a formerly opaque small wrapper into a second expansion level.
    let wrapper_bodies: FxHashSet<_> = module
        .functions
        .iter()
        .filter(|f| admit_noninterposable && small_scc_wrapper(f))
        .map(|f| f.name.clone())
        .collect();
    let recursive_bodies: FxHashSet<_> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(i, f)| recursive[*i] && wrapper_bodies.contains(&f.name))
        .map(|(_, f)| f.name.clone())
        .collect();
    let wrappers: FxHashSet<_> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(i, f)| {
            recursive[*i]
                && wrapper_bodies.contains(&f.name)
                && eligibility_reason(f, true).is_none()
                && !f.layout_insts().any(|(_, inst)| match f.inst_data(inst) {
                    InstData::Call { func, .. } => wrapper_bodies.contains(&f.callee(*func).name),
                    _ => false,
                })
        })
        .map(|(_, f)| f.name.clone())
        .collect();
    let explicit_definitions = sites.as_ref().map(|_| {
        module
            .functions
            .iter()
            .filter(|f| f.attrs.alwaysinline)
            .map(|f| f.name.clone())
            .collect::<FxHashSet<_>>()
    });
    // Freeze both templates and their rejection decisions at pass entry.
    // Keep the existing diagnostic precedence (noinline, recursion, legality,
    // profitability); the event stream uses these decisions, not a later probe.
    let mut templates = FxHashMap::default();
    let mut rejected = FxHashMap::default();
    for (index, f) in module.functions.iter().enumerate() {
        let legality = eligibility_reason(f, admit_noninterposable);
        let profitability = if legality.is_none() && !flatten {
            profitability_reason(f, admit_noninterposable)
        } else {
            None
        };
        let recursion = !f.attrs.noinline && recursive[index];
        let reason = if f.attrs.noinline {
            Some("noinline")
        } else if recursion && !wrappers.contains(&f.name) {
            Some("recursive_callee")
        } else {
            legality.or(profitability)
        };
        if let Some(reason) = reason {
            rejected.insert(f.name.clone(), reason);
        } else {
            templates.insert(f.name.clone(), index);
        }
    }
    // A template is copied only just before its first change as a caller;
    // until then the module's own definition is the pass-entry body.
    let mut frozen: Vec<Option<Box<Function>>> = Vec::new();
    frozen.resize_with(module.functions.len(), || None);
    // Templates retain their pass-entry bodies even when used as callers.
    // Only their immutable cost is cached; caller ledgers stay live below.
    let mut template_costs = vec![None; module.functions.len()];
    let mut report = InlineReport::default();
    let mut module_growth = if cheap_reserve { shared.used } else { 0 };
    let mut module_reserve_sites = 0;
    let tiny_templates: FxHashSet<_> = templates
        .values()
        .map(|&index| &module.functions[index])
        .filter(|f| admit_noninterposable && tiny_priority_leaf(f))
        .map(|f| f.name.as_str())
        .collect();
    let priority_sites: Vec<FxHashSet<_>> = module
        .functions
        .iter()
        .zip(&original_sites)
        .map(|(caller, calls)| {
            // Only a tiny template can be a priority site.
            if tiny_templates.is_empty() {
                return FxHashSet::default();
            }
            calls
                .iter()
                .copied()
                .filter(|&call| {
                    let InstData::Call { func, .. } = caller.inst_data(call) else {
                        return false;
                    };
                    let decl = caller.callee(*func);
                    admit_noninterposable
                        && templates.get(&decl.name).is_some_and(|&index| {
                            let f = &module.functions[index];
                            (decl.attrs.alwaysinline
                                || f.attrs.alwaysinline
                                || (cheap_reserve
                                    && (decl.attrs.inlinehint || f.attrs.inlinehint)
                                    && f.blocks().count() > 1
                                    && f.layout_insts().any(|(_, i)| {
                                        matches!(f.inst_data(i), InstData::Store { .. })
                                    })))
                                && tiny_templates.contains(f.name.as_str())
                        })
                })
                .collect()
        })
        .collect();
    let large_scc_callers: Vec<_> = module
        .functions
        .iter()
        .map(|caller| {
            !wrappers.is_empty() && caller.layout_insts().count() > MAX_SCC_CALLER_INSTRUCTIONS
        })
        .collect();
    let mut ledgers = vec![(0, 0, 0, 0u64); module.functions.len()];
    // Preserve the ordinary two phases exactly. Only sites denied solely by
    // module growth enter the separate retry queue; cloned calls never enter it.
    let mut schedule = Vec::new();
    for phase in 0..if admit_noninterposable { 2 } else { 1 } {
        let mut queues: std::collections::VecDeque<_> = original_sites
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                callers.is_none_or(|names| names.contains(&module.functions[*index].name))
            })
            .map(|(index, calls)| {
                (
                    index,
                    calls
                        .iter()
                        .copied()
                        .filter(|call| {
                            !admit_noninterposable
                                || priority_sites[index].contains(call) == (phase == 0)
                        })
                        .collect::<std::collections::VecDeque<_>>(),
                )
            })
            .filter(|(_, calls)| !calls.is_empty())
            .collect();
        while let Some((index, mut calls)) = queues.pop_front() {
            if cheap_reserve {
                schedule.push((
                    index,
                    calls.pop_front().expect("nonempty caller queue"),
                    false,
                ));
            } else {
                schedule.extend(calls.drain(..).map(|call| (index, call, false)));
            }
            if !calls.is_empty() {
                queues.push_back((index, calls));
            }
        }
    }
    let ordinary_end = schedule.len();
    let mut deferred = vec![Vec::new(); module.functions.len()];
    let mut cheap_growth = if cheap_reserve {
        shared.reserve_used
    } else {
        0
    };
    let mut cheap_sites = if cheap_reserve {
        shared.reserve_sites
    } else {
        0
    };
    let mut cursor = 0;
    loop {
        if cheap_reserve && cursor == ordinary_end {
            // One site per caller per sweep, so a high-fanout early caller
            // cannot spend the entire reserve ahead of late callers.
            let mut callers: std::collections::VecDeque<_> = deferred
                .iter_mut()
                .enumerate()
                .filter(|(_, calls)| !calls.is_empty())
                .map(|(index, calls)| (index, std::mem::take(calls).into_iter()))
                .collect();
            while let Some((index, mut calls)) = callers.pop_front() {
                schedule.push((index, calls.next().unwrap(), true));
                if calls.len() != 0 {
                    callers.push_back((index, calls));
                }
            }
        }
        if cursor == schedule.len() {
            break;
        }
        let (index, call, retry) = schedule[cursor];
        cursor += 1;
        let caller = &module.functions[index];
        let large_scc_caller = large_scc_callers[index];
        let (mut growth, mut reserve_sites, mut count, mut stack_growth) = ledgers[index];
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
                .is_some_and(|&t| template(&frozen, &module.functions, t).attrs.alwaysinline);
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
        } else if caller.attrs.optnone
            || keel_ir::FunctionPolicy::resolve(&caller.attrs).is_ok_and(|p| {
                p.disabled()
                    || p.inline == Some(false)
                    || (!p.permits_inlining()
                        && !caller.attrs.flatten
                        && cost_policy != InlineCost::Size)
            })
        {
            Some(("caller optimization policy", "caller_optimization_policy"))
        } else if decl.attrs.noinline || decl.attrs.optnone {
            Some(("noinline policy", "noinline"))
        } else if recursive_bodies.contains(&caller.name) {
            Some(("frozen recursive wrapper body", "recursive_wrapper_body"))
        } else if large_scc_caller && wrappers.contains(&decl.name) {
            Some((
                "recursive wrapper caller size",
                "recursive_wrapper_caller_size",
            ))
        } else if let Some(&t) = templates.get(&decl.name) {
            let callee = template(&frozen, &module.functions, t);
            if decl.sig != callee.sig {
                Some(("ABI signature mismatch", "abi_signature_mismatch"))
            } else if !caller
                .attrs
                .target_features
                .permits_inlining(&callee.attrs.target_features)
            {
                Some(("target feature mismatch", "target_feature_mismatch"))
            } else if callee.name == caller.name {
                Some(("recursive callee", "recursive_callee"))
            } else if !requested && callee.attrs.cold && !caller.attrs.cold && !caller.attrs.flatten
            {
                Some(("cold callee", "cold_callee"))
            } else if cost_policy == InlineCost::Size && !requested && !caller.attrs.flatten && {
                let constant = match caller.inst_data(call) {
                    InstData::Call { args, .. } => args.iter().any(|&argument| {
                        match caller.value_def(caller.resolve(argument)) {
                            keel_ir::ValueDef::Result(instruction, _) => matches!(
                                caller.inst_data(instruction),
                                InstData::Iconst { .. } | InstData::Fconst { .. }
                            ),
                            _ => false,
                        }
                    }),
                    _ => false,
                };
                callee.layout_insts().take(129).count() > 128
                    || !(callee.attrs.inlinehint
                        || constant
                        || (callee.local
                            && calls.get(&callee.name) == Some(&1)
                            && !roots.contains(&callee.name)))
            } {
                Some(("size profitability", "size_profitability"))
            } else if !caller.attrs.flatten && flatten {
                profitability_reason(callee, admit_noninterposable)
                    .map(|r| ("profitability threshold", r))
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
        let callee_index = templates[&decl.name];
        let callee = template(&frozen, &module.functions, callee_index);
        // Bound net layout growth before cleanup. The entry jump replaces
        // the removed call; each cloned return becomes one jump. The
        // continuation only receives moved instructions and parameters.
        // Counting all callee instructions also covers unreachable blocks
        // that the dominator traversal may omit. Snapshot IR is additional.
        let (cost, stack_cost) =
            *template_costs[callee_index].get_or_insert_with(|| inline_cost(callee));
        let recursive_stack_ok = !recursive[index]
            || stack_cost == 0
            || caller
                .stack_slots()
                .map(|(_, s)| u64::from(s.size))
                .sum::<u64>()
                + stack_cost
                <= RECURSIVE_CALLER_STACK;
        let no_larger_than_call = match caller.inst_data(call) {
            InstData::Call { args, .. } => {
                cost <= CALL_SITE_COST + args.len() + caller.inst_results(call).len()
            }
            _ => false,
        };
        let budget = if !recursive_stack_ok {
            Some("recursive_caller_stack_budget")
        } else if count == MAX_CALLS && !no_larger_than_call {
            Some("caller_call_count_budget")
        } else if growth + cost > MAX_GROWTH {
            Some("caller_instruction_growth_budget")
        } else if module_growth + cost > module_growth_limit {
            Some("module_instruction_growth_budget")
        } else if stack_growth + stack_cost > HARD_STACK {
            Some("caller_stack_growth_budget")
        } else {
            None
        };
        let cheap_candidate = cheap_reserve
            && !wrappers.contains(&callee.name)
            && cheap_helper(callee, cost, stack_cost)
            && recursive_stack_ok
            && count < MAX_CALLS
            && growth + cost <= MAX_GROWTH
            && stack_growth + stack_cost <= HARD_STACK;
        if !retry && budget == Some("module_instruction_growth_budget") && cheap_candidate {
            deferred[index].push(call);
            continue;
        }
        let use_cheap = retry
            && cheap_candidate
            && cheap_growth + cost <= CHEAP_MODULE_GROWTH
            && cheap_sites < CHEAP_MODULE_SITES;
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
        if let Some(detail) = budget.filter(|_| !use_reserve && !use_cheap) {
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
                if use_cheap {
                    "cheap_reserve_inlined"
                } else if use_reserve {
                    "experimental_reserve_inlined"
                } else {
                    "inlined"
                },
            ));
        }
        let before = (use_reserve || use_cheap).then(|| caller.layout_insts().count());
        if templates.get(&caller.name) == Some(&index) && frozen[index].is_none() {
            frozen[index] = Some(Box::new(caller.clone()));
        }
        // A recursive call was rejected above, so the callee is another function.
        let (caller, callee) = match frozen[callee_index].as_deref() {
            Some(callee) => (&mut module.functions[index], callee),
            None => two_functions(&mut module.functions, index, callee_index),
        };
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
                actual_net_growth <= cost
                    && cost
                        <= if use_cheap {
                            CHEAP_SITE_GROWTH
                        } else {
                            RESERVE_SITE_GROWTH
                        },
                "reserve emitted net growth exceeds conservative preflight"
            );
            if let Some(site) = sites.as_deref_mut().and_then(|s| s.last_mut()) {
                site.experimental_net_growth = Some(actual_net_growth);
            }
            if use_cheap {
                cheap_growth += cost;
                cheap_sites += 1;
                growth += cost;
            } else {
                reserve_sites += 1;
                module_reserve_sites += 1;
            }
        } else {
            growth += cost;
            module_growth += cost;
        }
        count += usize::from(!no_larger_than_call);
        stack_growth += stack_cost;
        report.changed = true;
        report.callers.push(index);
        ledgers[index] = (growth, reserve_sites, count, stack_growth);
    }
    if cheap_reserve {
        let budget = shared;
        budget.used = module_growth;
        budget.reserve_used = cheap_growth;
        budget.reserve_sites = cheap_sites;
    }
    report
}

/// The pass-entry body of template `index`.
fn template<'a>(
    frozen: &'a [Option<Box<Function>>],
    functions: &'a [Function],
    index: usize,
) -> &'a Function {
    frozen[index].as_deref().unwrap_or(&functions[index])
}

fn two_functions(
    functions: &mut [Function],
    caller: usize,
    callee: usize,
) -> (&mut Function, &Function) {
    assert_ne!(caller, callee);
    if caller < callee {
        let (low, high) = functions.split_at_mut(callee);
        (&mut low[caller], &high[0])
    } else {
        let (low, high) = functions.split_at_mut(caller);
        (&mut high[0], &low[callee])
    }
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
    let position = caller
        .block_insts(call_block)
        .iter()
        .position(|&i| i == call)
        .expect("a call is in its block");
    caller.move_insts_to_end(call_block, position + 1, continuation);
    let mut args = args;
    args.iter_mut().for_each(|v| *v = caller.resolve(*v));
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
    let used = inline_stack_slots(callee);
    let slots: FxHashMap<_, _> = callee
        .stack_slots()
        .filter(|(slot, _)| used[slot.index()])
        .map(|(slot, data)| (slot, caller.create_stack_slot(data.size, data.align)))
        .collect();
    let refs: FxHashMap<_, _> = callee
        .callees()
        .map(|(r, decl)| {
            let imported = caller.import_function(&decl.name, decl.sig.clone());
            // Keep contracts and facts on the caller's existing calls.
            caller.set_callee_attrs(imported, decl.attrs.clone());
            (r, imported)
        })
        .collect();
    let data: FxHashMap<_, _> = callee
        .data_symbols()
        .map(|(r, name)| (r, caller.import_data(name)))
        .collect();
    let sigs: FxHashMap<_, _> = callee
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
    let analysis = crate::analysis::get(callee);
    let dom = &analysis.dom;
    let mut blocks = FxHashMap::default();
    let mut values = FxHashMap::default();
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
                InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                    *slot = slots[slot]
                }
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
                args: args.to_vec(),
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
    fn dead_declarations_do_not_discount_required_byval_snapshots() {
        let mut module = keel_ir::parse(
            "func @helper(ptr byval [2 x i64]) local {
 ss0 = stack_slot 32, align 8
block0(v0: ptr):
 return
}
func @caller(ptr) {
block0(v0: ptr):
 call @helper(v0)
 return
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(
            inline_cost(&module.functions[0]),
            (1 + byval_copy_cost(16), 16)
        );
        let (caller, callee) = two_functions(&mut module.functions, 1, 0);
        let call = caller.block_insts(caller.entry_block().unwrap())[0];
        inline_call(caller, call, callee);
        assert_eq!(caller.num_stack_slots(), 1);
        assert_eq!(caller.stack_slots().next().unwrap().1.size, 16);
        assert_eq!(
            caller
                .layout_insts()
                .filter(|&(_, i)| matches!(caller.inst_data(i), InstData::Load { .. }))
                .count(),
            2,
            "the potentially trapping byval source reads remain"
        );
        keel_ir::verify_module(&module).unwrap();
    }

    #[test]
    fn implicit_aggregate_va_arg_home_is_charged_and_remapped() {
        let mut module = keel_ir::parse(
            "func @helper(ptr) -> ptr local {
 ss0 = stack_slot 16, align 8
 ss1 = stack_slot 16, align 8
 ss2 = stack_slot 16, align 8
block0(v0: ptr):
 v1 = va_arg_aggregate [2 x i64], v0, ss2
 return v1
}
func @caller(ptr) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = call @helper(v0)
 return v1
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(inline_cost(&module.functions[0]), (2, 16));
        // Unlike a scalar va_arg, this aggregate operation has an explicit
        // activation-local home, which must survive without a StackAddr.
        crate::run_passes(&mut module, &["inline"]).unwrap();
        let caller = &module.functions[1];
        assert_eq!(caller.num_stack_slots(), 2);
        let home = caller
            .layout_insts()
            .find_map(|(_, i)| match caller.inst_data(i) {
                InstData::AggregateVaArg { home, .. } => Some(*home),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            home.index(),
            1,
            "the old home ss2 gets a fresh caller identity"
        );
        assert_eq!(caller.stack_slots().nth(home.index()).unwrap().1.size, 16);
        keel_ir::verify_module(&module).unwrap();
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
        for (template, constraints, inlinable) in [
            ("", "", true),
            ("cmp %1, %2\n\tcmova %3, %0", "=r,0,r,r,r", true),
            ("1:\n\tjnz 1b", "~{cc}", true),
            (".Lspin_%=: pause; jmp .Lspin_%=", "", true),
            ("lbl_%=: 1: nop", "~{x0}", true),
            ("a == 1", "", true),
            ("spin: jmp spin", "", false),
            ("nop\n  done :", "", false),
            ("1: x%0: nop", "", false),
            ("sym = 1", "", false),
            (".set sym, 1", "", false),
            ("  .MACRO m\n.endm", "", false),
            ("nop", "~{x19}", false),
            ("nop", "~{r15}", false),
            ("nop", "{r15}", false),
            ("nop", "rsp", false),
            ("nop", &vec!["r"; 14].join(","), false),
        ] {
            let mut module = keel_ir::parse(source).unwrap();
            let f = &mut module.functions[0];
            let inst = f.layout_insts().next().unwrap().1;
            let outputs = constraints.matches('=').count();
            let asm =
                keel_ir::InlineAsm::new(template, constraints, &vec![Type::I64; outputs]).unwrap();
            let args = vec![keel_ir::Value::new(0); asm.inputs.len()];
            *f.inst_data_mut(inst) = InstData::InlineAsm {
                asm: Box::new(asm),
                args,
            };
            let expected = (!inlinable).then_some("callee_inline_asm");
            assert_eq!(
                eligibility_reason(f, false),
                expected,
                "{template:?} {constraints}"
            );
        }
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
                    args: vec![].into(),
                }
            } else {
                let func = f.import_function("external", sig);
                InstData::Call {
                    func,
                    args: vec![].into(),
                }
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

/// Expand mandatory C inline calls before validating deferred asm immediates.
/// This phase does not use optional optimizer profitability budgets. Hard
/// legality and module growth limits still produce explicit diagnostics.
/// C frame/stack observers bind to the emitted caller frame after expansion.
/// Ordinary IR inlining keeps its frame-preservation restrictions.
pub fn required_inline_asm(module: &mut Module) -> Result<(), String> {
    required_inline_asm_with_c_bodies(module, &[])
}

/// Finalize C asm after mandatory inlining, discarding owned GNU extern-inline
/// implementations while retaining their external ABI declarations.
/// The frontend supplies local strong bodies; ordinary IR callers use
/// [`required_inline_asm`].
///
/// # Errors
/// Returns an error when mandatory inlining is illegal, an owned body is not
/// local and strong, or immediate constraints cannot be finalized.
pub fn required_inline_asm_with_c_bodies(
    module: &mut Module,
    inline_only: &[&str],
) -> Result<(), String> {
    required_inline_asm_with_c_definitions(module, inline_only, &[])
}

/// Finalize mandatory C inlining with explicit ownership of strong public bodies.
/// Direct mandatory calls may use these definitions without changing their
/// public symbols or semantic-interposition attributes. Inline-only GNU bodies
/// retain the separate discard contract of [`required_inline_asm_with_c_bodies`].
///
/// # Errors
/// Returns an error for a missing, local, weak or non-mandatory owned definition,
/// an illegal mandatory call, or an invalid inline-only body.
pub fn required_inline_asm_with_c_definitions(
    module: &mut Module,
    inline_only: &[&str],
    definitions: &[&str],
) -> Result<(), String> {
    for name in definitions {
        let Some(function) = module.functions.iter().find(|f| f.name == *name) else {
            return Err(format!("C-owned mandatory definition @{name} is missing"));
        };
        if function.local
            || function.attrs.weak
            || function.weak_odr
            || !function.attrs.alwaysinline
            || inline_only.contains(name)
        {
            return Err(format!(
                "C-owned mandatory definition @{name} is not a strong public alwaysinline body"
            ));
        }
    }
    fn discard_unused(module: &mut Module) {
        let mut live: FxHashSet<String> = module
            .functions
            .iter()
            .filter(|f| {
                !f.local
                    || !f.attrs.alwaysinline
                    || f.attrs.used
                    || f.attrs.retain
                    || f.attrs.constructor
                    || f.attrs.destructor
            })
            .map(|f| f.name.clone())
            .collect();
        for d in &module.data {
            if let Some(init) = &d.init {
                for item in &init.items {
                    if let keel_ir::DataItem::Ptr { symbol, .. }
                    | keel_ir::DataItem::RttiName { symbol } = item
                    {
                        live.insert(symbol.clone());
                    }
                }
            }
        }
        loop {
            let before = live.len();
            for f in &module.functions {
                if !live.contains(&f.name) {
                    continue;
                }
                for (_, i) in f.layout_insts() {
                    match f.inst_data(i) {
                        InstData::Call { func, .. }
                        | InstData::ReturnCall { func, .. }
                        | InstData::Invoke { func, .. } => {
                            live.insert(f.callee(*func).name.clone());
                        }
                        InstData::SymbolAddr {
                            symbol: Symbol::Func(r),
                        } => {
                            live.insert(f.callee(*r).name.clone());
                        }
                        _ => {}
                    }
                }
            }
            if live.len() == before {
                break;
            }
        }
        for f in &module.functions {
            if !live.contains(&f.name) {
                let mut attrs = f.attrs.clone();
                // Placement belongs to an emitted definition, not the ABI
                // declaration retained after its unused body is discarded.
                attrs.section = None;
                if let Some(decl) = module.decls.iter_mut().find(|d| d.name == f.name) {
                    decl.attrs.section = None;
                } else {
                    module.decls.push(keel_ir::FuncDecl {
                        name: f.name.clone(),
                        sig: f.sig.clone(),
                        attrs,
                    });
                }
            }
        }
        module.functions.retain(|f| live.contains(&f.name));
        // Removed calls leave callee imports behind. Refresh their contracts
        // from the declarations that now own the discarded bodies' names.
        module.sync_function_attrs();
    }
    discard_unused(module);
    // Only mandatory edges drive expansion. Ordinary calls may close a cycle
    // through an alwaysinline wrapper without requiring infinite expansion.
    let mandatory_definitions: FxHashSet<&str> = module
        .functions
        .iter()
        .filter(|f| f.attrs.alwaysinline)
        .map(|f| f.name.as_str())
        .collect();
    let sites: Vec<Vec<Inst>> = module
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
                    let decl = f.callee(reference);
                    (decl.attrs.alwaysinline || mandatory_definitions.contains(decl.name.as_str()))
                        .then_some(i)
                })
                .collect()
        })
        .collect();
    let recursive = recursive_definitions(module, &sites);
    let recursive: FxHashSet<String> = module
        .functions
        .iter()
        .zip(recursive)
        .filter(|(_, r)| *r)
        .map(|(f, _)| f.name.clone())
        .collect();
    if sites.iter().any(|sites| !sites.is_empty()) {
        expand_required_with_fallback(module, definitions, &recursive, 1_000_000)?;
    }
    if !inline_only.is_empty() {
        for name in inline_only {
            if let Some(function) = module.functions.iter().find(|f| f.name == *name) {
                if !function.local || function.attrs.weak || function.weak_odr {
                    return Err(format!(
                        "GNU inline implementation @{name} is not a local strong body"
                    ));
                }
                if !module.decls.iter().any(|decl| decl.name == *name) {
                    module.decls.push(keel_ir::FuncDecl {
                        name: (*name).to_owned(),
                        sig: function.sig.clone(),
                        attrs: function.attrs.clone(),
                    });
                }
            }
        }
        module
            .functions
            .retain(|function| !inline_only.contains(&function.name.as_str()));
        for decl in &mut module.decls {
            if inline_only.contains(&decl.name.as_str()) {
                decl.attrs.alwaysinline = false;
                decl.attrs.section = None;
            }
        }
        module.sync_function_attrs();
    }
    discard_unused(module);
    // Constant cleanup is required for constraint legality, independently of -O.
    for f in &mut module.functions {
        if !f
            .layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }))
        {
            continue;
        }
        // Width-selecting C macros can contain invalid asm in constant-dead
        // branches even when every operand is already a legal register or
        // memory operand. Use the CFG so incoming goto edges stay reachable.
        for _ in 0..8 {
            let mut changed = crate::phi_simplify::run(f);
            changed |= crate::const_fold::run(f);
            changed |= crate::simplify_cfg::run(f);
            if !changed {
                break;
            }
        }
        if !f.layout_insts().any(|(_, i)| match f.inst_data(i) {
            InstData::InlineAsm { asm, args } => asm.inputs.iter().zip(args).any(|(op, arg)| {
                if !matches!(op, keel_ir::AsmOperand::Imm | keel_ir::AsmOperand::NumericImm | keel_ir::AsmOperand::RegImm | keel_ir::AsmOperand::RegSignedImm32 | keel_ir::AsmOperand::RegShiftImm5 | keel_ir::AsmOperand::PortImm8) { return false; }
                let numeric = matches!(f.value_def(f.resolve(*arg)), keel_ir::ValueDef::Result(def, _) if matches!(f.inst_data(def), InstData::Iconst { .. }));
                !numeric && !(*op == keel_ir::AsmOperand::Imm && keel_ir::symbolic_immediate(f, *arg).is_some())
            }),
            _ => false,
        }) { continue; }
        for _ in 0..8 {
            let mut changed = crate::phi_simplify::run(f);
            changed |= crate::gvn::run(f);
            changed |= crate::memory_forward::run(f);
            if !f.debug_vars.is_empty() {
                changed |= crate::memory_forward::finalize_asm_immediates(f);
            }
            changed |= crate::const_fold::run(f);
            changed |= crate::instcombine::run(f);
            changed |= crate::simplify_cfg::run(f);
            if !changed {
                break;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum RequiredCleanup {
    PerCall,
    PerCaller,
}

fn expand_required_with_fallback(
    module: &mut Module,
    definitions: &[&str],
    recursive: &FxHashSet<String>,
    limit: usize,
) -> Result<(), String> {
    // Inlining mutates only function bodies and their local imports. Keep the
    // original templates and budget accounting for an eager retry: deferred
    // cleanup can leave an invalid call in code that eager cleanup removes.
    let original = module.functions.clone();
    if expand_required_calls(
        module,
        definitions,
        recursive,
        limit,
        RequiredCleanup::PerCaller,
    )
    .is_err()
    {
        module.functions = original;
        return expand_required_calls(
            module,
            definitions,
            recursive,
            limit,
            RequiredCleanup::PerCall,
        );
    }
    Ok(())
}

fn expand_required_calls(
    module: &mut Module,
    definitions: &[&str],
    recursive: &FxHashSet<String>,
    limit: usize,
    cleanup: RequiredCleanup,
) -> Result<(), String> {
    let mut expanded = 0usize;
    // Expansion edits only the caller, so definitions keep their indices and
    // the functions before the caller have no mandatory sites left.
    let mut index_of: FxHashMap<String, usize> = FxHashMap::default();
    for (index, function) in module.functions.iter().enumerate() {
        index_of.entry(function.name.clone()).or_insert(index);
    }
    for first in 0..module.functions.len() {
        let mut dirty = false;
        loop {
            let mut next = None;
            let caller = &module.functions[first];
            for (_, call) in caller.layout_insts() {
                let reference = match caller.inst_data(call) {
                    InstData::Call { func, .. }
                    | InstData::ReturnCall { func, .. }
                    | InstData::Invoke { func, .. } => *func,
                    _ => continue,
                };
                let decl = caller.callee(reference);
                let definition = index_of.get(decl.name.as_str()).copied();
                if !decl.attrs.alwaysinline
                    && !definition.is_some_and(|i| module.functions[i].attrs.alwaysinline)
                {
                    continue;
                }
                let fail = |why: &str| {
                    format!(
                        "required inlining of @{} in @{} failed: {why}",
                        decl.name, caller.name
                    )
                };
                if !matches!(caller.inst_data(call), InstData::Call { .. }) {
                    return Err(fail("unsupported call instruction"));
                }
                let target = definition.ok_or_else(|| fail("no definition"))?;
                let callee = &module.functions[target];
                if recursive.contains(&callee.name) {
                    return Err(fail("recursive callee"));
                }
                if caller.sig.returns_twice || caller.personality.is_some() {
                    return Err(fail("unsupported caller"));
                }
                if decl.attrs.noinline {
                    return Err(fail("noinline conflicts with always_inline"));
                }
                if decl.sig != callee.sig {
                    return Err(fail("ABI signature mismatch"));
                }
                // Unannotated mandatory wrappers are compiled in the caller's
                // permission context. Every copied operation still passes the
                // backend's local legality checks. Explicit callee permissions
                // cannot be erased this way.
                if !callee.attrs.target_features.is_empty()
                    && !caller
                        .attrs
                        .target_features
                        .permits_inlining(&callee.attrs.target_features)
                {
                    return Err(fail("target feature mismatch"));
                }
                let reason = if definitions.contains(&callee.name.as_str()) {
                    body_eligibility_reason(callee, FrameSemantics::CRequiredInline)
                } else {
                    eligibility_reason_with_frames(callee, true, FrameSemantics::CRequiredInline)
                };
                if let Some(reason) = reason {
                    return Err(fail(reason));
                }
                expanded = expanded
                    .checked_add(callee.layout_insts().count())
                    .ok_or_else(|| fail("growth overflow"))?;
                if expanded > limit {
                    return Err(fail(
                        "required inline growth exceeds one million instructions",
                    ));
                }
                next = Some((call, target));
                break;
            }
            let Some((call, target)) = next else {
                if dirty {
                    crate::phi_simplify::run(&mut module.functions[first]);
                    crate::simplify_cfg::run(&mut module.functions[first]);
                    dirty = false;
                    // Cleanup can expose or remove sites. Finish this caller
                    // before its body can become a later caller's template.
                    continue;
                }
                break;
            };
            let (caller, callee) = two_functions(&mut module.functions, first, target);
            inline_call(caller, call, callee);
            match cleanup {
                RequiredCleanup::PerCall => {
                    crate::phi_simplify::run(caller);
                    crate::simplify_cfg::run(caller);
                }
                RequiredCleanup::PerCaller => dirty = true,
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    fn gated_call(target: &str) -> Module {
        keel_ir::parse(&format!(
            "decl @missing() alwaysinline\n\
             func @caller() {{\nblock0:\n v0 = call @gate()\n brif v0, block1, block2\n\
             block1:\n call @{target}()\n return\nblock2:\n return\n}}\n\
             func @gate() -> i8 alwaysinline local {{\nblock0:\n v0 = iconst.i8 0\n return v0\n}}\n\
             func @heavy() alwaysinline local {{\nblock0:\n v0 = iconst.i64 0\n v1 = iconst.i64 1\n return\n}}"
        ))
        .unwrap()
    }

    #[test]
    fn deferred_required_cleanup_retries_calls_that_become_dead() {
        let mut module = gated_call("missing");
        keel_ir::verify_module(&module).unwrap();
        let mut eager = module.clone();
        let recursive = FxHashSet::default();
        assert!(
            expand_required_calls(
                &mut module.clone(),
                &[],
                &recursive,
                1_000_000,
                RequiredCleanup::PerCaller
            )
            .is_err(),
            "the deferred scan still sees the dead missing definition"
        );
        expand_required_calls(
            &mut eager,
            &[],
            &recursive,
            1_000_000,
            RequiredCleanup::PerCall,
        )
        .unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 1_000_000).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_retries_calls_after_a_trapping_wrapper() {
        let mut module = keel_ir::parse(
            "decl @missing() alwaysinline\n\
             func @caller() {\nblock0:\n call @stop()\n call @missing()\n return\n}\n\
             func @stop() alwaysinline local {\nblock0:\n trap\n}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut eager = module.clone();
        let recursive = FxHashSet::default();
        expand_required_calls(
            &mut eager,
            &[],
            &recursive,
            1_000_000,
            RequiredCleanup::PerCall,
        )
        .unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 1_000_000).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_restores_growth_accounting_on_retry() {
        let mut module = gated_call("heavy");
        let recursive = FxHashSet::default();
        let mut eager = module.clone();
        // The gate costs two instructions; the dead heavy call must not
        // consume the remaining budget when the original algorithm retries.
        assert!(
            expand_required_calls(
                &mut module.clone(),
                &[],
                &recursive,
                2,
                RequiredCleanup::PerCaller
            )
            .is_err()
        );
        expand_required_calls(&mut eager, &[], &recursive, 2, RequiredCleanup::PerCall).unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 2).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());

        let mut eager = gated_call("heavy");
        let mut retried = eager.clone();
        let error = expand_required_calls(&mut eager, &[], &recursive, 1, RequiredCleanup::PerCall)
            .unwrap_err();
        assert_eq!(
            expand_required_with_fallback(&mut retried, &[], &recursive, 1).unwrap_err(),
            error
        );
        assert_eq!(retried.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_finishes_templates_before_later_callers() {
        let mut module = keel_ir::parse(
            "func @outer() -> i64 alwaysinline local {\nblock0:\n v0 = call @inner()\n return v0\n}\n\
             func @caller() -> i64 {\nblock0:\n v0 = call @outer()\n return v0\n}\n\
             func @inner() -> i64 alwaysinline local {\nblock0:\n v0 = iconst.i64 7\n return v0\n}"
        ).unwrap();
        expand_required_calls(
            &mut module,
            &[],
            &FxHashSet::default(),
            1_000_000,
            RequiredCleanup::PerCaller,
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        for f in &module.functions {
            assert_eq!(
                f.blocks().count(),
                1,
                "@{} retains unmerged inline blocks",
                f.name
            );
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. })),
                "@{} retains a required call",
                f.name
            );
        }
    }

    #[test]
    fn importing_an_existing_callee_preserves_its_ordered_policy_once() {
        let mut module = keel_ir::parse("decl @work(i64) -> i64 cold optimize(\"Os\")\nfunc @helper(i64) -> i64 local {\nblock0(v0:i64):\n v1 = call @work(v0)\n return v1\n}\nfunc @caller(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @work(v0)\n v2 = call @helper(v1)\n return v2\n}\n").unwrap();
        let (caller, callee) = two_functions(&mut module.functions, 1, 0);
        let call = caller
            .layout_insts()
            .find_map(|(_, inst)| match caller.inst_data(inst) {
                InstData::Call { func, .. } if caller.callee(*func).name == "helper" => Some(inst),
                _ => None,
            })
            .unwrap();
        inline_call(caller, call, callee);
        let work = caller.callees().find(|(_, d)| d.name == "work").unwrap().1;
        assert_eq!(work.attrs.optimize.iter().count(), 1);
        keel_ir::verify_module(&module).unwrap();
    }
}
