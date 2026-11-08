//! Experimental scalar owner displacement across call-free natural loops.
use super::{
    Allocation, AllocationInput, Constraint, ControlFlowGraph, Interval, Loc, OperandKind,
};
use keel_ir::{Block, EntityRef, Inst, InstData, Type, Value, ValueDef};
use keel_ir::{dominators::DominatorTree, fxhash::FxHashMap, loops::LoopInfo};

struct Plan {
    blocks: Vec<Block>,
    preheader: Block,
    exits: Vec<(Inst, usize)>,
    candidate: Value,
    owner: u8,
    reads: Vec<Inst>,
}

fn edited(input: &AllocationInput<'_>, alloc: &Allocation, block: Block) -> bool {
    input.func.block_insts(block).iter().any(|&inst| {
        alloc.edits.instruction_index(inst).is_some()
            || (0..input.func.inst_data(inst).branch_dests().len())
                .any(|edge| !alloc.edits.edge(inst, edge).is_empty())
    })
}

fn discover(
    input: &AllocationInput<'_>,
    alloc: &Allocation,
    intervals: &[Interval],
    cfg: &ControlFlowGraph,
) -> Vec<Plan> {
    let f = input.func;
    if f.sig.returns_twice
        || f.personality.is_some()
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || f.layout_insts().any(|(_, d)| input.returns_twice(d))
    {
        return Vec::new();
    }
    let dom = DominatorTree::compute(f, cfg);
    let loops = LoopInfo::compute(f, cfg, &dom);
    let mut bounds = vec![(0, 0); f.num_blocks()];
    let mut slot = 0;
    for &block in &alloc.order {
        let from = 2 * slot + 1;
        slot += 1 + f.block_insts(block).len() as u32;
        bounds[block.index()] = (from, 2 * slot - 1);
    }
    let mut plans = Vec::new();
    for natural in loops.loops() {
        let Some(preheader) = natural.preheader else {
            continue;
        };
        let entry = *f
            .block_insts(preheader)
            .last()
            .expect("preheader has terminator");
        if !matches!(f.inst_data(entry), InstData::Jump { .. }) {
            continue;
        }
        let mut inside = vec![false; f.num_blocks()];
        for &block in natural.blocks {
            inside[block.index()] = true;
        }
        let mut exits = Vec::new();
        for &block in natural.blocks {
            let term = *f
                .block_insts(block)
                .last()
                .expect("loop block has terminator");
            for (edge, dest) in f.inst_data(term).branch_dests().iter().enumerate() {
                if !inside[dest.block.index()] {
                    exits.push((term, edge));
                }
            }
        }
        if exits.is_empty() {
            continue;
        }
        if natural.blocks.iter().any(|&b| edited(input, alloc, b)) {
            continue;
        }
        let mut used = [false; 256];
        let mut barrier = false;
        let mut counts: FxHashMap<Value, Vec<Inst>> = FxHashMap::default();
        for &block in natural.blocks {
            for &inst in f.block_insts(block) {
                if input.selection.covered(inst) {
                    continue;
                }
                let data = f.inst_data(inst);
                barrier |= (input.is_call)(data)
                    || matches!(
                        data,
                        InstData::InlineAsm { .. }
                            | InstData::Intrinsic { .. }
                            | InstData::Invoke { .. }
                            | InstData::LandingPad { .. }
                            | InstData::X86RflagsRegion { .. }
                            | InstData::VaArg { .. }
                    );
                input.for_each_operand(inst, |op| {
                    barrier |= op.constraint != Constraint::Any;
                    if let Some(Loc::Reg(r)) = alloc.operand_loc(None, op.part, op.kind) {
                        used[r as usize] = true;
                    }
                    if op.kind != OperandKind::Use
                        || op.part.high
                        || !matches!(op.ty, Type::I64 | Type::Ptr)
                        || !matches!(alloc.loc(op.part.value), Some(Loc::Stack(_)))
                    {
                        return;
                    }
                    let def_block = match f.value_def(op.part.value) {
                        ValueDef::Param(b, _) => b,
                        ValueDef::Alias(_) => return,
                        ValueDef::Result(i, _) => {
                            if input.selection.omitted_defs.contains(&op.part.value)
                                || matches!(
                                    f.inst_data(i),
                                    InstData::Iconst { .. } | InstData::StackAddr { .. }
                                )
                            {
                                return;
                            }
                            f.inst_block(i)
                        }
                    };
                    if inside[def_block.index()] || !dom.dominates(def_block, preheader) {
                        return;
                    }
                    let reads = counts.entry(op.part.value).or_default();
                    if reads.last() != Some(&inst) {
                        reads.push(inst);
                    }
                });
                input.for_each_clobber(inst, |loc| {
                    if let Loc::Reg(r) = loc {
                        used[r as usize] = true;
                    }
                });
            }
        }
        if barrier {
            continue;
        }
        if counts.is_empty() {
            continue;
        }
        let valid_invariant = |value| {
            let block = match f.value_def(value) {
                ValueDef::Param(b, _) => b,
                ValueDef::Result(i, _)
                    if !input.selection.omitted_defs.contains(&value)
                        && !matches!(
                            f.inst_data(i),
                            InstData::Iconst { .. } | InstData::StackAddr { .. }
                        ) =>
                {
                    f.inst_block(i)
                }
                _ => return false,
            };
            !inside[block.index()] && dom.dominates(block, preheader)
        };
        let edge_claims = |inst: Inst, edge: usize, r: u8| {
            let dest = f.inst_data(inst).branch_dests()[edge].block;
            f.block_params(dest).iter().any(|&value| {
                [false, true].iter().any(|&high| {
                    alloc.param_loc(dest, super::ValuePart { value, high }) == Some(Loc::Reg(r))
                })
            }) || alloc
                .edits
                .edge(inst, edge)
                .iter()
                .any(|&(dst, _)| dst == Loc::Reg(r))
        };
        let owner = intervals
            .iter()
            .filter(|iv| {
                iv.half == 0
                    && matches!(f.value_type(iv.value), Type::I64 | Type::Ptr)
                    && valid_invariant(iv.value)
            })
            .filter_map(|iv| match alloc.loc(iv.value) {
                Some(Loc::Reg(r))
                    if !used[r as usize]
                        && !edge_claims(entry, 0, r)
                        && !exits.iter().any(|&(i, e)| edge_claims(i, e, r))
                        && natural.blocks.iter().all(|b| {
                            let (from, to) = bounds[b.index()];
                            iv.ranges.iter().any(|&(a, z)| a <= from && z >= to)
                        }) =>
                {
                    Some(r)
                }
                _ => None,
            })
            .min();
        let Some(owner) = owner else {
            continue;
        };
        let best = counts
            .into_iter()
            .filter(|(v, _)| {
                let home = alloc.loc(*v).expect("candidate has home");
                !alloc
                    .edits
                    .edge(entry, 0)
                    .iter()
                    .any(|&(dst, _)| dst == home)
            })
            .max_by_key(|(v, reads)| (reads.len(), std::cmp::Reverse(v.index())));
        let Some((candidate, reads)) = best else {
            continue;
        };

        plans.push(Plan {
            blocks: natural.blocks.to_vec(),
            preheader,
            exits,
            candidate,
            owner,
            reads,
        });
    }
    plans
}

