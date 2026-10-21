//! Eliminate block parameters whose incoming values are one dominating value.
//!
//! Self-referential loop edges are ignored only when a real incoming value
//! anchors the parameter. An unanchored cycle is not a copy. Every edge is
//! considered, including both arms of one branch and unreachable predecessors.

use keel_ir::dominators::DominatorTree;
use keel_ir::{Block, EntityRef, Function, Inst, Value, ValueDef};

fn replacement(
    func: &Function,
    dom: &DominatorTree,
    block: Block,
    param: Value,
    index: usize,
    incoming: &[(Inst, usize)],
) -> Option<Value> {
    let resolved = func.resolve(param);
    if resolved != param {
        return Some(resolved);
    }
    let mut candidate = None;
    for &(branch, edge) in incoming {
        let arg = func.resolve(func.inst_data(branch).branch_dests()[edge].args[index]);
        if arg == param {
            continue;
        }
        if candidate.is_some_and(|value| value != arg) {
            return None;
        }
        candidate = Some(arg);
    }
    let candidate = candidate?;
    let definition = match func.value_def(candidate) {
        ValueDef::Result(inst, _) => func.inst_block(inst),
        ValueDef::Param(block, _) => block,
        ValueDef::Alias(_) => unreachable!("resolved"),
    };
    // A result defined inside this block cannot replace a parameter at its
    // entry. Requiring strict block dominance also prevents parameter cycles.
    (definition != block && dom.dominates(definition, block)).then_some(candidate)
}

pub fn run(func: &mut Function) -> bool {
    // Only parameters of blocks other than the entry are candidates.
    if func
        .blocks()
        .skip(1)
        .all(|b| func.block_params(b).is_empty())
    {
        return false;
    }
    let analysis = crate::analysis::get(func);
    let dom = &analysis.dom;
    let mut incoming = vec![Vec::new(); func.num_blocks()];
    let mut users: Vec<Vec<Block>> = vec![Vec::new(); func.num_values()];
    // Only terminators branch.
    for block in func.blocks() {
        let Some(&inst) = func.block_insts(block).last() else {
            continue;
        };
        for (edge, dest) in func.inst_data(inst).branch_dests().iter().enumerate() {
            incoming[dest.block.index()].push((inst, edge));
            for &arg in &dest.args {
                users[func.resolve(arg).index()].push(dest.block);
            }
        }
    }
    // The entry's parameters are ABI arguments and must remain in place.
    let mut work: Vec<_> = dom.rpo().iter().copied().skip(1).rev().collect();
    let mut queued = vec![false; func.num_blocks()];
    for &block in &work {
        queued[block.index()] = true;
    }
    let mut changed = false;
    while let Some(block) = work.pop() {
        queued[block.index()] = false;
        for index in (0..func.block_params(block).len()).rev() {
            let param = func.block_params(block)[index];
            let Some(value) =
                replacement(func, &dom, block, param, index, &incoming[block.index()])
            else {
                continue;
            };
            if func.resolve(param) == param {
                func.replace_all_uses(param, value);
            }
            func.remove_block_param(block, index);
            for &(branch, edge) in &incoming[block.index()] {
                func.inst_data_mut(branch).branch_dests_mut()[edge]
                    .args
                    .remove(index);
            }
            let waiting = std::mem::take(&mut users[param.index()]);
            for &user in &waiting {
                if Some(user) != func.entry_block()
                    && dom.is_reachable(user)
                    && !queued[user.index()]
                {
                    queued[user.index()] = true;
                    work.push(user);
                }
            }
            users[value.index()].extend(waiting);
            changed = true;
        }
    }
    changed
}
