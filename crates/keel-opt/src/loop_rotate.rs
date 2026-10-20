//! Bounded guarded rotation. Header tests are copied to the latch; the original
//! entry guard remains, so no body access executes on an empty loop.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{Block, Function, InstData as D, Value};
use std::collections::{HashMap, HashSet};
pub(crate) struct LoopRotate;
impl crate::Pass for LoopRotate {
    fn name(&self) -> &'static str {
        "loop-rotate"
    }
    fn description(&self) -> &'static str {
        "rotate small single-exit loops while retaining their entry guard"
    }
    fn run(&self, f: &mut Function) -> bool {
        if f.debug_return.is_some()
            || f.debug_loc.is_some()
            || !f.debug_vars.is_empty()
            || !f.debug_scopes.is_empty()
            || f.layout_insts().any(|(_, i)| {
                f.inst_loc(i).is_some()
                    || matches!(f.inst_data(i), D::Call { .. } | D::CallIndirect { .. })
            })
        {
            return false;
        }
        let headers: Vec<_> = f.blocks().collect();
        let mut changed = false;
        for h in headers {
            changed |= rotate(f, h);
        }
        changed
    }
}
fn rotate(f: &mut Function, h: Block) -> bool {
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
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
        if !matches!(f.inst_data(i), D::Iconst { .. } | D::Icmp { .. })
            || f.inst_results(i).len() != 1
            || f.resolve(f.first_result(i)) != f.first_result(i)
        {
            return false;
        }
    }
    let mut region = HashSet::new();
    let mut work = vec![body];
    let mut latch = None;
    while let Some(b) = work.pop() {
        if b == h || b == exit || !region.insert(b) {
            continue;
        }
        if region.len() > 4 || !dom.dominates(h, b) || f.block_insts(b).len() > 32 {
            return false;
        }
        let Some(&term) = f.block_insts(b).last() else {
            return false;
        };
        if !matches!(f.inst_data(term), D::Jump { .. } | D::Brif { .. }) {
            return false;
        }
        for dest in f.inst_data(term).branch_dests() {
            if dest.block == h {
                if latch.replace(b).is_some() || !matches!(f.inst_data(term), D::Jump { .. }) {
                    return false;
                }
            } else if dest.block != exit {
                work.push(dest.block);
            }
        }
    }
    let Some(latch) = latch else {
        return false;
    };
    if cfg.preds(h).len() != 2
        || !cfg.preds(h).contains(&latch)
        || cfg
            .preds(exit)
            .iter()
            .any(|p| *p != h && !region.contains(p))
        || region
            .iter()
            .any(|&b| cfg.preds(b).iter().any(|p| *p != h && !region.contains(p)))
    {
        return false;
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
    let mut outside_uses = HashSet::new();
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
    let set: HashSet<_> = carried.iter().copied().collect();
    for (b, i) in f.layout_insts() {
        if b != h && b != exit && !region.contains(&b) {
            let mut escapes = false;
            f.inst_data(i)
                .for_each_operand(|v| escapes |= set.contains(&f.resolve(v)));
            if escapes {
                return false;
            }
        }
    }
    let body_map: HashMap<_, _> = carried
        .iter()
        .map(|&v| (v, f.append_block_param(body, f.value_type(v))))
        .collect();
    let exit_map: HashMap<_, _> = carried
        .iter()
        .map(|&v| (v, f.append_block_param(exit, f.value_type(v))))
        .collect();
    for b in region.iter().copied().chain([exit]) {
        let map = if b == exit { &exit_map } else { &body_map };
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
    let mut next: HashMap<Value, Value> = f.block_params(h).iter().copied().zip(args).collect();
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
            if dest.block == body || dest.block == exit {
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
