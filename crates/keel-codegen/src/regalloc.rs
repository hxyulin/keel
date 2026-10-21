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
//! 3. **Intervals.** Each value gets an interval: the ranges of program
//!    points where it is live. Within a block that is one range, from the
//!    block's start (if the value is live in) or its definition, to the
//!    block's end (if live out) or its last use; the ranges of consecutive
//!    blocks join up. In a loop, a value used at the top of the body is
//!    still needed on the next trip round, so it is live out of the latch
//!    at the bottom, and the interval covers the whole loop.
//!
//!    Between ranges are *lifetime holes*, where the value is not live
//!    because the blocks laid out there do not need it: a value defined in
//!    one `case` of a big `switch` and used in a tail block far below is not
//!    live in the cases in between. Another value may use its register
//!    there. (The first version used a single range from the first point to
//!    the last, which made such values block a register through every case
//!    in between.)
//! 4. **Scan.** Walk the intervals by start point, keeping the *active*
//!    ones (live at the current point, holding a register) and the
//!    *inactive* ones (holding a register, but in a hole). An inactive
//!    interval's register is free for a new interval that fits in the hole,
//!    that is, that does not intersect it. When no register is free, spill
//!    whichever interval ends furthest away: it would hold its register the
//!    longest, so evicting it helps the most intervals that come after.
//!    This is the scan of Wimmer & Franz, "Linear Scan Register Allocation
//!    on SSA Form" (CGO 2010), without its interval splitting.
//!
//! The default Fast and Full tiers use one location per value. [`AllocationEdits`]
//! also supports per-instruction locations and transfers for splitting allocators.
//! A spilled scalar lives in an 8-byte stack slot, a vector in 16 bytes, for
//! its whole life. There is no interval splitting, so a value spilled anywhere is spilled
//! everywhere. Spill slots are shared by values whose intervals do not
//! overlap, holes ignored.
//!
//! Integers and pointers go in general-purpose registers, floats and vectors
//! in SIMD/FP registers: two register classes that never interfere. One scan
//! handles both; each interval only competes for registers of its class.
//!
//! An `i128` is two 64-bit halves, each with its own interval over the same
//! range, so each half gets its own register or spill slot.
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
//! across one (live just before it and just after it) may only take a
//! callee-saved register, or be spilled. Vectors always spill across calls:
//! neither supported ABI preserves a full 128-bit SIMD register.
//! A value with a hole where the call is does not cross it. The call's arguments end at the
//! call and its result starts after it, so neither counts as crossing it.
//!
//! A call that returns twice, like `setjmp`, returns the second time from
//! a `longjmp` made later, possibly from code where a value live across
//! the call is not live, whose register or slot another value may have
//! taken. `longjmp` restores the callee-saved registers, but not what
//! happened to a spill slot in between. So a value live across such a call
//! keeps its location for the whole function.

use keel_ir::dominators::ControlFlowGraph;
#[cfg(test)]
use keel_ir::dominators::DominatorTree;
#[cfg(feature = "experimental-regalloc")]
mod local;
mod program;
mod remat;
#[cfg(feature = "experimental-regalloc")]
pub use remat::RematerializingAllocator;
pub(crate) use remat::{RematRecipe, remat_recipe};
#[cfg(feature = "experimental-regalloc")]
mod split;
#[cfg(feature = "experimental-regalloc")]
mod weighted;
pub use program::*;

use keel_ir::{Block, EntityRef, Function, InstData, Type, Value, ValueDef};

/// A physical register or eight-byte frame slot at an allocation boundary.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Loc {
    /// A general-purpose register, by number (`x9` is `Reg(9)`).
    Reg(u8),
    /// A SIMD/FP register, by number (`d19` is `FReg(19)`).
    FReg(u8),
    /// A spill slot, by index; the code generator decides the offset.
    Stack(u32),
    /// Immutable scalar recipe, computed at each read instead of stored.
    /// Valid only for the recipe's own value; never a copy destination.
    Remat(Value),
}

/// The registers the allocator may hand out.
#[derive(Copy, Clone, Debug)]
pub struct Pools<'a> {
    /// Clobbered by calls. Preferred for values not live across a call:
    /// they cost nothing to use.
    pub caller_saved: &'a [u8],
    /// Preserved by calls, but a function that writes one must save and
    /// restore it.
    pub callee_saved: &'a [u8],
    /// The same for floats, in the SIMD/FP registers.
    pub float_caller_saved: &'a [u8],
    pub float_callee_saved: &'a [u8],
}

pub struct Allocation {
    pub edits: AllocationEdits,
    /// The reachable blocks in the order the intervals were numbered, which
    /// is also the order they must be emitted in.
    pub order: Vec<Block>,
    /// Default locations, overridden at selected instruction boundaries by `edits`.
    /// By half (0, or 1 for the high half of an `i128`), then value index.
    /// `None` for values with no interval: those defined in unreachable
    /// code, and aliases.
    pub locs: [Vec<Option<Loc>>; 2],
    /// Number of 8-byte spill units the frame needs; each vector uses two.
    pub spill_slots: u32,
    /// The callee-saved registers handed out, sorted, which the prologue
    /// must save.
    pub callee_saved_used: Vec<u8>,
    pub float_callee_saved_used: Vec<u8>,
}

impl Allocation {
    /// The fallback location of `value`, before instruction overrides. Emitters
    /// use [`Self::operand_loc`] for instruction inputs/results. Resolve through
    /// [`Function::resolve`] first.
    pub fn loc(&self, value: Value) -> Option<Loc> {
        self.locs[0][value.index()]
    }

    /// The fallback location of the high half of `i128` value `value`.
    pub fn hi_loc(&self, value: Value) -> Option<Loc> {
        self.locs[1][value.index()]
    }
}

/// Replaceable allocation over selected lowering sequences. Assignments may
/// change at each instruction; explicit transfers preserve values between them.
/// Registers outside the input pools remain reserved by lowering. Constraints
/// must be satisfied or compilation rejects the allocation before emission.
pub trait RegisterAllocator {
    fn name(&self) -> &'static str;
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation;
}

/// Fast uses conservative contiguous intervals without register affinities.
/// Full preserves lifetime holes and uses selection affinities. Both tiers
/// share sparse CFG liveness, but Fast constructs only scalar endpoints and
/// uses a separate contiguous scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AllocationTier {
    Fast,
    /// Experimental block-local cache; keep Fast available as the reference.
    #[cfg(feature = "experimental-regalloc")]
    Local,
    /// Experimental splitting of spilled ranges into available registers.
    #[cfg(feature = "experimental-regalloc")]
    Split,
    /// Experimental loop-weighted spill density over Full intervals.
    #[cfg(feature = "experimental-regalloc")]
    Weighted,
    /// Experimental Full intervals with unanimous incoming scalar GPR preferences.
    #[cfg(feature = "experimental-regalloc")]
    FullAffinity,
    #[default]
    Full,
}
impl RegisterAllocator for AllocationTier {
    fn name(&self) -> &'static str {
        match self {
            Self::Fast => "fast",
            #[cfg(feature = "experimental-regalloc")]
            Self::Local => "local",
            #[cfg(feature = "experimental-regalloc")]
            Self::Split => "split",
            #[cfg(feature = "experimental-regalloc")]
            Self::Weighted => "weighted",
            #[cfg(feature = "experimental-regalloc")]
            Self::FullAffinity => "full-affinity",
            Self::Full => "full",
        }
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        #[cfg(feature = "experimental-regalloc")]
        if *self == Self::Weighted {
            return weighted::allocate(input);
        }
        #[cfg(feature = "experimental-regalloc")]
        if *self == Self::Split {
            return split::allocate(input);
        }
        #[cfg(feature = "experimental-regalloc")]
        if *self == Self::Local {
            return local::allocate(input);
        }
        allocate_tier(
            input.func,
            input.pools,
            input.is_call,
            input.returns_twice,
            input.selection,
            *self,
        )
    }
}

/// Allocates every value in `func`. `is_call` says which instructions
/// clobber the caller-saved registers, `returns_twice` which calls may
/// return again later.
#[cfg(test)]
pub fn allocate(
    func: &Function,
    pools: &Pools,
    is_call: impl Fn(&InstData) -> bool,
    returns_twice: impl Fn(&InstData) -> bool,
) -> Allocation {
    allocate_selected(
        func,
        pools,
        is_call,
        returns_twice,
        &crate::select::Selection::default(),
    )
}

