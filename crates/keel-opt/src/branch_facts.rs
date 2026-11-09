//! Fold comparisons established by a dominating conditional edge.
mod ranges;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InstData, IntCC, ValueDef};

fn key(f: &Function, value: keel_ir::Value) -> Option<InstData> {
    let ValueDef::Result(i, 0) = f.value_def(f.resolve(value)) else {
        return None;
    };
    matches!(f.inst_data(i), InstData::Icmp { .. })
        .then(|| crate::gvn::canonical(f, f.inst_data(i), None).into_owned())
}
fn inverse(data: &InstData) -> InstData {
    let InstData::Icmp { cond, args } = *data else {
        unreachable!("comparison key")
    };
    let cond = match cond {
        IntCC::Eq => IntCC::Ne,
        IntCC::Ne => IntCC::Eq,
        IntCC::Slt => IntCC::Sge,
        IntCC::Sle => IntCC::Sgt,
        IntCC::Sgt => IntCC::Sle,
        IntCC::Sge => IntCC::Slt,
        IntCC::Ult => IntCC::Uge,
        IntCC::Ule => IntCC::Ugt,
        IntCC::Ugt => IntCC::Ule,
        IntCC::Uge => IntCC::Ult,
    };
    InstData::Icmp { cond, args }
}
pub fn run(f: &mut Function) -> bool {
    let Some(entry) = f.entry_block() else {
        return false;
    };
    let analysis = crate::analysis::get(f);
    let children = analysis.dom.children();
    let mut facts = FxHashMap::default();
    let mut stack = vec![(entry, false, None)];
    let mut changed = false;
    while let Some((block, leaving, restore)) = stack.pop() {
        if leaving {
            if let Some((key, previous)) = restore {
                if let Some(value) = previous {
                    facts.insert(key, value);
                } else {
                    facts.remove(&key);
                }
            }
            continue;
        }
        let restore = if block != entry
            && let [pred] = analysis.cfg.preds(block)
            && let Some(&i) = f.block_insts(*pred).last()
            && let InstData::Brif { cond, dests } = f.inst_data(i)
            && dests[0].block != dests[1].block
            && let Some(key) = key(f, *cond)
        {
            let previous = facts.insert(key.clone(), dests[0].block == block);
            Some((key, previous))
        } else {
            None
        };
        stack.push((block, true, restore));
        if let Some(&i) = f.block_insts(block).last()
            && let InstData::Brif { cond, dests } = f.inst_data(i)
            && let Some(key) = key(f, *cond)
            && let Some(taken) = facts
                .get(&key)
                .copied()
                .or_else(|| facts.get(&inverse(&key)).map(|b| !b))
        {
            let dest = dests[usize::from(!taken)].clone();
            *f.inst_data_mut(i) = InstData::Jump { dest };
            changed = true;
        }
        for &child in children[block].iter().rev() {
            stack.push((child, false, None));
        }
    }
    if changed {
        crate::simplify_cfg::run(f);
        crate::dce::run(f);
    }
    changed
}

/// Explicit range propagation leaves the default exact-comparison pass intact.
pub fn run_ranges(f: &mut Function) -> bool {
    let changed = run(f);
    ranges::run(f) || changed
}

/// Apply scoped bounds only to checked arithmetic in the default pipelines.
pub(crate) fn run_checked_ranges(f: &mut Function, known: Vec<(u128, u128)>) -> bool {
    ranges::run_checked(f, known)
}
