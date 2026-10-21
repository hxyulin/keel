//! Consume explicit direct-callee contracts, without inferring contracts from bodies.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::FxHashSet;
use keel_ir::{Block, Clause, EntityRef, Function, Inst, InstData, ValueDef};

pub fn run(func: &mut Function) -> bool {
    let mut changed = false;
    // Keep invoke result handles and locations: call has the same result signature.
    let invokes: Vec<_> = func
        .layout_insts()
        .filter_map(|(_, inst)| {
            matches!(func.inst_data(inst), InstData::Invoke { func: callee, .. }
            if func.callee(*callee).attrs.nounwind)
            .then_some(inst)
        })
        .collect();
    for inst in invokes {
        if func.is_removed(inst) {
            continue;
        }
        let InstData::Invoke {
            func: callee,
            args,
            dests,
        } = func.inst_data(inst)
        else {
            continue;
        };
        let block = func.inst_block(inst);
        let normal = dests[0].clone();
        let cfg = ControlFlowGraph::compute(func);
        let dom = DominatorTree::compute(func, &cfg);
        if !dom.is_reachable(block) {
            continue;
        }
        // The verifier permits invoke results wherever the normal destination
        // dominates. A shared normal destination need not be dominated by the
        // invoke itself; a call result would be invalid in that case.
        let mut dominates_uses = true;
        for (use_block, use_inst) in func.layout_insts() {
            if !dom.is_reachable(use_block) {
                continue;
            }
            func.inst_data(use_inst).for_each_operand(|v| {
                if matches!(func.value_def(func.resolve(v)), ValueDef::Result(def, _) if def == inst)
                    && !dom.dominates(block, use_block) {
                    dominates_uses = false;
                }
            });
        }
        if !dominates_uses {
            continue;
        }
        let Some(dead) = deletion_plan(func, block, Some(normal.block), &[]) else {
            continue;
        };
        let data = InstData::Call {
            func: *callee,
            args: args.clone(),
        };
        let loc = func.inst_loc(inst);
        *func.inst_data_mut(inst) = data;
        let jump = func.append_inst(block, InstData::Jump { dest: normal });
        func.set_inst_loc(jump, loc);
        for block in dead {
            func.remove_block(block);
        }
        changed = true;
    }
    let calls: Vec<_> = func
        .layout_insts()
        .filter_map(|(_, inst)| {
            matches!(func.inst_data(inst), InstData::Call { func: callee, .. }
            if func.callee(*callee).attrs.noreturn)
            .then_some(inst)
        })
        .collect();
    for inst in calls {
        if func.is_removed(inst) {
            continue;
        }
        let block = func.inst_block(inst);
        let insts = func.block_insts(block);
        let index = insts.iter().position(|&i| i == inst).unwrap();
        let suffix = &insts[index + 1..];
        // Preserve an existing defensive trap and its original debug location.
        if matches!(suffix, [trap] if matches!(func.inst_data(*trap), InstData::Trap)) {
            continue;
        }
        let suffix = suffix.to_vec();
        let Some(dead) = deletion_plan(func, block, None, &suffix) else {
            continue;
        };
        let loc = func.inst_loc(inst);
        for i in suffix {
            func.remove_inst(i);
        }
        let trap = func.append_inst(block, InstData::Trap);
        func.set_inst_loc(trap, loc);
        for block in dead {
            func.remove_block(block);
        }
        changed = true;
    }
    changed
}

/// Plan the complete cut before mutating. Merely truncating a block can remove
/// definitions used on surviving paths (notably invoke results used in joins).
/// EH type IDs also require a retained matching catch clause, even when no SSA
/// operand refers to the pad. Skip such cuts rather than invent values/clauses.
fn deletion_plan(
    func: &Function,
    cut: Block,
    successor: Option<Block>,
    suffix: &[Inst],
) -> Option<Vec<Block>> {
    let cfg = ControlFlowGraph::compute(func);
    let mut reachable = vec![false; func.num_blocks()];
    let mut work = vec![func.entry_block()?];
    while let Some(block) = work.pop() {
        if reachable[block.index()] {
            continue;
        }
        reachable[block.index()] = true;
        if block == cut {
            work.extend(successor);
        } else {
            work.extend(cfg.succs(block));
        }
    }
    // Don't rewrite preexisting unreachable sites.
    if !reachable[cut.index()] {
        return None;
    }
    let dead: Vec<_> = func.blocks().filter(|b| !reachable[b.index()]).collect();
    let mut removed: FxHashSet<Inst> = suffix.iter().copied().collect();
    for &block in &dead {
        removed.extend(func.block_insts(block));
    }
    let mut catches = FxHashSet::default();
    for (_, inst) in func.layout_insts().filter(|(_, i)| !removed.contains(i)) {
        if let InstData::LandingPad { clauses } = func.inst_data(inst) {
            for clause in clauses {
                if let Clause::Catch(Some(symbol)) = clause {
                    catches.insert(*symbol);
                }
            }
        }
    }
    for (_, inst) in func.layout_insts().filter(|(_, i)| !removed.contains(i)) {
        let mut valid = true;
        func.inst_data(inst).for_each_operand(|v| {
            valid &= match func.value_def(func.resolve(v)) {
                ValueDef::Result(def, _) => !removed.contains(&def),
                ValueDef::Param(block, _) => reachable[block.index()],
                ValueDef::Alias(_) => unreachable!(),
            };
        });
        if !valid {
            return None;
        }
        if let InstData::EhTypeid { typeinfo } = func.inst_data(inst)
            && !catches.contains(typeinfo)
        {
            return None;
        }
    }
    Some(dead)
}