#[cfg(test)]
fn allocate_selected(
    func: &Function,
    pools: &Pools,
    is_call: impl Fn(&InstData) -> bool,
    returns_twice: impl Fn(&InstData) -> bool,
    selection: &crate::select::Selection,
) -> Allocation {
    allocate_tier(
        func,
        pools,
        is_call,
        returns_twice,
        selection,
        AllocationTier::Full,
    )
}

fn allocate_tier(
    func: &Function,
    pools: &Pools,
    is_call: impl Fn(&InstData) -> bool,
    returns_twice: impl Fn(&InstData) -> bool,
    selection: &crate::select::Selection,
    tier: AllocationTier,
) -> Allocation {
    profiling::scope!("register-allocation", tier.name());
    backend_scope!(Allocation);
    allocate_tier_inner(func, pools, is_call, returns_twice, selection, tier)
}

fn allocate_tier_inner(
    func: &Function,
    pools: &Pools,
    is_call: impl Fn(&InstData) -> bool,
    returns_twice: impl Fn(&InstData) -> bool,
    selection: &crate::select::Selection,
    tier: AllocationTier,
) -> Allocation {
    let cfg = ControlFlowGraph::compute(func);
    let order = cfg.reverse_postorder(func);
    let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
    let spill_slots = match tier {
        #[cfg(feature = "experimental-regalloc")]
        AllocationTier::Local | AllocationTier::Split | AllocationTier::Weighted => {
            unreachable!("experimental allocators have their own entry points")
        }
        AllocationTier::Fast => {
            let intervals =
                build_fast_intervals(func, &cfg, &order, &is_call, &returns_twice, selection);
            let mut spilled = fast_scan(&intervals, pools, &mut locs);
            assign_fast_spill_slots(&mut spilled, &mut locs)
        }
        AllocationTier::Full => {
            let intervals =
                build_intervals(func, &cfg, &order, &is_call, &returns_twice, selection);
            let mut spilled = linear_scan(&intervals, pools, &mut locs, &selection.hints);
            assign_spill_slots(&mut spilled, &mut locs)
        }
        #[cfg(feature = "experimental-regalloc")]
        AllocationTier::FullAffinity => {
            let intervals =
                build_intervals(func, &cfg, &order, &is_call, &returns_twice, selection);
            let incoming = IncomingAffinities::collect(func, &order, &returns_twice);
            let mut spilled = linear_scan_with_preference(
                &intervals,
                pools,
                &mut locs,
                &selection.hints,
                |cur, locs| {
                    if cur.half == 0 && !cur.float && !cur.vector {
                        // A consensus necessarily names an earlier assigned home,
                        // so affinity alone cannot introduce a new callee save.
                        incoming.consensus(cur.value, locs)
                    } else {
                        None
                    }
                },
                |a, b| b.end().cmp(&a.end()),
            );
            assign_spill_slots(&mut spilled, &mut locs)
        }
    };

    baseline_allocation(order, locs, spill_slots, pools)
}

/// Prefer spilling cheap recipes only when the ordinary scan exhausts its
/// registers. Liveness, free-register choices and slot assignment stay shared
/// with the reference tiers; the rematerialization wrapper replaces the homes.
#[cfg(feature = "experimental-regalloc")]
pub(super) fn allocate_tier_remat(
    input: &AllocationInput<'_>,
    tier: AllocationTier,
    eligible: &[bool],
) -> Allocation {
    profiling::scope!("rematerialization-register-allocation", tier.name());
    backend_scope!(Allocation);
    let func = input.func;
    assert_eq!(eligible.len(), func.num_values());
    let cfg = ControlFlowGraph::compute(func);
    let order = cfg.reverse_postorder(func);
    let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
    let spill_slots = match tier {
        AllocationTier::Fast => {
            let intervals = build_fast_intervals(
                func,
                &cfg,
                &order,
                input.is_call,
                input.returns_twice,
                input.selection,
            );
            let mut spilled =
                fast_scan_with_priority(&intervals, input.pools, &mut locs, |a, b| {
                    (!eligible[a.value.index()])
                        .cmp(&(!eligible[b.value.index()]))
                        .then_with(|| b.end.cmp(&a.end))
                });
            assign_fast_spill_slots(&mut spilled, &mut locs)
        }
        AllocationTier::Full => {
            let intervals = build_intervals(
                func,
                &cfg,
                &order,
                input.is_call,
                input.returns_twice,
                input.selection,
            );
            let mut spilled = linear_scan_with_priority(
                &intervals,
                input.pools,
                &mut locs,
                &input.selection.hints,
                |a, b| {
                    (!eligible[a.value.index()])
                        .cmp(&(!eligible[b.value.index()]))
                        .then_with(|| b.end().cmp(&a.end()))
                },
            );
            assign_spill_slots(&mut spilled, &mut locs)
        }
        _ => unreachable!("rematerialization pressure policy supports Fast and Full"),
    };
    baseline_allocation(order, locs, spill_slots, input.pools)
}

fn baseline_allocation(
    order: Vec<Block>,
    locs: [Vec<Option<Loc>>; 2],
    spill_slots: u32,
    pools: &Pools,
) -> Allocation {
    // Visit locations once, without collecting and sorting one entry for every
    // value that happens to use the same callee-saved register.
    let mut integer_used = RegisterMask::default();
    let mut float_used = RegisterMask::default();
    for &loc in locs.iter().flatten().flatten() {
        match loc {
            Loc::Reg(r) => integer_used.insert(r),
            Loc::FReg(r) => float_used.insert(r),
            Loc::Stack(_) | Loc::Remat(_) => {}
        }
    }
    let used = |seen: &RegisterMask, pool: &[u8]| {
        let mut result = RegisterMask::default();
        for &r in pool {
            if seen.contains(r) {
                result.insert(r);
            }
        }
        result.sorted_registers()
    };
    let callee_saved_used = used(&integer_used, pools.callee_saved);
    let float_callee_saved_used = used(&float_used, pools.float_callee_saved);

    Allocation {
        edits: AllocationEdits::default(),
        order,
        locs,
        spill_slots,
        callee_saved_used,
        float_callee_saved_used,
    }
}

#[derive(Clone, Debug)]
struct Interval {
    value: Value,
    /// 1 for the high half of an `i128`.
    half: usize,
    /// Sorted, disjoint `[from, to]` ranges of points where it is live.
    ranges: Ranges,
    crosses_call: bool,
    preserved_gprs: u64,
    /// Whether it needs an FP register rather than a general-purpose one.
    float: bool,
    vector: bool,
}

/// Live ranges of one value. Most values live within one contiguous range,
/// which is stored inline instead of in its own heap allocation.
#[derive(Clone, Debug)]
enum Ranges {
    One((u32, u32)),
    Many(Vec<(u32, u32)>),
}

impl Default for Ranges {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl Ranges {
    fn push(&mut self, range: (u32, u32)) {
        match self {
            Self::Many(list) if list.is_empty() => *self = Self::One(range),
            Self::One(first) => *self = Self::Many(vec![*first, range]),
            Self::Many(list) => list.push(range),
        }
    }

    fn last_mut(&mut self) -> Option<&mut (u32, u32)> {
        match self {
            Self::One(range) => Some(range),
            Self::Many(list) => list.last_mut(),
        }
    }
}

impl std::ops::Deref for Ranges {
    type Target = [(u32, u32)];

    fn deref(&self) -> &[(u32, u32)] {
        match self {
            Self::One(range) => std::slice::from_ref(range),
            Self::Many(list) => list,
        }
    }
}

impl From<Vec<(u32, u32)>> for Ranges {
    fn from(list: Vec<(u32, u32)>) -> Self {
        match *list {
            [range] => Self::One(range),
            _ => Self::Many(list),
        }
    }
}

impl<'a> IntoIterator for &'a Ranges {
    type Item = &'a (u32, u32);
    type IntoIter = std::slice::Iter<'a, (u32, u32)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Interval {
    fn start(&self) -> u32 {
        match &self.ranges {
            Ranges::One((from, _)) => *from,
            Ranges::Many(list) => list[0].0,
        }
    }

    fn end(&self) -> u32 {
        match &self.ranges {
            Ranges::One((_, to)) => *to,
            Ranges::Many(list) => list[list.len() - 1].1,
        }
    }

