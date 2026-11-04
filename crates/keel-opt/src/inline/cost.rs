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
