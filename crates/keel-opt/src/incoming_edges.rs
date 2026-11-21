use crate::scratch::Scratch;
use keel_ir::{Block, BlockCall, EntityRef, Function, Inst};

/// Syntactic edges into blocks with parameters, packed by destination in
/// layout and branch order. Duplicate edges and unreachable predecessors
/// still supply distinct arguments. Argument edits do not invalidate the index.
pub(crate) struct IncomingEdges {
    start: Scratch<u32>,
    edges: Scratch<(Inst, usize)>,
}

impl IncomingEdges {
    pub(crate) fn new(func: &Function, mut visit: impl FnMut(&BlockCall)) -> Self {
        let parameter_edges = || {
            func.blocks()
                .filter_map(|block| func.block_insts(block).last().copied())
                .flat_map(|inst| {
                    func.inst_data(inst)
                        .branch_dests()
                        .iter()
                        .enumerate()
                        .filter(|(_, dest)| !dest.args.is_empty())
                        .map(move |(edge, dest)| (inst, edge, dest))
                })
        };
        let mut start = Scratch::filled(func.num_blocks() + 1, 0u32);
        for (_, _, dest) in parameter_edges() {
            start[dest.block.index() + 1] += 1;
            visit(dest);
        }
        for i in 1..start.len() {
            start[i] += start[i - 1];
        }
        let mut edges = Scratch::filled(start[func.num_blocks()] as usize, (Inst::new(0), 0));
        if !edges.is_empty() {
            let mut fill = Scratch::new();
            fill.extend_from_slice(&start);
            for (inst, edge, dest) in parameter_edges() {
                let slot = &mut fill[dest.block.index()];
                edges[*slot as usize] = (inst, edge);
                *slot += 1;
            }
        }
        Self { start, edges }
    }

    pub(crate) fn get(&self, block: Block) -> &[(Inst, usize)] {
        let index = block.index();
        &self.edges[self.start[index] as usize..self.start[index + 1] as usize]
    }
}