    /// Whether the value is live at `point`.
    #[cfg(test)]
    fn covers(&self, point: u32) -> bool {
        if let Ranges::One((from, to)) = self.ranges {
            return from <= point && point <= to;
        }
        let i = self.ranges.partition_point(|&(_, to)| to < point);
        self.ranges.get(i).is_some_and(|&(from, _)| from <= point)
    }

    /// Whether the two are ever live at the same point.
    fn intersects(&self, other: &Interval) -> bool {
        // New intervals arrive in start order. An inactive interval may have
        // thousands of ranges behind that start; do not walk those prefixes
        // again for every later candidate.
        let mut i = self.ranges.partition_point(|&(_, to)| to < other.start());
        let mut j = other.ranges.partition_point(|&(_, to)| to < self.start());
        while i < self.ranges.len() && j < other.ranges.len() {
            let (a, b) = (self.ranges[i], other.ranges[j]);
            if a.0 <= b.1 && b.0 <= a.1 {
                return true;
            }
            if a.1 < b.1 {
                i += 1;
            } else {
                j += 1;
            }
        }
        false
    }
}

/// A register-number set covering the entire u8 pool API without allocation.
#[derive(Default)]
struct RegisterMask([u64; 4]);

impl RegisterMask {
    fn insert(&mut self, register: u8) {
        self.0[register as usize / 64] |= 1 << (register % 64);
    }

    fn contains(&self, register: u8) -> bool {
        self.0[register as usize / 64] & (1 << (register % 64)) != 0
    }

    fn sorted_registers(&self) -> Vec<u8> {
        let mut registers = Vec::new();
        for (i, &word) in self.0.iter().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                registers.push((i * 64 + remaining.trailing_zeros() as usize) as u8);
                remaining &= remaining - 1;
            }
        }
        registers
    }
}

fn register_number(loc: Loc) -> u8 {
    match loc {
        Loc::Reg(r) | Loc::FReg(r) => r,
        Loc::Stack(_) | Loc::Remat(_) => unreachable!("only registers participate in the scan"),
    }
}

/// Intersect proofs only at calls strictly inside actual live ranges. Calls in
/// lifetime holes cannot constrain the value; an unknown call contributes zero.
fn call_preserves(calls: &[(u32, u64)], ranges: &[(u32, u32)]) -> u64 {
    let mut preserved = u64::MAX;
    let mut work = 0;
    for &(from, to) in ranges {
        let start = calls.partition_point(|&(point, _)| point <= from);
        for &(point, mask) in &calls[start..] {
            if point >= to {
                break;
            }
            work += 1;
            // Bound extra proof work on values spanning many call sites.
            if work > 64 {
                return 0;
            }
            preserved &= mask;
            if preserved == 0 {
                return 0;
            }
        }
    }
    preserved
}

/// The fast tier never materializes lifetime holes, including for spills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FastInterval {
    value: Value,
    half: usize,
    start: u32,
    end: u32,
    crosses_call: bool,
    preserved_gprs: u64,
    float: bool,
    vector: bool,
}

fn build_fast_intervals(
    func: &Function,
    cfg: &ControlFlowGraph,
    order: &[Block],
    is_call: &dyn Fn(&InstData) -> bool,
    returns_twice: &dyn Fn(&InstData) -> bool,
    selection: &crate::select::Selection,
) -> Vec<FastInterval> {
    profiling::scope!("contiguous-live-intervals");
    backend_scope!(Intervals);
    let live = liveness(func, cfg, order, selection);
    let is_alias = |v: Value| matches!(func.value_def(v), ValueDef::Alias(_));
    let mut endpoints: Vec<Option<(u32, u32)>> = vec![None; func.num_values()];
    let mut open: Vec<Option<(u32, u32)>> = vec![None; func.num_values()];
    let mut touched = Vec::new();
    let mut crosses_call = vec![false; func.num_values()];
    let mut crosses_twice = vec![false; func.num_values()];
    let mut preserved_gprs = if selection.preserved_call_gprs.is_empty() {
        Vec::new()
    } else {
        vec![u64::MAX; func.num_values()]
    };
    let mut calls = Vec::new();
    let mut preserved_calls = Vec::new();
    let mut twice = Vec::new();
    let mut slot = 0u32;

    fn extend(open: &mut [Option<(u32, u32)>], v: Value, point: u32, touched: &mut Vec<Value>) {
        match &mut open[v.index()] {
            Some((from, to)) => {
                *from = (*from).min(point);
                *to = (*to).max(point);
            }
            range @ None => {
                touched.push(v);
                *range = Some((point, point));
            }
        }
    }
    let crosses = |points: &[u32], from: u32, to: u32| {
        let next = points.partition_point(|&c| c <= from);
        points.get(next).is_some_and(|&c| c < to)
    };
    for &block in order {
        let header = 2 * slot + 1;
        slot += 1;
        for &param in func.block_params(block) {
            if !is_alias(param) {
                extend(&mut open, param, header, &mut touched);
            }
        }
        for &v in &live.live_in[block.index()] {
            extend(&mut open, v, header, &mut touched);
        }
        for &inst in func.block_insts(block) {
            let (read, write) = (2 * slot, 2 * slot + 1);
            slot += 1;
            if selection.covered(inst) {
                continue;
            }
            selection.for_each_operand(func, inst, |v| extend(&mut open, v, read, &mut touched));
            for &result in func.inst_results(inst) {
                if !is_alias(result) {
                    extend(&mut open, result, write, &mut touched);
                }
            }
            let data = func.inst_data(inst);
            if is_call(data) {
                calls.push(read);
                if !selection.preserved_call_gprs.is_empty() {
                    preserved_calls.push((
                        read,
                        selection
                            .preserved_call_gprs
                            .get(&inst)
                            .copied()
                            .unwrap_or(0),
                    ));
                }
            }
            if returns_twice(data) {
                twice.push(read);
            }
        }
        let end = 2 * slot - 1;
        for &v in &live.live_out[block.index()] {
            extend(&mut open, v, end, &mut touched);
        }
        // Classify calls using this block's actual live range, before
        // collapsing the function-wide envelope. Calls in holes do not
        // restrict the register class or pin returns-twice lifetimes.
        for v in touched.drain(..) {
            let i = v.index();
            let (from, to) = open[i].take().unwrap();
            endpoints[i] = Some(match endpoints[i] {
                Some((start, end)) => (start.min(from), end.max(to)),
                None => (from, to),
            });
            crosses_call[i] |= crosses(&calls, from, to);
            if !selection.preserved_call_gprs.is_empty() {
                preserved_gprs[i] &= call_preserves(&preserved_calls, &[(from, to)]);
            }
            crosses_twice[i] |= crosses(&twice, from, to);
        }
        calls.clear();
        preserved_calls.clear();
        twice.clear();
    }
    let mut intervals = Vec::new();
    for (i, endpoint) in endpoints.into_iter().enumerate() {
        let Some((mut start, mut end)) = endpoint else {
            continue;
        };
        if crosses_twice[i] {
            (start, end) = (0, slot * 2);
        }
        let value = Value::new(i);
        let ty = func.value_type(value);
        for half in 0..if ty == Type::I128 { 2 } else { 1 } {
            intervals.push(FastInterval {
                value,
                half,
                start,
                end,
                crosses_call: crosses_call[i],
                preserved_gprs: if selection.preserved_call_gprs.is_empty() || crosses_twice[i] {
                    0
                } else {
                    preserved_gprs[i]
                },
                float: ty.is_float() || ty.is_vector(),
                vector: ty.is_vector(),
            });
        }
    }
    intervals
}

fn fast_scan<'a>(
    intervals: &'a [FastInterval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
) -> Vec<&'a FastInterval> {
    fast_scan_with_priority(intervals, pools, locs, |a, b| b.end.cmp(&a.end))
}