#[cfg(test)]
#[path = "loop_region_tests.rs"]
mod tests;

pub(super) fn repair(
    input: &AllocationInput<'_>,
    mut alloc: Allocation,
    intervals: &[Interval],
    cfg: &ControlFlowGraph,
) -> (Allocation, Vec<Block>) {
    let plans = discover(input, &alloc, intervals, cfg);
    let Some(p) = plans
        .into_iter()
        .max_by_key(|p| (p.reads.len(), std::cmp::Reverse(p.preheader.index())))
    else {
        return (alloc, Vec::new());
    };
    let entry = *input
        .func
        .block_insts(p.preheader)
        .last()
        .expect("preheader terminator");
    let temp = Loc::Stack(alloc.spill_slots);
    alloc.spill_slots += 1;
    let register = Loc::Reg(p.owner);
    let home = alloc.loc(p.candidate).expect("candidate stack home");
    alloc
        .edits
        .edge_mut(entry, 0)
        .extend([(temp, register), (register, home)]);
    for inst in p.reads {
        alloc
            .edits
            .instruction_mut(inst)
            .uses
            .push((super::ValuePart::low(p.candidate), register));
    }
    for (inst, edge) in p.exits {
        alloc.edits.edge_mut(inst, edge).push((register, temp));
    }
    let mut reserved = p.blocks;
    reserved.push(p.preheader);
    (alloc, reserved)
}
