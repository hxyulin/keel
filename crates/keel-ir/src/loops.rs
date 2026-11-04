//! Natural loops and loop nesting depth.
//!
//! A back edge is an edge `b -> h` where `h` dominates `b`; `h` is a loop
//! header, `b` is one of its latches, and the loop is `h` plus every block
//! that reaches a latch without going through `h`. Back edges to the same
//! header form one loop. A block's depth is the number of loops containing
//! it. Only reachable blocks belong to loops.

use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{Block, EntityRef, Function};
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub struct LoopInfo {
    depth: Vec<u32>,
    loops: Vec<LoopData>,
    /// One more than the index into `loops` of each block's loop, or 0 if
    /// the block heads no loop.
    by_header: Vec<u32>,
    /// The latches and then the blocks of every loop, one range each.
    members: Vec<Block>,
}

#[derive(Debug, PartialEq, Eq)]
struct LoopData {
    header: Block,
    preheader: Option<Block>,
    latches: Range<u32>,
    blocks: Range<u32>,
}

/// One natural loop, borrowed from its [`LoopInfo`].
#[derive(Clone, Copy, Debug)]
pub struct Loop<'a> {
    pub header: Block,
    /// The header's only predecessor outside the loop, when that block's
    /// only successor is the header.
    pub preheader: Option<Block>,
    /// The sources of the back edges, in the header's predecessor order.
    pub latches: &'a [Block],
    /// Every block of the loop, the header first.
    pub blocks: &'a [Block],
}

impl LoopInfo {
    pub fn compute(func: &Function, cfg: &ControlFlowGraph, domtree: &DominatorTree) -> Self {
        let mut depth = vec![0; func.num_blocks()];
        let mut by_header = vec![0u32; func.num_blocks()];
        let mut loops = Vec::new();
        let mut members = Vec::new();
        // Loop membership is stamped with the header's RPO position, so one
        // array serves every loop without clearing.
        let mut stamp = vec![0u32; func.num_blocks()];
        let mut stack = Vec::new();
        for (number, &header) in (1u32..).zip(domtree.rpo()) {
            let latches_start = members.len() as u32;
            members.extend(
                cfg.preds(header)
                    .iter()
                    .copied()
                    .filter(|&pred| domtree.dominates(header, pred)),
            );
            let blocks_start = members.len() as u32;
            if blocks_start == latches_start {
                continue;
            }
            stack.extend_from_slice(&members[latches_start as usize..]);
            // Walk backwards from the latches; the header stops the walk.
            stamp[header.index()] = number;
            depth[header.index()] += 1;
            members.push(header);
            while let Some(block) = stack.pop() {
                if std::mem::replace(&mut stamp[block.index()], number) == number {
                    continue;
                }
                depth[block.index()] += 1;
                members.push(block);
                stack.extend(
                    cfg.preds(block)
                        .iter()
                        .filter(|&&p| domtree.is_reachable(p)),
                );
            }
            let mut outside = cfg
                .preds(header)
                .iter()
                .copied()
                .filter(|&p| stamp[p.index()] != number);
            let preheader = match (outside.next(), outside.next()) {
                (Some(pred), None) if cfg.succs(pred) == [header] => Some(pred),
                _ => None,
            };
            loops.push(LoopData {
                header,
                preheader,
                latches: latches_start..blocks_start,
                blocks: blocks_start..members.len() as u32,
            });
            by_header[header.index()] = loops.len() as u32;
        }
        Self {
            depth,
            loops,
            by_header,
            members,
        }
    }

    pub fn depth(&self, block: Block) -> u32 {
        self.depth[block.index()]
    }

    /// The loop headed by `header`, if it heads one.
    pub fn loop_at(&self, header: Block) -> Option<Loop<'_>> {
        let index = self.by_header.get(header.index())?.checked_sub(1)?;
        Some(self.view(&self.loops[index as usize]))
    }

    /// Every loop, in reverse postorder of the headers, so an outer loop
    /// comes before the loops nested in it.
    pub fn loops(&self) -> impl Iterator<Item = Loop<'_>> {
        self.loops.iter().map(|data| self.view(data))
    }

    fn view(&self, data: &LoopData) -> Loop<'_> {
        let slice = |range: &Range<u32>| &self.members[range.start as usize..range.end as usize];
        Loop {
            header: data.header,
            preheader: data.preheader,
            latches: slice(&data.latches),
            blocks: slice(&data.blocks),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(source: &str) -> (Function, LoopInfo) {
        let mut module = crate::parse(source).unwrap();
        crate::verify_module(&module).unwrap();
        let func = module.functions.remove(0);
        let cfg = ControlFlowGraph::compute(&func);
        let dom = DominatorTree::compute(&func, &cfg);
        let info = LoopInfo::compute(&func, &cfg, &dom);
        (func, info)
    }

    #[test]
    fn nested_loops_report_headers_latches_preheaders_and_depths() {
        let (func, info) = info(
            "func @nest(i8) {
entry(v0:i8):
 jump outer
outer:
 brif v0,inner,exit
inner:
 brif v0,inner,latch
latch:
 jump outer
exit:
 return
}",
        );
        let [entry, outer, inner, latch, exit] = func.blocks().collect::<Vec<_>>()[..] else {
            unreachable!()
        };
        let depths: Vec<_> = func.blocks().map(|b| info.depth(b)).collect();
        assert_eq!(depths, [0, 1, 2, 1, 0]);
        let loops: Vec<_> = info.loops().map(|l| l.header).collect();
        assert_eq!(loops, [outer, inner]);

        let outer_loop = info.loop_at(outer).unwrap();
        assert_eq!(outer_loop.preheader, Some(entry));
        assert_eq!(outer_loop.latches, [latch]);
        assert_eq!(outer_loop.blocks[0], outer);
        let mut blocks = outer_loop.blocks.to_vec();
        blocks.sort();
        assert_eq!(blocks, [outer, inner, latch]);

        // The inner loop latches on itself, and its outside predecessor
        // `outer` also branches to `exit`, so it has no preheader.
        let inner_loop = info.loop_at(inner).unwrap();
        assert_eq!(inner_loop.latches, [inner]);
        assert_eq!(inner_loop.blocks, [inner]);
        assert_eq!(inner_loop.preheader, None);
        assert!(info.loop_at(latch).is_none());
        assert!(info.loop_at(exit).is_none());
    }

    #[test]
    fn unreachable_cycles_and_two_entry_headers_have_no_preheader_or_loop() {
        let (func, info) = info(
            "func @odd(i8) {
entry(v0:i8):
 brif v0,left,right
left:
 jump head
right:
 jump head
head:
 brif v0,head,exit
exit:
 return
dead:
 jump dead
}",
        );
        let blocks: Vec<_> = func.blocks().collect();
        let (head, dead) = (blocks[3], blocks[5]);
        let head_loop = info.loop_at(head).unwrap();
        assert_eq!(head_loop.preheader, None);
        assert_eq!(head_loop.latches, [head]);
        assert!(info.loop_at(dead).is_none());
        assert_eq!(info.depth(dead), 0);
    }
}