fn fast_scan_with_priority<'a>(
    intervals: &'a [FastInterval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
    priority: impl Fn(&FastInterval, &FastInterval) -> std::cmp::Ordering,
) -> Vec<&'a FastInterval> {
    profiling::scope!("contiguous-linear-scan");
    backend_scope!(Scan);
    let mut sorted: Vec<_> = intervals.iter().collect();
    sorted.sort_unstable_by_key(|iv| (iv.start, iv.value, iv.half));
    // With no holes, at most one live interval owns each register.
    let mut active: Vec<(&FastInterval, Loc)> = Vec::new();
    let mut spilled = Vec::new();
    for cur in sorted {
        active.retain(|&(iv, _)| iv.end >= cur.start);
        let (caller, callee, class): (_, _, fn(u8) -> Loc) = if cur.float {
            (
                pools.float_caller_saved,
                pools.float_callee_saved,
                Loc::FReg,
            )
        } else {
            (pools.caller_saved, pools.callee_saved, Loc::Reg)
        };
        let caller_allowed = |r: u8| {
            !cur.crosses_call
                || !cur.float && (cur.preserved_gprs & 1u64.checked_shl(r.into()).unwrap_or(0) != 0)
        };
        let callee = if cur.crosses_call && cur.vector {
            &[][..]
        } else {
            callee
        };
        let mut occupied = RegisterMask::default();
        for &(iv, reg) in &active {
            if iv.float == cur.float {
                occupied.insert(register_number(reg));
            }
        }
        let free = caller
            .iter()
            .copied()
            .filter(|&r| caller_allowed(r))
            .chain(callee.iter().copied())
            .find(|&r| !occupied.contains(r));
        if let Some(r) = free {
            let reg = class(r);
            locs[cur.half][cur.value.index()] = Some(reg);
            active.push((cur, reg));
            continue;
        }
        let victim = active
            .iter()
            .enumerate()
            .filter(|(_, (iv, reg))| {
                iv.float == cur.float
                    && (caller.contains(&register_number(*reg))
                        && caller_allowed(register_number(*reg))
                        || callee.contains(&register_number(*reg)))
            })
            .max_by(|(_, (a, _)), (_, (b, _))| priority(b, a))
            .map(|(i, _)| i);
        match victim {
            Some(i) if priority(cur, active[i].0).is_gt() => {
                let (evicted, reg) = active.swap_remove(i);
                locs[evicted.half][evicted.value.index()] = None;
                spilled.push(evicted);
                locs[cur.half][cur.value.index()] = Some(reg);
                active.push((cur, reg));
            }
            _ => spilled.push(cur),
        }
    }
    spilled
}

fn assign_fast_spill_slots(spilled: &mut [&FastInterval], locs: &mut [Vec<Option<Loc>>; 2]) -> u32 {
    profiling::scope!("spill-slot-assignment");
    backend_scope!(SpillSlots);
    spilled.sort_unstable_by_key(|iv| (iv.start, iv.value, iv.half));
    let mut count = 0;
    // Separate pools keep each reused home's width stable. Vector homes take
    // two eight-byte units; the bitset still tracks one home per live vector.
    for vector in [false, true] {
        let base = count;
        let units = if vector { 2 } else { 1 };
        let mut slots = SpillSlots::default();
        for iv in spilled.iter().filter(|iv| iv.vector == vector) {
            let slot = base + units * slots.assign(iv.start, iv.end);
            locs[iv.half][iv.value.index()] = Some(Loc::Stack(slot));
        }
        count += units * slots.count;
    }
    count
}

/// Live-in and live-out sets, indexed by block.
struct Liveness {
    live_in: BlockSets,
    live_out: BlockSets,
}

/// Sorted value sets for every block, packed into one array.
struct BlockSets {
    start: Vec<u32>,
    values: Vec<Value>,
}

impl BlockSets {
    fn empty(blocks: usize) -> Self {
        Self {
            start: vec![0; blocks + 1],
            values: Vec::new(),
        }
    }

    /// Groups `(block, value)` entries by block, keeping their order.
    fn from_entries(blocks: usize, entries: &[(u32, Value)]) -> Self {
        let mut start = vec![0u32; blocks + 1];
        for &(b, _) in entries {
            start[b as usize + 1] += 1;
        }
        for b in 0..blocks {
            start[b + 1] += start[b];
        }
        let mut fill = start.clone();
        let mut values = vec![Value::new(0); entries.len()];
        for &(b, v) in entries {
            let slot = &mut fill[b as usize];
            values[*slot as usize] = v;
            *slot += 1;
        }
        Self { start, values }
    }
}

impl std::ops::Index<usize> for BlockSets {
    type Output = [Value];

    fn index(&self, block: usize) -> &[Value] {
        &self.values[self.start[block] as usize..self.start[block + 1] as usize]
    }
}

