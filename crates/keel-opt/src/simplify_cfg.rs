//! CFG cleanup, repeated until nothing changes:
//!
//! 1. A `brif` on a constant, or with two identical targets, becomes a `jump`.
//! 2. A branch to a block that only jumps on (no parameters, one `jump`)
//!    goes straight to that jump's target. The jump's arguments are defined
//!    in blocks dominating the skipped block, so they dominate every
//!    branch into it too.
//! 3. Unreachable blocks are removed.
//! 4. A block whose only predecessor ends in a `jump` to it is merged into
//!    that predecessor: its parameters become the jump's arguments.

use crate::const_fold::constant;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{Block, BlockCall, EntityRef, Function, InstData};

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
        let InstData::Brif { cond, dests } = func.inst_data(term) else {
            continue;
        };
        let taken = match constant(func, *cond) {
            Some(c) if c.is_zero() => Some(&dests[1]),
            Some(_) => Some(&dests[0]),
            None if same_call(func, &dests[0], &dests[1]) => Some(&dests[0]),
            None => None,
        };
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

/// The jump a block consists of, if it has no parameters and nothing else.
fn forwarding_target(func: &Function, block: Block) -> Option<BlockCall> {
    if !func.block_params(block).is_empty() {
        return None;
    }
    match func.block_insts(block) {
        [only] => match func.inst_data(*only) {
            InstData::Jump { dest } if dest.block != block => Some(dest.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn skip_forwarding_blocks(func: &mut Function) -> bool {
    let mut changed = false;
    let blocks: Vec<Block> = func.blocks().collect();
    for block in blocks {
        let Some(&term) = func.block_insts(block).last() else {
            continue;
        };
        for i in 0..func.inst_data(term).branch_dests().len() {
            let dest = func.inst_data(term).branch_dests()[i].block;
            // A block that forwards to itself through others would loop
            // forever here; one step per round is enough.
            if dest == block {
                continue;
            }
            if let Some(target) = forwarding_target(func, dest) {
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
    let mut changed = false;
    loop {
        // Count branch edges into each block; a `brif` with both arms to
        // the same block counts twice, so it is never merged.
        let mut edges = vec![0u32; func.num_blocks()];
        for (_, inst) in func.layout_insts() {
            for dest in func.inst_data(inst).branch_dests() {
                edges[dest.block.index()] += 1;
            }
        }
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
        for inst in func.block_insts(block).to_vec() {
            func.move_inst(inst, keel_ir::InsertPoint::End(pred));
        }
        func.remove_block(block);
        changed = true;
    }
}
