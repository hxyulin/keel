//! Price explicit GPR caller saves against whole-value spilling.
//! AArch64 can also offer its argument bank, with scratch-sequence clobbers
//! and stack staging for operands consumed after ABI register setup.
use super::*;
use keel_ir::Inst;

struct Clobber {
    inst: Inst,
    point: u32,
    split: bool,
    scratch: bool,
    preserved: u64,
}

fn scan(
    input: &AllocationInput<'_>,
    tier: AllocationTier,
    order: &[Block],
    weights: &[u32],
    intervals: &[Interval],
    callee_first: bool,
) -> Allocation {
    let mut locs = [
        vec![None; input.func.num_values()],
        vec![None; input.func.num_values()],
    ];
    let mut spills = weighted::linear_scan(
        input.func,
        input.selection,
        order,
        intervals,
        input.pools,
        &mut locs,
        weights,
        callee_first,
        weighted::LengthPenalty::Linear,
    );
    let slots = if tier == AllocationTier::Compact {
        assign_compact_spill_slots(input.func, &mut spills, &mut locs, &input.selection.hints)
    } else {
        assign_spill_slots(&mut spills, &mut locs)
    };
    baseline_allocation(order.to_vec(), locs, slots, input.pools)
}

// Count memory transfers, including actual sparse overrides and edge copies.
// A loop weight is an estimate, not an execution-count or trip-count proof.
fn cost(input: &AllocationInput<'_>, allocation: &Allocation, weights: &[u32]) -> (u64, u64) {
    let frame =
        2 * (allocation.callee_saved_used.len() + allocation.float_callee_saved_used.len()) as u64;
    let mut result = (frame, frame);
    let memory = |loc| u64::from(matches!(loc, Loc::Stack(_)));
    let f = input.func;
    for &block in &allocation.order {
        let mut count = 0;
        for &inst in f.block_insts(block) {
            let index = allocation.edits.instruction_index(inst);
            input.for_each_operand(inst, |op| {
                count += allocation
                    .operand_loc(index, op.part, op.kind)
                    .map_or(0, memory);
            });
            if let Some(index) = index {
                let edit = allocation.edits.instruction(index);
                for &(dst, src) in edit.before.iter().chain(&edit.after) {
                    if dst != src {
                        count += memory(dst) + memory(src);
                    }
                }
            }
            for (edge, dest) in f.inst_data(inst).branch_dests().iter().enumerate() {
                for (&param, &arg) in f.block_params(dest.block).iter().zip(&dest.args) {
                    for high in [false, true] {
                        if high && f.value_type(param) != Type::I128 {
                            continue;
                        }
                        let part = |value| ValuePart { value, high };
                        let src =
                            allocation.operand_loc(index, part(f.resolve(arg)), OperandKind::Use);
                        // Selected operands include edge arguments. Replace
                        // that read estimate with the actual parallel copy.
                        count -= src.map_or(0, memory);
                        let (Some(dst), Some(src)) =
                            (allocation.param_loc(dest.block, part(param)), src)
                        else {
                            continue;
                        };
                        if dst != src {
                            count += memory(dst) + memory(src);
                        }
                    }
                }
                for &(dst, src) in allocation.edits.edge(inst, edge) {
                    if dst != src {
                        count += memory(dst) + memory(src);
                    }
                }
            }
        }
        result.0 += count;
        result.1 += count * u64::from(weights[block.index()]);
    }
    result
}

