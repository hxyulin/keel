//! Natural loops, for loop nesting depth.
//!
//! A back edge is an edge `b -> h` where `h` dominates `b`; `h` is a loop
//! header, and the loop is `h` plus every block that reaches `b` without
//! going through `h`. Back edges to the same header form one loop. A
//! block's depth is the number of loops containing it.

use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{Block, EntityRef, Function};

pub struct LoopDepth {
    depth: Vec<u32>,
}

impl LoopDepth {
    pub fn compute(func: &Function, cfg: &ControlFlowGraph, domtree: &DominatorTree) -> Self {
        let mut depth = vec![0; func.num_blocks()];
        for &header in domtree.rpo() {
            let latches: Vec<Block> = cfg
                .preds(header)
                .iter()
                .copied()
                .filter(|&pred| domtree.dominates(header, pred))
                .collect();
            if latches.is_empty() {
                continue;
            }
            // Walk backwards from the latches; the header stops the walk.
            let mut in_loop = vec![false; func.num_blocks()];
            in_loop[header.index()] = true;
            let mut stack = latches;
            while let Some(block) = stack.pop() {
                if std::mem::replace(&mut in_loop[block.index()], true) {
                    continue;
                }
                stack.extend(
                    cfg.preds(block)
                        .iter()
                        .filter(|&&p| domtree.is_reachable(p)),
                );
            }
            for (d, inside) in depth.iter_mut().zip(in_loop) {
                *d += u32::from(inside);
            }
        }
        Self { depth }
    }

    pub fn depth(&self, block: Block) -> u32 {
        self.depth[block.index()]
    }
}
