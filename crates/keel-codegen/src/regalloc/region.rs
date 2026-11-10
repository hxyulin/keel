//! Temporarily displace idle scalar owners inside bounded call-free regions.
use super::{
    Allocation, AllocationInput, AllocationTier, Constraint, ControlFlowGraph, Interval, Loc,
    OperandKind, ValuePart, allocation_order, assign_spill_slots, baseline_allocation,
    build_intervals, weighted,
};
use keel_ir::Inst;
use keel_ir::{EntityRef, InstData, Type};

// Bound opportunity discovery per chunk. This limits window size, not the
// number of chunks or total work in a function.
const MAX_REGION_INSTRUCTIONS: usize = 64;

#[cfg(test)]
#[path = "region_tests.rs"]
mod tests;

/// Full homes with one register segment per bounded call-free window.
pub(super) fn allocate(input: &AllocationInput<'_>) -> Allocation {
    profiling::scope!("bounded-spill-repair");
    backend_scope!(Allocation);
    // Retain Full's intervals for segment repair rather than reconstructing
    // CFG liveness after the scan. Allocation order, weights and homes match Full.
    let func = input.func;
    let cfg = ControlFlowGraph::compute(func);
    let (order, weights) = allocation_order(func, &cfg, AllocationTier::Full);
    let intervals = build_intervals(
        func,
        &cfg,
        &order,
        input.is_call,
        input.returns_twice,
        input.selection,
    );
    let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
    let mut spills = weighted::linear_scan(
        func,
        input.selection,
        &order,
        &intervals,
        input.pools,
        &mut locs,
        &weights,
    );
    let spill_slots = assign_spill_slots(&mut spills, &mut locs);
    let alloc = baseline_allocation(order, locs, spill_slots, input.pools);
    // Region repair only reloads low halves of these scalar types. Other
    // spills, including i128 halves and vector values, cannot create a segment.
    if !spills.iter().any(|iv| {
        iv.half == 0 && matches!(func.value_type(iv.value), Type::I32 | Type::I64 | Type::Ptr)
    }) {
        return alloc;
    }
    drop((spills, weights, cfg));
    repair_free(input, alloc, &intervals)
}

#[cfg(any(test, feature = "experimental-regalloc"))]
pub(super) fn repair(
    input: &AllocationInput<'_>,
    alloc: Allocation,
    intervals: &[Interval],
    reserved: &[keel_ir::Block],
) -> Allocation {
    repair_inner(input, alloc, intervals, reserved, false)
}

pub(super) fn repair_free(
    input: &AllocationInput<'_>,
    alloc: Allocation,
    intervals: &[Interval],
) -> Allocation {
    repair_inner(input, alloc, intervals, &[], true)
}

fn repair_inner(
    input: &AllocationInput<'_>,
    mut alloc: Allocation,
    intervals: &[Interval],
    reserved: &[keel_ir::Block],
    allow_free: bool,
) -> Allocation {
    let f = input.func;
    if f.sig.returns_twice
        || f.personality.is_some()
        || !f.debug_vars.is_empty()
        || f.layout_insts().any(|(_, d)| input.returns_twice(d))
    {
        return alloc;
    }
    let mut slot = 0u32;
    let mut region = Vec::new();
    let mut temp = None;
    let mut live = None;
    for block in alloc.order.clone() {
        // Existing local edits own this entire block, including boundary copies.
        // Local caching flushes dirty homes at edges, so untouched blocks start
        // with stable homes and cannot overlap its resident cache segments.
        if reserved.contains(&block)
            || f.block_insts(block).iter().any(|&inst| {
                alloc.edits.instruction_index(inst).is_some()
                    || (0..f.inst_data(inst).branch_dests().len())
                        .any(|edge| !alloc.edits.edge(inst, edge).is_empty())
            })
        {
            slot += 1 + f.block_insts(block).len() as u32;
            continue;
        }
        slot += 1;
        for &inst in f.block_insts(block) {
            let read = 2 * slot;
            slot += 1;
            if input.selection.covered(inst) {
                continue;
            }
            let d = f.inst_data(inst);
            // Only an instruction with a recorded constraint can have one
            // other than `Any`.
            let mut constrained = false;
            if input.selection.has_constraint(inst) {
                input.for_each_operand(inst, |op| constrained |= op.constraint != Constraint::Any);
            }
            let barrier = constrained
                || (input.is_call)(d)
                || d.is_terminator()
                || matches!(
                    d,
                    InstData::InlineAsm { .. }
                        | InstData::Intrinsic { .. }
                        | InstData::Invoke { .. }
                        | InstData::LandingPad { .. }
                        | InstData::X86RflagsRegion { .. }
                        | InstData::VaArg { .. }
                );
            if barrier || region.len() == MAX_REGION_INSTRUCTIONS {
                apply(
                    input, &mut alloc, intervals, &mut live, &region, &mut temp, allow_free,
                );
                region.clear();
            }
            if !barrier {
                region.push((inst, read));
            }
        }
        apply(
            input, &mut alloc, intervals, &mut live, &region, &mut temp, allow_free,
        );
        region.clear();
    }
    alloc
}

/// The intervals that can meet a window of program points, for windows
/// visited in increasing order: those starting by the window's end and
/// ending no earlier than its start. An interval that ends before one
/// window cannot meet a later one.
struct Live<'a> {
    by_start: Vec<&'a Interval>,
    next: usize,
    active: Vec<&'a Interval>,
}

impl<'a> Live<'a> {
    fn new(intervals: &'a [Interval]) -> Self {
        let mut by_start: Vec<_> = intervals.iter().collect();
        by_start.sort_by_key(|iv| iv.start());
        Self {
            by_start,
            next: 0,
            active: Vec::new(),
        }
    }