pub(crate) fn allocate(
    input: &AllocationInput<'_>,
    tier: AllocationTier,
    bank: bool,
) -> Option<Allocation> {
    let f = input.func;
    if input.pools.caller_saved.is_empty()
        || f.attrs.optnone
        || f.attrs.naked
        || f.attrs.interrupt
        || f.sig.returns_twice
        || f.personality.is_some()
        || !f.debug_vars.is_empty()
        || f.layout_insts().any(|(_, inst)| input.returns_twice(inst))
        || (!bank
            && !f.layout_insts().any(|(_, i)| {
                matches!(f.inst_data(i), InstData::Call { .. })
                    || (tier != AllocationTier::Compact
                        && matches!(f.inst_data(i), InstData::CallIndirect { .. }))
            }))
    {
        return None;
    }
    let cfg = ControlFlowGraph::compute(f);
    let (order, weights) = allocation_order(f, &cfg, AllocationTier::Full);
    let mut calls = Vec::new();
    let mut slot = 0;
    let mut headers = vec![0; f.num_blocks()];
    let mut banned = vec![false; f.num_values()];
    for &block in &order {
        headers[block.index()] = 2 * slot + 1;
        slot += 1;
        for &inst in f.block_insts(block) {
            let point = 2 * slot;
            slot += 1;
            let data = f.inst_data(inst);
            if input.selection.covered(inst) {
                continue;
            }
            let mut wide = false;
            if bank {
                input.for_each_operand(inst, |op| wide |= op.ty == Type::I128);
            }
            let scratch = bank
                && (wide
                    || matches!(
                        data,
                        InstData::AtomicLoad { .. }
                            | InstData::AtomicStore { .. }
                            | InstData::AtomicRmw { .. }
                            | InstData::AtomicCas { .. }
                    ));
            if !(input.is_call)(data)
                && !scratch
                && !(bank
                    && matches!(
                        data,
                        InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. }
                    ))
            {
                continue;
            }
            if scratch {
                for &value in f.inst_results(inst) {
                    banned[value.index()] = true;
                }
            }
            let split = match data {
                InstData::Call { func, .. } => {
                    !f.callee(*func).attrs.noreturn && !f.callee(*func).sig.returns_twice
                }
                InstData::CallIndirect { sig, .. } => !f.signature(*sig).returns_twice,
                _ => scratch,
            };
            let preserved = if scratch && !(input.is_call)(data) {
                !0xff
            } else {
                input
                    .selection
                    .preserved_call_gprs
                    .get(&inst)
                    .copied()
                    .unwrap_or(0)
            };
            calls.push(Clobber {
                inst,
                point,
                split,
                scratch,
                preserved: preserved & if bank { !0xff } else { u64::MAX },
            });
        }
    }
    if calls.is_empty() {
        return None;
    }
    let mut intervals = build_intervals(
        f,
        &cfg,
        &order,
        input.is_call,
        input.returns_twice,
        input.selection,
    );
    if bank {
        let opaque: Vec<_> = calls
            .iter()
            .filter(|c| c.scratch)
            .map(|c| c.point)
            .collect();
        for iv in &mut intervals {
            if iv.float {
                continue;
            }
            let crosses_scratch = iv.ranges.iter().any(|&(from, to)| {
                let first = opaque.partition_point(|&point| point <= from);
                opaque.get(first).is_some_and(|&point| point < to)
            });
            iv.crosses_call |=
                crosses_scratch || banned[iv.value.index()] || f.value_type(iv.value) == Type::I128;
        }
    }
    constrain(&mut intervals, &calls, input, false, bank, &banned);
    let mut baseline = scan(input, tier, &order, &weights, &intervals, false);
    if bank {
        protect(
            input,
            &mut baseline,
            &intervals,
            &calls,
            &headers,
            false,
            bank,
        );
    }
    let mut reserved: Vec<_> = calls
        .iter()
        .filter(|c| c.scratch)
        .map(|c| f.inst_block(c.inst))
        .collect();
    reserved.sort_unstable();
    reserved.dedup();
    if tier == AllocationTier::Bounded {
        baseline = region::repair_free_with_reserved(input, baseline, &intervals, &reserved);
    }
    if baseline.spill_slots == 0 || !calls.iter().any(|call| call.split) {
        return Some(baseline);
    }
    let before = cost(input, &baseline, &weights);
    constrain(&mut intervals, &calls, input, true, bank, &banned);
    let mut allocation = scan(input, tier, &order, &weights, &intervals, true);
    protect(
        input,
        &mut allocation,
        &intervals,
        &calls,
        &headers,
        true,
        bank,
    );
    if tier == AllocationTier::Bounded {
        allocation = region::repair_free_with_reserved(input, allocation, &intervals, &reserved);
    }
    let after = cost(input, &allocation, &weights);
    let accept = after.0 < before.0 && after.1 < before.1;
    Some(if accept { allocation } else { baseline })
}

