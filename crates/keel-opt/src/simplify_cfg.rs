//! CFG cleanup, repeated until nothing changes:
//!
//! 1. A `brif` or `switch` on a constant, or whose targets are all the
//!    same, becomes a `jump`.
//! 2. Bypass jump-only blocks, substituting incoming arguments for their
//!    parameters. A parameter used outside the forwarding block prevents
//!    bypassing it: that definition must still dominate its external uses.
//! 3. Unreachable blocks are removed.
//! 4. A block whose only predecessor ends in a `jump` to it is merged into
//!    that predecessor: its parameters become the jump's arguments.

use crate::const_fold::constant;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{Block, BlockCall, EntityRef, Function, InstData, ValueDef};

pub fn run(func: &mut Function) -> bool {
    let mut changed = false;
    loop {
        let round = fold_branches(func) | skip_forwarding_blocks(func);
        let round = round | remove_unreachable(func) | merge_blocks(func);
        if !round {
            return changed;
        }
        changed = true;
    }
}

fn fold_branches(func: &mut Function) -> bool {
    let mut changed = false;
    let blocks: Vec<Block> = func.blocks().collect();
    for block in blocks {
        let Some(&term) = func.block_insts(block).last() else {
            continue;
        };
        let taken = match func.inst_data(term) {
            InstData::Brif { cond, dests } => match constant(func, *cond) {
                Some(c) if c.is_zero() => Some(&dests[1]),
                Some(_) => Some(&dests[0]),
                None => None,
            },
            InstData::Switch { arg, dests, cases } => constant(func, *arg).map(|c| {
                let hit = cases.iter().position(|&case| c.same_bits(case));
                &dests[hit.map_or(0, |i| i + 1)]
            }),
            _ => continue,
        };
        let dests = func.inst_data(term).branch_dests();
        let taken = taken.or_else(|| {
            dests[1..]
                .iter()
                .all(|d| same_call(func, &dests[0], d))
                .then_some(&dests[0])
        });
        if let Some(dest) = taken {
            let dest = dest.clone();
            *func.inst_data_mut(term) = InstData::Jump { dest };
            changed = true;
        }
    }
    changed
}

/// Whether two block calls go to the same block with the same arguments.
fn same_call(func: &Function, a: &BlockCall, b: &BlockCall) -> bool {
    a.block == b.block
        && a.args.len() == b.args.len()
        && a.args
            .iter()
            .zip(&b.args)
            .all(|(&x, &y)| func.resolve(x) == func.resolve(y))
}

/// Compose one incoming block call with a jump-only block's outgoing call.
/// Substitution is simultaneous, so permutations and repeated arguments work.
fn forwarding_target(func: &Function, incoming: &BlockCall) -> Option<BlockCall> {
    let block = incoming.block;
    match func.block_insts(block) {
        [only] => match func.inst_data(*only) {
            InstData::Jump { dest } if dest.block != block => {
                let mut target = dest.clone();
                for arg in &mut target.args {
                    *arg = func.resolve(*arg);
                    if let ValueDef::Param(def, n) = func.value_def(*arg)
                        && def == block
                    {
                        *arg = incoming.args[n as usize];
                    }
                }
                Some(target)
            }
            _ => None,
        },
        _ => None,
    }
}

fn skip_forwarding_blocks(func: &mut Function) -> bool {
    // Removing a forwarding block's parameters is only legal when all
    // their uses are in that block's jump. Resolve aliases before checking.
    let mut escaping = vec![false; func.num_blocks()];
    for (block, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|value| {
            if let ValueDef::Param(def, _) = func.value_def(func.resolve(value))
                && def != block
            {
                escaping[def.index()] = true;
            }
        });
    }
    let mut changed = false;
    let blocks: Vec<Block> = func.blocks().collect();
    for block in blocks {
        let Some(&term) = func.block_insts(block).last() else {
            continue;
        };
        for i in 0..func.inst_data(term).branch_dests().len() {
            let dest = &func.inst_data(term).branch_dests()[i];
            // A block that forwards to itself through others would loop
            // forever here; one step per round is enough.
            if dest.block == block || escaping[dest.block.index()] {
                continue;
            }
            // An invoke's destinations take no arguments.
            let invoke = matches!(func.inst_data(term), InstData::Invoke { .. });
            if let Some(target) = forwarding_target(func, dest)
                && !(invoke && !target.args.is_empty())
            {
                func.inst_data_mut(term).branch_dests_mut()[i] = target;
                changed = true;
            }
        }
    }
    changed
}

fn remove_unreachable(func: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(func);
    let domtree = DominatorTree::compute(func, &cfg);
    let dead: Vec<Block> = func
        .blocks()
        .filter(|&b| !domtree.is_reachable(b))
        .collect();
    for &block in &dead {
        func.remove_block(block);
    }
    !dead.is_empty()
}

fn merge_blocks(func: &mut Function) -> bool {
    // Count branch edges, not unique predecessors: duplicate brif/switch
    // destinations must still prevent a merge. Block indices remain stable
    // when blocks leave the layout, so this census lasts for this invocation.
    let mut edges = vec![0u32; func.num_blocks()];
    for (_, inst) in func.layout_insts() {
        for dest in func.inst_data(inst).branch_dests() {
            edges[dest.block.index()] += 1;
        }
    }
    let mut changed = false;
    loop {
        let candidate = func.blocks().find_map(|pred| {
            let &term = func.block_insts(pred).last()?;
            match func.inst_data(term) {
                InstData::Jump { dest }
                    if dest.block != pred
                        && edges[dest.block.index()] == 1
                        && Some(dest.block) != func.entry_block() =>
                {
                    Some((pred, term, dest.clone()))
                }
                _ => None,
            }
        });
        let Some((pred, jump, dest)) = candidate else {
            return changed;
        };
        let block = dest.block;
        for (&param, &arg) in func.block_params(block).to_vec().iter().zip(&dest.args) {
            func.replace_all_uses(param, arg);
        }
        func.remove_inst(jump);
        // This is the only deleted edge. Parameter aliases affect values,
        // not block destinations, and moving instructions preserves all
        // their outgoing edges (including edges back to pred). The removed
        // block cannot have a self-edge: its sole incoming edge was jump.
        edges[block.index()] -= 1;
        for inst in func.block_insts(block).to_vec() {
            func.move_inst(inst, keel_ir::InsertPoint::End(pred));
        }
        func.remove_block(block);
        changed = true;
    }
}