/// The least solution of the classic backward dataflow:
///
/// ```text
/// live_out(B) = union of live_in(S) over successors S
/// live_in(B)  = uses(B) + (live_out(B) - defs(B))
/// ```
///
/// `uses(B)` are values read in `B` before any definition in `B`; `defs(B)`
/// are `B`'s parameters and results. Block arguments are ordinary operands
/// of the predecessor's terminator, and parameters are definitions, so
/// block arguments need no special case (phi nodes would).
///
/// Rather than iterating set unions to a fixed point, each value is walked
/// backwards from its upward-exposed uses until its definition: a value is
/// live into exactly the blocks from which a use is reachable without passing
/// its definition. Every set entry is produced once, and visiting values in
/// increasing order leaves each set sorted without merging.
fn liveness(
    func: &Function,
    cfg: &ControlFlowGraph,
    order: &[Block],
    selection: &crate::select::Selection,
) -> Liveness {
    profiling::scope!("liveness");
    backend_scope!(Liveness);
    let n = func.num_blocks();
    // In verified SSA, every operand in the only reachable block is defined
    // there (parameters included). Even a self edge introduces no live-in
    // value: edge arguments are explicit reads and parameters are definitions.
    // The exact fixed point is empty, so avoid the use/def walk.
    if order.len() <= 1 {
        return Liveness {
            live_in: BlockSets::empty(n),
            live_out: BlockSets::empty(n),
        };
    }
    let num_values = func.num_values();
    // Upward-exposed uses as (value, block) occurrences, at most one per pair.
    let mut exposed: Vec<(u32, u32)> = Vec::new();
    // Block stamps avoid trees and clearing a value-sized set per block.
    let mut defined = vec![0u32; num_values];
    let mut used = vec![0u32; num_values];
    // The reachable block holding each value's (non-covered) definition.
    let mut def_block = vec![u32::MAX; num_values];
    let mut reachable = vec![false; n];
    for &block in order {
        let b = block.index() as u32;
        let stamp = b + 1;
        reachable[block.index()] = true;
        for &param in func.block_params(block) {
            defined[param.index()] = stamp;
            def_block[param.index()] = b;
        }
        for &inst in func.block_insts(block) {
            if selection.covered(inst) {
                continue;
            }
            selection.for_each_operand(func, inst, |v| {
                let i = v.index();
                if defined[i] != stamp && used[i] != stamp {
                    exposed.push((i as u32, b));
                    used[i] = stamp;
                }
            });
            for &v in func.inst_results(inst) {
                defined[v.index()] = stamp;
                def_block[v.index()] = b;
            }
        }
    }
    drop(defined);
    drop(used);
    // Group the use blocks by value (a counting sort keeps block order).
    let mut start = vec![0u32; num_values + 1];
    for &(v, _) in &exposed {
        start[v as usize + 1] += 1;
    }
    for i in 0..num_values {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut use_blocks = vec![0u32; exposed.len()];
    for &(v, b) in &exposed {
        let slot = &mut fill[v as usize];
        use_blocks[*slot as usize] = b;
        *slot += 1;
    }
    drop(exposed);
    drop(fill);

    // (block, value) entries, produced in increasing value order.
    let mut live_in = Vec::new();
    let mut live_out = Vec::new();
    // The last value entered into each block's live-in and live-out set.
    let mut in_mark = vec![u32::MAX; n];
    let mut out_mark = vec![u32::MAX; n];
    let mut stack = Vec::new();
    for v in 0..num_values {
        let blocks = &use_blocks[start[v] as usize..start[v + 1] as usize];
        if blocks.is_empty() {
            continue;
        }
        let value = Value::new(v);
        let mark = v as u32;
        for &b in blocks {
            if in_mark[b as usize] != mark {
                in_mark[b as usize] = mark;
                live_in.push((b, value));
                stack.push(b);
            }
        }
        while let Some(b) = stack.pop() {
            for &pred in cfg.preds(Block::new(b as usize)) {
                let p = pred.index();
                if !reachable[p] {
                    continue;
                }
                if out_mark[p] != mark {
                    out_mark[p] = mark;
                    live_out.push((p as u32, value));
                }
                if def_block[v] != p as u32 && in_mark[p] != mark {
                    in_mark[p] = mark;
                    live_in.push((p as u32, value));
                    stack.push(p as u32);
                }
            }
        }
    }
    Liveness {
        live_in: BlockSets::from_entries(n, &live_in),
        live_out: BlockSets::from_entries(n, &live_out),
    }
}

/// Numbers the program points (see the module docs) and builds one interval
/// per defined value.
fn build_intervals(
    func: &Function,
    cfg: &ControlFlowGraph,
    order: &[Block],
    is_call: &dyn Fn(&InstData) -> bool,
    returns_twice: &dyn Fn(&InstData) -> bool,
    selection: &crate::select::Selection,
) -> Vec<Interval> {
    profiling::scope!("live-intervals");
    backend_scope!(Intervals);
    let live = liveness(func, cfg, order, selection);
    let is_alias = |v: Value| matches!(func.value_def(v), ValueDef::Alias(_));

    // Each value's ranges, built block by block: `open` is the range in
    // the current block, from the first point to the last seen there.
    let mut ranges: Vec<Ranges> = Vec::new();
    ranges.resize_with(func.num_values(), Ranges::default);
    let mut open: Vec<Option<(u32, u32)>> = vec![None; func.num_values()];
    let mut touched: Vec<Value> = Vec::new();
    fn extend(open: &mut [Option<(u32, u32)>], v: Value, point: u32, touched: &mut Vec<Value>) {
        let range = &mut open[v.index()];
        *range = Some(match *range {
            None => {
                touched.push(v);
                (point, point)
            }
            Some((start, end)) => (start.min(point), end.max(point)),
        });
    }
    // Read points of calls, and of calls that return twice, in increasing
    // order.
    let mut calls = Vec::new();
    let mut preserved_calls = Vec::new();
    let mut twice = Vec::new();

    // Slot `k` owns the read point 2k and the write point 2k + 1.
    let mut slot = 0u32;
    for &block in order {
        let header = 2 * slot + 1;
        slot += 1;
        for &param in func.block_params(block) {
            if !is_alias(param) {
                extend(&mut open, param, header, &mut touched);
            }
        }
        for &v in &live.live_in[block.index()] {
            extend(&mut open, v, header, &mut touched);
        }

        for &inst in func.block_insts(block) {
            let (read, write) = (2 * slot, 2 * slot + 1);
            slot += 1;
            let data = func.inst_data(inst);
            if selection.covered(inst) {
                continue;
            }
            selection.for_each_operand(func, inst, |v| extend(&mut open, v, read, &mut touched));
            for &result in func.inst_results(inst) {
                // An alias's instruction is dead; its result goes nowhere.
                if !is_alias(result) {
                    extend(&mut open, result, write, &mut touched);
                }
            }
            if is_call(data) {
                calls.push(read);
                if !selection.preserved_call_gprs.is_empty() {
                    preserved_calls.push((
                        read,
                        selection
                            .preserved_call_gprs
                            .get(&inst)
                            .copied()
                            .unwrap_or(0),
                    ));
                }
            }
            if returns_twice(data) {
                twice.push(read);
            }
        }

        // The terminator's write point: values live out survive the
        // block-argument copies on the way out.
        let end = 2 * slot - 1;
        for &v in &live.live_out[block.index()] {
            extend(&mut open, v, end, &mut touched);
        }

        // Close this block's ranges. Live out of the previous block and
        // into this one, it continues the previous range: the only point
        // between them is a read point no instruction owns.
        for v in touched.drain(..) {
            let (from, to) = open[v.index()].take().unwrap();
            let list = &mut ranges[v.index()];
            match list.last_mut() {
                Some(last) if last.1 + 2 >= from => last.1 = to,
                _ => list.push((from, to)),
            }
        }
    }

    // Crossing a call means being live on both sides of its read point.
    let crosses = |points: &[u32], iv: &[(u32, u32)]| {
        iv.iter().any(|&(from, to)| {
            let next = points.partition_point(|&c| c <= from);
            points.get(next).is_some_and(|&c| c < to)
        })
    };
    let mut intervals = Vec::new();
    for (i, mut list) in ranges.into_iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let crosses_call = crosses(&calls, &list);
        let preserved_gprs = if selection.preserved_call_gprs.is_empty() || crosses(&twice, &list) {
            0
        } else {
            call_preserves(&preserved_calls, &list)
        };
        if crosses(&twice, &list) {
            list = Ranges::One((0, slot * 2));
        }
        let value = Value::new(i);
        let ty = func.value_type(value);
        let high_ranges = (ty == Type::I128).then(|| list.clone());
        intervals.push(Interval {
            value,
            half: 0,
            ranges: list,
            crosses_call,
            preserved_gprs,
            float: ty.is_float() || ty.is_vector(),
            vector: ty.is_vector(),
        });
        if let Some(ranges) = high_ranges {
            intervals.push(Interval {
                value,
                half: 1,
                ranges,
                crosses_call,
                preserved_gprs,
                float: false,
                vector: false,
            });
        }
    }
    intervals
}

// Gather once, with O(values + edge arguments) storage. The instruction walk
// detects returns-twice; no CFG or liveness analysis is repeated per interval.
// Keep duplicate predecessor edges: two arms of one branch can disagree.
#[cfg(feature = "experimental-regalloc")]
#[derive(Default)]
struct IncomingAffinities {
    sources: Vec<Vec<Value>>,
}

#[cfg(feature = "experimental-regalloc")]
impl IncomingAffinities {
    fn collect(
        func: &Function,
        order: &[Block],
        returns_twice: &impl Fn(&InstData) -> bool,
    ) -> Self {
        if func.sig.returns_twice
            || order.iter().any(|&block| {
                func.block_insts(block)
                    .iter()
                    .any(|&inst| returns_twice(func.inst_data(inst)))
            })
        {
            return Self::default();
        }
        let mut sources = vec![Vec::new(); func.num_values()];
        let mut excluded = vec![false; func.num_values()];
        for &block in order {
            for &param in func.block_params(block) {
                // Entry parameters also have implicit incoming ABI values.
                excluded[param.index()] = Some(block) == func.entry_block()
                    || !matches!(
                        func.value_type(param),
                        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                    );
            }
        }
        for &block in order {
            let Some(&term) = func.block_insts(block).last() else {
                continue;
            };
            let data = func.inst_data(term);
            for dest in data.branch_dests() {
                if matches!(data, InstData::Invoke { .. }) {
                    // Neither the normal nor exceptional Invoke target is in scope,
                    // independently of whether this edge carries arguments.
                    for &param in func.block_params(dest.block) {
                        excluded[param.index()] = true;
                    }
                }
                for (&param, &arg) in func.block_params(dest.block).iter().zip(&dest.args) {
                    sources[param.index()].push(func.resolve(arg));
                }
            }
        }
        for (args, excluded) in sources.iter_mut().zip(excluded) {
            if excluded {
                args.clear();
            }
        }
        Self { sources }
    }

    fn consensus(&self, value: Value, locs: &[Vec<Option<Loc>>; 2]) -> Option<Loc> {
        let args = self.sources.get(value.index())?;
        let (&first, rest) = args.split_first()?;
        let reg = locs[0][first.index()]?;
        if !matches!(reg, Loc::Reg(_)) {
            return None;
        }
        // None includes future backedges and sources already evicted to a spill.
        rest.iter()
            .all(|arg| locs[0][arg.index()] == Some(reg))
            .then_some(reg)
    }
}

/// Gives each interval a register from `pools` or spills it. Fills in the
/// register locations and returns the spilled intervals.
fn linear_scan<'a>(
    intervals: &'a [Interval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
    hints: &keel_ir::fxhash::FxHashMap<Value, Vec<Value>>,
) -> Vec<&'a Interval> {
    linear_scan_with_priority(intervals, pools, locs, hints, |a, b| b.end().cmp(&a.end()))
}

/// A higher priority keeps a value in a register. The default policy prefers
/// the earliest endpoint; experimental policies can compare spill costs.
fn linear_scan_with_priority<'a>(
    intervals: &'a [Interval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
    hints: &keel_ir::fxhash::FxHashMap<Value, Vec<Value>>,
    priority: impl Fn(&Interval, &Interval) -> std::cmp::Ordering,
) -> Vec<&'a Interval> {
    // The ordinary tiers build no incoming-edge table or extra scan state.
    linear_scan_with_preference(intervals, pools, locs, hints, |_, _| None, priority)
}