fn constrain(
    intervals: &mut [Interval],
    calls: &[Clobber],
    input: &AllocationInput<'_>,
    split: bool,
    bank: bool,
    banned: &[bool],
) {
    let mut blockers: Vec<(u64, Vec<u32>)> = Vec::new();
    for &register in input.pools.caller_saved {
        let bit = 1u64.checked_shl(register.into()).unwrap_or(0);
        let points: Vec<_> = calls
            .iter()
            .filter(|call| !(split && call.split) && call.preserved & bit == 0)
            .map(|call| call.point)
            .collect();
        if let Some((mask, _)) = blockers.iter_mut().find(|(_, other)| *other == points) {
            *mask |= bit;
        } else {
            blockers.push((bit, points));
        }
    }
    for iv in intervals {
        if iv.float || !iv.crosses_call {
            continue;
        }
        iv.preserved_gprs = 0;
        for (bit, points) in &blockers {
            let blocked = iv.ranges.iter().any(|&(from, to)| {
                let first = points.partition_point(|&point| point <= from);
                points.get(first).is_some_and(|&point| point < to)
            });
            if !blocked {
                iv.preserved_gprs |= bit;
            }
        }
        if bank && (banned[iv.value.index()] || input.func.value_type(iv.value) == Type::I128) {
            iv.preserved_gprs &= !0xff;
        }
    }
}

fn protect(
    input: &AllocationInput<'_>,
    allocation: &mut Allocation,
    intervals: &[Interval],
    calls: &[Clobber],
    headers: &[u32],
    split: bool,
    bank: bool,
) {
    let mut homes = [None; 256];
    if !split {
        if bank {
            stage_arguments(input, allocation, calls, &mut homes);
        }
        return;
    }
    let mut live = vec![Vec::new(); calls.len()];
    for (iv_index, iv) in intervals.iter().enumerate() {
        let Some(Loc::Reg(register)) = allocation.locs[iv.half][iv.value.index()] else {
            continue;
        };
        if !input.pools.caller_saved.contains(&register) {
            continue;
        }
        let bit = 1u64.checked_shl(register.into()).unwrap_or(0);
        for &(from, to) in &iv.ranges {
            let first = calls.partition_point(|call| call.point <= from);
            for (index, call) in calls
                .iter()
                .enumerate()
                .skip(first)
                .take_while(|(_, call)| call.point < to)
            {
                if split && call.split && call.preserved & bit == 0 {
                    live[index].push((register, iv_index));
                }
            }
        }
    }
    for (call, values) in calls.iter().zip(&live) {
        for &(register, iv_index) in values {
            let home = *homes[register as usize].get_or_insert_with(|| {
                let slot = allocation.spill_slots;
                allocation.spill_slots += 1;
                slot
            });
            allocation
                .edits
                .instruction_mut(call.inst)
                .before
                .push((Loc::Stack(home), Loc::Reg(register)));
            let dests = input.func.inst_data(call.inst).branch_dests();
            if dests.is_empty() && !input.func.inst_data(call.inst).is_terminator() {
                allocation
                    .edits
                    .instruction_mut(call.inst)
                    .after
                    .push((Loc::Reg(register), Loc::Stack(home)));
            } else {
                let iv = &intervals[iv_index];
                for (edge, dest) in dests.iter().enumerate() {
                    if matches!(input.func.value_def(iv.value), ValueDef::Param(block, _) if block == dest.block)
                    {
                        continue;
                    }
                    let header = headers[dest.block.index()];
                    if iv
                        .ranges
                        .iter()
                        .any(|&(from, to)| from <= header && header <= to)
                    {
                        allocation
                            .edits
                            .edge_mut(call.inst, edge)
                            .push((Loc::Reg(register), Loc::Stack(home)));
                    }
                }
            }
        }
    }
    if bank {
        stage_arguments(input, allocation, calls, &mut homes);
    }
}

