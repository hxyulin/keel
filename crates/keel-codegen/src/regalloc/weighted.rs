//! Full's spill priority: operand traffic per live program point, with each
//! use and definition weighted by its block's loop depth (x8 per loop, capped
//! at 4096). The scan keeps the denser of two values in a register.
use super::*;
use keel_ir::dominators::DominatorTree;

fn block_weights(func: &Function, cfg: &ControlFlowGraph, order: &[Block]) -> Vec<u32> {
    let mut weights = vec![1u32; func.num_blocks()];
    let mut positions = vec![usize::MAX; func.num_blocks()];
    for (i, &b) in order.iter().enumerate() {
        positions[b.index()] = i;
    }
    if !order.iter().any(|&b| {
        cfg.succs(b)
            .iter()
            .any(|s| positions[s.index()] <= positions[b.index()])
    }) {
        return weights;
    }
    let dom = DominatorTree::compute(func, cfg);
    let mut members = vec![0usize; func.num_blocks()];
    let mut touched = Vec::new();
    let mut stack = Vec::new();
    // Union all latches for a header before adding its nesting weight. A loop
    // with two continue edges is still one loop, not two nested loops.
    for &header in order {
        let stamp = header.index() + 1;
        members[header.index()] = stamp;
        touched.clear();
        touched.push(header);
        for &latch in cfg.preds(header) {
            if latch != header
                && positions[latch.index()] >= positions[header.index()]
                && dom.dominates(header, latch)
            {
                members[latch.index()] = stamp;
                touched.push(latch);
                stack.push(latch);
            }
        }
        let self_edge = cfg.succs(header).contains(&header);
        if stack.is_empty() && !self_edge {
            continue;
        }
        while let Some(block) = stack.pop() {
            for &pred in cfg.preds(block) {
                if dom.is_reachable(pred) && members[pred.index()] != stamp {
                    members[pred.index()] = stamp;
                    touched.push(pred);
                    stack.push(pred);
                }
            }
        }
        for &block in &touched {
            let w = &mut weights[block.index()];
            *w = (*w * 8).min(4096);
        }
    }
    weights
}

pub(super) fn linear_scan<'a>(
    func: &Function,
    selection: &crate::select::Selection,
    cfg: &ControlFlowGraph,
    order: &[Block],
    intervals: &'a [Interval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
) -> Vec<&'a Interval> {
    // Most functions never exhaust their pools. Build loop weights and operand
    // costs only when the scan must choose between spilling two values.
    let costs = std::cell::OnceCell::new();
    linear_scan_with_priority(intervals, pools, locs, &selection.hints, |a, b| {
        let (costs, lengths) =
            costs.get_or_init(|| spill_costs(func, selection, cfg, order, intervals));
        // Products can exceed u64 for very large functions. Keep comparisons
        // exact and deterministic without floating point or truncating ratios.
        let score_a = u128::from(costs[a.value.index()]) * u128::from(lengths[b.value.index()]);
        let score_b = u128::from(costs[b.value.index()]) * u128::from(lengths[a.value.index()]);
        score_a.cmp(&score_b).then_with(|| b.end().cmp(&a.end()))
    })
}

pub(super) fn spill_costs(
    func: &Function,
    selection: &crate::select::Selection,
    cfg: &ControlFlowGraph,
    order: &[Block],
    intervals: &[Interval],
) -> (Vec<u64>, Vec<u64>) {
    let weights = block_weights(func, cfg, order);
    let mut costs = vec![0u64; func.num_values()];
    for &block in order {
        let weight = u64::from(weights[block.index()]);
        for &param in func.block_params(block) {
            costs[param.index()] += weight;
        }
        for &inst in func.block_insts(block) {
            if selection.covered(inst) {
                continue;
            }
            selection.for_each_operand(func, inst, |v| costs[v.index()] += weight);
            for &v in func.inst_results(inst) {
                costs[v.index()] += weight;
            }
        }
    }
    let mut lengths = vec![1u64; func.num_values()];
    for iv in intervals {
        lengths[iv.value.index()] = iv.ranges.iter().map(|&(a, b)| u64::from(b - a) + 1).sum();
    }
    (costs, lengths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_multiple_latches_and_unreachable_cycles() {
        let module = keel_ir::parse(
            "func @loops(i8) -> i64 {
block0(v0: i8):
    jump block1
block1:
    brif v0, block2, block6
block2:
    brif v0, block3, block4
block3:
    brif v0, block3, block5
block4:
    jump block1
block5:
    jump block1
block6:
    v1 = iconst.i64 0
    return v1
block7:
    jump block7
}",
        )
        .unwrap();
        let f = &module.functions[0];
        let cfg = ControlFlowGraph::compute(f);
        let weights = block_weights(f, &cfg, &cfg.reverse_postorder(f));
        assert_eq!(weights, [1, 8, 8, 64, 8, 8, 1, 1]);
    }
}