fn linear_scan_with_preference<'a>(
    intervals: &'a [Interval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
    hints: &keel_ir::fxhash::FxHashMap<Value, Vec<Value>>,
    preference: impl Fn(&Interval, &[Vec<Option<Loc>>; 2]) -> Option<Loc>,
    priority: impl Fn(&Interval, &Interval) -> std::cmp::Ordering,
) -> Vec<&'a Interval> {
    profiling::scope!("linear-scan");
    backend_scope!(Scan);
    // Sort by (start, value, half) on packed integer keys rather than
    // re-reading each interval's ranges for every comparison.
    let mut keys: Vec<(u64, u64)> = intervals
        .iter()
        .enumerate()
        .map(|(i, iv)| {
            let primary = u64::from(iv.start()) << 32 | iv.value.index() as u64;
            (primary, (iv.half as u64) << 32 | i as u64)
        })
        .collect();
    keys.sort_unstable();
    let sorted = keys.into_iter().map(|(_, i)| &intervals[i as u32 as usize]);

    // Pool membership as masks, so a candidate test is a bit test rather
    // than a search of the pool. Indexed by class (1 for SIMD/FP).
    let mask_of = |pool: &[u8]| {
        let mut mask = RegisterMask::default();
        for &r in pool {
            mask.insert(r);
        }
        mask
    };
    let caller_masks = [
        mask_of(pools.caller_saved),
        mask_of(pools.float_caller_saved),
    ];
    let callee_masks = [
        mask_of(pools.callee_saved),
        mask_of(pools.float_callee_saved),
    ];

    // Intervals holding a register, with that register: live at the
    // current point (active), or in a hole (inactive).
    let mut active: Vec<Held> = Vec::new();
    let mut inactive: Vec<Held> = Vec::new();
    let mut spilled = Vec::new();
    let mut moved = Vec::new();
    // No active interval leaves its current range before the point after
    // this one, and no inactive interval reaches its next range before this
    // one: until then the lists need no maintenance.
    let mut active_until = u32::MAX;
    let mut inactive_until = u32::MAX;
    // How many active intervals of each class hold each register, kept up to
    // date with `active` rather than recounted for every interval.
    let mut occupied = [[0u32; 256]; 2];
    for cur in sorted {
        let pos = cur.start();
        // Intervals that have ended give up their register; the others
        // move between active and inactive as they enter or leave holes.
        if pos > active_until {
            let mut until = u32::MAX;
            active.retain_mut(|held| {
                if held.until >= pos || held.end >= pos && held.refresh(pos) {
                    until = until.min(held.until);
                    return true;
                }
                occupied[held.float as usize][held.number as usize] -= 1;
                if held.end >= pos {
                    moved.push(*held);
                }
                false
            });
            active_until = until;
        }
        if pos >= inactive_until {
            let mut until = u32::MAX;
            inactive.retain_mut(|held| {
                if held.until > pos {
                    until = until.min(held.until);
                    true
                } else if held.end < pos {
                    false
                } else if held.refresh(pos) {
                    occupied[held.float as usize][held.number as usize] += 1;
                    active_until = active_until.min(held.until);
                    active.push(*held);
                    false
                } else {
                    until = until.min(held.until);
                    true
                }
            });
            inactive_until = until;
        }
        for held in &moved {
            inactive_until = inactive_until.min(held.until);
        }
        inactive.extend(moved.drain(..));

        // Live across a call: only a callee-saved register survives it.
        // Otherwise prefer caller-saved ones, which need no save/restore.
        let (caller_saved, callee_saved, class): (_, _, fn(u8) -> Loc) = if cur.float {
            (
                pools.float_caller_saved,
                pools.float_callee_saved,
                Loc::FReg,
            )
        } else {
            (pools.caller_saved, pools.callee_saved, Loc::Reg)
        };
        let callers = caller_saved;
        let caller_allowed = |r: u8| {
            !cur.crosses_call
                || !cur.float && (cur.preserved_gprs & 1u64.checked_shl(r.into()).unwrap_or(0) != 0)
        };
        let vector_across_call = cur.crosses_call && cur.vector;
        let callee_saved = if vector_across_call {
            &[][..]
        } else {
            callee_saved
        };
        let caller_mask = &caller_masks[cur.float as usize];
        let callee_mask = &callee_masks[cur.float as usize];
        let allowed_number = |n: u8| {
            caller_mask.contains(n) && caller_allowed(n)
                || !vector_across_call && callee_mask.contains(n)
        };
        let allowed = |r: Loc| match r {
            Loc::Reg(n) if !cur.float => allowed_number(n),
            Loc::FReg(n) if cur.float => allowed_number(n),
            _ => false,
        };
        // Compute this once, rather than rescanning inactive for every
        // candidate register and again for every eviction candidate.
        let held_by_class = &occupied[cur.float as usize];
        let mut blocked = RegisterMask::default();
        for held in &inactive {
            if held.float == cur.float && !blocked.contains(held.number) && held.iv.intersects(cur)
            {
                blocked.insert(held.number);
            }
        }
        let blocked_by_inactive = |reg: Loc| blocked.contains(register_number(reg));
        let available_number =
            |n: u8| allowed_number(n) && held_by_class[n as usize] == 0 && !blocked.contains(n);
        let available = |r: Loc| match r {
            Loc::Reg(n) if !cur.float => available_number(n),
            Loc::FReg(n) if cur.float => available_number(n),
            _ => false,
        };
        let preferred = if hints.is_empty() {
            None
        } else {
            hints
                .get(&cur.value)
                .into_iter()
                .flatten()
                .filter_map(|v| locs[cur.half][v.index()])
                .find(|&r| available(r))
        };
        let free = preferred
            .or_else(|| preference(cur, locs).filter(|&reg| available(reg)))
            .or_else(|| {
                callers
                    .iter()
                    .chain(callee_saved)
                    .copied()
                    .find(|&n| available_number(n))
                    .map(class)
            });
        if let Some(reg) = free {
            locs[cur.half][cur.value.index()] = Some(reg);
            occupied[cur.float as usize][register_number(reg) as usize] += 1;
            let held = Held::new(cur, reg);
            active_until = active_until.min(held.until);
            active.push(held);
            continue;
        }

        // Out of registers. Choose the lowest-priority eligible active
        // interval. Replace it only if keeping `cur` is more valuable.
        // The default policy evicts the interval whose endpoint is latest.
        let victim = active
            .iter()
            .enumerate()
            .filter(|(_, held)| allowed(held.reg) && !blocked_by_inactive(held.reg))
            .max_by(|(_, a), (_, b)| priority(b.iv, a.iv))
            .map(|(i, _)| i);
        match victim {
            Some(i) if priority(cur, active[i].iv).is_gt() => {
                let Held {
                    iv: evicted, reg, ..
                } = active.swap_remove(i);
                locs[evicted.half][evicted.value.index()] = None;
                spilled.push(evicted);
                locs[cur.half][cur.value.index()] = Some(reg);
                // The register passes between intervals of the same class.
                occupied[evicted.float as usize][register_number(reg) as usize] -= 1;
                occupied[cur.float as usize][register_number(reg) as usize] += 1;
                let held = Held::new(cur, reg);
                active_until = active_until.min(held.until);
                active.push(held);
            }
            _ => spilled.push(cur),
        }
    }
    spilled
}

/// An interval holding a register during the scan, with its end cached.
#[derive(Clone, Copy)]
struct Held<'a> {
    iv: &'a Interval,
    reg: Loc,
    end: u32,
    /// While active, the end of the range covering the current point; while
    /// inactive, the start of the next range.
    until: u32,
    number: u8,
    float: bool,
}

impl<'a> Held<'a> {
    /// `iv` held from its start, which its first range covers.
    fn new(iv: &'a Interval, reg: Loc) -> Self {
        Self {
            iv,
            reg,
            end: iv.end(),
            until: iv.ranges[0].1,
            number: register_number(reg),
            float: iv.float,
        }
    }

    /// Whether the interval covers `pos`, at most its end, updating `until`.
    fn refresh(&mut self, pos: u32) -> bool {
        let ranges = &self.iv.ranges;
        let i = ranges.partition_point(|&(_, to)| to < pos);
        let (from, to) = ranges[i];
        if from <= pos {
            self.until = to;
            true
        } else {
            self.until = from;
            false
        }
    }
}

