//! Linear-scan register allocation, after Poletto & Sarkar, "Linear Scan
//! Register Allocation" (TOPLAS 1999).
//!
//! Graph colouring builds a full interference graph and is slow to run;
//! linear scan gets most of the quality in one pass over sorted intervals.
//! It works in four steps:
//!
//! 1. **Linearize.** Put the reachable blocks in reverse postorder (each
//!    block after its dominator, so definitions come before uses) and number
//!    every program point. That order is also the order the code is emitted
//!    in.
//! 2. **Liveness.** A backward dataflow over blocks finds which values are
//!    live on entry to and exit from each block.
//! 3. **Intervals.** Each value gets one interval `[start, end]` of program
//!    points: from its definition to its last use, stretched over every
//!    block where it is live in or out. In a loop, a value used at the top of
//!    the body is still needed on the next trip round, so it is live out of
//!    the latch at the bottom, and the interval covers the whole loop. One
//!    interval with no holes is conservative (a value may look live where it
//!    is not), but it is always safe.
//! 4. **Scan.** Walk the intervals by start point, keeping the *active*
//!    ones (currently holding a register). An interval that has ended frees
//!    its register. When none is free, spill whichever interval ends
//!    furthest away: it would hold its register the longest, so evicting it
//!    helps the most intervals that come after.
//!
//! A spilled value lives in an 8-byte stack slot for its whole life; there
//! is no interval splitting. Spill slots are shared by values whose
//! intervals do not overlap, with the same scan.
//!
//! # Program points
//!
//! Every instruction gets two points: it reads its operands at the even
//! point `2k` and writes its results at the odd point `2k + 1`. An operand
//! whose last use is this instruction has ended by the time the result
//! starts, so the result may take the same register (`add x9, x9, x10`).
//! The code generator must therefore read every operand before it writes
//! the destination.
//!
//! A block's parameters are defined at its own header point, before its
//! first instruction. Block-call arguments are operands of the terminator,
//! so they are read at the end of the predecessor. A value live out of a
//! block is extended to the terminator's write point, past the copies onto
//! the edge.
//!
//! # Calls
//!
//! A call clobbers the caller-saved registers. An interval that is live
//! across one (defined before it and used after it) may only take a
//! callee-saved register, or be spilled. The call's arguments end at the
//! call and its result starts after it, so neither counts as crossing it.

use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{Block, EntityRef, Function, InstData, Value, ValueDef};
use std::collections::BTreeSet;

/// Where a value lives for its whole life.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Loc {
    /// A general-purpose register, by number (`x9` is `Reg(9)`).
    Reg(u8),
    /// A spill slot, by index; the code generator decides the offset.
    Stack(u32),
}

/// The registers the allocator may hand out.
#[derive(Copy, Clone, Debug)]
pub struct Pools {
    /// Clobbered by calls. Preferred for values not live across a call:
    /// they cost nothing to use.
    pub caller_saved: &'static [u8],
    /// Preserved by calls, but a function that writes one must save and
    /// restore it.
    pub callee_saved: &'static [u8],
}

pub struct Allocation {
    /// The reachable blocks in the order the intervals were numbered, which
    /// is also the order they must be emitted in.
    pub order: Vec<Block>,
    /// By value index. `None` for values with no interval: those defined in
    /// unreachable code, and aliases.
    locs: Vec<Option<Loc>>,
    /// Number of 8-byte spill slots the frame needs.
    pub spill_slots: u32,
    /// The callee-saved registers handed out, sorted, which the prologue
    /// must save.
    pub callee_saved_used: Vec<u8>,
}

impl Allocation {
    /// The location of `value`. Operands must be resolved through
    /// [`Function::resolve`] first.
    pub fn loc(&self, value: Value) -> Option<Loc> {
        self.locs[value.index()]
    }
}

