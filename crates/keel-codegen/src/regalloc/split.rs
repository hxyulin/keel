//! Split spilled ranges into register segments without disturbing the global
//! assignments. Borrow only registers unused at both ends of a lowering
//! sequence, and return every dirty value to its existing home at block edges.
use super::*;
use keel_ir::Inst;

#[path = "split_call.rs"]
mod call;
#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;

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
    cache_spills(input, alloc, &intervals, true)
}

pub(super) fn allocate_adaptive(input: &AllocationInput<'_>) -> Allocation {
    profiling::scope!("split-spilled-ranges");
    backend_scope!(Allocation);
    if input
        .func
        .layout_insts()
        .any(|(_, i)| input.returns_twice(i))
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
    if spills.is_empty() {
        return alloc;
    }
    let allowed = repeated_spills(input, &alloc);
    if !allowed.iter().any(|&yes| yes) {
        return alloc;
    }
    let alloc = cache_spills_filtered(input, alloc, &intervals, true, Some(&allowed));
    super::region::repair(input, alloc, &intervals)
}

// A conservative opportunity gate, not a profitability proof. Count distinct
// selected reads within a block, excluding duplicate operands at one boundary.
fn repeated_spills(input: &AllocationInput<'_>, alloc: &Allocation) -> Vec<bool> {
    let f = input.func;
    let mut allowed = vec![false; f.num_values()];
    let mut seen = vec![None; f.num_values()];
    for &block in &alloc.order {
        for &inst in f.block_insts(block) {
            input.selection.for_each_operand(f, inst, |v| {
                if f.value_type(v).is_vector()
                    || !alloc
                        .locs
                        .iter()
                        .any(|half| matches!(half[v.index()], Some(Loc::Stack(_))))
                {
                    return;
                }
                if let Some((b, i)) = seen[v.index()]
                    && b == block
                    && i != inst
                {
                    allowed[v.index()] = true;
                }
                seen[v.index()] = Some((block, inst));
            });
        }
    }
    allowed
}