/// Gives each spilled interval a stack slot, reusing the slots of intervals
/// that have ended: linear scan again, with as many "registers" as needed.
/// Returns the number of slots.
fn assign_spill_slots(spilled: &mut [&Interval], locs: &mut [Vec<Option<Loc>>; 2]) -> u32 {
    profiling::scope!("spill-slot-assignment");
    backend_scope!(SpillSlots);
    spilled.sort_unstable_by_key(|iv| (iv.start(), iv.value, iv.half));
    let mut count = 0;
    // Separate pools keep each reused home's width stable. Vector homes take
    // two eight-byte units; the bitset still tracks one home per live vector.
    for vector in [false, true] {
        let base = count;
        let units = if vector { 2 } else { 1 };
        let mut slots = SpillSlots::default();
        for iv in spilled.iter().filter(|iv| iv.vector == vector) {
            let slot = base + units * slots.assign(iv.start(), iv.end());
            locs[iv.half][iv.value.index()] = Some(Loc::Stack(slot));
        }
        count += units * slots.count;
    }
    count
}

/// Assign the lowest free stack slot after expiring intervals in start order.
/// The occupancy words avoid scanning every active interval for every smaller
/// slot when pressure is high; expiration retains inclusive range endpoints.
#[derive(Default)]
struct SpillSlots {
    active: Vec<(u32, u32)>,
    // No owner can expire until start passes this inclusive endpoint.
    min_end: Option<u32>,
    // Most functions never need more than 64 slots. Keep their occupancy
    // inline and allocate extra words only under greater spill pressure.
    first_occupied: u64,
    occupied_tail: Vec<u64>,
    count: u32,
}