/// Allocates every value in `func`. `is_call` says which instructions
/// clobber the caller-saved registers.
pub fn allocate(func: &Function, pools: &Pools, is_call: impl Fn(&InstData) -> bool) -> Allocation {
    let cfg = ControlFlowGraph::compute(func);
    let order = DominatorTree::compute(func, &cfg).rpo().to_vec();
    let intervals = build_intervals(func, &cfg, &order, &is_call);

    let mut locs = vec![None; func.num_values()];
    let mut spilled = linear_scan(&intervals, pools, &mut locs);
    let spill_slots = assign_spill_slots(&mut spilled, &mut locs);

    let mut callee_saved_used: Vec<u8> = locs
        .iter()
        .filter_map(|&loc| match loc {
            Some(Loc::Reg(r)) if pools.callee_saved.contains(&r) => Some(r),
            _ => None,
        })
        .collect();
    callee_saved_used.sort_unstable();
    callee_saved_used.dedup();

    Allocation {
        order,
        locs,
        spill_slots,
        callee_saved_used,
    }
}

#[derive(Clone, Debug)]
struct Interval {
    value: Value,
    start: u32,
    end: u32,
    crosses_call: bool,
}

/// Live-in and live-out sets, indexed by block.
struct Liveness {
    live_in: Vec<BTreeSet<Value>>,
    live_out: Vec<BTreeSet<Value>>,
}

/// The classic backward dataflow:
///
/// ```text
/// live_out(B) = union of live_in(S) over successors S
/// live_in(B)  = uses(B) + (live_out(B) - defs(B))
/// ```
///
/// `uses(B)` are values read in `B` before any definition in `B`; `defs(B)`
/// are `B`'s parameters and results. Block arguments are ordinary operands
/// of the predecessor's terminator, and parameters are definitions, so
/// block arguments need no special case (phi nodes would). Visiting blocks
/// in postorder makes most information flow in one pass; loops need a
/// second to carry live-ins back round the back edge.
fn liveness(func: &Function, cfg: &ControlFlowGraph, order: &[Block]) -> Liveness {
    let n = func.num_blocks();
    let mut uses = vec![BTreeSet::new(); n];
    let mut defs = vec![BTreeSet::new(); n];
    for &block in order {
        let mut block_uses = BTreeSet::new();
        let mut block_defs: BTreeSet<Value> = func.block_params(block).iter().copied().collect();
        for &inst in func.block_insts(block) {
            func.inst_data(inst).for_each_operand(|v| {
                let v = func.resolve(v);
                if !block_defs.contains(&v) {
                    block_uses.insert(v);
                }
            });
            block_defs.extend(func.inst_results(inst));
        }
        uses[block.index()] = block_uses;
        defs[block.index()] = block_defs;
    }

    let mut live = Liveness {
        live_in: vec![BTreeSet::new(); n],
        live_out: vec![BTreeSet::new(); n],
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &block in order.iter().rev() {
            let b = block.index();
            let out: BTreeSet<Value> = cfg
                .succs(block)
                .iter()
                .flat_map(|s| live.live_in[s.index()].iter().copied())
                .collect();
            let mut live_in = uses[b].clone();
            live_in.extend(out.difference(&defs[b]));
            if live_in != live.live_in[b] {
                live.live_in[b] = live_in;
                changed = true;
            }
            live.live_out[b] = out;
        }
    }
    live
}

/// Numbers the program points (see the module docs) and builds one interval
/// per defined value.
fn build_intervals(
    func: &Function,
    cfg: &ControlFlowGraph,
    order: &[Block],
    is_call: &dyn Fn(&InstData) -> bool,
) -> Vec<Interval> {
    let live = liveness(func, cfg, order);
    let is_alias = |v: Value| matches!(func.value_def(v), ValueDef::Alias(_));

    // The smallest range covering every point passed to `extend`.
    let mut ranges: Vec<Option<(u32, u32)>> = vec![None; func.num_values()];
    let mut extend = |v: Value, point: u32| {
        let range = &mut ranges[v.index()];
        *range = Some(match *range {
            None => (point, point),
            Some((start, end)) => (start.min(point), end.max(point)),
        });
    };
    // Read points of calls, in increasing order.
    let mut calls = Vec::new();

    // Slot `k` owns the read point 2k and the write point 2k + 1.
    let mut slot = 0u32;
    for &block in order {
        let header = 2 * slot + 1;
        slot += 1;
        for &param in func.block_params(block) {
            if !is_alias(param) {
                extend(param, header);
            }
        }
        for &v in &live.live_in[block.index()] {
            extend(v, header);
        }

        for &inst in func.block_insts(block) {
            let (read, write) = (2 * slot, 2 * slot + 1);
            slot += 1;
            let data = func.inst_data(inst);
            data.for_each_operand(|v| extend(func.resolve(v), read));
            for &result in func.inst_results(inst) {
                // An alias's instruction is dead; its result goes nowhere.
                if !is_alias(result) {
                    extend(result, write);
                }
            }
            if is_call(data) {
                calls.push(read);
            }
        }

        // The terminator's write point: values live out survive the
        // block-argument copies on the way out.
        let end = 2 * slot - 1;
        for &v in &live.live_out[block.index()] {
            extend(v, end);
        }
    }

    ranges
        .iter()
        .enumerate()
        .filter_map(|(i, range)| {
            let (start, end) = (*range)?;
            // Crossing a call means being live on both sides of its read
            // point: the first call after `start` comes before `end`.
            let next_call = calls.partition_point(|&c| c <= start);
            let crosses_call = calls.get(next_call).is_some_and(|&c| c < end);
            Some(Interval {
                value: Value::new(i),
                start,
                end,
                crosses_call,
            })
        })
        .collect()
}

