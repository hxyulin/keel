//! Bounded block-local overwrites of private, in-bounds stack bytes (opt-in).
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{BinaryOp, Function, InstData};

const MAX_INSTS: usize = 8192;
const MAX_PENDING: usize = 64;

pub fn run(f: &mut Function) -> bool {
    if !f.debug_vars.is_empty() || f.debug_loc.is_some() || f.num_values() > 32768 {
        return false;
    }
    let insts: Vec<_> = f
        .layout_insts()
        .take(MAX_INSTS + 1)
        .map(|(_, i)| i)
        .collect();
    if insts.len() > MAX_INSTS
        || insts.iter().any(|&i| {
            matches!(
                f.inst_data(i),
                InstData::InlineAsm { .. }
                    | InstData::Invoke { .. }
                    | InstData::LandingPad { .. }
                    | InstData::Intrinsic {
                        id: keel_ir::Intrinsic::FrameAddress
                            | keel_ir::Intrinsic::StackSave
                            | keel_ir::Intrinsic::StackRestore,
                        ..
                    }
            )
        })
    {
        return false;
    }
    let sizes: FxHashMap<_, _> = f.stack_slots().map(|(s, d)| (s, d.size as usize)).collect();
    let mut addresses = FxHashMap::default();
    for &i in &insts {
        if let InstData::StackAddr { slot } = *f.inst_data(i) {
            let v = f.first_result(i);
            if f.resolve(v) == v {
                addresses.insert(v, (slot, 0));
            }
        }
    }
    crate::stack_address::propagate(f, insts.iter().copied(), &sizes, &mut addresses, Some(8));
    let mut escaped = FxHashSet::default();
    for &i in &insts {
        if let InstData::AggregateVaArg { home, .. } = f.inst_data(i) {
            escaped.insert(*home);
        }
        let d = f.inst_data(i);
        if f.lifetime_marker_slot(i).is_some() {
            continue;
        }
        d.for_each_operand(|v| {
            let v = f.resolve(v);
            let Some(&(slot, offset)) = addresses.get(&v) else {
                return;
            };
            let allowed = match d {
                InstData::Load { addr, ty, flags } => {
                    !flags.volatile
                        && f.resolve(*addr) == v
                        && ty
                            .bytes()
                            .is_some_and(|n| offset + n as usize <= sizes[&slot])
                }
                InstData::Store { addr, value, flags } => {
                    !flags.volatile
                        && f.resolve(*addr) == v
                        && f.resolve(*value) != v
                        && f.value_type(*value)
                            .bytes()
                            .is_some_and(|n| offset + n as usize <= sizes[&slot])
                }
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args,
                } => f.resolve(args[0]) == v && addresses.contains_key(&f.first_result(i)),
                _ => false,
            };
            if !allowed {
                escaped.insert(slot);
            }
        });
    }
    let blocks: Vec<_> = f.blocks().collect();
    let mut changed = false;
    for b in blocks {
        let mut pending = Vec::new();
        let mut dead = FxHashSet::default();
        for &i in f.block_insts(b) {
            if f.inst_loc(i).is_some() {
                pending.clear();
                continue;
            }
            match *f.inst_data(i) {
                InstData::Store { addr, value, flags } if !flags.volatile && flags.notrap => {
                    let Some(&(slot, offset)) = addresses.get(&f.resolve(addr)) else {
                        pending.clear();
                        continue;
                    };
                    if escaped.contains(&slot) {
                        pending.clear();
                        continue;
                    }
                    let Some(bytes) = f.value_type(value).bytes() else {
                        pending.clear();
                        continue;
                    };
                    let end = offset + bytes as usize;
                    if end > sizes[&slot] {
                        pending.clear();
                        continue;
                    }
                    pending.retain(|&(old, s, start, stop)| {
                        let overwritten = s == slot && offset <= start && end >= stop;
                        if overwritten {
                            dead.insert(old);
                        }
                        !overwritten
                    });
                    if pending.len() == MAX_PENDING {
                        pending.clear();
                    }
                    pending.push((i, slot, offset, end));
                }
                // Even a successful read observes earlier stores. Calls, atomics,
                // traps and lifetime resets end the overwrite window.
                ref d
                    if !d.is_pinned()
                        && !d.memory_effects().may_read()
                        && !d.memory_effects().may_write() => {}
                InstData::StackAddr { .. } => {}
                _ => pending.clear(),
            }
        }
        let removed: Vec<_> = f
            .block_insts(b)
            .iter()
            .copied()
            .filter(|i| dead.contains(i))
            .collect();
        changed |= !removed.is_empty();
        f.remove_insts(b, &removed);
    }
    changed
}
