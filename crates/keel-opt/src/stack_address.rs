//! Constant in-bounds fixed-stack address propagation shared by stack passes.
//!
//! Callers seed canonical `stack_addr` results and retain their own escape,
//! lifetime and access checks. Propagation never follows aliased definitions,
//! negative offsets or offsets beyond the one-past address of a fixed slot.

use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, Function, Inst, InstData, StackSlot, Value};

pub(crate) fn propagate(
    f: &Function,
    insts: impl Iterator<Item = Inst> + Clone,
    sizes: &FxHashMap<StackSlot, usize>,
    addresses: &mut FxHashMap<Value, (StackSlot, usize)>,
    max_scans: Option<usize>,
) {
    let mut scans = 0;
    loop {
        if max_scans.is_some_and(|limit| scans >= limit) {
            break;
        }
        scans += 1;
        let before = addresses.len();
        for i in insts.clone() {
            if let InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } = *f.inst_data(i)
                && let Some(&(slot, offset)) = addresses.get(&f.resolve(args[0]))
                && let Some(n) = constant(f, args[1]).and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(n).filter(|&n| n <= sizes[&slot])
                && f.resolve(f.first_result(i)) == f.first_result(i)
            {
                addresses.insert(f.first_result(i), (slot, end));
            }
        }
        if addresses.len() == before {
            break;
        }
    }
}

pub(crate) fn constant(f: &Function, v: Value) -> Option<i64> {
    match crate::const_fold::constant_ref(f, v)? {
        InstData::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}
