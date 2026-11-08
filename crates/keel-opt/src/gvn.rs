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
//! may see different memory. A dominating conditional edge also determines
//! later branches on the same SSA condition, without changing its integer value.

use crate::scratch::Scratch;
use keel_ir::fxhash::{FxBuildHasher, FxHashMap};
use keel_ir::{BinaryOp, EntityRef, Function, Inst, InstData, IntCC, OverflowOp, Value};
use std::borrow::Cow;
use std::hash::BuildHasher;

pub fn run(func: &mut Function) -> bool {
    number(func, None)
}

/// Reuse expressions exposed by memory forwarding after GCM, without merging
/// its per-block copies into long register live ranges. Only computations
/// dependent on these removed reads may merge. Addresses stay at their uses.
/// Literal IDs in keys are semantic representatives only; operands stay local.
pub(crate) fn run_late(func: &mut Function, reused: &[Value]) -> bool {
    // Late merging cannot be rescheduled afterwards. Keep its register-pressure
    // and scratch costs bounded, including arena entries left by earlier passes.
    const MAX_INSTS: usize = 2048;
    const MAX_VALUES: usize = 8192;
    if func.num_values() > MAX_VALUES || func.layout_insts().take(MAX_INSTS + 1).count() > MAX_INSTS
    {
        return false;
    }
    number(func, Some(reused))
}

