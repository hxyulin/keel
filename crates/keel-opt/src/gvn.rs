//! Global value numbering: a pure instruction equivalent to one that
//! dominates it is replaced by that one.
//!
//! Blocks are walked in dominator-tree preorder with a scoped hash table
//! from instruction (with operands resolved) to the value it computes.
//! Entering a block opens a scope; leaving it closes the scope, and entries
//! from closed blocks no longer match, so a lookup only ever finds an
//! instruction that dominates the current one, and the IR stays valid
//! without moving anything.
//!
//! Merging two identical computations in sibling branches (hoisting them to
//! a common dominator) would need code motion first; that is GCM's job.
//! Pinned instructions are never numbered: two loads of the same address
//! may see different memory.

use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, EntityRef, Function, Inst, InstData, IntCC, OverflowOp};
use std::collections::hash_map::Entry;

pub fn run(func: &mut Function) -> bool {
    let analysis = crate::analysis::get(func);
    let domtree = &analysis.dom;
    let Some(entry) = func.entry_block() else {
        return false;
    };
    let children = domtree.children();

    // Entries are never removed. One is in scope while its block is open on
    // the walk, i.e. while that block dominates the current one; a stale
    // entry from a closed sibling is simply overwritten.
    let layout: usize = func.blocks().map(|b| func.block_insts(b).len()).sum();
    let mut table: FxHashMap<InstData, Inst> =
        FxHashMap::with_capacity_and_hasher(layout, Default::default());
    let mut open = vec![false; func.num_blocks()];
    // (block, whether its instructions were visited)
    let mut stack = vec![(entry, false)];
    let mut changed = false;
    while let Some((block, visited)) = stack.pop() {
        if visited {
            open[block.index()] = false;
            continue;
        }
        open[block.index()] = true;
        stack.push((block, true));
        let mut i = 0;
        while let Some(&inst) = func.block_insts(block).get(i) {
            let data = func.inst_data(inst);
            if data.is_pinned() {
                i += 1;
                continue;
            }
            let key = canonical(func, data);
            match table.entry(key) {
                Entry::Occupied(entry) if open[func.inst_block(*entry.get()).index()] => {
                    let existing = *entry.get();
                    let n = func.inst_results(inst).len();
                    for k in 0..n.min(func.inst_results(existing).len()) {
                        let result = func.inst_results(inst)[k];
                        let existing = func.inst_results(existing)[k];
                        func.replace_all_uses(result, existing);
                    }
                    // The next instruction moves into position `i`.
                    func.remove_inst(inst);
                    changed = true;
                }
                Entry::Occupied(mut entry) => {
                    *entry.get_mut() = inst;
                    i += 1;
                }
                Entry::Vacant(entry) => {
                    entry.insert(inst);
                    i += 1;
                }
            }
        }
        for &child in children[block].iter().rev() {
            stack.push((child, false));
        }
    }
    changed
}

/// `data` with operands resolved, and operands of commutative operations
/// sorted, so `iadd v1, v2` and `iadd v2, v1` get the same key. Integer
/// comparisons reverse their predicate when their operands are swapped.
fn canonical(func: &Function, data: &InstData) -> InstData {
    let mut key = data.clone();
    key.for_each_operand_mut(|v| *v = func.resolve(*v));
    match &mut key {
        InstData::Binary { op, args } if is_commutative(*op) => args.sort_by_key(|v| v.index()),
        InstData::Overflow { op, args } if is_commutative_overflow(*op) => {
            args.sort_by_key(|v| v.index())
        }
        InstData::Icmp { cond, args } if args[0].index() > args[1].index() => {
            args.swap(0, 1);
            *cond = reverse(*cond);
        }
        _ => {}
    }
    key
}

/// Swap the operands without negating the predicate. Signedness is retained.
pub(crate) fn reverse(cond: IntCC) -> IntCC {
    match cond {
        IntCC::Eq | IntCC::Ne => cond,
        IntCC::Slt => IntCC::Sgt,
        IntCC::Sle => IntCC::Sge,
        IntCC::Sgt => IntCC::Slt,
        IntCC::Sge => IntCC::Sle,
        IntCC::Ult => IntCC::Ugt,
        IntCC::Ule => IntCC::Uge,
        IntCC::Ugt => IntCC::Ult,
        IntCC::Uge => IntCC::Ule,
    }
}

pub(crate) fn is_commutative(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Iadd
            | BinaryOp::Imul
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Xor
            | BinaryOp::Umulhi
            | BinaryOp::Smulhi
            // Only the payload of a NaN result could depend on operand
            // order, and the IR leaves that unspecified.
            | BinaryOp::Fadd
            | BinaryOp::Fmul
    )
}

pub(crate) fn is_commutative_overflow(op: OverflowOp) -> bool {
    matches!(
        op,
        OverflowOp::Uadd | OverflowOp::Sadd | OverflowOp::Umul | OverflowOp::Smul
    )
}
