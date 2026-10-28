//! Block-local register caching. No global liveness fixed point or live-range
//! construction: cross-block values have stable stack homes, local homes are
//! allocated only on eviction, and dead local slots are recycled after reads.
use super::*;
use keel_ir::Inst;

#[derive(Clone, Copy)]
struct Register {
    loc: Loc,
    part: Option<ValuePart>,
    dirty: bool,
    pinned: bool,
    saved: bool,
}
struct Cache<'a> {
    input: &'a AllocationInput<'a>,
    alloc: Allocation,
    registers: Vec<Register>,
    resident: Vec<u32>,
    cross: Vec<bool>,
    used: Vec<bool>,
    last: Vec<usize>,
    free_slots: Vec<u32>,
}
fn key(part: ValuePart) -> usize {
    2 * part.value.index() + usize::from(part.high)
}

pub(super) fn allocate(input: &AllocationInput<'_>) -> Allocation {
    profiling::scope!("block-local-allocation");
    backend_scope!(Allocation);
    // A second return can observe a value after its first lifetime ended.
    // Keep the established function-wide pinning rule for these rare functions.
    let has_vectors =
        (0..input.func.num_values()).any(|i| input.func.value_type(Value::new(i)).is_vector());
    if input
        .func
        .layout_insts()
        .any(|(_, i)| input.returns_twice(i))
        // Sparse cache copies currently move scalar halves only. Keep vector
        // functions on the baseline scan until these experiments model widths.
        || has_vectors
    {
        return allocate_tier_inner(
            input.func,
            input.pools,
            input.is_call,
            input.returns_twice,
            input.selection,
            if has_vectors {
                AllocationTier::Full
            } else {
                AllocationTier::Fast
            },
        );
    }
    let func = input.func;
    let cfg = ControlFlowGraph::compute(func);
    let order = cfg.reverse_postorder(func);
    let mut cross = vec![false; func.num_values()];
    let mut used = vec![false; func.num_values()];
    for &block in &order {
        for &inst in func.block_insts(block) {
            input.selection.for_each_operand(func, inst, |value| {
                used[value.index()] = true;
                let owner = match func.value_def(value) {
                    ValueDef::Param(b, _) => b,
                    ValueDef::Result(i, _) => func.inst_block(i),
                    ValueDef::Alias(_) => unreachable!("selected operands are resolved"),
                };
                cross[value.index()] |= owner != block;
            });
        }
    }
    let mut registers = Vec::new();
    for (pool, float, saved) in [
        (input.pools.caller_saved, false, false),
        (input.pools.callee_saved, false, true),
        (input.pools.float_caller_saved, true, false),
        (input.pools.float_callee_saved, true, true),
    ] {
        for &r in pool {
            registers.push(Register {
                loc: if float { Loc::FReg(r) } else { Loc::Reg(r) },
                part: None,
                dirty: false,
                pinned: false,
                saved,
            });
        }
    }
    let mut cache = Cache {
        input,
        alloc: Allocation {
            order,
            locs: [vec![None; func.num_values()], vec![None; func.num_values()]],
            edits: AllocationEdits::default(),
            spill_slots: 0,
            callee_saved_used: vec![],
            float_callee_saved_used: vec![],
        },
        registers,
        resident: vec![u32::MAX; 2 * func.num_values()],
        cross,
        used,
        last: vec![0; func.num_values()],
        free_slots: vec![],
    };
    // These homes survive every block and are never recycled. Unlike local
    // slots, their contents must remain valid on paths not visited by the scan.
    for i in 0..func.num_values() {
        if cache.cross[i] {
            for high in [false, true] {
                if high && func.value_type(Value::new(i)) != Type::I128 {
                    continue;
                }
                cache.home(ValuePart {
                    value: Value::new(i),
                    high,
                });
            }
        }
    }
    for b in 0..cache.alloc.order.len() {
        cache.block(cache.alloc.order[b]);
    }
    cache.alloc
}
impl Cache<'_> {
    fn home(&mut self, part: ValuePart) -> Loc {
        let loc = &mut self.alloc.locs[usize::from(part.high)][part.value.index()];
        *loc.get_or_insert_with(|| {
            let slot = self.free_slots.pop().unwrap_or_else(|| {
                let slot = self.alloc.spill_slots;
                self.alloc.spill_slots += 1;
                slot
            });
            Loc::Stack(slot)
        })
    }
    fn needed(&self, part: ValuePart, position: usize) -> bool {
        self.cross[part.value.index()] || self.last[part.value.index()] > position
    }
    fn forget(&mut self, index: usize) {
        if let Some(part) = self.registers[index].part.take() {
            self.resident[key(part)] = u32::MAX;
        }
        self.registers[index].dirty = false;
    }
    fn save(&mut self, index: usize, inst: Inst) {
        let r = self.registers[index];
        if r.dirty {
            let home = self.home(r.part.unwrap());
            self.alloc
                .edits
                .instruction_mut(inst)
                .before
                .push((home, r.loc));
            self.registers[index].dirty = false;
        }
    }
    fn acquire(
        &mut self,
        part: ValuePart,
        inst: Inst,
        position: usize,
        saved: bool,
    ) -> Option<usize> {
        let float = self.input.func.value_type(part.value).is_float();
        let class = |r: &Register| matches!(r.loc, Loc::FReg(_)) == float && !r.pinned;
        let index = self
            .registers
            .iter()
            .enumerate()
            .filter(|(_, r)| class(r))
            .min_by_key(|(_, r)| {
                (
                    r.part.is_some(),
                    r.saved != saved,
                    r.dirty,
                    std::cmp::Reverse(r.part.map_or(usize::MAX, |p| self.last[p.value.index()])),
                )
            })
            .map(|(i, _)| i)?;
        if let Some(old) = self.registers[index].part {
            if self.needed(old, position) {
                self.save(index, inst);
            }
            self.forget(index);
        }
        self.registers[index].part = Some(part);
        self.resident[key(part)] = index as u32;
        Some(index)
    }
    fn block(&mut self, block: Block) {
        let func = self.input.func;
        let insts = func.block_insts(block);
        let mut calls = Vec::new();
        for (position, &inst) in insts.iter().enumerate() {
            self.input
                .selection
                .for_each_operand(func, inst, |v| self.last[v.index()] = position + 1);
            if (self.input.is_call)(func.inst_data(inst)) {
                calls.push(position + 1);
            }
        }
        for i in 0..self.registers.len() {
            self.forget(i);
            self.registers[i].pinned = false;
        }
        // Parameters enter in per-block register locations when possible. Edge
        // copies reconcile these independently chosen locations, including cycles.
        for &value in func.block_params(block) {
            if !self.used[value.index()] || func.resolve(value) != value {
                continue;
            }
            for high in [false, true] {
                if high && func.value_type(value) != Type::I128 {
                    continue;
                }
                let part = ValuePart { value, high };
                let float = func.value_type(value).is_float();
                let index = self
                    .registers
                    .iter()
                    .position(|r| r.part.is_none() && matches!(r.loc, Loc::FReg(_)) == float);
                let loc = if let Some(i) = index {
                    self.registers[i].part = Some(part);
                    self.registers[i].dirty = true;
                    self.resident[key(part)] = i as u32;
                    self.registers[i].loc
                } else {
                    self.home(part)
                };
                self.alloc.edits.set_param(block, part, loc);
            }
        }
        let mut operands = Vec::new();
        let mut retire = Vec::new();
        let mut call_cursor = 0;
        for (ordinal, &inst) in insts.iter().enumerate() {
            if self.input.selection.covered(inst) {
                continue;
            }
            let position = ordinal + 1;
            operands.clear();
            self.input.for_each_operand(inst, |op| {
                if !operands
                    .iter()
                    .any(|old: &Operand| old.part == op.part && old.kind == op.kind)
                {
                    operands.push(op);
                }
            });
            for r in &mut self.registers {
                r.pinned = false;
            }
            // Pin every resident input before choosing victims for missing ones.
            for op in &operands {
                if op.kind == OperandKind::Use {
                    let i = self.resident[key(op.part)];
                    if i != u32::MAX {
                        self.registers[i as usize].pinned = true;
                    }
                }
            }
            while calls.get(call_cursor).is_some_and(|&c| c <= position) {
                call_cursor += 1;
            }
            let next_call = calls.get(call_cursor).copied();
            for op in &operands {
                if op.kind != OperandKind::Use {
                    continue;
                }
                let part = op.part;
                let index = self.resident[key(part)];
                let index = if index != u32::MAX {
                    Some(index as usize)
                } else {
                    let saved = next_call.is_some_and(|c| c < self.last[part.value.index()]);
                    let index = self.acquire(part, inst, position, saved);
                    let home = self.home(part);
                    if let Some(i) = index {
                        self.alloc
                            .edits
                            .instruction_mut(inst)
                            .before
                            .push((self.registers[i].loc, home));
                        self.registers[i].dirty = false;
                    }
                    index
                };
                let loc = if let Some(i) = index {
                    self.registers[i].pinned = true;
                    self.registers[i].loc
                } else {
                    self.home(part)
                };
                self.alloc
                    .edits
                    .instruction_mut(inst)
                    .uses
                    .push((part, loc));
            }
            // Caller-saved values that survive the sequence need their homes.
            // Reloaded inputs are clean, so every saved source refers to the
            // pre-instruction state even though the copies execute in parallel.
            if (self.input.is_call)(func.inst_data(inst)) {
                for i in 0..self.registers.len() {
                    if !self.registers[i].saved
                        && let Some(part) = self.registers[i].part
                    {
                        if self.needed(part, position) {
                            self.save(i, inst);
                        }
                        self.forget(i);
                    }
                }
            }
            if !func.inst_data(inst).branch_dests().is_empty() {
                for i in 0..self.registers.len() {
                    if let Some(part) = self.registers[i].part
                        && self.cross[part.value.index()]
                    {
                        self.save(i, inst);
                    }
                }
            }
            // Inputs are read before results are written. Their registers can
            // now be reused, while any surviving dirty input is saved first.
            for r in &mut self.registers {
                r.pinned = false;
            }
            for op in &operands {
                if op.kind != OperandKind::Def || !self.used[op.part.value.index()] {
                    continue;
                }
                let saved = next_call.is_some_and(|c| c < self.last[op.part.value.index()]);
                let index = self.acquire(op.part, inst, position, saved);
                let loc = if let Some(i) = index {
                    self.registers[i].dirty = true;
                    self.registers[i].pinned = true;
                    self.registers[i].loc
                } else {
                    self.home(op.part)
                };
                self.alloc
                    .edits
                    .instruction_mut(inst)
                    .defs
                    .push((op.part, loc));
                if func.inst_data(inst).is_terminator() && self.cross[op.part.value.index()] {
                    let home = self.home(op.part);
                    self.alloc.edits.edge_mut(inst, 0).push((home, loc));
                }
            }
            retire.clear();
            for op in &operands {
                if self.last[op.part.value.index()] <= position
                    && !self.cross[op.part.value.index()]
                {
                    let i = self.resident[key(op.part)];
                    if i != u32::MAX {
                        self.forget(i as usize);
                    }
                    if let Some(Loc::Stack(slot)) =
                        self.alloc.locs[usize::from(op.part.high)][op.part.value.index()]
                        && !retire.contains(&slot)
                    {
                        retire.push(slot);
                    }
                }
            }
            // Recycle only after this instruction's reads and all its before
            // copies: an input may still be read from the retiring stack slot.
            self.free_slots.extend(retire.iter().copied());
        }
    }
}
