//! Control-flow graph edges and the dominator tree.
//!
//! Uses the iterative algorithm from Cooper, Harvey & Kennedy, "A Simple,
//! Fast Dominance Algorithm" (2001): walk blocks in reverse postorder and
//! intersect the dominators of each block's predecessors until nothing changes.

use crate::{Block, EntityRef, Function};

/// Predecessors and successors of each block, read from branch instructions.
/// Edge lists are packed into two arrays (no allocation per block): block
/// `b`'s successors are `succs[succ_range[b]]` and its predecessors
/// `preds[pred_start[b]..pred_start[b + 1]]`.
pub struct ControlFlowGraph {
    succ_range: Vec<(u32, u32)>,
    succs: Vec<Block>,
    pred_start: Vec<u32>,
    preds: Vec<Block>,
}

impl ControlFlowGraph {
    pub fn compute(func: &Function) -> Self {
        let n = func.num_blocks();
        let mut succ_range = vec![(0, 0); n];
        let mut succs = Vec::new();
        let mut pred_count = vec![0u32; n + 1];
        for block in func.blocks() {
            let start = succs.len();
            for &inst in func.block_insts(block) {
                for dest in func.inst_data(inst).branch_dests() {
                    // A block reached twice (e.g. both arms of a `brif`) is
                    // still one predecessor.
                    if !succs[start..].contains(&dest.block) {
                        succs.push(dest.block);
                        pred_count[dest.block.index()] += 1;
                    }
                }
            }
            succ_range[block.index()] = (index_u32(start), index_u32(succs.len()));
        }
        // Predecessors in the same order as before: sources in layout
        // order, each source's successors in branch order.
        let mut pred_start = pred_count;
        let mut total = 0u32;
        for slot in &mut pred_start {
            let count = *slot;
            *slot = total;
            total += count;
        }
        let mut fill = pred_start.clone();
        let mut preds = vec![Block::new(0); total as usize];
        for block in func.blocks() {
            let (start, end) = succ_range[block.index()];
            for &succ in &succs[start as usize..end as usize] {
                let slot = &mut fill[succ.index()];
                preds[*slot as usize] = block;
                *slot += 1;
            }
        }
        Self {
            succ_range,
            succs,
            pred_start,
            preds,
        }
    }

    /// Reachable blocks in reverse postorder, without computing dominators.
    /// Successor traversal matches [`DominatorTree::compute`], so callers may
    /// use this order for instruction numbering and assembly emission.
    pub fn reverse_postorder(&self, func: &Function) -> Vec<Block> {
        let Some(entry) = func.entry_block() else {
            return Vec::new();
        };
        let mut order = Vec::new();
        let mut visited = vec![false; func.num_blocks()];
        let mut stack = vec![(entry, 0usize)];
        visited[entry.index()] = true;
        while let Some((block, next)) = stack.last_mut() {
            let block = *block;
            if let Some(&succ) = self.succs(block).get(*next) {
                *next += 1;
                if !visited[succ.index()] {
                    visited[succ.index()] = true;
                    stack.push((succ, 0));
                }
            } else {
                order.push(block);
                stack.pop();
            }
        }
        order.reverse();
        order
    }

    pub fn preds(&self, block: Block) -> &[Block] {
        let i = block.index();
        &self.preds[self.pred_start[i] as usize..self.pred_start[i + 1] as usize]
    }

    pub fn succs(&self, block: Block) -> &[Block] {
        let (start, end) = self.succ_range[block.index()];
        &self.succs[start as usize..end as usize]
    }
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("CFG edge count overflow")
}

pub struct DominatorTree {
    /// Reachable blocks in reverse postorder; the entry block comes first.
    rpo: Vec<Block>,
    /// Position in `rpo` plus one; 0 means unreachable.
    rpo_number: Vec<u32>,
    /// Immediate dominator. `None` for the entry and unreachable blocks.
    idom: Vec<Option<Block>>,
}

impl DominatorTree {
    pub fn compute(func: &Function, cfg: &ControlFlowGraph) -> Self {
        let n = func.num_blocks();
        let mut tree = Self {
            rpo: Vec::new(),
            rpo_number: vec![0; n],
            idom: vec![None; n],
        };
        let Some(entry) = func.entry_block() else {
            return tree;
        };
        tree.rpo = cfg.reverse_postorder(func);
        for (i, &block) in tree.rpo.iter().enumerate() {
            tree.rpo_number[block.index()] = i as u32 + 1;
        }

        // The entry dominates itself while iterating; reset to `None` at the end.
        tree.idom[entry.index()] = Some(entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &block in &tree.rpo[1..] {
                let mut new_idom = None;
                for &pred in cfg.preds(block) {
                    if tree.idom[pred.index()].is_none() {
                        continue; // unreachable, or not processed yet
                    }
                    new_idom = Some(match new_idom {
                        None => pred,
                        Some(current) => tree.intersect(current, pred),
                    });
                }
                if new_idom != tree.idom[block.index()] {
                    tree.idom[block.index()] = new_idom;
                    changed = true;
                }
            }
        }
        tree.idom[entry.index()] = None;
        tree
    }

