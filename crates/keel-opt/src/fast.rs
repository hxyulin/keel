//! Fixed-work cleanup for the opt-in Fast tier. There is no convergence loop,
//! dominator tree, loop analysis, use graph, or cross-block value table.
//!
//! Budgets limit retained local expressions and instruction movement, rather
//! than skipping large functions. Every reachable instruction is visited.
//! The IR's Vec-based removal and alias APIs still determine mutation costs;
//! these limits are not a claim of strict linear time for arbitrary input.

use crate::{Pass, instcombine, simplify_cfg};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Block, EntityRef, Function, Inst, InstData};

const LOCAL_EXPRESSIONS: usize = 256;
const MERGED_INSTRUCTIONS: usize = 64;

/// DFS directly over terminators, without building predecessor lists or
/// deduplicating a potentially large switch's successors by linear search.
fn reverse_postorder(func: &Function) -> Vec<Block> {
    let Some(entry) = func.entry_block() else {
        return Vec::new();
    };
    let mut seen = vec![false; func.num_blocks()];
    let mut stack = vec![(entry, 0)];
    let mut order = Vec::new();
    seen[entry.index()] = true;
    while let Some((block, next)) = stack.last_mut() {
        let block = *block;
        let dests = func
            .block_insts(block)
            .last()
            .map_or(&[][..], |&inst| func.inst_data(inst).branch_dests());
        if let Some(dest) = dests.get(*next) {
            *next += 1;
            if !std::mem::replace(&mut seen[dest.block.index()], true) {
                stack.push((dest.block, 0));
            }
        } else {
            order.push(block);
            stack.pop();
        }
    }
    order.reverse();
    order
}

pub struct Cfg;
impl Pass for Cfg {
    fn name(&self) -> &'static str {
        "fast-cfg"
    }
    fn description(&self) -> &'static str {
        "one CFG cleanup sweep, merging chains of at most 64 instructions"
    }
    fn run(&self, func: &mut Function) -> bool {
        let mut changed = simplify_cfg::fold_branches(func);
        changed |= simplify_cfg::skip_forwarding_blocks(func);
        let mut reachable = vec![false; func.num_blocks()];
        for block in reverse_postorder(func) {
            reachable[block.index()] = true;
        }
        for block in func.blocks().collect::<Vec<_>>() {
            if !reachable[block.index()] {
                func.remove_block(block);
                changed = true;
            }
        }
        changed | simplify_cfg::merge_small_blocks(func, MERGED_INSTRUCTIONS)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

pub struct Local;
impl Pass for Local {
    fn name(&self) -> &'static str {
        "fast-local"
    }
    fn description(&self) -> &'static str {
        "one forward simplification sweep with a bounded block-local expression table"
    }
    fn run(&self, func: &mut Function) -> bool {
        let mut changed = false;
        let mut table: FxHashMap<InstData, Inst> = FxHashMap::default();
        let mut quotients = instcombine::Quotients::default();
        for block in reverse_postorder(func) {
            table.clear();
            quotients.clear();
            for inst in func.block_insts(block).to_vec() {
                if quotients.len() == LOCAL_EXPRESSIONS {
                    quotients.clear();
                }
                changed |= instcombine::reuse_quotient(func, inst, &mut quotients);
                changed |= instcombine::simplify_once(func, inst);
                if func.is_removed(inst) || func.inst_data(inst).is_pinned() {
                    continue;
                }
                // Keys only refer to earlier definitions in this block. The
                // forward sweep never rewrites them again, so the table stays
                // valid after later substitutions. Multi-result pure ops are
                // safe to reuse with their entire result signature.
                let mut key = func.inst_data(inst).clone();
                crate::canonicalize::normalize(func, &mut key);
                if let Some(&earlier) = table.get(&key) {
                    let earlier = func.inst_results(earlier).to_vec();
                    for (result, value) in func.inst_results(inst).to_vec().into_iter().zip(earlier)
                    {
                        func.replace_all_uses(result, value);
                    }
                    func.remove_inst(inst);
                    changed = true;
                } else {
                    if table.len() == LOCAL_EXPRESSIONS {
                        table.clear();
                    }
                    table.insert(key, inst);
                }
            }
        }
        changed
    }
    fn deterministic(&self) -> bool {
        true
    }
}
