//! Global value numbering: a pure instruction equivalent to one that
//! dominates it is replaced by that one.
//!
//! Blocks are walked in dominator-tree preorder with a scoped hash table
//! from instruction (with operands resolved) to the value it computes.
//! Entering a block opens a scope; leaving it forgets what the block added,
//! so a lookup only ever finds an instruction that dominates the current
//! one, and the IR stays valid without moving anything.
//!
//! Merging two identical computations in sibling branches (hoisting them to
//! a common dominator) would need code motion first; that is GCM's job.
//! Pinned instructions are never numbered: two loads of the same address
//! may see different memory.

use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, EntityRef, Function, Inst, InstData, IntCC, OverflowOp};

pub fn run(func: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(func);
    let domtree = DominatorTree::compute(func, &cfg);
    let Some(entry) = func.entry_block() else {
        return false;
    };
    let children = domtree.children();

    let mut table: FxHashMap<InstData, Inst> = FxHashMap::default();
    // Keys added, in order, so a scope can be popped.
    let mut added: Vec<InstData> = Vec::new();
    // (block, scope start in `added`, whether its instructions were visited)
    let mut stack = vec![(entry, 0, false)];
    let mut changed = false;
    while let Some((block, scope, visited)) = stack.pop() {
        if visited {
            for key in added.drain(scope..) {
                table.remove(&key);
            }
            continue;
        }
        stack.push((block, added.len(), true));
        for inst in func.block_insts(block).to_vec() {
            let data = func.inst_data(inst);
            if data.is_pinned() {
                continue;
            }
            let key = canonical(func, data);
            match table.get(&key) {
                Some(&existing) => {
                    let pairs: Vec<_> = func
                        .inst_results(inst)
                        .iter()
                        .copied()
                        .zip(func.inst_results(existing).to_vec())
                        .collect();
                    for (result, existing) in pairs {
                        func.replace_all_uses(result, existing);
                    }
                    func.remove_inst(inst);
                    changed = true;
                }
                None => {
                    table.insert(key.clone(), inst);
                    added.push(key);
                }
            }
        }
        for &child in children[block].iter().rev() {
            stack.push((child, 0, false));
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
