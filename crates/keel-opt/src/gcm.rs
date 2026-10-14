//! Global code motion (Click, "Global Code Motion / Global Value Numbering",
//! 1995): the scheduler for floating instructions.
//!
//! Pinned instructions stay put. Each pure instruction may legally go
//! anywhere between two blocks on the dominator tree:
//!
//! - **early**: the deepest block defining one of its operands. Anything
//!   higher would come before an operand exists.
//! - **late**: the common dominator of the blocks that use it. Anything
//!   lower would miss a use.
//!
//! Every block on the dominator-tree path from late up to early is
//! correct; GCM picks the one in the fewest loops, and among those the
//! latest, so a computation leaves loops when it can, and otherwise moves
//! into the branch that needs it, off the paths that do not.
//!
//! An instruction whose block does not change keeps its place: the order
//! within a block is already valid, and moving each instruction to just
//! before its use lengthens the lives of its operands instead (a chain of
//! `select`s sinking to the end of a function keeps every condition alive
//! across everything in between). An instruction that changes block goes
//! just before its first use there, or before the terminator.
//! Fixed object addresses with constant field offsets stay at the late
//! placement: recomputing them is cheap, while keeping hundreds of them
//! live across an interpreter loop makes the allocator spill them.
//!
//! Instructions without operands (constants, symbol and stack addresses)
//! are *rematerialized* instead: each block that uses one gets its own copy
//! just before its first use there. Recomputing one costs an instruction or
//! two; keeping one value alive from the top of the function, or across a
//! loop, costs a register the whole way, and GVN merges every `iconst 1` in
//! a function into one. (A register allocator that rematerializes would do
//! this better, at the point where it actually runs out of registers.)
//!
//! Users are placed before the instructions they use, so "first use" is
//! already final when an instruction is placed. Pure instructions cannot
//! trap, so running one on a path that did not run it before (hoisting out
//! of a loop that runs zero times) is harmless.

use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::loops::LoopDepth;
use keel_ir::{BinaryOp, Block, EntityRef, Function, InsertPoint, Inst, InstData, Value, ValueDef};

pub fn run(func: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(func);
    let domtree = DominatorTree::compute(func, &cfg);
    let loops = LoopDepth::compute(func, &cfg, &domtree);

    // Depth of each block in the dominator tree; the entry is 0.
    let mut dom_depth = vec![0u32; func.num_blocks()];
    for &block in &domtree.rpo()[1..] {
        let idom = domtree.idom(block).expect("reachable non-entry block");
        dom_depth[block.index()] = dom_depth[idom.index()] + 1;
    }

    let floating: Vec<Inst> = domtree
        .rpo()
        .iter()
        .flat_map(|&b| func.block_insts(b).to_vec())
        .filter(|&inst| !func.inst_data(inst).is_pinned())
        .collect();
    // Who uses each value, among reachable instructions.
    let mut users: Vec<Vec<Inst>> = vec![Vec::new(); func.num_values()];
    for &block in domtree.rpo() {
        for &inst in func.block_insts(block) {
            func.inst_data(inst).for_each_operand(|v| {
                let users = &mut users[func.resolve(v).index()];
                if !users.contains(&inst) {
                    users.push(inst);
                }
            });
        }
    }

    // Early placement, with operands before users: blocks in RPO and
    // definitions before uses within a block.
    let mut early = vec![None; func.num_insts()];
    for &inst in &floating {
        let mut block = func.entry_block().expect("reachable code has an entry");
        func.inst_data(inst).for_each_operand(|v| {
            let def_block = match func.value_def(func.resolve(v)) {
                ValueDef::Param(b, _) => b,
                ValueDef::Result(def, _) => {
                    early[def.index()].unwrap_or_else(|| func.inst_block(def))
                }
                ValueDef::Alias(_) => unreachable!("resolved"),
            };
            if dom_depth[def_block.index()] > dom_depth[block.index()] {
                block = def_block;
            }
        });
        early[inst.index()] = Some(block);
    }

    // Late placement and the final choice, users first.
    let mut changed = false;
    for &inst in floating.iter().rev() {
        // The users of all its results (`*_overflow` has two).
        let mut inst_users: Vec<Inst> = Vec::new();
        for &result in func.inst_results(inst) {
            for &user in &users[result.index()] {
                if !inst_users.contains(&user) {
                    inst_users.push(user);
                }
            }
        }
        let mut late: Option<Block> = None;
        for &user in &inst_users {
            let b = func.inst_block(user);
            late = Some(match late {
                None => b,
                Some(l) => domtree.common_dominator(l, b),
            });
        }
        // Unused: leave it for DCE.
        let Some(late) = late else { continue };
        let early = early[inst.index()].unwrap();

        let mut operands = 0;
        func.inst_data(inst).for_each_operand(|_| operands += 1);
        if operands == 0 {
            changed |= rematerialize(func, inst, &inst_users);
            continue;
        }

        let mut best = late;
        let mut block = late;
        let cheap_address = fixed_address(func, func.first_result(inst));
        while block != early {
            block = domtree.idom(block).expect("early dominates late");
            // An address of a fixed object plus a constant is cheap to
            // recompute. Hoisting it keeps a separate pointer live across
            // the loop (and calls), often costing more spills than it saves.
            if !cheap_address && loops.depth(block) < loops.depth(best) {
                best = block;
            }
        }

        if best == func.inst_block(inst) {
            continue;
        }
        let at = match first_user_in(func, best, &inst_users) {
            Some(user) => InsertPoint::Before(user),
            None => InsertPoint::Before(*func.block_insts(best).last().unwrap()),
        };
        func.move_inst(inst, at);
        changed = true;
    }
    changed
}