// Keep the Full scan separate from the cache walk so token tests can supply
// forced homes and lifetime holes without depending on scan victim choices.
// Only tests disable the preference; the public opt-in remains the Split tier.
fn cache_spills(
    input: &AllocationInput<'_>,
    alloc: Allocation,
    intervals: &[Interval],
    prefer_call: bool,
) -> Allocation {
    cache_spills_filtered(input, alloc, intervals, prefer_call, None)
}
fn cache_spills_filtered(
    input: &AllocationInput<'_>,
    alloc: Allocation,
    intervals: &[Interval],
    prefer_call: bool,
    allowed: Option<&[bool]>,
) -> Allocation {
    let func = input.func;
    // These definitions may disappear in target folding after frame layout.
    // Keep their baseline locations rather than introducing transfers for
    // operands that the emitter will encode as constants or stack addresses.
    let eligible = |value: Value| {
        allowed.is_none_or(|a| a[value.index()])
            && match func.value_def(value) {
                ValueDef::Result(inst, _) => !matches!(
                    func.inst_data(inst),
                    InstData::Iconst { .. } | InstData::StackAddr { .. }
                ),
                _ => true,
            }
    };
    if !intervals.iter().any(|interval| {
        matches!(
            alloc.locs[interval.half][interval.value.index()],
            Some(Loc::Stack(_))
        ) && eligible(interval.value)
    }) {
        return alloc;
    }
    let mut ends = vec![0; 2 * func.num_values()];
    let mut events = Vec::new();
    for interval in intervals {
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
    let mut call_preference = call::Preference::new(&cache, &events, prefer_call);
    let mut live = [0u64; 8];
    let mut cursor = 0;
    let mut slot = 0u32;
    let mut operands = Vec::new();
    let mut sources = Vec::new();
    let mut edge_values = Vec::new();
    for b in 0..cache.alloc.order.len() {
        let block = cache.alloc.order[b];
        slot += 1;
        call_preference.prepare(input, block, slot, &cache.ends);
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
                            position: match kind {
                                OperandKind::Use => OperandPosition::Early,
                                OperandKind::Def => OperandPosition::Late,
                            },
                            ty: func.value_type(value),
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
                    call_preference
                        .acquire(&mut cache, op.part, inst, read, &unavailable)
                        .or_else(|| cache.acquire(op.part, inst, read, &unavailable))
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
    fn occupy(&mut self, index: usize, part: ValuePart) -> usize {
        self.registers[index].part = Some(part);
        self.active += 1;
        self.resident[key(part)] = index as u32;
        index
    }
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
        Some(self.occupy(index, part))
    }
}

#[cfg(test)]
mod mixed_vectors {
    use super::*;
    #[test]
    fn mixed_vector_homes_and_scalar_tokens_survive_repairs() {
        let source = r#"decl @callback(i64) -> i64 cc(c)

func @across_call(ptr, i64) -> i64 cc(c) no_semantic_interposition {
block0(v0: ptr, v1: i64):
    v100 = load.i64x2 trap_ub v0
    v2 = load.i64 trap_ub v0
    v3 = iconst.i64 8
    v4 = ptradd v0, v3
    v5 = load.i64 trap_ub v4
    v6 = iconst.i64 16
    v7 = ptradd v0, v6
    v8 = load.i64 trap_ub v7
    v9 = iconst.i64 24
    v10 = ptradd v0, v9
    v11 = load.i64 trap_ub v10
    v12 = iconst.i64 32
    v13 = ptradd v0, v12
    v14 = load.i64 trap_ub v13
    v15 = iconst.i64 40
    v16 = ptradd v0, v15
    v17 = load.i64 trap_ub v16
    v18 = iconst.i64 48
    v19 = ptradd v0, v18
    v20 = load.i64 trap_ub v19
    v21 = iconst.i64 56
    v22 = ptradd v0, v21
    v23 = load.i64 trap_ub v22
    v24 = iconst.i64 64
    v25 = ptradd v0, v24
    v26 = load.i64 trap_ub v25
    v27 = iconst.i64 72
    v28 = ptradd v0, v27
    v29 = load.i64 trap_ub v28
    v30 = iconst.i64 80
    v31 = ptradd v0, v30
    v32 = load.i64 trap_ub v31
    v33 = iconst.i64 88
    v34 = ptradd v0, v33
    v35 = load.i64 trap_ub v34
    v36 = call @callback(v1)
    store v100, v0
    v37 = iadd v2, v5
    v38 = iadd v8, v37
    v39 = iadd v11, v38
    v40 = iadd v14, v39
    v41 = iadd v17, v40
    v42 = iadd v20, v41
    v43 = iadd v23, v42
    v44 = iadd v26, v43
    v45 = iadd v29, v44
    v46 = iadd v32, v45
    v47 = iadd v35, v46
    v48 = iadd v36, v47
    v49 = iadd v48, v2
    return v49
}
"#;
        let module = keel_ir::parse(source).unwrap();
        let func = &module.functions[0];
        let selection = crate::select::Selection::default();
        let pools = Pools {
            caller_saved: &[6, 7, 8],
            callee_saved: &[12],
            float_caller_saved: &[0, 1],
            float_callee_saved: &[8],
        };
        let input = AllocationInput {
            func,
            pools: &pools,
            selection: &selection,
            is_call: &|d| matches!(d, InstData::Call { .. }),
            returns_twice: &|_| false,
        };
        let full = AllocationTier::Full.allocate(&input);
        let repaired = allocate_adaptive(&input);
        assert!(!repaired.edits.is_empty());
        repaired.validate(&input).unwrap();
        repaired
            .edits
            .for_each_binding(|part, _| assert!(!func.value_type(part.value).is_vector()));
        for value in 0..func.num_values() {
            if func.value_type(Value::new(value)).is_vector() {
                assert_eq!(full.locs[0][value], repaired.locs[0][value]);
            }
        }
        for seed in 0..16 {
            super::super::transfer_tests::check(&input, &repaired, seed);
        }
    }
}

#[cfg(test)]
mod eligibility_tests {
    use super::*;
    #[test]
    fn repeated_reads_require_distinct_instructions_in_one_block() {
        for (body, expected) in [
            ("v2 = iadd v0,v0\n return v2", false),
            ("v2 = iadd v0,v1\n v3 = isub v0,v2\n return v3", true),
            ("v2 = iadd v0,v1\n jump block1\nblock1:\n return v0", false),
        ] {
            let module = keel_ir::parse(&format!(
                "func @f(i64,i64) -> i64 {{\nblock0(v0:i64,v1:i64):\n{body}\n}}"
            ))
            .unwrap();
            let f = &module.functions[0];
            let selection = crate::select::Selection::default();
            let pools = Pools {
                caller_saved: &[],
                callee_saved: &[],
                float_caller_saved: &[],
                float_callee_saved: &[],
            };
            let input = AllocationInput {
                func: f,
                selection: &selection,
                pools: &pools,
                is_call: &|_| false,
                returns_twice: &|_| false,
            };
            let full = AllocationTier::Full.allocate(&input);
            let value = f.block_params(f.entry_block().unwrap())[0];
            assert_eq!(
                repeated_spills(&input, &full)[value.index()],
                expected,
                "{body}"
            );
        }
    }
}