impl SpillSlots {
    fn assign(&mut self, start: u32, end: u32) -> u32 {
        if self.min_end.is_some_and(|until| until < start) {
            let first = &mut self.first_occupied;
            let tail = &mut self.occupied_tail;
            let mut min_end: Option<u32> = None;
            self.active.retain(|&(until, slot)| {
                if until >= start {
                    min_end = Some(min_end.map_or(until, |min| min.min(until)));
                    true
                } else {
                    let word = if slot < 64 {
                        &mut *first
                    } else {
                        &mut tail[slot as usize / 64 - 1]
                    };
                    *word &= !(1u64 << (slot % 64));
                    false
                }
            });
            self.min_end = min_end;
        }
        let slot = if self.first_occupied != u64::MAX {
            let bit = self.first_occupied.trailing_ones();
            self.first_occupied |= 1u64 << bit;
            bit
        } else {
            let word = match self.occupied_tail.iter().position(|&bits| bits != u64::MAX) {
                Some(word) => word,
                None => {
                    self.occupied_tail.push(0);
                    self.occupied_tail.len() - 1
                }
            };
            let bit = self.occupied_tail[word].trailing_ones();
            self.occupied_tail[word] |= 1u64 << bit;
            (word as u32 + 1) * 64 + bit
        };
        self.active.push((end, slot));
        self.min_end = Some(self.min_end.map_or(end, |min| min.min(end)));
        self.count = self.count.max(slot + 1);
        slot
    }
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
        build_intervals(
            func,
            &cfg,
            &order,
            &|d| matches!(d, InstData::Call { .. }),
            &|_| false,
            &crate::select::Selection::default(),
        )
    }

    fn interval(intervals: &[Interval], index: usize) -> &Interval {
        intervals
            .iter()
            .find(|iv| iv.value.index() == index)
            .expect("value has an interval")
    }

    #[test]
    fn vector_spills_keep_two_units_and_do_not_use_partial_callee_saves() {
        let func = function(
            "decl @clobber()\nfunc @f(ptr) {\nblock0(v0: ptr):\n              v1 = load.i32x4 v0\n v2 = load.i32x4 v0\n call @clobber()\n              v3 = viadd v1, v2\n store v3, v0\n return\n}",
        );
        let pools = Pools {
            caller_saved: &[9],
            callee_saved: &[19],
            float_caller_saved: &[19],
            float_callee_saved: &[8],
        };
        for tier in [AllocationTier::Fast, AllocationTier::Full] {
            let allocation = allocate_tier(
                &func,
                &pools,
                |data: &InstData| matches!(data, InstData::Call { .. }),
                |_: &InstData| false,
                &crate::select::Selection::default(),
                tier,
            );
            let homes: Vec<_> = [Value::new(1), Value::new(2)]
                .into_iter()
                .map(|v| match allocation.loc(v).unwrap() {
                    Loc::Stack(slot) => slot,
                    loc => panic!("cross-call vector assigned to {loc:?}"),
                })
                .collect();
            assert!(homes[0].abs_diff(homes[1]) >= 2);
            assert!(homes.iter().all(|s| s + 2 <= allocation.spill_slots));
        }
    }

    #[cfg(feature = "experimental-regalloc")]
    #[test]
    fn rematerialization_priority_evicts_cheap_values_before_longer_expensive_values() {
        let pools = Pools {
            caller_saved: &[9],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let intervals = [
            Interval {
                value: Value::new(0),
                half: 0,
                ranges: vec![(0, 5)].into(),
                crosses_call: false,
                preserved_gprs: 0,
                float: false,
                vector: false,
            },
            Interval {
                value: Value::new(1),
                half: 0,
                ranges: vec![(2, 9)].into(),
                crosses_call: false,
                preserved_gprs: 0,
                float: false,
                vector: false,
            },
        ];
        let fast = intervals.each_ref().map(|iv| FastInterval {
            value: iv.value,
            half: iv.half,
            start: iv.start(),
            end: iv.end(),
            crosses_call: iv.crosses_call,
            preserved_gprs: iv.preserved_gprs,
            float: iv.float,
            vector: iv.vector,
        });
        // With no eligible recipes, the longer interval is spilled as usual.
        // Marking the shorter first value cheap must instead preserve the
        // expensive current value, despite its later endpoint.
        for eligible in [[false, false], [true, false]] {
            let mut locs = [vec![None; 2], vec![None; 2]];
            let spilled = fast_scan_with_priority(&fast, &pools, &mut locs, |a, b| {
                (!eligible[a.value.index()])
                    .cmp(&(!eligible[b.value.index()]))
                    .then_with(|| b.end.cmp(&a.end))
            });
            let expected = usize::from(!eligible[0]);
            assert_eq!(spilled.len(), 1);
            assert_eq!(spilled[0].value.index(), expected);

            let mut locs = [vec![None; 2], vec![None; 2]];
            let spilled = linear_scan_with_priority(
                &intervals,
                &pools,
                &mut locs,
                &Default::default(),
                |a, b| {
                    (!eligible[a.value.index()])
                        .cmp(&(!eligible[b.value.index()]))
                        .then_with(|| b.end().cmp(&a.end()))
                },
            );
            assert_eq!(spilled.len(), 1);
            assert_eq!(spilled[0].value.index(), expected);
        }
    }

    fn check_spill_slots_against_reference(intervals: impl IntoIterator<Item = (u32, u32)>) {
        let mut slots = SpillSlots::default();
        let mut reference = Vec::new();
        let mut count = 0;
        for (i, (start, end)) in intervals.into_iter().enumerate() {
            reference.retain(|&(until, _)| until >= start);
            let expected = (0..)
                .find(|&slot| reference.iter().all(|&(_, used)| used != slot))
                .unwrap();
            reference.push((end, expected));
            count = count.max(expected + 1);
            assert_eq!(
                slots.assign(start, end),
                expected,
                "interval {i}: {start}..={end}"
            );
            assert_eq!(slots.count, count);
        }
    }

    #[test]
    fn spill_slots_match_lowest_free_reference() {
        // Include inclusive endpoint collisions, large simultaneous pressure,
        // holes in the free slots, and complete expiration between groups.
        for pattern in 0..8u32 {
            let mut start = 0;
            check_spill_slots_against_reference((0..1024u32).map(|i| {
                start += match pattern {
                    0 => 0,
                    1 => 1,
                    2 => i % 3,
                    3 => {
                        if i % 131 == 0 {
                            1024
                        } else {
                            0
                        }
                    }
                    _ => (i.wrapping_mul(1664525).wrapping_add(pattern) >> 20) % 5,
                };
                let end = start
                    + match pattern {
                        0 => i % 17,
                        1 => 127,
                        2 => i % 257,
                        3 => 1 + i % 128,
                        _ => (i.wrapping_mul(1013904223) >> 17) % 513,
                    };
                (start, end)
            }));
        }
    }

    #[test]
    fn spill_slots_expiry_boundaries_match_reference() {
        check_spill_slots_against_reference([
            (0, 100),
            (1, 5),     // A new end precedes the previous minimum.
            (5, 5),     // Equal endpoints still occupy distinct homes.
            (6, 7),     // Expire both short owners, keeping the long owner.
            (8, 8),     // Recompute the minimum after partial expiration.
            (101, 200), // Expire all owners before refilling.
            (102, 102),
            (103, 103),
            (u32::MAX, u32::MAX),
            (u32::MAX, u32::MAX), // MAX is an endpoint, not an empty sentinel.
        ]);
        // The helper's existing behavior also permits starts out of order.
        check_spill_slots_against_reference([(10, 20), (0, 5), (6, 9), (4, 4), (21, 21)]);
        // Exercise retained and expired bits in each of three occupancy words.
        check_spill_slots_against_reference(
            (0..193)
                .map(|i| (0, i % 3))
                .chain((0..193).map(|i| (i / 65 + 1, i / 65 + 2))),
        );
    }

    #[test]
    fn spill_home_pools_preserve_widths_and_i128_halves() {
        let intervals: Vec<_> = [
            (0, 0, false, 0, 4),
            (0, 1, false, 0, 4),
            (1, 0, true, 0, 1),
            (2, 0, false, 5, 6),
            (3, 0, true, 2, 4),
            (4, 0, true, 4, 5),
        ]
        .into_iter()
        .map(|(value, half, vector, start, end)| Interval {
            value: Value::new(value),
            half,
            ranges: vec![(start, end)].into(),
            crosses_call: false,
            preserved_gprs: 0,
            float: vector,
            vector,
        })
        .collect();
        let fast: Vec<_> = intervals
            .iter()
            .map(|iv| FastInterval {
                value: iv.value,
                half: iv.half,
                start: iv.start(),
                end: iv.end(),
                crosses_call: iv.crosses_call,
                preserved_gprs: iv.preserved_gprs,
                float: iv.float,
                vector: iv.vector,
            })
            .collect();
        let mut full_spilled: Vec<_> = intervals.iter().rev().collect();
        let mut fast_spilled: Vec<_> = fast.iter().rev().collect();
        let mut full_locs = [vec![None; 5], vec![None; 5]];
        let mut fast_locs = full_locs.clone();
        assert_eq!(assign_spill_slots(&mut full_spilled, &mut full_locs), 6);
        assert_eq!(
            assign_fast_spill_slots(&mut fast_spilled, &mut fast_locs),
            6
        );
        assert_eq!(full_locs, fast_locs);
        assert_eq!(
            full_locs,
            [
                vec![
                    Some(Loc::Stack(0)),
                    Some(Loc::Stack(2)),
                    Some(Loc::Stack(0)),
                    Some(Loc::Stack(2)),
                    Some(Loc::Stack(4))
                ],
                vec![Some(Loc::Stack(1)), None, None, None, None],
            ]
        );
    }

    #[test]
    fn spill_occupancy_is_inline_until_slot_64() {
        let mut slots = SpillSlots::default();
        for expected in 0..64 {
            assert_eq!(slots.assign(0, 0), expected);
        }
        assert_eq!(slots.occupied_tail.capacity(), 0);
        // All 64 expire, and reusing them still needs no overflow allocation.
        for expected in 0..64 {
            assert_eq!(slots.assign(1, 1), expected);
        }
        assert_eq!(slots.occupied_tail.capacity(), 0);
        assert_eq!(slots.assign(1, 1), 64);
        assert_eq!(slots.occupied_tail, [1]);
        assert_eq!(slots.assign(2, 2), 0);
        assert_eq!(slots.occupied_tail, [0]);
    }

    #[test]
    fn callee_saved_discovery_matches_sorted_unique_locations() {
        let locs = [
            vec![
                Some(Loc::Reg(255)),
                Some(Loc::Reg(64)),
                Some(Loc::FReg(64)),
                None,
            ],
            vec![
                Some(Loc::Reg(64)),
                Some(Loc::FReg(255)),
                Some(Loc::Stack(64)),
                Some(Loc::Reg(7)),
            ],
        ];
        let alloc = baseline_allocation(
            Vec::new(),
            locs.clone(),
            65,
            &Pools {
                caller_saved: &[],
                callee_saved: &[255, 7, 64, 64, 0],
                float_caller_saved: &[],
                float_callee_saved: &[255, 64, 255, 0],
            },
        );
        assert_eq!(alloc.locs, locs);
        assert_eq!(alloc.callee_saved_used, [7, 64, 255]);
        assert_eq!(alloc.float_callee_saved_used, [64, 255]);
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
        let live = liveness(&func, &cfg, &order, &crate::select::Selection::default());
        let set = |vs: &[usize]| vs.iter().map(|&i| Value::new(i)).collect::<Vec<_>>();
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
        assert!(v1.intersects(v5) && v1.covers(v5.end()), "{v1:?} vs {v5:?}");
        // A dying operand ends before the result of its last use starts, so
        // they can share a register: v3 is last read by `v4 = iadd v3, v1`.
        assert!(interval(&ivs, 3).end() < interval(&ivs, 4).start());
    }

    /// v2 is defined in block0 and used in block3; block1 and block2,
    /// laid out in between, do not need it unless they flow to block3.
    /// Only block2 does, so v2 has a hole over block1, where v3 may take
    /// its register even with a single register to hand out.
    #[test]
    fn values_share_a_register_in_a_hole() {
        let func = function(
            "
            func @f(i64, i8) -> i64 {
            block0(v0: i64, v1: i8):
                v2 = iadd v0, v0
                brif v1, block2, block1
            block1:
                v3 = iconst.i64 7
                return v3
            block2:
                jump block3
            block3:
                return v2
            }",
        );
        let ivs = intervals(&func);
        let (v2, v3) = (interval(&ivs, 2), interval(&ivs, 3));
        assert!(
            v2.ranges.len() == 2 && !v2.intersects(v3),
            "{v2:?} vs {v3:?}"
        );
        assert!(v2.start() < v3.start() && v3.end() < v2.end());
        let pools = Pools {
            caller_saved: &[9],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let alloc = allocate(&func, &pools, |_| false, |_| false);
        assert_eq!(alloc.loc(Value::new(2)), Some(Loc::Reg(9)));
        assert_eq!(alloc.loc(Value::new(3)), Some(Loc::Reg(9)));
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
            float_caller_saved: &[19, 20],
            float_callee_saved: &[8, 9],
        };
        let is_call = |d: &InstData| {
            matches!(
                d,
                InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Intrinsic { .. }
            )
        };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "keel") {
                continue;
            }
            let module = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for func in &module.functions {
                for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                    let selection = crate::select::Selection::compute(func, target);
                    for tier in [AllocationTier::Fast, AllocationTier::Full] {
                        let alloc = allocate_tier(
                            func,
                            &pools,
                            &is_call,
                            &|_: &InstData| false,
                            &selection,
                            tier,
                        );
                        let cfg = ControlFlowGraph::compute(func);
                        let ivs = build_intervals(
                            func,
                            &cfg,
                            &alloc.order,
                            &is_call,
                            &|_| false,
                            &selection,
                        );
                        for (i, a) in ivs.iter().enumerate() {
                            let loc = alloc.locs[a.half][a.value.index()]
                                .expect("every interval is allocated");
                            assert!(
                                match loc {
                                    Loc::Reg(_) => !a.float,
                                    Loc::FReg(_) => a.float,
                                    Loc::Stack(_) | Loc::Remat(_) => true,
                                },
                                "{}: {} is in the wrong register class: {loc:?}",
                                func.name,
                                a.value
                            );
                            if a.crosses_call {
                                assert!(
                                    !matches!(loc, Loc::Reg(r) if pools.caller_saved.contains(&r))
                                        && !matches!(loc, Loc::FReg(r) if pools.float_caller_saved.contains(&r)),
                                    "{}: {} crosses a call in {loc:?}",
                                    func.name,
                                    a.value
                                );
                            }
                            for b in &ivs[i + 1..] {
                                let overlap = a.intersects(b);
                                assert!(
                                    !overlap || alloc.locs[b.half][b.value.index()] != Some(loc),
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
            }
        }
        assert!(checked > 0);
    }
}

#[cfg(test)]
mod differential_tests;

#[cfg(test)]
mod transfer_tests;

#[cfg(all(test, feature = "experimental-regalloc"))]
mod affinity_tests;

#[cfg(test)]
mod call_preserves_tests;