/// Gives each interval a register from `pools` or spills it. Fills in the
/// register locations and returns the spilled intervals.
fn linear_scan(intervals: &[Interval], pools: &Pools, locs: &mut [Option<Loc>]) -> Vec<Interval> {
    let mut sorted: Vec<&Interval> = intervals.iter().collect();
    sorted.sort_by_key(|iv| (iv.start, iv.value));

    // Intervals holding a register, with that register.
    let mut active: Vec<(&Interval, u8)> = Vec::new();
    let mut spilled = Vec::new();
    for cur in sorted {
        // Free the registers of intervals that ended before this one starts.
        active.retain(|(iv, _)| iv.end >= cur.start);

        // Live across a call: only a callee-saved register survives it.
        // Otherwise prefer caller-saved ones, which need no save/restore.
        let allowed: Vec<u8> = if cur.crosses_call {
            pools.callee_saved.to_vec()
        } else {
            [pools.caller_saved, pools.callee_saved].concat()
        };
        let free = allowed
            .iter()
            .copied()
            .find(|&r| active.iter().all(|&(_, used)| used != r));
        if let Some(reg) = free {
            locs[cur.value.index()] = Some(Loc::Reg(reg));
            active.push((cur, reg));
            continue;
        }

        // Out of registers. Of the active intervals whose register `cur`
        // could use, find the one that ends last. If it outlives `cur`,
        // take its register and spill it; otherwise spill `cur`.
        let victim = active
            .iter()
            .enumerate()
            .filter(|(_, (_, reg))| allowed.contains(reg))
            .max_by_key(|(_, (iv, _))| iv.end)
            .map(|(i, _)| i);
        match victim {
            Some(i) if active[i].0.end > cur.end => {
                let (evicted, reg) = active.swap_remove(i);
                locs[evicted.value.index()] = None;
                spilled.push(evicted.clone());
                locs[cur.value.index()] = Some(Loc::Reg(reg));
                active.push((cur, reg));
            }
            _ => spilled.push(cur.clone()),
        }
    }
    spilled
}

