//! Structural normalization, without folding, reassociation, or new instructions.
//!
//! Resolve operands; put integer literals last, then order equivalent integer
//! operand pairs by value ID. Comparisons reverse their predicate on a swap.
//! Floating-point operand order and noncommutative operations are preserved.

use keel_ir::{Function, InstData, Value};

fn order(func: &Function, value: Value) -> (bool, Value) {
    let literal = matches!(
        crate::const_fold::constant_data(func, value),
        Some(InstData::Iconst { .. })
    );
    (literal, value)
}

/// Normalize a payload against current aliases. Shared by local rewrites so
/// their worklist can restore the form after discovering new constants.
pub(crate) fn normalize(func: &Function, data: &mut InstData) {
    data.for_each_operand_mut(|v| *v = func.resolve(*v));
    match data {
        InstData::Binary { op, args } if !op.is_float() && crate::gvn::is_commutative(*op) => {
            if order(func, args[0]) > order(func, args[1]) {
                args.swap(0, 1);
            }
        }
        InstData::Overflow { op, args } if crate::gvn::is_commutative_overflow(*op) => {
            if order(func, args[0]) > order(func, args[1]) {
                args.swap(0, 1);
            }
        }
        InstData::Icmp { cond, args } if order(func, args[0]) > order(func, args[1]) => {
            args.swap(0, 1);
            *cond = crate::gvn::reverse(*cond);
        }
        _ => {}
    }
}

pub fn run(func: &mut Function) -> bool {
    let mut changed = false;
    for block in func.blocks().collect::<Vec<_>>() {
        for inst in func.block_insts(block).to_vec() {
            let mut data = func.inst_data(inst).clone();
            normalize(func, &mut data);
            if data != *func.inst_data(inst) {
                *func.inst_data_mut(inst) = data;
                changed = true;
            }
        }
    }
    changed
}