// ABI lowering can reuse x0-x7 before every input has been consumed. Present
// those inputs as ordinary spilled operands, whose lowering is already supported.
fn stage_arguments(
    input: &AllocationInput<'_>,
    allocation: &mut Allocation,
    calls: &[Clobber],
    homes: &mut [Option<u32>; 256],
) {
    for call in calls {
        let mut late = Vec::new();
        let mut add_arguments = |args: &keel_ir::CallArgs, params: &[keel_ir::AbiParam]| {
            for (i, &arg) in args.iter().enumerate() {
                if args
                    .abi(i)
                    .or_else(|| params.get(i))
                    .is_some_and(|p| p.layout().is_some())
                {
                    late.push(input.func.resolve(arg));
                }
            }
        };
        let only_late = !call.scratch
            && match input.func.inst_data(call.inst) {
                InstData::Call { func, args } | InstData::ReturnCall { func, args } => {
                    add_arguments(args, &input.func.callee(*func).sig.params);
                    true
                }
                InstData::CallIndirect { sig, callee, args }
                | InstData::ReturnCallIndirect { sig, callee, args } => {
                    add_arguments(args, &input.func.signature(*sig).params);
                    late.push(input.func.resolve(*callee));
                    true
                }
                InstData::Intrinsic {
                    id:
                        keel_ir::Intrinsic::Memcpy
                        | keel_ir::Intrinsic::Memmove
                        | keel_ir::Intrinsic::Memset
                        | keel_ir::Intrinsic::Memcmp,
                    ..
                } => true,
                _ => false,
            };
        let mut stored = [false; 8];
        if let Some(index) = allocation.edits.instruction_index(call.inst) {
            for &(dst, src) in &allocation.edits.instruction(index).before {
                if let (Loc::Stack(_), Loc::Reg(r)) = (dst, src)
                    && r < 8
                {
                    stored[r as usize] = true;
                }
            }
        }
        input.for_each_operand(call.inst, |op| {
            if op.kind != OperandKind::Use || (only_late && !late.contains(&op.part.value)) {
                return;
            }
            let Some(Loc::Reg(register)) =
                allocation.locs[usize::from(op.part.high)][op.part.value.index()]
            else {
                return;
            };
            if register >= 8 {
                return;
            }
            let home = *homes[register as usize].get_or_insert_with(|| {
                let slot = allocation.spill_slots;
                allocation.spill_slots += 1;
                slot
            });
            let edit = allocation.edits.instruction_mut(call.inst);
            if !stored[register as usize] {
                edit.before.push((Loc::Stack(home), Loc::Reg(register)));
                stored[register as usize] = true;
            }
            if !edit.uses.iter().any(|&(part, _)| part == op.part) {
                edit.uses.push((op.part, Loc::Stack(home)));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(source: &str, float: bool) {
        let module = keel_ir::parse(source).unwrap();
        let f = &module.functions[0];
        let selection = crate::select::Selection::default();
        let pools = Pools {
            caller_saved: &[9, 10, 11, 12],
            callee_saved: &[],
            float_caller_saved: &[16],
            float_callee_saved: &[],
        };
        let input = AllocationInput {
            func: f,
            pools: &pools,
            selection: &selection,
            is_call: &|data| matches!(data, InstData::Call { .. } | InstData::CallIndirect { .. }),
            returns_twice: &|_| false,
        };
        let allocation = CallSiteAllocator {
            base: AllocationTier::Full,
        }
        .allocate(&input);
        allocation.validate(&input).unwrap();
        let call = f.block_insts(f.entry_block().unwrap())[0];
        let edit = allocation
            .edits
            .instruction_index(call)
            .expect("profitable caller saves");
        assert!(!allocation.edits.instruction(edit).before.is_empty());
        if float {
            assert!(matches!(allocation.loc(Value::new(0)), Some(Loc::Stack(_))));
        }
        for seed in 0..16 {
            super::super::transfer_tests::check(&input, &allocation, seed);
        }
    }
    #[test]
    fn caller_saves_preserve_live_inputs_without_overwriting_call_results() {
        check(
            "decl @h(i64) -> i64\nfunc @f(i64,i64) -> i64 {\nblock0(v0:i64,v1:i64):\n v2=call @h(v0)\n v3=iadd v2,v1\n v4=xor v3,v1\n v5=iadd v4,v1\n v6=xor v5,v1\n v7=iadd v6,v1\n return v7\n}",
            false,
        );
    }
    #[test]
    fn indirect_only_functions_preserve_live_inputs_and_results() {
        check(
            "func @f(ptr,i64,i64) -> i64 {
block0(v0:ptr,v1:i64,v2:i64):
 v3=call_indirect v0(v1) : (i64) -> i64
 v4=iadd v3,v2
 v5=xor v4,v2
 v6=iadd v5,v2
 v7=xor v6,v2
 v8=iadd v7,v2
 return v8
}",
            false,
        );
    }
    #[test]
    fn compact_requires_a_direct_call_without_the_argument_bank() {
        let module = keel_ir::parse(
            "func @f(ptr,i64) -> i64 {
block0(v0:ptr,v1:i64):
 v2=call_indirect v0(v1) : (i64) -> i64
 v3=iadd v2,v1
 return v3
}",
        )
        .unwrap();
        let f = &module.functions[0];
        let pools = Pools {
            caller_saved: &[9, 10],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let selection = crate::select::Selection::default();
        let input = AllocationInput {
            func: f,
            pools: &pools,
            selection: &selection,
            is_call: &|d| matches!(d, InstData::CallIndirect { .. }),
            returns_twice: &|_| false,
        };
        assert!(allocate(&input, AllocationTier::Compact, false).is_none());
        assert!(allocate(&input, AllocationTier::Bounded, false).is_some());
    }

    #[test]
    fn floating_values_keep_their_spill_homes_across_calls() {
        check(
            "decl @h(i64) -> i64\nfunc @f(f64,i64) -> i64 {\nblock0(v0:f64,v1:i64):\n v2=call @h(v1)\n v3=bitcast.i64 v0\n v4=iadd v2,v1\n v5=xor v4,v1\n v6=iadd v5,v1\n v7=xor v6,v1\n v8=iadd v7,v1\n v9=xor v8,v3\n return v9\n}",
            true,
        );
    }
    #[test]
    fn both_i128_halves_survive_call_clobbers() {
        check(
            "decl @h()\nfunc @f(i128) -> i128 {\nblock0(v0:i128):\n call @h()\n v1=iadd v0,v0\n v2=xor v1,v0\n v3=iadd v2,v0\n v4=xor v3,v0\n v5=iadd v4,v0\n return v5\n}",
            false,
        );
    }
    #[test]
    fn selected_corpus_preserves_transfer_dataflow() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "keel") {
                continue;
            }
            let module = keel_ir::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
            for f in &module.functions {
                for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                    let mut selection = crate::select::Selection::compute(f, target);
                    selection.fuse_compares(f, |_| vec![], &crate::fold::use_counts(f));
                    for bank in [false, true] {
                        if bank && !matches!(target, crate::select::Target::Aarch64) {
                            continue;
                        }
                        let pools = Pools {
                            caller_saved: if bank {
                                &[9, 10, 0, 1, 2, 3, 4, 5, 6, 7]
                            } else {
                                &[9, 10]
                            },
                            callee_saved: &[19, 20],
                            float_caller_saved: &[16, 17],
                            float_callee_saved: &[8, 9],
                        };
                        let input = AllocationInput {
                            func: f,
                            pools: &pools,
                            selection: &selection,
                            is_call: &|d| {
                                matches!(
                                    d,
                                    InstData::Call { .. }
                                        | InstData::CallIndirect { .. }
                                        | InstData::Invoke { .. }
                                )
                            },
                            returns_twice: &|d| match d {
                                InstData::Call { func, .. } | InstData::Invoke { func, .. } => {
                                    f.callee(*func).sig.returns_twice
                                }
                                InstData::CallIndirect { sig, .. } => {
                                    f.signature(*sig).returns_twice
                                }
                                _ => false,
                            },
                        };
                        for tier in [
                            AllocationTier::Full,
                            AllocationTier::Compact,
                            AllocationTier::Bounded,
                        ] {
                            if let Some(allocation) = allocate(&input, tier, bank) {
                                allocation.validate(&input).unwrap();
                                for seed in 0..16 {
                                    super::super::transfer_tests::check_with_clobbers(
                                        &input,
                                        &allocation,
                                        seed,
                                        |inst, emit| {
                                            if !bank {
                                                return;
                                            }
                                            let mut wide = false;
                                            input.for_each_operand(inst, |op| {
                                                wide |= op.ty == Type::I128
                                            });
                                            if wide
                                                || matches!(
                                                    f.inst_data(inst),
                                                    InstData::AtomicLoad { .. }
                                                        | InstData::AtomicStore { .. }
                                                        | InstData::AtomicRmw { .. }
                                                        | InstData::AtomicCas { .. }
                                                )
                                            {
                                                for r in 0..8 {
                                                    emit(Loc::Reg(r));
                                                }
                                            }
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