/// Gives each spilled interval a stack slot, reusing the slots of intervals
/// that have ended: linear scan again, with as many "registers" as needed.
/// Returns the number of slots.
fn assign_spill_slots(spilled: &mut [Interval], locs: &mut [Option<Loc>]) -> u32 {
    spilled.sort_by_key(|iv| (iv.start, iv.value));
    // (end, slot) of slots in use.
    let mut active: Vec<(u32, u32)> = Vec::new();
    let mut count = 0;
    for iv in spilled.iter() {
        active.retain(|&(end, _)| end >= iv.start);
        let slot = (0..)
            .find(|&s| active.iter().all(|&(_, used)| used != s))
            .expect("some slot is free");
        active.push((iv.end, slot));
        count = count.max(slot + 1);
        locs[iv.value.index()] = Some(Loc::Stack(slot));
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_ir::{Module, parse};

    fn function(src: &str) -> Function {
        let module: Module = parse(src).expect("test IR parses");
        module.functions.into_iter().next().expect("one function")
    }

    fn intervals(func: &Function) -> Vec<Interval> {
        let cfg = ControlFlowGraph::compute(func);
        let order = DominatorTree::compute(func, &cfg).rpo().to_vec();
        build_intervals(func, &cfg, &order, &|d| matches!(d, InstData::Call { .. }))
    }

    fn interval(intervals: &[Interval], index: usize) -> &Interval {
        intervals
            .iter()
            .find(|iv| iv.value.index() == index)
            .expect("value has an interval")
    }

    const LOOP: &str = "
        func @f(i64, i64) -> i64 {
        block0(v0: i64, v1: i64):
            v2 = iconst.i64 0
            jump block1(v2)
        block1(v3: i64):
            v4 = iadd v3, v1
            v5 = icmp.ult v4, v0
            brif v5, block1(v4), block2
        block2:
            return v4
        }";

    #[test]
    fn loop_liveness() {
        let func = function(LOOP);
        let cfg = ControlFlowGraph::compute(&func);
        let order = DominatorTree::compute(&func, &cfg).rpo().to_vec();
        let live = liveness(&func, &cfg, &order);
        let set = |vs: &[usize]| vs.iter().map(|&i| Value::new(i)).collect::<BTreeSet<_>>();
        // v0 and v1 come round the back edge; v3 is a parameter, so it is
        // defined in block1, not live into it.
        assert_eq!(live.live_in[1], set(&[0, 1]));
        assert_eq!(live.live_out[1], set(&[0, 1, 4]));
        assert_eq!(live.live_in[2], set(&[4]));
        assert_eq!(live.live_out[2], set(&[]));
    }

    #[test]
    fn interval_covers_the_loop() {
        let func = function(LOOP);
        let ivs = intervals(&func);
        // v1's last use is the first instruction of the loop, but the next
        // iteration reads it again: it must overlap v5, defined after that
        // use, or v5 could take its register.
        let (v1, v5) = (interval(&ivs, 1), interval(&ivs, 5));
        assert!(v1.start < v5.start && v1.end >= v5.end, "{v1:?} vs {v5:?}");
        // A dying operand ends before the result of its last use starts, so
        // they can share a register: v3 is last read by `v4 = iadd v3, v1`.
        assert!(interval(&ivs, 3).end < interval(&ivs, 4).start);
    }

    #[test]
    fn calls_are_crossed_only_by_values_live_on_both_sides() {
        let func = function(
            "
            func @f(i64) -> i64 {
            block0(v0: i64):
                v1 = iconst.i64 1
                v2 = call @f(v1)
                v3 = iadd v2, v0
                return v3
            }",
        );
        let ivs = intervals(&func);
        assert!(interval(&ivs, 0).crosses_call);
        assert!(!interval(&ivs, 1).crosses_call, "argument ends at the call");
        assert!(!interval(&ivs, 2).crosses_call, "result starts after it");
    }

    /// Checks the allocation of every function in every end-to-end program
    /// with a tiny register set, where most values spill: overlapping
    /// intervals never share a location, and values live across calls are
    /// never in caller-saved registers.
    #[test]
    fn allocations_respect_interference_and_calls() {
        let pools = Pools {
            caller_saved: &[9, 10],
            callee_saved: &[19, 20],
        };
        let is_call =
            |d: &InstData| matches!(d, InstData::Call { .. } | InstData::Intrinsic { .. });
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "keel") {
                continue;
            }
            let module = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for func in &module.functions {
                let alloc = allocate(func, &pools, is_call);
                let cfg = ControlFlowGraph::compute(func);
                let ivs = build_intervals(func, &cfg, &alloc.order, &is_call);
                for (i, a) in ivs.iter().enumerate() {
                    let loc = alloc.loc(a.value).expect("every interval is allocated");
                    if a.crosses_call {
                        assert!(
                            !matches!(loc, Loc::Reg(r) if pools.caller_saved.contains(&r)),
                            "{}: {} crosses a call in {loc:?}",
                            func.name,
                            a.value
                        );
                    }
                    for b in &ivs[i + 1..] {
                        let overlap = a.start <= b.end && b.start <= a.end;
                        assert!(
                            !overlap || alloc.loc(b.value) != Some(loc),
                            "{}: {} and {} overlap but share {loc:?}",
                            func.name,
                            a.value,
                            b.value
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 0);
    }
}