    /// Every interval with a point in `[from, to]`, and possibly others.
    /// Neither bound may be less than in the previous call.
    fn window(&mut self, from: u32, to: u32) -> &[&'a Interval] {
        while let Some(&iv) = self.by_start.get(self.next)
            && iv.start() <= to
        {
            self.active.push(iv);
            self.next += 1;
        }
        self.active.retain(|iv| iv.end() >= from);
        &self.active
    }
}

fn apply<'a>(
    input: &AllocationInput<'_>,
    alloc: &mut Allocation,
    intervals: &'a [Interval],
    live: &mut Option<Live<'a>>,
    region: &[(Inst, u32)],
    temp: &mut Option<u32>,
    allow_free: bool,
) {
    if region.len() < if allow_free { 2 } else { 4 } {
        return;
    }
    let f = input.func;
    let mut counts = keel_ir::fxhash::FxHashMap::default();
    // A window holds at most `MAX_REGION_INSTRUCTIONS`: short lists suffice.
    let mut defined = Vec::new();
    let mut seen = Vec::new();
    for &(inst, _) in region {
        seen.clear();
        input.for_each_operand(inst, |op| {
            if op.kind == OperandKind::Def {
                defined.push(op.part.value);
            }
            if op.kind == OperandKind::Use
                && !op.part.high
                && !seen.contains(&op.part.value)
                && {
                    seen.push(op.part.value);
                    true
                }
                && matches!(op.ty, Type::I32 | Type::I64 | Type::Ptr)
                && matches!(alloc.loc(op.part.value), Some(Loc::Stack(_)))
            {
                *counts.entry(op.part.value).or_insert(0u32) += 1;
            }
        });
    }
    let Some((&candidate, _)) = counts
        .iter()
        .filter(|(v, n)| **n >= if allow_free { 2 } else { 4 } && !defined.contains(v))
        .max_by_key(|(v, n)| (**n, std::cmp::Reverse(v.index())))
    else {
        return;
    };
    // Save only across the candidate's actual use window. Uses outside that
    // window no longer disqualify an otherwise idle register owner.
    let reads = |inst| {
        let mut found = false;
        input.for_each_operand(inst, |op| {
            found |= op.kind == OperandKind::Use && op.part == ValuePart::low(candidate)
        });
        found
    };
    let first = region
        .iter()
        .position(|&(inst, _)| reads(inst))
        .expect("candidate has at least four uses");
    let last = region
        .iter()
        .rposition(|&(inst, _)| reads(inst))
        .expect("candidate has at least four uses");
    let region = &region[first..=last];
    let mut used = [false; 256];
    for &(inst, _) in region {
        input.for_each_operand(inst, |op| {
            if let Some(Loc::Reg(r)) = alloc.operand_loc(None, op.part, op.kind) {
                used[r as usize] = true;
            }
        });
        input.for_each_clobber(inst, |loc| {
            if let Loc::Reg(r) = loc {
                used[r as usize] = true;
            }
        });
    }
    let from = region[0].1;
    let to = region.last().expect("candidate window is nonempty").1 + 1;
    // Both checks below only consider intervals with a point in [from, to].
    let intervals = live
        .get_or_insert_with(|| Live::new(intervals))
        .window(from, to);
    let owner = intervals
        .iter()
        .filter(|iv| {
            iv.half == 0 && matches!(f.value_type(iv.value), Type::I32 | Type::I64 | Type::Ptr)
        })
        .filter_map(|iv| match alloc.loc(iv.value) {
            Some(Loc::Reg(r))
                if !used[r as usize] && iv.ranges.iter().any(|&(a, b)| a <= from && b >= to) =>
            {
                Some(r)
            }
            _ => None,
        })
        .min();
    // A dead caller-saved register needs no owner save/restore. Exclude
    // every interval intersecting either endpoint, including lifetime holes.
    let free = allow_free
        .then(|| {
            input.pools.caller_saved.iter().copied().find(|&r| {
                !used[r as usize]
                    && !intervals.iter().any(|iv| {
                        alloc.locs[iv.half][iv.value.index()] == Some(Loc::Reg(r))
                            && iv.ranges.iter().any(|&(a, b)| a <= to && b >= from)
                    })
            })
        })
        .flatten();
    // Owner save/restore costs two transfers in addition to the load.
    // Two reads suffice only when the register is genuinely free.
    let owner = owner.filter(|_| counts[&candidate] >= 4);
    let Some(r) = free.or(owner) else {
        return;
    };
    let home = alloc.loc(candidate).expect("candidate has a stack home");
    let spill = free.is_none().then(|| {
        *temp.get_or_insert_with(|| {
            let s = alloc.spill_slots;
            alloc.spill_slots += 1;
            s
        })
    });
    let before = &mut alloc.edits.instruction_mut(region[0].0).before;
    if let Some(spill) = spill {
        before.push((Loc::Stack(spill), Loc::Reg(r)));
    }
    before.push((Loc::Reg(r), home));
    for &(inst, _) in region {
        let mut found = false;
        input.for_each_operand(inst, |op| {
            found |= op.kind == OperandKind::Use && op.part == ValuePart::low(candidate);
        });
        if found {
            alloc
                .edits
                .instruction_mut(inst)
                .uses
                .push((ValuePart::low(candidate), Loc::Reg(r)));
        }
    }
    if let Some(spill) = spill {
        alloc
            .edits
            .instruction_mut(region.last().expect("candidate window is nonempty").0)
            .after
            .push((Loc::Reg(r), Loc::Stack(spill)));
    }
}
