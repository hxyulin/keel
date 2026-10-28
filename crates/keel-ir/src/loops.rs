//! Natural loops, for loop nesting depth.
//!
//! A back edge is an edge `b -> h` where `h` dominates `b`; `h` is a loop
//! header, and the loop is `h` plus every block that reaches `b` without
//! going through `h`. Back edges to the same header form one loop. A
//! block's depth is the number of loops containing it.

use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{Block, EntityRef, Function};

#[derive(Debug)]
pub struct LoopDepth {
    depth: Vec<u32>,
}

impl LoopDepth {
    pub fn compute(func: &Function, cfg: &ControlFlowGraph, domtree: &DominatorTree) -> Self {
        let mut depth = vec![0; func.num_blocks()];
        // Loop membership is stamped with the header's RPO position, so one
        // array serves every loop without clearing.
        let mut stamp = vec![0u32; func.num_blocks()];
        let mut stack = Vec::new();
        for (number, &header) in (1u32..).zip(domtree.rpo()) {
            stack.extend(
                cfg.preds(header)
                    .iter()
                    .copied()
                    .filter(|&pred| domtree.dominates(header, pred)),
            );
            if stack.is_empty() {
                continue;
            }
            // Walk backwards from the latches; the header stops the walk.
            stamp[header.index()] = number;
            depth[header.index()] += 1;
            while let Some(block) = stack.pop() {
                if std::mem::replace(&mut stamp[block.index()], number) == number {
                    continue;
                }
                depth[block.index()] += 1;
                stack.extend(
                    cfg.preds(block)
                        .iter()
                        .filter(|&&p| domtree.is_reachable(p)),
                );
            }
        }
        Self { depth }
    }

    pub fn depth(&self, block: Block) -> u32 {
        self.depth[block.index()]
    }
}
