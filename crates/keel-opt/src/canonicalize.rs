//! Structural normalization and recognition of complementary constant rotates.
//!
//! Resolve operands; put integer literals last, then order equivalent integer
//! operand pairs by value ID. Comparisons reverse their predicate on a swap.
//! Floating-point operand order and noncommutative operations are preserved.

use keel_ir::{BinaryOp, Function, InstData, Type, Value, ValueDef};

fn order(func: &Function, value: Value) -> (bool, Value) {
    let literal = matches!(
        crate::const_fold::constant_data(func, value),
        Some(InstData::Iconst { .. })
    );
    (literal, value)
}

// Only full-width, nonzero constant shifts are eligible. In particular, do
// not use hardware-masked counts to recognize out-of-range IR shifts.
fn shift(func: &Function, value: Value, expected: BinaryOp) -> Option<(Value, Value, i64)> {
    let value = func.resolve(value);
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    let InstData::Binary { op, args } = *func.inst_data(inst) else {
        return None;
    };
    let [source, count] = args.map(|v| func.resolve(v));
    if op != expected || func.value_type(value) != func.value_type(source) {
        return None;
    }
    let InstData::Iconst { imm, .. } = crate::const_fold::constant_data(func, count)? else {
        return None;
    };
    Some((source, count, imm))
}

fn rotate(func: &Function, args: [Value; 2]) -> Option<InstData> {
    for [left, right] in [args, [args[1], args[0]]] {
        let Some((x, count, l)) = shift(func, left, BinaryOp::Shl) else {
            continue;
        };
        let Some((y, _, r)) = shift(func, right, BinaryOp::Ushr) else {
            continue;
        };
        let width = match func.value_type(x) {
            Type::I32 => 32,
            Type::I64 => 64,
            _ => continue,
        };
        if x == y && l > 0 && l < width && r > 0 && r < width && l + r == width {
            return Some(InstData::Binary {
                op: BinaryOp::Rotl,
                args: [x, count],
            });
        }
    }
    None
}

/// Normalize a payload against current aliases. Shared by local rewrites so
/// their worklist can restore the form after discovering new constants.
pub(crate) fn normalize(func: &Function, data: &mut InstData) {
    data.for_each_operand_mut(|v| *v = func.resolve(*v));
    if let InstData::Binary { op: BinaryOp::Or, args } = *data
        && let Some(rotated) = rotate(func, args)
    {
        *data = rotated;
    }
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
