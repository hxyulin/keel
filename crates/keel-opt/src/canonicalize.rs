//! Structural normalization and recognition of complementary rotates.
//!
//! Resolve operands; put integer literals last, then order equivalent integer
//! operand pairs by value ID. Comparisons reverse their predicate on a swap.
//! Floating-point operand order and noncommutative operations are preserved.

use crate::scratch::Scratch;
use keel_ir::{BinaryOp, Block, ConvertOp, Function, InstData, Type, UnaryOp, Value, ValueDef};

fn literal(func: &Function, value: Value) -> bool {
    match func.value_def(func.resolve(value)) {
        ValueDef::Result(inst, _) => matches!(func.inst_data(inst), InstData::Iconst { .. }),
        _ => false,
    }
}

/// Whether `(literal, value)` orders `a` after `b`, so the pair needs a swap.
/// Decides on the value IDs first, which spares one or both literal lookups.
fn out_of_order(func: &Function, a: Value, b: Value) -> bool {
    match a.cmp(&b) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => literal(func, a) && !literal(func, b),
        std::cmp::Ordering::Greater => !literal(func, b) || literal(func, a),
    }
}

// Keep the shifted value at its full width; arithmetic right shifts do not
// form rotates. Count expressions are interpreted using IR's modulo rule.
fn shift(func: &Function, value: Value, expected: BinaryOp) -> Option<(Value, Value)> {
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
    Some((source, count))
}

/// A variable count as `sign * base + offset`, modulo the shifted width.
/// Integer conversions preserve the low five/six bits, even sign extensions.
/// Stop after six producers, enough for C's mask/trunc/subtract/mask/extend.
fn variable_count(
    func: &Function,
    value: Value,
    mask: i64,
    depth: u32,
) -> Option<(Value, bool, i64)> {
    let value = func.resolve(value);
    if depth == 0 || crate::ir_match::int_const(func, value).is_some() {
        return None;
    }
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return Some((value, false, 0));
    };
    let next = |v| variable_count(func, v, mask, depth - 1);
    match *func.inst_data(inst) {
        InstData::Convert {
            op: ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext,
            arg,
            ..
        } if func.value_type(arg).is_int() => next(arg),
        InstData::Unary {
            op: UnaryOp::Ineg,
            arg,
        } => {
            let (base, negative, offset) = next(arg)?;
            Some((base, !negative, offset.wrapping_neg() & mask))
        }
        InstData::Binary { op, args: [a, b] } => {
            let literal = |v| crate::ir_match::int_const(func, v);
            match (op, literal(a), literal(b)) {
                (BinaryOp::And, _, Some(n)) if n & mask == mask => next(a),
                (BinaryOp::And, Some(n), _) if n & mask == mask => next(b),
                (BinaryOp::Isub, Some(n), _) => {
                    let (base, negative, offset) = next(b)?;
                    Some((base, !negative, n.wrapping_sub(offset) & mask))
                }
                _ => Some((value, false, 0)),
            }
        }
        _ => Some((value, false, 0)),
    }
}

fn rotate(func: &Function, args: [Value; 2]) -> Option<InstData> {
    for [left, right] in [args, [args[1], args[0]]] {
        let Some((x, left_count)) = shift(func, left, BinaryOp::Shl) else {
            continue;
        };
        let Some((y, count)) = shift(func, right, BinaryOp::Ushr) else {
            continue;
        };
        let width = match func.value_type(x) {
            Type::I32 => 32,
            Type::I64 => 64,
            _ => continue,
        };
        if x != y {
            continue;
        }
        let complementary = match (
            crate::ir_match::int_const(func, left_count),
            crate::ir_match::int_const(func, count),
        ) {
            (Some(l), Some(r)) => l > 0 && l < width && r > 0 && r < width && l + r == width,
            _ => {
                let mask = width - 1;
                if let (Some((a, an, ac)), Some((b, bn, bc))) = (
                    variable_count(func, left_count, mask, 6),
                    variable_count(func, count, mask, 6),
                ) && a == b
                    && an != bn
                    && ac.wrapping_add(bc) & mask == 0
                {
                    // Prefer the base count over C's mask/cast/complement
                    // chain when its low bits already give the rotation.
                    if ac == 0 && func.value_type(a).bytes().is_some_and(|n| n <= 8) {
                        return Some(InstData::Binary {
                            op: if an { BinaryOp::Rotr } else { BinaryOp::Rotl },
                            args: [x, a],
                        });
                    }
                    true
                } else {
                    false
                }
            }
        };
        if complementary {
            return Some(InstData::Binary {
                op: BinaryOp::Rotr,
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
    if let InstData::Binary {
        op: BinaryOp::Or,
        args,
    } = *data
        && let Some(rotated) = rotate(func, args)
    {
        *data = rotated;
    }
    match data {
        InstData::Binary { op, args } if !op.is_float() && crate::gvn::is_commutative(*op) => {
            if out_of_order(func, args[0], args[1]) {
                args.swap(0, 1);
            }
        }
        InstData::Overflow { op, args } if crate::gvn::is_commutative_overflow(*op) => {
            if out_of_order(func, args[0], args[1]) {
                args.swap(0, 1);
            }
        }
        InstData::Icmp { cond, args } if out_of_order(func, args[0], args[1]) => {
            args.swap(0, 1);
            *cond = crate::gvn::reverse(*cond);
        }
        _ => {}
    }
}

/// Whether `normalize` would change `data`. Most instructions are already
/// normal, and checking first avoids copying them.
pub(crate) fn needs_normalize(func: &Function, data: &InstData) -> bool {
    // An operand resolves to another value exactly when it is an alias.
    let aliased = |v: Value| matches!(func.value_def(v), ValueDef::Alias(_));
    // The common shapes are matched once for both checks.
    match *data {
        InstData::Binary { op, args } => {
            aliased(args[0])
                || aliased(args[1])
                || (op == BinaryOp::Or && rotate(func, args).is_some())
                || (!op.is_float()
                    && crate::gvn::is_commutative(op)
                    && out_of_order(func, args[0], args[1]))
        }
        InstData::Overflow { op, args } => {
            aliased(args[0])
                || aliased(args[1])
                || (crate::gvn::is_commutative_overflow(op) && out_of_order(func, args[0], args[1]))
        }
        InstData::Icmp { args, .. } => {
            aliased(args[0]) || aliased(args[1]) || out_of_order(func, args[0], args[1])
        }
        InstData::Iconst { .. } | InstData::Fconst { .. } => false,
        InstData::Load { addr, .. } => aliased(addr),
        InstData::Store { value, addr, .. } => aliased(value) || aliased(addr),
        _ => {
            let mut any = false;
            data.for_each_operand(|v| any |= aliased(v));
            any
        }
    }
}

pub fn run(func: &mut Function) -> bool {
    let mut changed = false;
    let mut blocks: Scratch<Block> = Scratch::new();
    blocks.extend(func.blocks());
    for &block in &blocks {
        // Normalizing rewrites payloads in place; the layout does not change.
        for i in 0..func.block_insts(block).len() {
            let inst = func.block_insts(block)[i];
            if !needs_normalize(func, func.inst_data(inst)) {
                continue;
            }
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
