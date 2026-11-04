//! Size limits, callee profitability and the cost of inlining one callee.
use super::clone::{byval_copy_chunks, inline_stack_slots};
use keel_ir::{BinaryOp, EntityRef, Function, InstData, Purpose};

pub(super) const CHEAP_SITE_GROWTH: usize = 48;
pub(crate) const CHEAP_MODULE_GROWTH: usize = 4096;
pub(super) const CHEAP_MODULE_SITES: usize = 256;

// A cheap predicate or guarded single slow-call wrapper, independent of names
// and inline attributes. Charge all layout instructions and ABI snapshots;
// there is no speculative cleanup credit. Cloning preserves memory/trap order.
pub(super) fn cheap_helper(f: &Function, cost: usize, stack: u64) -> bool {
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

// Contracting a recursive wrapper still adds branches, stack snapshots and
// live values to its caller. Keep that exception out of very large functions;
// ordinary profitable inlining and read-only leaf admission remain available.
pub(super) const MAX_SCC_CALLER_INSTRUCTIONS: usize = 8192;

pub(super) const MAX_INSTS: usize = 64;
pub(super) const MAX_BLOCKS: usize = 8;
pub(super) const MAX_STACK: u64 = 512;
pub(super) const MAX_GROWTH: usize = 2048;
pub(super) const MAX_CALLS: usize = 128;
// A call costs about this many instructions before its arguments and
// results: LLVM's call penalty of 25 at 5 per instruction. A callee that
// costs no more than its call does not count against MAX_CALLS.
pub(super) const CALL_SITE_COST: usize = 5;
pub(super) const MAX_MODULE_GROWTH: usize = 16_384;
// Preserve the old three Full rounds' aggregate allowance for small modules.
pub(super) const FULL_MIN_MODULE_GROWTH: usize = 196_608;
// Charge at most 50% of entry IR above the small-module floor, across all rounds.
pub(super) const FULL_GROWTH_INPUT_DIVISOR: usize = 2;
pub(super) const HARD_STACK: u64 = 65_536;
// Every inlined site gets its own slots, and each recursion level pays for
// all of them. Cap the whole frame of a caller in a call cycle, measured
// live so later rounds see slots from earlier ones (LLVM uses 1024 too).
pub(super) const RECURSIVE_CALLER_STACK: u64 = 1024;

// Switches spend blocks on cheap returns. Admit only a small, acyclic integer
// leaf, charging every branch destination as well as every instruction. This
// is a structural exception to MAX_BLOCKS, not to binding or growth budgets.
pub(super) fn cheap_leaf_switch(f: &Function) -> bool {
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

pub(super) fn acyclic_cfg(f: &Function) -> bool {
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
pub(super) fn readonly_leaf(f: &Function) -> bool {
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
pub(super) fn small_scc_wrapper(f: &Function) -> bool {
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

pub(super) fn byval_copy_cost(size: u64) -> usize {
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
pub(super) fn tiny_priority_leaf(f: &Function) -> bool {
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

pub(super) fn inline_cost(callee: &Function) -> (usize, u64) {
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

pub(super) fn profitability_reason(f: &Function, full: bool) -> Option<&'static str> {
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

/// One call site and its callee, summarized for the budget check.
#[derive(Clone, Copy, Debug)]
pub(super) struct SiteCost {
    /// Instructions inlining adds, from [`inline_cost`].
    pub(super) insts: usize,
    /// Stack bytes inlining adds, from [`inline_cost`].
    pub(super) stack: u64,
    /// Arguments plus results of the call.
    pub(super) operands: usize,
    /// The caller's declared stack bytes, when the caller is in a call cycle.
    pub(super) recursive_caller_stack: Option<u64>,
    /// The callee may spend the cheap-helper reserve.
    pub(super) cheap_helper: bool,
}

impl SiteCost {
    /// A callee that costs no more than its call does not count against
    /// [`MAX_CALLS`].
    pub(super) fn counts_as_call(&self) -> bool {
        self.insts > CALL_SITE_COST + self.operands
    }
}

/// What one caller has accepted so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CallerLedger {
    pub(super) growth: usize,
    /// Inlined sites that [count as calls](SiteCost::counts_as_call).
    pub(super) calls: usize,
    pub(super) stack: u64,
}

/// What the module has accepted so far, against its ordinary limit and the
/// separate cheap-helper reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ModuleLedger {
    pub(super) growth: usize,
    pub(super) limit: usize,
    pub(super) reserve_growth: usize,
    pub(super) reserve_sites: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Inline the site, charging the cheap-helper reserve when `reserve`.
    Inline { reserve: bool },
    /// Only module growth denies the site: retry it from the reserve after
    /// the ordinary schedule.
    Defer,
    /// A budget denies the site; the reason names the budget.
    Reject(&'static str),
}

/// Applies the growth budgets to one site. `retry` marks a deferred site's
/// second visit, the only visit that may spend the reserve.
pub(super) fn budget_verdict(
    site: &SiteCost,
    caller: &CallerLedger,
    module: &ModuleLedger,
    retry: bool,
) -> Verdict {
    let recursive_stack_ok = site.stack == 0
        || site
            .recursive_caller_stack
            .is_none_or(|stack| stack + site.stack <= RECURSIVE_CALLER_STACK);
    let budget = if !recursive_stack_ok {
        Some("recursive_caller_stack_budget")
    } else if caller.calls == MAX_CALLS && site.counts_as_call() {
        Some("caller_call_count_budget")
    } else if caller.growth + site.insts > MAX_GROWTH {
        Some("caller_instruction_growth_budget")
    } else if module.growth + site.insts > module.limit {
        Some("module_instruction_growth_budget")
    } else if caller.stack + site.stack > HARD_STACK {
        Some("caller_stack_growth_budget")
    } else {
        None
    };
    let cheap_candidate = site.cheap_helper
        && recursive_stack_ok
        && caller.calls < MAX_CALLS
        && caller.growth + site.insts <= MAX_GROWTH
        && caller.stack + site.stack <= HARD_STACK;
    if !retry && budget == Some("module_instruction_growth_budget") && cheap_candidate {
        return Verdict::Defer;
    }
    let reserve = retry
        && cheap_candidate
        && module.reserve_growth + site.insts <= CHEAP_MODULE_GROWTH
        && module.reserve_sites < CHEAP_MODULE_SITES;
    match budget {
        Some(reason) if !reserve => Verdict::Reject(reason),
        _ => Verdict::Inline { reserve },
    }
}

/// Charges an inlined site to its caller and to the module budget the
/// verdict chose.
pub(super) fn charge(
    site: &SiteCost,
    caller: &mut CallerLedger,
    module: &mut ModuleLedger,
    reserve: bool,
) {
    caller.growth += site.insts;
    caller.calls += usize::from(site.counts_as_call());
    caller.stack += site.stack;
    if reserve {
        module.reserve_growth += site.insts;
        module.reserve_sites += 1;
    } else {
        module.growth += site.insts;
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    const SITE: SiteCost = SiteCost {
        insts: 20,
        stack: 16,
        operands: 2,
        recursive_caller_stack: None,
        cheap_helper: false,
    };
    const MODULE: ModuleLedger = ModuleLedger {
        growth: 0,
        limit: MAX_MODULE_GROWTH,
        reserve_growth: 0,
        reserve_sites: 0,
    };

    fn verdict(site: SiteCost, caller: CallerLedger, module: ModuleLedger) -> Verdict {
        budget_verdict(&site, &caller, &module, false)
    }

    #[test]
    fn a_site_within_every_budget_inlines_from_the_ordinary_budget() {
        let caller = CallerLedger::default();
        assert_eq!(
            verdict(SITE, caller, MODULE),
            Verdict::Inline { reserve: false }
        );
        let (mut caller, mut module) = (caller, MODULE);
        charge(&SITE, &mut caller, &mut module, false);
        assert_eq!(
            caller,
            CallerLedger {
                growth: 20,
                calls: 1,
                stack: 16
            }
        );
        assert_eq!((module.growth, module.reserve_growth), (20, 0));
    }

    #[test]
    fn each_budget_names_itself_at_its_boundary() {
        let fits = CallerLedger::default();
        let cases = [
            (
                SiteCost {
                    recursive_caller_stack: Some(RECURSIVE_CALLER_STACK - 16),
                    ..SITE
                },
                fits,
                MODULE,
                None,
            ),
            (
                SiteCost {
                    recursive_caller_stack: Some(RECURSIVE_CALLER_STACK - 15),
                    ..SITE
                },
                fits,
                MODULE,
                Some("recursive_caller_stack_budget"),
            ),
            (
                SITE,
                CallerLedger {
                    calls: MAX_CALLS,
                    ..fits
                },
                MODULE,
                Some("caller_call_count_budget"),
            ),
            (
                SITE,
                CallerLedger {
                    growth: MAX_GROWTH - 20,
                    ..fits
                },
                MODULE,
                None,
            ),
            (
                SITE,
                CallerLedger {
                    growth: MAX_GROWTH - 19,
                    ..fits
                },
                MODULE,
                Some("caller_instruction_growth_budget"),
            ),
            (
                SITE,
                fits,
                ModuleLedger {
                    growth: MAX_MODULE_GROWTH - 19,
                    ..MODULE
                },
                Some("module_instruction_growth_budget"),
            ),
            (
                SITE,
                CallerLedger {
                    stack: HARD_STACK - 15,
                    ..fits
                },
                MODULE,
                Some("caller_stack_growth_budget"),
            ),
        ];
        for (site, caller, module, expected) in cases {
            let expected = expected.map_or(Verdict::Inline { reserve: false }, Verdict::Reject);
            assert_eq!(
                verdict(site, caller, module),
                expected,
                "{site:?} {caller:?} {module:?}"
            );
        }
    }

    #[test]
    fn the_recursive_stack_cap_ignores_stackless_callees_and_acyclic_callers() {
        let full = Some(RECURSIVE_CALLER_STACK);
        for site in [
            SiteCost {
                stack: 0,
                recursive_caller_stack: full,
                ..SITE
            },
            SiteCost {
                recursive_caller_stack: None,
                ..SITE
            },
        ] {
            assert_eq!(
                verdict(site, CallerLedger::default(), MODULE),
                Verdict::Inline { reserve: false }
            );
        }
    }

    #[test]
    fn a_callee_no_larger_than_its_call_ignores_the_call_count() {
        let caller = CallerLedger {
            calls: MAX_CALLS,
            ..CallerLedger::default()
        };
        let small = SiteCost {
            insts: CALL_SITE_COST + SITE.operands,
            ..SITE
        };
        assert!(!small.counts_as_call());
        assert_eq!(
            verdict(small, caller, MODULE),
            Verdict::Inline { reserve: false }
        );
        let mut charged = CallerLedger::default();
        charge(&small, &mut charged, &mut { MODULE }, false);
        assert_eq!(charged.calls, 0);
        let large = SiteCost {
            insts: small.insts + 1,
            ..SITE
        };
        assert_eq!(
            verdict(large, caller, MODULE),
            Verdict::Reject("caller_call_count_budget")
        );
    }

    #[test]
    fn a_cheap_helper_denied_only_by_module_growth_retries_from_the_reserve() {
        let cheap = SiteCost {
            cheap_helper: true,
            ..SITE
        };
        let full = ModuleLedger {
            growth: MAX_MODULE_GROWTH,
            ..MODULE
        };
        let caller = CallerLedger::default();
        assert_eq!(verdict(cheap, caller, full), Verdict::Defer);
        assert_eq!(
            budget_verdict(&cheap, &caller, &full, true),
            Verdict::Inline { reserve: true }
        );
        let (mut charged, mut module) = (caller, full);
        charge(&cheap, &mut charged, &mut module, true);
        assert_eq!(charged.growth, 20);
        assert_eq!(
            (module.growth, module.reserve_growth, module.reserve_sites),
            (MAX_MODULE_GROWTH, 20, 1)
        );
        // An ordinary callee, or a reserve without room, gets the module reason.
        let denied = Verdict::Reject("module_instruction_growth_budget");
        assert_eq!(verdict(SITE, caller, full), denied);
        assert_eq!(budget_verdict(&SITE, &caller, &full, true), denied);
        for spent in [
            ModuleLedger {
                reserve_growth: CHEAP_MODULE_GROWTH - 19,
                ..full
            },
            ModuleLedger {
                reserve_sites: CHEAP_MODULE_SITES,
                ..full
            },
        ] {
            assert_eq!(budget_verdict(&cheap, &caller, &spent, true), denied);
        }
    }

    #[test]
    fn the_reserve_does_not_lift_caller_budgets() {
        let cheap = SiteCost {
            cheap_helper: true,
            ..SITE
        };
        let full = ModuleLedger {
            growth: MAX_MODULE_GROWTH,
            ..MODULE
        };
        let caller = CallerLedger {
            growth: MAX_GROWTH,
            ..CallerLedger::default()
        };
        let denied = Verdict::Reject("caller_instruction_growth_budget");
        assert_eq!(verdict(cheap, caller, full), denied);
        assert_eq!(budget_verdict(&cheap, &caller, &full, true), denied);
    }
}