    fn intersect(&self, mut a: Block, mut b: Block) -> Block {
        while a != b {
            while self.rpo_number[a.index()] > self.rpo_number[b.index()] {
                a = self.idom[a.index()].expect("processed block has an idom");
            }
            while self.rpo_number[b.index()] > self.rpo_number[a.index()] {
                b = self.idom[b.index()].expect("processed block has an idom");
            }
        }
        a
    }

    /// The deepest block dominating both `a` and `b`, which must be
    /// reachable.
    pub fn common_dominator(&self, a: Block, b: Block) -> Block {
        assert!(self.is_reachable(a) && self.is_reachable(b));
        self.intersect(a, b)
    }

    /// Children of each block in the dominator tree, indexed by block.
    pub fn children(&self) -> Vec<Vec<Block>> {
        let mut children = vec![Vec::new(); self.idom.len()];
        for &block in &self.rpo {
            if let Some(idom) = self.idom[block.index()] {
                children[idom.index()].push(block);
            }
        }
        children
    }

    pub fn is_reachable(&self, block: Block) -> bool {
        self.rpo_number[block.index()] != 0
    }

    pub fn idom(&self, block: Block) -> Option<Block> {
        self.idom[block.index()]
    }

    /// Reachable blocks in reverse postorder.
    pub fn rpo(&self) -> &[Block] {
        &self.rpo
    }

    /// Whether every path from the entry to `b` passes through `a`.
    /// A block dominates itself. Unreachable blocks dominate nothing.
    pub fn dominates(&self, a: Block, mut b: Block) -> bool {
        if !self.is_reachable(a) || !self.is_reachable(b) {
            return false;
        }
        // Walk up from `b`; dominators always have smaller RPO numbers.
        while self.rpo_number[b.index()] > self.rpo_number[a.index()] {
            b = self.idom[b.index()].expect("non-entry reachable block has an idom");
        }
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_rpo_preserves_successor_order_loops_and_reachability() {
        let module = crate::parse(
            "\
            func @f(i8) -> i64 {
            block0(v0: i8):
                brif v0, block1, block2
            block1:
                brif v0, block3, block1
            block2:
                jump block3
            block3:
                v1 = iconst.i64 7
                return v1
            block4:
                jump block1
            }",
        )
        .unwrap();
        let func = &module.functions[0];
        let cfg = ControlFlowGraph::compute(func);
        let expected: Vec<_> = [0, 2, 1, 3].map(Block::new).into();
        assert_eq!(cfg.reverse_postorder(func), expected);
        let dom = DominatorTree::compute(func, &cfg);
        assert_eq!(dom.rpo(), expected);
        assert_eq!(dom.idom(Block::new(3)), Some(Block::new(0)));
        assert!(!dom.is_reachable(Block::new(4)));
    }

    /// The packed edge lists hold exactly what per-block lists built the
    /// original way do, in the same order: duplicate destinations, edges
    /// from blocks out of index order, unreachable and removed blocks.
    #[test]
    fn packed_edges_match_per_block_lists() {
        let module = crate::parse(
            "\
            func @f(i8, i64) -> i64 {
            block0(v0: i8, v1: i64):
                brif v0, block3, block1
            block3:
                switch v1, block2, [1: block1, 2: block2, 3: block5, 4: block1]
            block1:
                brif v0, block1, block1
            block2:
                brif v0, block4, block3
            block5:
                jump block2
            block4:
                return v1
            block6:
                jump block4
            }",
        )
        .unwrap();
        let mut func = module.functions[0].clone();
        let check = |func: &Function| {
            let n = func.num_blocks();
            let (mut preds, mut succs) = (vec![Vec::new(); n], vec![Vec::new(); n]);
            for block in func.blocks() {
                for &inst in func.block_insts(block) {
                    for dest in func.inst_data(inst).branch_dests() {
                        if !succs[block.index()].contains(&dest.block) {
                            succs[block.index()].push(dest.block);
                            preds[dest.block.index()].push(block);
                        }
                    }
                }
            }
            let cfg = ControlFlowGraph::compute(func);
            for b in 0..n {
                assert_eq!(cfg.preds(Block::new(b)), &preds[b][..], "preds of block{b}");
                assert_eq!(cfg.succs(Block::new(b)), &succs[b][..], "succs of block{b}");
            }
            assert!(
                preds.iter().any(|p| p.len() > 2),
                "exercise several predecessors"
            );
        };
        check(&func);
        func.remove_block(Block::new(6));
        check(&func);
    }

    #[test]
    fn cfg_rpo_is_empty_without_an_entry() {
        let func = Function::new("empty", crate::Signature::new(&[], &[]));
        let cfg = ControlFlowGraph::compute(&func);
        assert!(cfg.reverse_postorder(&func).is_empty());
        assert!(DominatorTree::compute(&func, &cfg).rpo().is_empty());
    }
}