fn number(func: &mut Function, reused: Option<&[Value]>) -> bool {
    let keep_literals = reused.is_some();
    let mut affected = reused.map(|values| {
        let mut flags = vec![false; func.num_values()];
        for v in values {
            flags[v.index()] = true;
        }
        flags
    });
    let literals = keep_literals.then(|| {
        let mut representatives = FxHashMap::default();
        let mut ids: Vec<_> = (0..func.num_values()).map(Value::new).collect();
        for (_, i) in func.layout_insts() {
            if let InstData::Iconst { ty, imm } = *func.inst_data(i) {
                let v = func.first_result(i);
                ids[v.index()] = *representatives.entry((ty, imm)).or_insert(v);
            }
        }
        ids
    });
    let analysis = crate::analysis::get(func);
    let domtree = &analysis.dom;
    let Some(entry) = func.entry_block() else {
        return false;
    };
    // Folding branches only deletes edges. The original dominators remain
    // valid, and their predecessor census may miss new facts but cannot
    // establish one from an edge that did not dominate the original block.
    let children = domtree.children();

    // Entries are never removed. One is in scope while its block is open on
    // the walk, i.e. while that block dominates the current one; a stale
    // entry from a closed sibling is simply overwritten.
    //
    // The table is open-addressed (key hash, instruction) slots, at most half
    // full, keyed by each instruction's `canonical` form. Keys are recomputed
    // to compare rather than stored: an instruction's operands are defined
    // above it on the walk, so none of them is replaced after it is numbered,
    // and its key stays what it was.
    let layout: usize = func.blocks().map(|b| func.block_insts(b).len()).sum();
    let mask = (2 * layout).next_power_of_two().max(16) - 1;
    // No instruction has the largest index; it marks an empty slot.
    let empty = Inst::new(u32::MAX as usize);
    let mut table = Scratch::filled(mask + 1, (0u32, empty));
    let mut open = Scratch::filled(func.num_blocks(), false);
    // A true branch proves nonzero, not the integer value one. Keep these
    // facts separate from value numbering and use them only for branches.
    let mut truth = vec![None; func.num_values()];
    // (block, whether visited, condition fact to restore on leaving)
    let mut stack = vec![(entry, false, None)];
    let mut changed = false;
    while let Some((block, visited, restore)) = stack.pop() {
        if visited {
            open[block.index()] = false;
            if let Some((condition, previous)) = restore {
                truth[condition] = previous;
            }
            continue;
        }
        open[block.index()] = true;
        // A unique predecessor is not a unique edge when both branch arms
        // enter the same block. Loop headers and joins cannot acquire a fact
        // from just one of their incoming paths.
        let restore = if block != entry
            && let [pred] = analysis.cfg.preds(block)
            && let Some(&term) = func.block_insts(*pred).last()
            && let InstData::Brif { cond, dests } = func.inst_data(term)
            && dests[0].block != dests[1].block
        {
            let condition = func.resolve(*cond).index();
            Some((condition, truth[condition].replace(dests[0].block == block)))
        } else {
            None
        };
        stack.push((block, true, restore));
        let mut i = 0;
        while let Some(&inst) = func.block_insts(block).get(i) {
            let data = func.inst_data(inst);
            let branch_affected = affected.as_ref().is_none_or(
                |flags| matches!(data, InstData::Brif {cond,..} if flags[cond.index()]),
            );
            if branch_affected
                && let InstData::Brif { cond, dests } = data
                && let Some(taken) = truth[func.resolve(*cond).index()]
            {
                let dest = dests[usize::from(!taken)].clone();
                *func.inst_data_mut(inst) = InstData::Jump { dest };
                changed = true;
                i += 1;
                continue;
            }
            let mut has_operand = false;
            if keep_literals {
                data.for_each_operand(|_| has_operand = true);
            }
            if data.is_pinned()
                || (keep_literals
                    && (!has_operand
                        || matches!(
                            data,
                            InstData::Binary {
                                op: BinaryOp::Ptradd,
                                ..
                            }
                        )))
            {
                i += 1;
                continue;
            }
            // Only computations depending on reads removed by this invocation
            // may merge late. Keep unrelated GCM placement decisions intact.
            let can_merge = if let Some(affected) = &mut affected {
                let mut dependent = false;
                data.for_each_operand(|v| dependent |= affected[v.index()]);
                for v in func.inst_results(inst) {
                    affected[v.index()] = dependent;
                }
                dependent
            } else {
                true
            };
            let key = canonical(func, data, literals.as_deref());
            let hash = FxBuildHasher::default().hash_one(&*key);
            let tag = (hash >> 32) as u32;
            let mut slot = hash as usize & mask;
            let existing = loop {
                let (t, existing) = table[slot];
                if existing == empty {
                    break None;
                }
                if t == tag
                    && *canonical(func, func.inst_data(existing), literals.as_deref()) == *key
                {
                    break Some(existing);
                }
                slot = (slot + 1) & mask;
            };
            match existing {
                Some(existing) if open[func.inst_block(existing).index()] => {
                    if !can_merge {
                        i += 1;
                        continue;
                    }
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
                _ => {
                    table[slot] = (tag, inst);
                    i += 1;
                }
            }
        }
        for &child in children[block].iter().rev() {
            stack.push((child, false, None));
        }
    }
    changed
}

/// `data` with operands resolved, and operands of commutative operations
/// sorted, so `iadd v1, v2` and `iadd v2, v1` get the same key. Integer
/// comparisons reverse their predicate when their operands are swapped.
/// Most instructions are already in that form and lend themselves.
pub(crate) fn canonical<'a>(
    func: &Function,
    data: &'a InstData,
    literals: Option<&[Value]>,
) -> Cow<'a, InstData> {
    let resolve = |v| {
        let v = func.resolve(v);
        literals.map_or(v, |ids| ids[v.index()])
    };
    let mut resolved = true;
    data.for_each_operand(|v| resolved &= resolve(v) == v);
    let ordered = match data {
        InstData::Binary { op, args } if is_commutative(*op) => args[0].index() <= args[1].index(),
        InstData::Overflow { op, args } if is_commutative_overflow(*op) => {
            args[0].index() <= args[1].index()
        }
        InstData::Icmp { args, .. } => args[0].index() <= args[1].index(),
        _ => true,
    };
    if resolved && ordered {
        return Cow::Borrowed(data);
    }
    let mut key = data.clone();
    key.for_each_operand_mut(|v| *v = resolve(*v));
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
    Cow::Owned(key)
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
