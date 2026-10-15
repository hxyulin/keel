//! Split spilled ranges into register segments without disturbing the global
//! assignments. Borrow only registers unused at both ends of a lowering
//! sequence, and return every dirty value to its existing home at block edges.
use super::*;
use keel_ir::Inst;

#[derive(Clone, Copy)]
struct Register {
    loc: Loc,
    part: Option<ValuePart>,
    dirty: bool,
    pinned: bool,
}
struct Cache<'a> {
    input: &'a AllocationInput<'a>,
    alloc: Allocation,
    registers: Vec<Register>,
    resident: Vec<u32>,
    ends: Vec<u32>,
    active: usize,
}
fn key(part: ValuePart) -> usize {
    2 * part.value.index() + usize::from(part.high)
}
fn physical(loc: Loc) -> usize {
    match loc {
        Loc::Reg(r) => r as usize,
        Loc::FReg(r) => 256 + r as usize,
        Loc::Stack(_) | Loc::Remat(_) => unreachable!(),
    }
}
fn bitset(bits: &mut [u64; 8], loc: Loc, live: bool) {
    let i = physical(loc);
    if live {
        bits[i / 64] |= 1 << (i % 64)
    } else {
        bits[i / 64] &= !(1 << (i % 64))
    }
}
fn blocked(bits: &[u64; 8], loc: Loc) -> bool {
    let i = physical(loc);
    bits[i / 64] & (1 << (i % 64)) != 0
}

