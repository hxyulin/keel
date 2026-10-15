use keel_codegen::regalloc::{
    Allocation, AllocationInput, AllocationTier, Loc, OperandKind, RegisterAllocator, ValuePart,
};
use keel_ir::EntityRef;

/// Intentionally slow: stable stack homes, fresh register locations at every
/// instruction boundary. This tests the location/transfer contract without
/// depending on a particular allocator's heuristics.
pub struct BoundaryAllocator;
impl RegisterAllocator for BoundaryAllocator {
    fn name(&self) -> &'static str {
        "test-boundary-locations"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        let mut slot = 0;
        for loc in alloc.locs.iter_mut().flatten() {
            if loc.is_some() {
                *loc = Some(Loc::Stack(slot));
                slot += 1;
            }
        }
        alloc.spill_slots = slot;
        alloc.callee_saved_used.clear();
        alloc.float_callee_saved_used.clear();
        for (_, inst) in input.func.layout_insts() {
            let mut regs = Vec::new();
            let mut float_regs = Vec::new();
            input.for_each_operand(inst, |op| {
                let home = alloc.locs[usize::from(op.part.high)][op.part.value.index()];
                let Some(home) = home else { return };
                let edit = alloc.edits.instruction_mut(inst);
                let bindings = match op.kind {
                    OperandKind::Use => &mut edit.uses,
                    OperandKind::Def => &mut edit.defs,
                };
                if bindings.iter().any(|&(p, _)| p == op.part) {
                    return;
                }
                let (used, caller, callee, make): (_, _, _, fn(u8) -> Loc) = if op.float {
                    (
                        &mut float_regs,
                        input.pools.float_caller_saved,
                        input.pools.float_callee_saved,
                        Loc::FReg,
                    )
                } else {
                    (
                        &mut regs,
                        input.pools.caller_saved,
                        input.pools.callee_saved,
                        Loc::Reg,
                    )
                };
                // Alternate direction to exercise callee-save discovery as well.
                let available: Vec<_> = caller.iter().chain(callee).copied().collect();
                let index = (used.len() + inst.index()) % available.len().max(1);
                let reg = available
                    .iter()
                    .cycle()
                    .skip(index)
                    .take(available.len())
                    .find(|r| !used.contains(*r))
                    .copied();
                if let Some(reg) = reg {
                    used.push(reg);
                    let loc = make(reg);
                    bindings.push((op.part, loc));
                    match op.kind {
                        OperandKind::Use => edit.before.push((loc, home)),
                        OperandKind::Def => edit.after.push((home, loc)),
                    }
                }
            });
            // An invoke returns on the normal edge; the exceptional edge must
            // never run transfers of results the call did not produce.
            if input.func.inst_data(inst).is_terminator() {
                let after = std::mem::take(&mut alloc.edits.instruction_mut(inst).after);
                if !after.is_empty() {
                    alloc.edits.edge_mut(inst, 0).extend(after);
                }
            }
        }
        alloc
    }
}

/// Save a live-through value on an edge at the same time a block parameter
/// overwrites its old home. Both edges target the same block. Treating copies
/// as separate batches, or identifying an edge by target alone, loses data.
pub struct EdgeAllocator;
impl RegisterAllocator for EdgeAllocator {
    fn name(&self) -> &'static str {
        "test-edge-locations"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        if input.func.name.starts_with("empty_") {
            let mut alloc = AllocationTier::Full.allocate(input);
            let mut slot = 0;
            for loc in alloc.locs.iter_mut().flatten() {
                if loc.is_some() {
                    *loc = Some(Loc::Stack(slot));
                    slot += 1;
                }
            }
            alloc.spill_slots = slot;
            let entry = alloc.order[0];
            let params = input.func.block_params(entry);
            let (a, b) = (params[1], params[2]);
            let (aloc, bloc) = (alloc.loc(a).unwrap(), alloc.loc(b).unwrap());
            let term = *input.func.block_insts(entry).last().unwrap();
            // Every edge requires a swap, including edges without block args.
            // There are five separate switch edges, four sharing a target.
            for edge in 0..input.func.inst_data(term).branch_dests().len() {
                alloc
                    .edits
                    .edge_mut(term, edge)
                    .extend([(aloc, bloc), (bloc, aloc)]);
            }
            for &block in &alloc.order.clone()[1..] {
                for &inst in input.func.block_insts(block) {
                    input.for_each_operand(inst, |op| {
                        if op.kind == OperandKind::Use {
                            let loc = if op.part.value == a {
                                bloc
                            } else if op.part.value == b {
                                aloc
                            } else {
                                return;
                            };
                            alloc.edits.instruction_mut(inst).uses.push((op.part, loc));
                        }
                    });
                }
            }
            return alloc;
        }
        if input.func.name != "edge_locations" {
            return BoundaryAllocator.allocate(input);
        }
        let mut alloc = AllocationTier::Full.allocate(input);
        let mut slot = 0;
        for loc in alloc.locs.iter_mut().flatten() {
            if loc.is_some() {
                *loc = Some(Loc::Stack(slot));
                slot += 1;
            }
        }
        let blocks: Vec<_> = alloc.order.clone();
        let entry = blocks[0];
        let dest = blocks[1];
        let a = input.func.block_params(entry)[1];
        let param = input.func.block_params(dest)[0];
        let a_home = alloc.loc(a).unwrap();
        let save = Loc::Stack(slot);
        alloc.spill_slots = slot + 1;
        alloc.edits.set_param(dest, ValuePart::low(param), a_home);
        let term = *input.func.block_insts(entry).last().unwrap();
        for edge in 0..2 {
            alloc.edits.edge_mut(term, edge).push((save, a_home));
        }
        for &inst in input.func.block_insts(dest) {
            input.for_each_operand(inst, |op| {
                if op.kind == OperandKind::Use {
                    let loc = if op.part.value == a {
                        save
                    } else if op.part.value == param {
                        a_home
                    } else {
                        return;
                    };
                    alloc.edits.instruction_mut(inst).uses.push((op.part, loc));
                }
            });
        }
        // Edge-specific cyclic transfers with no block arguments, on a switch
        // and a conditional branch, are covered by the companion runtime fixtures.
        alloc
    }
}
