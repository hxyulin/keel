//! Marks loads and stores `notrap` when they provably fall inside a fixed
//! stack slot.
//!
//! A fixed slot stays mapped for the whole frame, so an access within its
//! bounds cannot fault, even when the slot's address escapes. The address
//! must be a slot's `stack_addr` or a chain of constant, non-negative
//! `ptradd`s from it, as [`crate::stack_address::propagate`] proves. Slots
//! with lifetime markers are skipped: the interpreter traps on an access
//! outside a slot's lifetime. Alignment is not inferred. Volatile accesses
//! are left alone.
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{Function, InstData, Intrinsic, Type};

pub fn run(f: &mut Function) -> bool {
    if f.num_stack_slots() == 0 {
        return false;
    }
    let mut marked = FxHashSet::default();
    let mut seeds = FxHashMap::default();
    for (_, i) in f.layout_insts() {
        match f.inst_data(i) {
            InstData::Intrinsic {
                id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                args,
            } => {
                // The verifier requires a direct `stack_addr` operand.
                if let keel_ir::ValueDef::Result(def, _) = f.value_def(f.resolve(args[0]))
                    && let InstData::StackAddr { slot } = *f.inst_data(def)
                {
                    marked.insert(slot);
                }
            }
            &InstData::StackAddr { slot } if f.resolve(f.first_result(i)) == f.first_result(i) => {
                seeds.insert(f.first_result(i), (slot, 0usize));
            }
            _ => {}
        }
    }
    seeds.retain(|_, (slot, _)| !marked.contains(slot));
    if seeds.is_empty() {
        return false;
    }
    let sizes: FxHashMap<_, _> = f
        .stack_slots()
        .map(|(slot, data)| (slot, data.size as usize))
        .collect();
    let mut addresses = seeds;
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    crate::stack_address::propagate(f, insts.iter().copied(), &sizes, &mut addresses, None);
    // A pointer is at most 8 bytes on every target.
    let width = |ty: Type| ty.bytes().unwrap_or(8) as usize;
    let inside = |addr, ty| {
        addresses
            .get(&f.resolve(addr))
            .is_some_and(|&(slot, offset)| offset + width(ty) <= sizes[&slot])
    };
    let work: Vec<_> = f
        .layout_insts()
        .filter(|&(_, i)| match *f.inst_data(i) {
            InstData::Load { flags, ty, addr } => {
                !flags.notrap && !flags.volatile && inside(addr, ty)
            }
            InstData::Store { flags, value, addr } => {
                !flags.notrap && !flags.volatile && inside(addr, f.value_type(f.resolve(value)))
            }
            _ => false,
        })
        .map(|(_, i)| i)
        .collect();
    for &i in &work {
        if let InstData::Load { flags, .. } | InstData::Store { flags, .. } = f.inst_data_mut(i) {
            flags.notrap = true;
        }
    }
    !work.is_empty()
}