pub(super) fn allocate(input: &AllocationInput<'_>) -> Allocation {
    profiling::scope!("split-spilled-ranges");
    backend_scope!(Allocation);
    if input
        .func
        .layout_insts()
        .any(|(_, i)| input.returns_twice(i))
        // Sparse cache copies currently move scalar halves only. Keep vector
        // functions on the baseline scan until these experiments model widths.
        || (0..input.func.num_values())
            .any(|i| input.func.value_type(Value::new(i)).is_vector())
    {
        return allocate_tier_inner(
            input.func,
            input.pools,
            input.is_call,
            input.returns_twice,
            input.selection,
            AllocationTier::Full,
        );
    }
    let func = input.func;
    let cfg = ControlFlowGraph::compute(func);
    let order = cfg.reverse_postorder(func);
    let intervals = build_intervals(
        func,
        &cfg,
        &order,
        input.is_call,
        input.returns_twice,
        input.selection,
    );
    let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
    let mut spills = linear_scan(&intervals, input.pools, &mut locs, &input.selection.hints);
    let spill_slots = assign_spill_slots(&mut spills, &mut locs);
    let alloc = baseline_allocation(order, locs, spill_slots, input.pools);
    if spills.is_empty() {
        return alloc;
    }
    // These definitions may disappear in target folding after frame layout.
    // Keep their baseline locations rather than introducing transfers for
    // operands that the emitter will encode as constants or stack addresses.
    let eligible = |value| match func.value_def(value) {
        ValueDef::Result(inst, _) => !matches!(
            func.inst_data(inst),
            InstData::Iconst { .. } | InstData::StackAddr { .. }
        ),
        _ => true,
    };
    if !spills.iter().any(|interval| eligible(interval.value)) {
        return alloc;
    }
    let mut ends = vec![0; 2 * func.num_values()];
    let mut events = Vec::new();
    for interval in &intervals {
        let part = ValuePart {
            value: interval.value,
            high: interval.half == 1,
        };
        match alloc.locs[interval.half][interval.value.index()] {
            Some(Loc::Stack(_)) if eligible(part.value) => ends[key(part)] = interval.end(),
            Some(Loc::Stack(_)) => {}
            Some(loc) => {
                for &(from, to) in &interval.ranges {
                    events.push((from, true, loc));
                    events.push((to + 1, false, loc));
                }
            }
            None => {}
        }
    }
    // At a shared endpoint remove the expired owner before installing its
    // successor. The global allocator guarantees that register ranges do not overlap.
    events.sort_unstable_by_key(|&(point, enter, _)| (point, enter));
    let mut registers = Vec::new();
    for (pool, float) in [
        (input.pools.caller_saved, false),
        (input.pools.callee_saved, false),
        (input.pools.float_caller_saved, true),
        (input.pools.float_callee_saved, true),
    ] {
        for &r in pool {
            registers.push(Register {
                loc: if float { Loc::FReg(r) } else { Loc::Reg(r) },
                part: None,
                dirty: false,
                pinned: false,
            });
        }
    }
    let mut cache = Cache {
        input,
        alloc,
        registers,
        resident: vec![u32::MAX; 2 * func.num_values()],
        ends,
        active: 0,
    };
    let mut live = [0u64; 8];
    let mut cursor = 0;
    let mut slot = 0u32;
    let mut operands = Vec::new();
    let mut sources = Vec::new();
    let mut edge_values = Vec::new();
    for b in 0..cache.alloc.order.len() {
        let block = cache.alloc.order[b];
        slot += 1;
        for &inst in func.block_insts(block) {
            let read = 2 * slot;
            let write = read + 1;
            slot += 1;
            if input.selection.covered(inst) {
                continue;
            }
            operands.clear();
            let mut operand = |value: Value, kind| {
                for high in [false, true] {
                    let part = ValuePart { value, high };
                    if cache.ends[key(part)] == 0 {
                        continue;
                    }
                    if !operands
                        .iter()
                        .any(|old: &Operand| old.part == part && old.kind == kind)
                    {
                        operands.push(Operand {
                            part,
                            kind,
                            float: func.value_type(value).is_float(),
                            constraint: input.selection.constraint(inst, part, kind),
                        });
                    }
                }
            };
            input
                .selection
                .for_each_operand(func, inst, |value| operand(value, OperandKind::Use));
            for &value in func.inst_results(inst) {
                operand(value, OperandKind::Def);
            }
            // Most instructions do not touch a spilled range. Delay the event
            // walk while no segment is resident, and avoid class/constraint
            // enumeration and register scans on this overwhelmingly common path.
            if operands.is_empty() && cache.active == 0 {
                continue;
            }
            while cursor < events.len() && events[cursor].0 <= read {
                let (_, enter, loc) = events[cursor];
                bitset(&mut live, loc, enter);
                cursor += 1;
            }
            let mut unavailable = live;
            while cursor < events.len() && events[cursor].0 <= write {
                let (_, enter, loc) = events[cursor];
                bitset(&mut live, loc, enter);
                if enter {
                    bitset(&mut unavailable, loc, true);
                }
                cursor += 1;
            }
            if input.selection.covered(inst) {
                continue;
            }
            input.for_each_clobber(inst, |loc| bitset(&mut unavailable, loc, true));
            sources.clear();
            for i in 0..cache.registers.len() {
                cache.registers[i].pinned = false;
                if let Some(part) = cache.registers[i].part {
                    if blocked(&unavailable, cache.registers[i].loc) {
                        sources.push((part, cache.registers[i].loc));
                        cache.evict(i, inst, read);
                    } else if cache.ends[key(part)] < read {
                        cache.forget(i);
                    }
                }
            }
            edge_values.clear();
            let dests = func.inst_data(inst).branch_dests();
            for dest in dests {
                edge_values.extend(dest.args.iter().map(|&v| func.resolve(v)));
            }
            if !dests.is_empty() {
                // Preserve stack-to-stack identity copies on phi edges. Every
                // existing dirty segment reaches its stable home before the
                // terminator; edge-only reads can then use those homes directly.
                for i in 0..cache.registers.len() {
                    let r = cache.registers[i];
                    if r.dirty
                        && let Some(part) = r.part
                        && cache.ends[key(part)] >= read
                    {
                        let home = cache.home(part);
                        cache
                            .alloc
                            .edits
                            .instruction_mut(inst)
                            .before
                            .push((home, r.loc));
                        cache.registers[i].dirty = false;
                    }
                }
            }
            operands.retain(|op| {
                !(op.kind == OperandKind::Use && edge_values.contains(&op.part.value))
            });
            for op in &operands {
                if op.kind == OperandKind::Use {
                    let i = cache.resident[key(op.part)];
                    if i != u32::MAX {
                        cache.registers[i as usize].pinned = true;
                    }
                }
            }
            for op in &operands {
                if op.kind != OperandKind::Use {
                    continue;
                }
                // A global result may claim this register at the write point.
                // The cached input is still readable before that write. Forward
                // its old location instead of reloading a home saved by the
                // same parallel-copy batch (which would read the old home).
                if let Some(&(_, loc)) = sources.iter().find(|&&(part, _)| part == op.part) {
                    cache
                        .alloc
                        .edits
                        .instruction_mut(inst)
                        .uses
                        .push((op.part, loc));
                    continue;
                }
                let current = cache.resident[key(op.part)];
                let index = if current != u32::MAX {
                    Some(current as usize)
                } else {
                    cache.acquire(op.part, inst, read, &unavailable)
                };
                if let Some(i) = index {
                    if current == u32::MAX {
                        let home = cache.home(op.part);
                        cache
                            .alloc
                            .edits
                            .instruction_mut(inst)
                            .before
                            .push((cache.registers[i].loc, home));
                    }
                    cache.registers[i].pinned = true;
                    cache
                        .alloc
                        .edits
                        .instruction_mut(inst)
                        .uses
                        .push((op.part, cache.registers[i].loc));
                }
            }
            for r in &mut cache.registers {
                r.pinned = false;
            }
            for op in &operands {
                if op.kind != OperandKind::Def {
                    continue;
                }
                if let Some(i) = cache.acquire(op.part, inst, read, &unavailable) {
                    cache.registers[i].dirty = true;
                    cache.registers[i].pinned = true;
                    cache
                        .alloc
                        .edits
                        .instruction_mut(inst)
                        .defs
                        .push((op.part, cache.registers[i].loc));
                }
            }
            if func.inst_data(inst).is_terminator() {
                for i in 0..cache.registers.len() {
                    let r = cache.registers[i];
                    if r.dirty
                        && let Some(part) = r.part
                        && cache.ends[key(part)] > read
                    {
                        let home = cache.home(part);
                        if operands
                            .iter()
                            .any(|op| op.kind == OperandKind::Def && op.part == part)
                        {
                            cache.alloc.edits.edge_mut(inst, 0).push((home, r.loc));
                        } else {
                            cache
                                .alloc
                                .edits
                                .instruction_mut(inst)
                                .before
                                .push((home, r.loc));
                        }
                    }
                    cache.forget(i);
                }
            }
        }
    }
    cache.alloc
}
impl Cache<'_> {
    fn home(&self, part: ValuePart) -> Loc {
        self.alloc.locs[usize::from(part.high)][part.value.index()].unwrap()
    }
    fn forget(&mut self, index: usize) {
        if let Some(part) = self.registers[index].part.take() {
            self.resident[key(part)] = u32::MAX;
            self.active -= 1;
        }
        self.registers[index].dirty = false;
    }
    fn evict(&mut self, index: usize, inst: Inst, point: u32) {
        let r = self.registers[index];
        if r.dirty
            && let Some(part) = r.part
            && self.ends[key(part)] >= point
        {
            let home = self.home(part);
            self.alloc
                .edits
                .instruction_mut(inst)
                .before
                .push((home, r.loc));
        }
        self.forget(index);
    }
    fn acquire(
        &mut self,
        part: ValuePart,
        inst: Inst,
        point: u32,
        unavailable: &[u64; 8],
    ) -> Option<usize> {
        let float = self.input.func.value_type(part.value).is_float();
        let index = self
            .registers
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                !r.pinned && matches!(r.loc, Loc::FReg(_)) == float && !blocked(unavailable, r.loc)
            })
            .min_by_key(|(_, r)| {
                (
                    r.part.is_some(),
                    r.dirty,
                    std::cmp::Reverse(r.part.map_or(u32::MAX, |p| self.ends[key(p)])),
                )
            })
            .map(|(i, _)| i)?;
        self.evict(index, inst, point);
        self.registers[index].part = Some(part);
        self.active += 1;
        self.resident[key(part)] = index as u32;
        Some(index)
    }
}
