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

pub(crate) fn fold_branches(func: &mut Function) -> bool {
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

pub(crate) fn skip_forwarding_blocks(func: &mut Function) -> bool {
    // Removing a forwarding block's parameters is only legal when all
    // their uses are in that block's jump. Resolve aliases before checking.
    // Only a jump-only block can forward, and one without parameters has
    // nothing to escape, so the use scan is skipped when no block has both.
    let mut escaping = vec![false; func.num_blocks()];
    let mut forwarding = vec![false; func.num_blocks()];
    let mut any = false;
    for block in func.blocks() {
        if let [only] = func.block_insts(block)
            && matches!(func.inst_data(*only), InstData::Jump { .. })
            && !func.block_params(block).is_empty()
        {
            forwarding[block.index()] = true;
            any = true;
        }
    }
    if any {
        for (block, inst) in func.layout_insts() {
            func.inst_data(inst).for_each_operand(|value| {
                if let ValueDef::Param(def, _) = func.value_def(func.resolve(value))
                    && def != block
                    && forwarding[def.index()]
                {
                    escaping[def.index()] = true;
                }
            });
        }
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
    // Only reachability is needed: walk the terminators' edges from the entry.
    let Some(entry) = func.entry_block() else {
        return false;
    };
    let mut reachable = vec![false; func.num_blocks()];
    reachable[entry.index()] = true;
    let mut stack = vec![entry];
    while let Some(block) = stack.pop() {
        if let Some(&term) = func.block_insts(block).last() {
            for dest in func.inst_data(term).branch_dests() {
                if !std::mem::replace(&mut reachable[dest.block.index()], true) {
                    stack.push(dest.block);
                }
            }
        }
    }
    let dead: Vec<Block> = func
        .blocks()
        .filter(|&b| !reachable[b.index()])
        .collect();
    func.remove_blocks(&dead);
    !dead.is_empty()
}

fn merge_blocks(func: &mut Function) -> bool {
    merge_small_blocks(func, usize::MAX)
}

/// Limit movement through chains for the bounded pipeline. The legacy pass
/// keeps its unlimited merge policy.
pub(crate) fn merge_small_blocks(func: &mut Function, max_insts: usize) -> bool {
    // Count branch edges, not unique predecessors: duplicate brif/switch
    // destinations must still prevent a merge. Block indices remain stable
    // when blocks leave the layout, so this census lasts for this invocation.
    let mut edges = vec![0u32; func.num_blocks()];
    for block in func.blocks() {
        if let Some(&term) = func.block_insts(block).last() {
            for dest in func.inst_data(term).branch_dests() {
                edges[dest.block.index()] += 1;
            }
        }
    }
    let mut changed = false;
    // Merged blocks leave the layout together at the end; until then they
    // are empty, which is all the walk below observes of them.
    let mut merged = Vec::new();
    // Only the absorbing predecessor acquires a new terminator. Incoming
    // counts for every surviving block stay unchanged, so earlier rejected
    // predecessors cannot become candidates. Retry pred before advancing to
    // preserve the old first-eligible-layout order without restarting a scan.
    let blocks: Vec<Block> = func.blocks().collect();
    for pred in blocks {
        loop {
            // A block consumed from later in the snapshot has no instructions.
            let Some(&term) = func.block_insts(pred).last() else {
                break;
            };
            let dest = match func.inst_data(term) {
                InstData::Jump { dest }
                    if dest.block != pred
                        && edges[dest.block.index()] == 1
                        && Some(dest.block) != func.entry_block() =>
                {
                    dest.clone()
                }
                _ => break,
            };
            let block = dest.block;
            if func
                .block_insts(pred)
                .len()
                .saturating_add(func.block_insts(block).len())
                > max_insts
            {
                break;
            }
            for (&param, &arg) in func.block_params(block).to_vec().iter().zip(&dest.args) {
                func.replace_all_uses(param, arg);
            }
            func.remove_inst(term);
            // This is the only deleted edge. Parameter aliases affect values,
            // not block destinations, and moving instructions preserves all
            // their outgoing edges (including edges back to pred). The removed
            // block cannot have a self-edge: its sole incoming edge was jump.
            edges[block.index()] -= 1;
            for inst in func.block_insts(block).to_vec() {
                func.move_inst(inst, keel_ir::InsertPoint::End(pred));
            }
            merged.push(block);
            changed = true;
        }
    }
    func.remove_blocks(&merged);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_cycle_preserves_entry_and_stops_at_the_new_self_edge() {
        let mut module = keel_ir::parse(
            "func @f() {
block0:
    jump block1
block1:
    v0 = iconst.i64 1
    jump block2
block2:
    v1 = iconst.i64 2
    jump block1
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let f = &mut module.functions[0];
        let entry = f.entry_block().unwrap();
        let entry_jump = f.block_insts(entry)[0];
        let cycle = f.inst_data(entry_jump).branch_dests()[0].block;
        let first = f.block_insts(cycle)[0];
        let other = f.blocks().last().unwrap();
        let expected = vec![first, f.block_insts(other)[0], f.block_insts(other)[1]];
        assert!(merge_blocks(f));
        keel_ir::verify(f).unwrap();
        assert_eq!(f.blocks().collect::<Vec<_>>(), [entry, cycle]);
        assert_eq!(f.block_insts(entry), [entry_jump]);
        assert_eq!(f.inst_data(entry_jump).branch_dests()[0].block, cycle);
        assert_eq!(f.block_insts(cycle), expected);
        let term = *f.block_insts(cycle).last().unwrap();
        assert_eq!(f.inst_data(term).branch_dests()[0].block, cycle);
        assert!(!merge_blocks(f));
    }

    #[test]
    fn merging_snapshot_handles_both_consumed_and_already_visited_blocks() {
        // No outer cleanup round may repair an incomplete candidate walk.
        // Every layout permutation includes an earlier rejected return and/or
        // a later consumed snapshot entry. The entry's brif prevents it from
        // absorbing the chain, leaving an unchanged prefix on each old rescan.
        for order in [
            [1, 2, 3],
            [1, 3, 2],
            [2, 1, 3],
            [2, 3, 1],
            [3, 1, 2],
            [3, 2, 1],
        ] {
            let mut src = String::from(
                "func @f(i64, i8) -> i64 {\n\
                 block0(v0: i64, v1: i8):\n brif v1, block3(v0), block4(v0)\n",
            );
            for block in order {
                src.push_str(match block {
                    1 => "block1(v2: i64):\n return v2\n",
                    2 => "block2(v3: i64):\n v4 = iadd v3, v0\n jump block1(v4)\n",
                    3 => "block3(v5: i64):\n v6 = isub v5, v0\n jump block2(v6)\n",
                    _ => unreachable!(),
                });
            }
            src.push_str("block4(v7: i64):\n return v7\n}");
            let mut module = keel_ir::parse(&src).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let f = &mut module.functions[0];
            let entry = f.entry_block().unwrap();
            let term = *f.block_insts(entry).last().unwrap();
            let start = f.inst_data(term).branch_dests()[0].block;
            let other = f.inst_data(term).branch_dests()[1].block;
            let middle_term = *f.block_insts(start).last().unwrap();
            let middle = f.inst_data(middle_term).branch_dests()[0].block;
            let tail_term = *f.block_insts(middle).last().unwrap();
            let tail = f.inst_data(tail_term).branch_dests()[0].block;
            let expected = vec![
                f.block_insts(start)[0],
                f.block_insts(middle)[0],
                f.block_insts(tail)[0],
            ];
            assert!(merge_blocks(f), "layout {order:?}");
            keel_ir::verify(f).unwrap();
            assert_eq!(f.blocks().collect::<Vec<_>>(), [entry, start, other]);
            assert_eq!(f.block_insts(start), expected);
            assert!(f.block_insts(middle).is_empty());
            assert!(f.block_insts(tail).is_empty());
            assert!(!merge_blocks(f), "layout {order:?} needs another walk");
        }
    }

    #[test]
    fn merging_refreshes_aliased_arguments_and_preserves_instruction_locations() {
        let mut module = keel_ir::parse(
            "func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iadd v0, v0
    jump block2(v1)
block1(v2: i64):
    v3 = isub v2, v0
    return v3
block2(v4: i64):
    v5 = iadd v4, v0
    jump block1(v5)
}",
        )
        .unwrap();
        module.files.push("cfg-worklist.c".into());
        let f = &mut module.functions[0];
        let entry = f.entry_block().unwrap();
        let first = f.block_insts(entry)[0];
        let original = f.first_result(first);
        let arg = f.block_params(entry)[0];
        // Retain an actual in-memory alias; printing/reparsing loses it.
        f.replace_all_uses(original, arg);
        let jump = *f.block_insts(entry).last().unwrap();
        let middle = f.inst_data(jump).branch_dests()[0].block;
        let middle_param = f.block_params(middle)[0];
        let middle_inst = f.block_insts(middle)[0];
        let jump = *f.block_insts(middle).last().unwrap();
        let tail = f.inst_data(jump).branch_dests()[0].block;
        let tail_param = f.block_params(tail)[0];
        let tail_insts = f.block_insts(tail).to_vec();
        let expected = vec![first, middle_inst, tail_insts[0], tail_insts[1]];
        for (line, &inst) in expected.iter().enumerate() {
            f.set_inst_loc(
                inst,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: line as u32 + 1,
                    col: 7,
                    scope: None,
                }),
            );
        }
        let locations: Vec<_> = expected.iter().map(|&inst| f.inst_loc(inst)).collect();
        keel_ir::verify(f).unwrap();
        assert!(merge_blocks(f));
        keel_ir::verify(f).unwrap();
        assert_eq!(f.blocks().collect::<Vec<_>>(), [entry]);
        assert_eq!(f.block_insts(entry), expected);
        assert_eq!(f.resolve(original), arg);
        assert_eq!(f.resolve(middle_param), arg);
        assert_eq!(f.resolve(tail_param), f.first_result(middle_inst));
        assert_eq!(
            expected
                .iter()
                .map(|&inst| f.inst_loc(inst))
                .collect::<Vec<_>>(),
            locations
        );
        assert!(!merge_blocks(f));
        keel_ir::verify_module(&module).unwrap();
    }
}