/// Fixed object addresses, including constant field offsets. Keep these
/// near their users; ordinary pointer arithmetic still gets loop motion.
fn fixed_address(func: &Function, value: Value) -> bool {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(value)) else {
        return false;
    };
    match func.inst_data(inst) {
        InstData::StackAddr { .. } | InstData::SymbolAddr { .. } => true,
        InstData::Binary {
            op: BinaryOp::Ptradd,
            args,
        } => {
            let ValueDef::Result(offset, _) = func.value_def(func.resolve(args[1])) else {
                return false;
            };
            matches!(func.inst_data(offset), InstData::Iconst { .. })
                && fixed_address(func, args[0])
        }
        _ => false,
    }
}

/// Gives each block using operand-free `inst` its own copy, just before the
/// first use in that block. `inst` itself serves its own block if it has
/// users there, or else the first user block.
fn rematerialize(func: &mut Function, inst: Inst, users: &[Inst]) -> bool {
    let result = func.first_result(inst);
    let mut blocks: Vec<Block> = Vec::new();
    for &user in users {
        let b = func.inst_block(user);
        if !blocks.contains(&b) {
            blocks.push(b);
        }
    }
    let home = func.inst_block(inst);
    if let Some(i) = blocks.iter().position(|&b| b == home) {
        // Keep its place in its own block.
        blocks.remove(i);
    } else {
        let first = blocks.remove(0);
        let user = first_user_in(func, first, users).expect("a user in its block");
        func.move_inst(inst, InsertPoint::Before(user));
    }
    let changed = !blocks.is_empty() || func.inst_block(inst) != home;
    let data = func.inst_data(inst).clone();
    for block in blocks {
        let first = first_user_in(func, block, users).expect("a user in its block");
        let copy = func.insert_inst(InsertPoint::Before(first), data.clone());
        func.set_inst_loc(copy, func.inst_loc(inst));
        let copy = func.first_result(copy);
        let local: Vec<Inst> = users
            .iter()
            .copied()
            .filter(|&u| func.inst_block(u) == block)
            .collect();
        for user in local {
            let mut data = func.inst_data(user).clone();
            data.for_each_operand_mut(|v| {
                if func.resolve(*v) == result {
                    *v = copy;
                }
            });
            *func.inst_data_mut(user) = data;
        }
    }
    changed
}

/// The first instruction in `block`, in layout order, among `users`.
fn first_user_in(func: &Function, block: Block, users: &[Inst]) -> Option<Inst> {
    func.block_insts(block)
        .iter()
        .copied()
        .find(|inst| users.contains(inst))
}
