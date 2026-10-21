//! Integer intervals from dominating conditional edges. Facts are scoped to
//! dominator subtrees and never transported through memory or block arguments.
//! Exact equality facts also cover pointers; they establish no dereferenceability.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{EntityRef, Function, InstData, IntCC, Type, Value, ValueDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    lo: u128,
    hi: u128,
}
type Key = (Value, bool);
type Equality = (Value, Value);

fn equality(func: &Function, data: &InstData) -> Option<(Equality, bool)> {
    let InstData::Icmp { cond, args } = *data else {
        return None;
    };
    let equal = match cond {
        IntCC::Eq => true,
        IntCC::Ne => false,
        _ => return None,
    };
    let mut args = args.map(|v| func.resolve(v));
    args.sort_unstable();
    Some(((args[0], args[1]), equal))
}

fn comparison(func: &Function, data: &InstData) -> Option<(IntCC, Value, u128, u32)> {
    if !matches!(data, InstData::Icmp { .. }) {
        return None;
    }
    let mut data = data.clone();
    crate::canonicalize::normalize(func, &mut data);
    let InstData::Icmp {
        cond,
        args: [value, constant],
    } = data
    else {
        return None;
    };
    let ty = func.value_type(value);
    if !ty.is_int() {
        return None;
    }
    let Some(InstData::Iconst { imm, .. }) = crate::const_fold::constant_data(func, constant)
    else {
        return None;
    };
    let width = ty.bytes().unwrap() * 8;
    Some((
        cond,
        value,
        (imm as i128 as u128) & (u128::MAX >> (128 - width)),
        width,
    ))
}
fn signed(cond: IntCC) -> bool {
    matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge)
}
fn invert(cond: IntCC) -> IntCC {
    match cond {
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
    }
}
fn refine(mut r: Range, cond: IntCC, c: u128) -> Option<Range> {
    match cond {
        IntCC::Eq => {
            r.lo = r.lo.max(c);
            r.hi = r.hi.min(c);
        }
        IntCC::Ne => {
            if r.lo == c {
                r.lo = r.lo.checked_add(1)?;
            }
            if r.hi == c {
                r.hi = r.hi.checked_sub(1)?;
            }
        }
        IntCC::Slt | IntCC::Ult => r.hi = r.hi.min(c.checked_sub(1)?),
        IntCC::Sle | IntCC::Ule => r.hi = r.hi.min(c),
        IntCC::Sgt | IntCC::Ugt => r.lo = r.lo.max(c.checked_add(1)?),
        IntCC::Sge | IntCC::Uge => r.lo = r.lo.max(c),
    }
    (r.lo <= r.hi).then_some(r)
}
fn prove(r: Range, cond: IntCC, c: u128) -> Option<bool> {
    match cond {
        IntCC::Eq if r.lo == r.hi && r.lo == c => Some(true),
        IntCC::Eq if c < r.lo || c > r.hi => Some(false),
        IntCC::Ne => prove(r, IntCC::Eq, c).map(|v| !v),
        IntCC::Slt | IntCC::Ult if r.hi < c => Some(true),
        IntCC::Slt | IntCC::Ult if r.lo >= c => Some(false),
        IntCC::Sle | IntCC::Ule if r.hi <= c => Some(true),
        IntCC::Sle | IntCC::Ule if r.lo > c => Some(false),
        IntCC::Sgt | IntCC::Ugt => prove(r, IntCC::Ule, c).map(|v| !v),
        IntCC::Sge | IntCC::Uge => prove(r, IntCC::Ult, c).map(|v| !v),
        _ => None,
    }
}

pub fn run(func: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(func);
    let dom = DominatorTree::compute(func, &cfg);
    let Some(entry) = func.entry_block() else {
        return false;
    };
    let children = dom.children();
    let mut facts: FxHashMap<Key, Range> = FxHashMap::default();
    let mut undo: Vec<(Key, Option<Range>)> = Vec::new();
    let mut equalities: FxHashMap<Equality, bool> = FxHashMap::default();
    let mut equality_undo = Vec::new();
    let mut stack = vec![(entry, 0, 0, false)];
    let mut changed = false;
    while let Some((block, scope, equality_scope, visited)) = stack.pop() {
        if visited {
            for (key, old) in undo.drain(scope..).rev() {
                if let Some(old) = old {
                    facts.insert(key, old);
                } else {
                    facts.remove(&key);
                }
            }
            for (key, old) in equality_undo.drain(equality_scope..).rev() {
                if let Some(old) = old {
                    equalities.insert(key, old);
                } else {
                    equalities.remove(&key);
                }
            }
            continue;
        }
        let scope = undo.len();
        let equality_scope = equality_undo.len();
        if block != entry
            && let [pred] = cfg.preds(block)
            && let Some(&term) = func.block_insts(*pred).last()
            && let InstData::Brif { cond, dests } = func.inst_data(term)
            && dests[0].block != dests[1].block
            && let ValueDef::Result(inst, 0) = func.value_def(func.resolve(*cond))
            && let Some((key, equal)) = equality(func, func.inst_data(inst))
        {
            let truth = if dests[0].block == block {
                equal
            } else {
                !equal
            };
            equality_undo.push((key, equalities.insert(key, truth)));
        }
        // CFG predecessors deduplicate parallel edges, so explicitly reject a
        // conditional whose two arms enter the same block.
        if block != entry
            && let [pred] = cfg.preds(block)
            && let Some(&term) = func.block_insts(*pred).last()
            && let InstData::Brif { cond, dests } = func.inst_data(term)
            && dests[0].block != dests[1].block
            && let ValueDef::Result(inst, 0) = func.value_def(func.resolve(*cond))
            && let Some((mut cond, value, c, width)) = comparison(func, func.inst_data(inst))
        {
            if dests[1].block == block {
                cond = invert(cond);
            }
            let domains: &[bool] = if matches!(cond, IntCC::Eq | IntCC::Ne) {
                &[false, true]
            } else if signed(cond) {
                &[true]
            } else {
                &[false]
            };
            for &is_signed in domains {
                let c = if is_signed {
                    c ^ (1u128 << (width - 1))
                } else {
                    c
                };
                let key = (value, is_signed);
                let old = facts.get(&key).copied();
                let full = Range {
                    lo: 0,
                    hi: u128::MAX >> (128 - width),
                };
                // Contradictory paths are left conservative rather than used as
                // a license to rewrite arbitrary operations in unreachable code.
                if let Some(range) = refine(old.unwrap_or(full), cond, c) {
                    undo.push((key, old));
                    facts.insert(key, range);
                }
            }
        }
        stack.push((block, scope, equality_scope, true));
        for inst in func.block_insts(block).to_vec() {
            if let Some((key, equal)) = equality(func, func.inst_data(inst))
                && let Some(&truth) = equalities.get(&key)
            {
                *func.inst_data_mut(inst) = InstData::Iconst {
                    ty: Type::I8,
                    imm: i64::from(truth == equal),
                };
                changed = true;
                continue;
            }
            if let Some((cond, value, c, width)) = comparison(func, func.inst_data(inst)) {
                let is_signed = signed(cond);
                let c = if is_signed {
                    c ^ (1u128 << (width - 1))
                } else {
                    c
                };
                if let Some(range) = facts.get(&(value, is_signed))
                    && let Some(truth) = prove(*range, cond, c)
                {
                    *func.inst_data_mut(inst) = InstData::Iconst {
                        ty: Type::I8,
                        imm: i64::from(truth),
                    };
                    changed = true;
                }
            }
        }
        for &child in children[block.index()].iter().rev() {
            stack.push((child, 0, 0, false));
        }
    }
    changed
}
