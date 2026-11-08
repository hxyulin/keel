//! Bounded guarded rotation. Header tests are copied to the latch; the original
//! entry guard remains, so no body access executes on an empty loop.
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{Block, Function, InstData as D, Value};
pub(crate) struct LoopRotate;
impl crate::Pass for LoopRotate {
    fn name(&self) -> &'static str {
        "loop-rotate"
    }
    fn description(&self) -> &'static str {
        "rotate small loops with early exits while retaining their entry guard"
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        if f.debug_return.is_some() || f.debug_loc.is_some() || !f.debug_vars.is_empty() {
            return false;
        }
        let headers: Vec<_> = f.blocks().collect();
        let mut changed = false;
        for h in headers {
            changed |= rotate(f, h);
        }
        changed
    }
    fn deterministic(&self) -> bool {
        true
    }
}
fn rotate(f: &mut Function, h: Block) -> bool {
    let analysis = crate::analysis::get(f);
    let (cfg, dom) = (&analysis.cfg, &analysis.dom);
    let hi = f.block_insts(h).to_vec();
    if hi.len() < 2 || hi.len() > 5 || f.block_params(h).len() > 4 {
        return false;
    }
    let branch = *hi.last().unwrap();
    let D::Brif { dests, .. } = f.inst_data(branch) else {
        return false;
    };
    let (body, exit) = (dests[0].block, dests[1].block);
    if body == h
        || exit == h
        || body == exit
        || cfg.preds(body) != [h]
        || !f.block_params(body).is_empty()
        || !dests[0].args.is_empty()
    {
        return false;
    }
    // Only cheap total tests may be duplicated. In particular never duplicate a
    // load, divide, overflow check, call, or debugger location.
    for &i in &hi[..hi.len() - 1] {
        if !matches!(
            f.inst_data(i),
            D::Iconst { .. }
                | D::Icmp { .. }
                | D::Binary {
                    op: keel_ir::BinaryOp::Iadd | keel_ir::BinaryOp::Isub,
                    ..
                }
        ) || f.inst_results(i).len() != 1
            || f.resolve(f.first_result(i)) != f.first_result(i)
        {
            return false;
        }
    }
    // Find the natural loop from its unique backedge. Early returns and the
    // common tail are exits, rather than extra blocks to absorb into the loop.
    let backedges: Vec<_> = cfg
        .preds(h)
        .iter()
        .copied()
        .filter(|&b| dom.dominates(h, b))
        .collect();
    let [latch] = backedges.as_slice() else {
        return false;
    };
    let latch = *latch;
    if latch == h || cfg.preds(h).len() != 2 {
        return false;
    }
    let Some(&back) = f.block_insts(latch).last() else {
        return false;
    };
    if !matches!(f.inst_data(back), D::Jump { dest } if dest.block == h) {
        return false;
    }
    let mut region = FxHashSet::default();
    let mut work = vec![latch];
    while let Some(b) = work.pop() {
        if b == h || !region.insert(b) {
            continue;
        }
        if region.len() > 4 || !dom.dominates(h, b) || f.block_insts(b).len() > 32 {
            return false;
        }
        work.extend(cfg.preds(b).iter().copied());
    }
    if !region.contains(&body) || region.contains(&exit) {
        return false;
    }
    let mut exits = FxHashSet::default();
    for b in region.iter().copied().chain([h]) {
        let Some(&term) = f.block_insts(b).last() else {
            return false;
        };
        if !matches!(f.inst_data(term), D::Jump { .. } | D::Brif { .. }) {
            return false;
        }
        for dest in f.inst_data(term).branch_dests() {
            if dest.block != h && !region.contains(&dest.block) {
                exits.insert(dest.block);
            }
        }
        // Calls and source locations in the loop retain their original graph.
        if f.block_insts(b).iter().any(|&i| {
            f.inst_loc(i).is_some()
                || matches!(f.inst_data(i), D::Call { .. } | D::CallIndirect { .. })
        }) {
            return false;
        }
    }
    // A bounded topological elimination also rejects irreducible body cycles.
    let mut pending = region.clone();
    while !pending.is_empty() {
        let ready: Vec<_> = pending
            .iter()
            .copied()
            .filter(|&b| cfg.preds(b).iter().all(|p| !pending.contains(p)))
            .collect();
        if ready.is_empty() {
            return false;
        }
        for b in ready {
            pending.remove(&b);
        }
    }
    // Forward only values used beyond the old header. Forwarding a test result
    // used only by its branch would force compare materialization in codegen.
    let mut outside_uses = FxHashSet::default();
    for (b, i) in f.layout_insts() {
        if b != h {
            f.inst_data(i).for_each_operand(|v| {
                outside_uses.insert(f.resolve(v));
            });
        }
    }
    let mut carried = f.block_params(h).to_vec();
    carried.extend(hi[..hi.len() - 1].iter().map(|&i| f.first_result(i)));
    carried.retain(|v| outside_uses.contains(v));
    if f.block_params(h).iter().any(|&v| f.resolve(v) != v) {
        return false;
    }
    let set: FxHashSet<_> = carried.iter().copied().collect();
    let mut forwarded_exits = FxHashSet::default();
    for (b, i) in f.layout_insts() {
        if b != h && !region.contains(&b) {
            let mut uses_header = false;
            f.inst_data(i)
                .for_each_operand(|v| uses_header |= set.contains(&f.resolve(v)));
            if uses_header {
                // Existing branch arguments already transport values to shared
                // tails. Add new parameters only for direct header-value uses,
                // and only when every predecessor can supply the current value.
                if !exits.contains(&b)
                    || cfg.preds(b).iter().any(|p| *p != h && !region.contains(p))
                {
                    return false;
                }
                forwarded_exits.insert(b);
            }
        }
    }
    let body_map: FxHashMap<_, _> = carried
        .iter()
        .map(|&v| (v, f.append_block_param(body, f.value_type(v))))
        .collect();
    let exit_maps: FxHashMap<_, FxHashMap<_, _>> = forwarded_exits
        .iter()
        .map(|&b| {
            (
                b,
                carried
                    .iter()
                    .map(|&v| (v, f.append_block_param(b, f.value_type(v))))
                    .collect(),
            )
        })
        .collect();
    for b in region
        .iter()
        .copied()
        .chain(forwarded_exits.iter().copied())
    {
        let map = exit_maps.get(&b).unwrap_or(&body_map);
        for i in f.block_insts(b).to_vec() {
            let mut d = f.inst_data(i).clone();
            d.for_each_operand_mut(|v| {
                *v = map.get(&f.resolve(*v)).copied().unwrap_or(f.resolve(*v));
            });
            *f.inst_data_mut(i) = d;
        }
    }
    let back = *f.block_insts(latch).last().unwrap();
    let args = f.inst_data(back).branch_dests()[0].args.clone();
    let mut next: FxHashMap<Value, Value> = f.block_params(h).iter().copied().zip(args).collect();
    for &i in &hi[..hi.len() - 1] {
        let mut d = f.inst_data(i).clone();
        d.for_each_operand_mut(|v| {
            *v = next.get(&f.resolve(*v)).copied().unwrap_or(f.resolve(*v));
        });
        let cloned = f.insert_inst(keel_ir::InsertPoint::Before(back), d);
        next.insert(f.first_result(i), f.first_result(cloned));
    }
    let mut d = f.inst_data(branch).clone();
    d.for_each_operand_mut(|v| {
        *v = next.get(&f.resolve(*v)).copied().unwrap_or(f.resolve(*v));
    });
    *f.inst_data_mut(back) = d;
    // Existing exit arguments precede the new forwarding arguments.
    for b in region.iter().copied().chain([h]) {
        let term = *f.block_insts(b).last().unwrap();
        for dest in f.inst_data_mut(term).branch_dests_mut() {
            if dest.block == body || forwarded_exits.contains(&dest.block) {
                let map = if b == h {
                    None
                } else if b == latch {
                    Some(&next)
                } else {
                    Some(&body_map)
                };
                dest.args
                    .extend(carried.iter().map(|v| map.map_or(*v, |m| m[v])));
            }
        }
    }
    true
}
