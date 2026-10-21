//! Eliminate block parameters whose incoming values are one dominating value.
//!
//! Self-referential loop edges are ignored only when a real incoming value
//! anchors the parameter. An unanchored cycle is not a copy. Every edge is
//! considered, including both arms of one branch and unreachable predecessors.

use crate::scratch::Scratch;
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
    // Every edge into each block, in layout and branch order, packed by
    // destination; and the blocks each value is passed to, as lists.
    let mut incoming_start = Scratch::filled(func.num_blocks() + 1, 0u32);
    let mut users = UserLists::new(func.num_values());
    // Only terminators branch.
    for block in func.blocks() {
        let Some(&inst) = func.block_insts(block).last() else {
            continue;
        };
        for dest in func.inst_data(inst).branch_dests() {
            incoming_start[dest.block.index() + 1] += 1;
            for &arg in &dest.args {
                users.push(func.resolve(arg), dest.block);
            }
        }
    }
    for i in 1..incoming_start.len() {
        incoming_start[i] += incoming_start[i - 1];
    }
    let mut fill: Scratch<u32> = Scratch::new();
    fill.extend_from_slice(&incoming_start);
    let mut incoming_edges =
        Scratch::filled(*incoming_start.last().unwrap() as usize, (Inst::new(0), 0));
    for block in func.blocks() {
        let Some(&inst) = func.block_insts(block).last() else {
            continue;
        };
        for (edge, dest) in func.inst_data(inst).branch_dests().iter().enumerate() {
            let slot = &mut fill[dest.block.index()];
            incoming_edges[*slot as usize] = (inst, edge);
            *slot += 1;
        }
    }
    drop(fill);
    let incoming = |block: Block| {
        let b = block.index();
        &incoming_edges[incoming_start[b] as usize..incoming_start[b + 1] as usize]
    };
    // The entry's parameters are ABI arguments and must remain in place.
    let mut work: Scratch<Block> = Scratch::new();
    work.extend(dom.rpo().iter().copied().skip(1).rev());
    let mut queued = Scratch::filled(func.num_blocks(), false);
    for &block in &work {
        queued[block.index()] = true;
    }
    let mut changed = false;
    while let Some(block) = work.pop() {
        queued[block.index()] = false;
        for index in (0..func.block_params(block).len()).rev() {
            let param = func.block_params(block)[index];
            let Some(value) = replacement(func, dom, block, param, index, incoming(block)) else {
                continue;
            };
            if func.resolve(param) == param {
                func.replace_all_uses(param, value);
            }
            func.remove_block_param(block, index);
            for &(branch, edge) in incoming(block) {
                func.inst_data_mut(branch).branch_dests_mut()[edge]
                    .args
                    .remove(index);
            }
            // The parameter's users now pass `value`.
            let mut node = users.head[param.index()];
            while node != NIL {
                let (user, next) = users.nodes[node as usize];
                if Some(user) != func.entry_block()
                    && dom.is_reachable(user)
                    && !queued[user.index()]
                {
                    queued[user.index()] = true;
                    work.push(user);
                }
                node = next;
            }
            users.move_list(param, value);
            changed = true;
        }
    }
    changed
}

const NIL: u32 = u32::MAX;

/// For each value, the blocks it is passed to, in insertion order: linked
/// lists threaded through one node array instead of a vector per value.
struct UserLists {
    head: Scratch<u32>,
    tail: Scratch<u32>,
    /// (user, next node)
    nodes: Vec<(Block, u32)>,
}

impl UserLists {
    fn new(values: usize) -> Self {
        Self {
            head: Scratch::filled(values, NIL),
            tail: Scratch::filled(values, NIL),
            nodes: Vec::new(),
        }
    }

    fn push(&mut self, value: Value, user: Block) {
        let node = u32::try_from(self.nodes.len()).expect("user list overflow");
        self.nodes.push((user, NIL));
        self.link(value, node, node);
    }

    /// Appends the chained nodes `first..=last` to `value`'s list.
    fn link(&mut self, value: Value, first: u32, last: u32) {
        let v = value.index();
        match self.tail[v] {
            NIL => self.head[v] = first,
            tail => self.nodes[tail as usize].1 = first,
        }
        self.tail[v] = last;
    }

    /// Empties `from`'s list onto the end of `to`'s.
    fn move_list(&mut self, from: Value, to: Value) {
        let (first, last) = (self.head[from.index()], self.tail[from.index()]);
        self.head[from.index()] = NIL;
        self.tail[from.index()] = NIL;
        if first != NIL {
            self.link(to, first, last);
        }
    }
}
