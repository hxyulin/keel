//! A local preference for clean spill reads, not another scan or range splitter.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct NextUse {
    read: u32,
    calls: usize,
    barriers: usize,
    unconstrained: bool,
}

struct Candidate {
    cache_index: usize,
    // Sorted enter points, including inactive global owners returning from holes.
    starts: Vec<u32>,
    cursor: usize,
}

#[derive(Default)]
pub(super) struct Preference {
    candidates: Vec<Candidate>,
    next: HashMap<Value, NextUse>,
    // Only repeated, eligible selected uses with a proved single-call window.
    windows: HashMap<(Inst, Value), u32>,
}

impl Preference {
    pub(super) fn new(cache: &Cache<'_>, events: &[(u32, bool, Loc)], enabled: bool) -> Self {
        let mut result = Self::default();
        if !enabled {
            return result;
        }
        let mut indices = [usize::MAX; 256];
        // The cache uses target-pool order. Never add a save to enable a window.
        for (cache_index, register) in cache.registers.iter().enumerate() {
            let Loc::Reg(r) = register.loc else { continue };
            if cache.input.pools.callee_saved.contains(&r)
                && !cache.input.pools.caller_saved.contains(&r)
                && cache.alloc.callee_saved_used.contains(&r)
                && cache.input.permits(register.loc)
            {
                indices[r as usize] = result.candidates.len();
                result.candidates.push(Candidate {
                    cache_index,
                    starts: Vec::new(),
                    cursor: 0,
                });
            }
        }
        for &(point, enter, loc) in events {
            if enter && let Loc::Reg(r) = loc {
                let index = indices[r as usize];
                if index != usize::MAX {
                    result.candidates[index].starts.push(point);
                }
            }
        }
        result
    }

    pub(super) fn prepare(
        &mut self,
        input: &AllocationInput<'_>,
        block: Block,
        first_slot: u32,
        ends: &[u32],
    ) {
        // A nonempty HashMap::clear touches retained capacity. Fresh tables
        // charge each old allocation's drop once, to the block that grew it;
        // many tiny later blocks must not repeatedly reset a wide block's table.
        self.windows = HashMap::new();
        self.next = HashMap::new();
        if self.candidates.is_empty() {
            return;
        }
        let func = input.func;
        // A preference can displace a DIFFERENT value's later constrained
        // resident. Preserve every original cache choice in a constrained block,
        // including Reuse definitions and their otherwise-Any inputs.
        let mut has_constraints = false;
        for &inst in func.block_insts(block) {
            input.for_each_operand(inst, |op| {
                has_constraints |= op.constraint != Constraint::Any
            });
        }
        if has_constraints {
            return;
        }
        let mut calls = 0;
        let mut barriers = 0;
        for (offset, &inst) in func.block_insts(block).iter().enumerate().rev() {
            let read = 2 * (first_slot + offset as u32);
            let data = func.inst_data(inst);
            let covered = input.selection.covered(inst);
            let mut constrained = false;
            input.for_each_operand(inst, |op| constrained |= op.constraint != Constraint::Any);
            let ordinary = !covered
                && !input.returns_twice(inst)
                && (match data {
                    InstData::Call { func: callee, .. } => !func.callee(*callee).sig.returns_twice,
                    InstData::CallIndirect { sig, .. } => !func.signature(*sig).returns_twice,
                    _ => false,
                })
                && (input.is_call)(data);
            // Do not infer preservation for inline asm, helper-like lowering or
            // unknown clobbers. Constraints are a barrier, including a Reuse on
            // another result that refers to this input. Terminators are never
            // candidate endpoints: edge-only uses keep their canonical homes.
            let barrier = constrained
                || data.is_terminator()
                || input.returns_twice(inst)
                || ((input.is_call)(data) && !ordinary)
                || matches!(
                    data,
                    InstData::InlineAsm { .. }
                        | InstData::Intrinsic { .. }
                        | InstData::VaArg { .. }
                        | InstData::LandingPad { .. }
                )
                || (!ordinary
                    && matches!(data, InstData::Call { .. } | InstData::CallIndirect { .. }));
            let later_calls = calls;
            let later_barriers = barriers;
            calls += usize::from(ordinary);
            barriers += usize::from(barrier);
            // A definition must not let a future use become a preceding use's
            // next use, even in direct Function/alias-state test inputs.
            for value in func.inst_results(inst) {
                self.next.remove(value);
            }
            if covered {
                continue;
            }
            input.selection.for_each_operand(func, inst, |value| {
                if func.resolve(value) != value
                    || !matches!(func.value_type(value), Type::Ptr | Type::I64)
                    || ends[key(ValuePart::low(value))] == 0
                {
                    return;
                }
                if let Some(next) = self.next.get(&value) {
                    if next.read == read {
                        return; // Repeated reads at this one boundary are one use.
                    }
                    // Counts stored at the next read include that instruction's
                    // call; later_calls excludes the current read's call. Thus
                    // exactly one call is strictly BETWEEN the selected uses.
                    if !barrier
                        && next.unconstrained
                        && later_calls == next.calls + 1
                        && later_barriers == next.barriers
                    {
                        self.windows.insert((inst, value), next.read + 1);
                    }
                }
                self.next.insert(
                    value,
                    NextUse {
                        read,
                        calls,
                        barriers,
                        unconstrained: !barrier,
                    },
                );
            });
        }
    }

    #[cfg(test)]
    pub(super) fn metadata_capacities(&self) -> [usize; 2] {
        [self.next.capacity(), self.windows.capacity()]
    }

    pub(super) fn acquire(
        &mut self,
        cache: &mut Cache<'_>,
        part: ValuePart,
        inst: Inst,
        read: u32,
        unavailable: &[u64; 8],
    ) -> Option<usize> {
        if part.high || cache.resident[key(part)] != u32::MAX {
            return None;
        }
        let &until = self.windows.get(&(inst, part.value))?;
        for candidate in &mut self.candidates {
            // Every cursor is monotone across the RPO walk. Total advancement
            // is O(global enter events), not a forward scan for each spill use.
            while candidate.cursor < candidate.starts.len()
                && candidate.starts[candidate.cursor] <= read
            {
                candidate.cursor += 1;
            }
            let register = cache.registers[candidate.cache_index];
            if register.part.is_none()
                && !register.dirty
                && !register.pinned
                && !blocked(unavailable, register.loc)
                && candidate
                    .starts
                    .get(candidate.cursor)
                    .is_none_or(|&point| point > until)
            {
                return Some(cache.occupy(candidate.cache_index, part));
            }
        }
        None
    }
}
