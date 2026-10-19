//! Local instruction combining using the IR's wrapping integer semantics.
//!
//! No reassociation of IEEE floating-point operations is permitted. Division
//! identities require a known nonzero divisor; x/x and 0/x may still trap.
//! A use worklist revisits consumers after substitutions, including when
//! layout order differs from definition order across blocks.

mod constant_division;

use crate::const_fold::{self, constant_data as constant};
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InsertPoint, Inst, InstData, IntCC, Type, UnaryOp,
    Value, ValueDef,
};

enum Fold {
    Value(Value),
    Data(InstData),
    Multi([InstData; 2]),
}

fn bits(func: &Function, v: Value) -> Option<u128> {
    let InstData::Iconst { ty, imm } = constant(func, v)? else {
        return None;
    };
    Some((imm as i128 as u128) & mask(ty))
}

fn mask(ty: Type) -> u128 {
    u128::MAX >> (128 - 8 * ty.bytes().unwrap())
}

fn zero(ty: Type) -> Fold {
    Fold::Data(InstData::Iconst { ty, imm: 0 })
}

fn binary(func: &Function, value: Value) -> Option<(BinaryOp, [Value; 2])> {
    let ValueDef::Result(inst, 0) = func.value_def(func.resolve(value)) else {
        return None;
    };
    let InstData::Binary { op, args } = *func.inst_data(inst) else {
        return None;
    };
    Some((op, args.map(|v| func.resolve(v))))
}

fn boolean(func: &Function, op: BinaryOp, a: Value, b: Value, ty: Type) -> Option<Fold> {
    if !matches!(op, BinaryOp::And | BinaryOp::Or | BinaryOp::Xor) {
        return None;
    }
    for (a, b) in [(a, b), (b, a)] {
        if let ValueDef::Result(inst, 0) = func.value_def(b)
            && let InstData::Unary {
                op: UnaryOp::Bnot,
                arg,
            } = *func.inst_data(inst)
            && func.resolve(arg) == a
        {
            return Some(if op == BinaryOp::And {
                zero(ty)
            } else {
                Fold::Data(InstData::Iconst { ty, imm: -1 })
            });
        }
        if let Some((inner, args)) = binary(func, b) {
            let other = if args[0] == a {
                Some(args[1])
            } else if args[1] == a {
                Some(args[0])
            } else {
                None
            };
            if let Some(other) = other {
                match (op, inner) {
                    (BinaryOp::And, BinaryOp::Or) | (BinaryOp::Or, BinaryOp::And) => {
                        return Some(Fold::Value(a));
                    }
                    (BinaryOp::Xor, BinaryOp::Xor) => return Some(Fold::Value(other)),
                    (BinaryOp::And, BinaryOp::And) | (BinaryOp::Or, BinaryOp::Or) => {
                        return Some(Fold::Value(b));
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn algebra(
    func: &mut Function,
    inst: Inst,
    op: BinaryOp,
    a: Value,
    b: Value,
    ty: Type,
    users: &[Vec<Inst>],
) -> Option<Fold> {
    if !matches!(
        op,
        BinaryOp::Iadd
            | BinaryOp::Isub
            | BinaryOp::Imul
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Xor
    ) {
        return None;
    }
    if op == BinaryOp::Isub {
        if let Some((BinaryOp::Iadd, args)) = binary(func, a) {
            if args[0] == b {
                return Some(Fold::Value(args[1]));
            }
            if args[1] == b {
                return Some(Fold::Value(args[0]));
            }
        }
        if let Some((BinaryOp::Isub, [left, right])) = binary(func, b)
            && a == left
        {
            return Some(Fold::Value(right));
        }
        if let (Some((BinaryOp::Iadd, left)), Some((BinaryOp::Iadd, right))) =
            (binary(func, a), binary(func, b))
        {
            for i in 0..2 {
                for j in 0..2 {
                    if left[i] == right[j] {
                        return Some(Fold::Data(InstData::Binary {
                            op: BinaryOp::Isub,
                            args: [left[1 - i], right[1 - j]],
                        }));
                    }
                }
            }
        }
    }
    if op == BinaryOp::Iadd {
        for (a, b) in [(a, b), (b, a)] {
            if let Some((BinaryOp::Isub, [left, right])) = binary(func, a)
                && b == right
            {
                return Some(Fold::Value(left));
            }
            if let ValueDef::Result(inner, 0) = func.value_def(b)
                && let InstData::Unary {
                    op: UnaryOp::Ineg,
                    arg,
                } = *func.inst_data(inner)
                && func.resolve(arg) == a
            {
                return Some(zero(ty));
            }
        }
    }
    // Reassociation must remove the inner operation, rather than duplicate a
    // shared computation or merely move constants between live expressions.
    if op != BinaryOp::Isub
        && users[a.index()].len() == 1
        && let Some((inner_op, args)) = binary(func, a)
        && inner_op == op
        && let (Some(c1), Some(c2)) = (bits(func, args[1]), bits(func, b))
    {
        let combined = match op {
            BinaryOp::Iadd => c1.wrapping_add(c2),
            BinaryOp::Imul => c1.wrapping_mul(c2),
            BinaryOp::And => c1 & c2,
            BinaryOp::Or => c1 | c2,
            BinaryOp::Xor => c1 ^ c2,
            _ => unreachable!(),
        } & mask(ty);
        let imm = combined as i64;
        if ty == Type::I128 && imm as i128 as u128 != combined {
            return None;
        }
        let c = func.insert_inst(InsertPoint::Before(inst), InstData::Iconst { ty, imm });
        func.set_inst_loc(c, func.inst_loc(inst));
        return Some(Fold::Data(InstData::Binary {
            op,
            args: [args[0], func.first_result(c)],
        }));
    }
    None
}

// Extensions preserve zero/nonzero only when they strictly widen integers.
// Resolve both the producer and its operand: aliases need not be in layout order.
fn widening_source(func: &Function, value: Value) -> Option<Value> {
    let value = func.resolve(value);
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    let InstData::Convert {
        op: ConvertOp::Uext | ConvertOp::Sext,
        ty,
        arg,
    } = *func.inst_data(inst)
    else {
        return None;
    };
    let arg = func.resolve(arg);
    let from = func.value_type(arg);
    (from.is_int() && ty.is_int() && func.value_type(value) == ty && from.bytes() < ty.bytes())
        .then_some(arg)
}

fn invert_integer(cond: IntCC) -> IntCC {
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

fn simplify_branch(func: &Function, inst: Inst) -> Option<InstData> {
    let InstData::Brif { cond, .. } = func.inst_data(inst) else {
        return None;
    };
    let mut source = widening_source(func, *cond)?;
    while let Some(narrower) = widening_source(func, source) {
        source = narrower;
    }
    let mut data = func.inst_data(inst).clone();
    if let InstData::Brif { cond, .. } = &mut data {
        *cond = source;
    }
    crate::canonicalize::normalize(func, &mut data);
    Some(data)
}

fn simplify(func: &mut Function, inst: Inst, users: Option<&[Vec<Inst>]>) -> Option<Fold> {
    let mut data = func.inst_data(inst).clone();
    crate::canonicalize::normalize(func, &mut data);
    let canonicalized = data != *func.inst_data(inst);

    if let Some(constants) = const_fold::fold_overflow(&data, |v| constant(func, v)) {
        return Some(Fold::Multi(constants));
    }
    let result = *func.inst_results(inst).first()?;
    let ty = func.value_type(result);
    if let InstData::Select { args: [cond, a, b] } = data {
        if a == b {
            return Some(Fold::Value(a));
        }
        if let Some(c) = bits(func, cond) {
            return Some(Fold::Value(if c == 0 { b } else { a }));
        }
    }
    if !matches!(data, InstData::Iconst { .. } | InstData::Fconst { .. })
        && func.inst_results(inst).len() == 1
        && let Some(folded) = const_fold::fold(&data, |v| constant(func, v))
    {
        return Some(Fold::Data(folded));
    }
    match data {
        InstData::Binary { op, args: [a, b] } if !op.is_float() => {
            let (x, y) = (bits(func, a), bits(func, b));
            match op {
                BinaryOp::Iadd | BinaryOp::Or | BinaryOp::Xor if y == Some(0) => {
                    return Some(Fold::Value(a));
                }
                BinaryOp::Iadd | BinaryOp::Or | BinaryOp::Xor if x == Some(0) => {
                    return Some(Fold::Value(b));
                }
                BinaryOp::Isub if y == Some(0) => return Some(Fold::Value(a)),
                BinaryOp::Isub | BinaryOp::Xor if a == b => return Some(zero(ty)),
                BinaryOp::And | BinaryOp::Or if a == b => return Some(Fold::Value(a)),
                BinaryOp::Imul if y == Some(1) => return Some(Fold::Value(a)),
                BinaryOp::Imul if x == Some(1) => return Some(Fold::Value(b)),
                BinaryOp::Imul | BinaryOp::And if x == Some(0) || y == Some(0) => {
                    return Some(zero(ty));
                }
                BinaryOp::And if x == Some(mask(ty)) => return Some(Fold::Value(b)),
                BinaryOp::And if y == Some(mask(ty)) => return Some(Fold::Value(a)),
                BinaryOp::Or if x == Some(mask(ty)) || y == Some(mask(ty)) => {
                    return Some(Fold::Data(InstData::Iconst { ty, imm: -1 }));
                }
                BinaryOp::Udiv | BinaryOp::Sdiv if y == Some(1) => return Some(Fold::Value(a)),
                BinaryOp::Urem | BinaryOp::Srem if y == Some(1) => return Some(zero(ty)),
                // Unlike signed division, MIN % -1 is defined and equals zero.
                BinaryOp::Srem if y == Some(mask(ty)) => return Some(zero(ty)),
                BinaryOp::Shl
                | BinaryOp::Ushr
                | BinaryOp::Sshr
                | BinaryOp::Rotl
                | BinaryOp::Rotr
                    if y.is_some_and(|n| n % (8 * ty.bytes().unwrap()) as u128 == 0) =>
                {
                    return Some(Fold::Value(a));
                }
                BinaryOp::Ptradd if y == Some(0) => return Some(Fold::Value(a)),
                _ => {}
            }
            if let Some(fold) = boolean(func, op, a, b, ty) {
                return Some(fold);
            }
            if let Some(users) = users
                && let Some(fold) = algebra(func, inst, op, a, b, ty, users)
            {
                return Some(fold);
            }
            let power = match op {
                BinaryOp::Imul => y.or(x),
                BinaryOp::Udiv | BinaryOp::Urem => y,
                _ => None,
            };
            if let Some(power) = power.filter(|n| n.is_power_of_two()) {
                let shift = power.trailing_zeros();
                let operand = if op == BinaryOp::Imul && y.is_none() {
                    b
                } else {
                    a
                };
                let new_op = match op {
                    BinaryOp::Imul => BinaryOp::Shl,
                    BinaryOp::Udiv => BinaryOp::Ushr,
                    BinaryOp::Urem => {
                        // Remainder by a power of two is a mask; use a
                        // same-width constant and retain the result handle.
                        if ty == Type::I128 && power > (1u128 << 63) {
                            return None;
                        }
                        let c = func.insert_inst(
                            InsertPoint::Before(inst),
                            InstData::Iconst {
                                ty,
                                imm: (power - 1) as i64,
                            },
                        );
                        func.set_inst_loc(c, func.inst_loc(inst));
                        return Some(Fold::Data(InstData::Binary {
                            op: BinaryOp::And,
                            args: [a, func.first_result(c)],
                        }));
                    }
                    _ => return None,
                };
                let c = func.insert_inst(
                    InsertPoint::Before(inst),
                    InstData::Iconst {
                        ty: Type::I64,
                        imm: shift as i64,
                    },
                );
                func.set_inst_loc(c, func.inst_loc(inst));
                return Some(Fold::Data(InstData::Binary {
                    op: new_op,
                    args: [operand, func.first_result(c)],
                }));
            }
            if matches!(op, BinaryOp::Udiv | BinaryOp::Urem)
                && users.is_some()
                && let Some(divisor) = y
                && let Some(data) = constant_division::reduce(func, inst, op, a, b, ty, divisor)
            {
                return Some(Fold::Data(data));
            }
        }
        InstData::Unary {
            op: UnaryOp::Ineg | UnaryOp::Bnot,
            arg,
        } => {
            let ValueDef::Result(inner, _) = func.value_def(arg) else {
                return None;
            };
            if let InstData::Unary {
                op: inner_op,
                arg: original,
            } = *func.inst_data(inner)
                && let InstData::Unary { op, .. } = data
                && inner_op == op
            {
                return Some(Fold::Value(original));
            }
        }
        InstData::Convert { op, ty, arg } => {
            if matches!(op, ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc)
                && ty == func.value_type(arg)
            {
                return Some(Fold::Value(arg));
            }
            let ValueDef::Result(inner, _) = func.value_def(arg) else {
                return None;
            };
            if let InstData::Convert {
                op: inner_op,
                arg: original,
                ..
            } = *func.inst_data(inner)
                && matches!(op, ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext)
                && matches!(
                    inner_op,
                    ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext
                )
            {
                let original = func.resolve(original);
                let from = func.value_type(original);
                let combined = match (op, inner_op) {
                    (ConvertOp::Trunc, _) if ty == from => return Some(Fold::Value(original)),
                    (ConvertOp::Trunc, _) if ty.bytes() < from.bytes() => Some(ConvertOp::Trunc),
                    (ConvertOp::Trunc, ConvertOp::Uext | ConvertOp::Sext) => Some(inner_op),
                    (ConvertOp::Uext | ConvertOp::Sext, ConvertOp::Uext) => Some(ConvertOp::Uext),
                    (ConvertOp::Sext, ConvertOp::Sext) => Some(ConvertOp::Sext),
                    // Zero-extending a sign extension preserves the intermediate
                    // width's sign bits, not those of the final width.
                    _ => None,
                };
                if let Some(op) = combined {
                    return Some(Fold::Data(InstData::Convert {
                        op,
                        ty,
                        arg: original,
                    }));
                }
            }
        }
        InstData::Icmp { cond, args: [a, b] } if a == b => {
            let truth = matches!(
                cond,
                IntCC::Eq | IntCC::Sle | IntCC::Sge | IntCC::Ule | IntCC::Uge
            );
            return Some(Fold::Data(InstData::Iconst {
                ty: Type::I8,
                imm: i64::from(truth),
            }));
        }
        InstData::Icmp { cond, args: [a, b] } if matches!(cond, IntCC::Eq | IntCC::Ne) => {
            if bits(func, b) == Some(0) {
                if let Some(source) = widening_source(func, a) {
                    let zero = func.insert_inst(
                        InsertPoint::Before(inst),
                        InstData::Iconst {
                            ty: func.value_type(source),
                            imm: 0,
                        },
                    );
                    func.set_inst_loc(zero, func.inst_loc(inst));
                    return Some(Fold::Data(InstData::Icmp {
                        cond,
                        args: [source, func.first_result(zero)],
                    }));
                }
                if func.value_type(a) == Type::I8
                    && let ValueDef::Result(inner, 0) = func.value_def(a)
                    && let InstData::Icmp {
                        cond: inner_cond,
                        args,
                    } = *func.inst_data(inner)
                {
                    // Integer comparisons are canonical i8 0/1. Copying their
                    // payload preserves this result's identity and location,
                    // without changing any other use of the inner comparison.
                    return Some(Fold::Data(InstData::Icmp {
                        cond: if cond == IntCC::Eq {
                            invert_integer(inner_cond)
                        } else {
                            inner_cond
                        },
                        args: args.map(|v| func.resolve(v)),
                    }));
                }
            }
            if let ValueDef::Result(left, 0) = func.value_def(a)
                && let ValueDef::Result(right, 0) = func.value_def(b)
                && let InstData::Convert { op, ty, arg: x } = *func.inst_data(left)
                && matches!(op, ConvertOp::Sext | ConvertOp::Uext)
                && let InstData::Convert {
                    op: right_op,
                    ty: right_ty,
                    arg: y,
                } = *func.inst_data(right)
                && op == right_op
                && ty == right_ty
            {
                let (x, y) = (func.resolve(x), func.resolve(y));
                let from = func.value_type(x);
                // Matching widening extensions are injective. Truncation and
                // mixed sign/zero extensions do not preserve equality this way.
                if from.is_int()
                    && ty.is_int()
                    && from == func.value_type(y)
                    && from.bytes() < ty.bytes()
                {
                    return Some(Fold::Data(InstData::Icmp { cond, args: [x, y] }));
                }
            }
        }
        _ => {}
    }
    canonicalized.then_some(Fold::Data(data))
}

/// Reuse only a quotient already executed earlier in this block. Keeping that
/// division preserves zero-divisor traps and signed MIN/-1 overflow traps;
/// introducing a signed division for a standalone remainder would not.
fn reuse_quotients(func: &mut Function) -> bool {
    let mut quotients = std::collections::HashMap::new();
    let mut changed = false;
    for block in func.blocks().collect::<Vec<_>>() {
        quotients.clear();
        for inst in func.block_insts(block).to_vec() {
            changed |= reuse_quotient(func, inst, &mut quotients);
        }
    }
    changed
}

pub(crate) type Quotients = std::collections::HashMap<(BinaryOp, [Value; 2]), Value>;

/// The caller clears this table at every block boundary and may cap its size.
/// Only an earlier quotient can guard the remainder's zero/overflow behavior.
pub(crate) fn reuse_quotient(func: &mut Function, inst: Inst, quotients: &mut Quotients) -> bool {
    let InstData::Binary { op, args } = *func.inst_data(inst) else {
        return false;
    };
    let args = args.map(|v| func.resolve(v));
    let division = match op {
        BinaryOp::Sdiv | BinaryOp::Udiv => {
            quotients.insert((op, args), func.first_result(inst));
            return false;
        }
        BinaryOp::Srem => BinaryOp::Sdiv,
        BinaryOp::Urem => BinaryOp::Udiv,
        _ => return false,
    };
    let Some(&quotient) = quotients.get(&(division, args)) else {
        return false;
    };
    let product = func.insert_inst(
        InsertPoint::Before(inst),
        InstData::Binary {
            op: BinaryOp::Imul,
            args: [quotient, args[1]],
        },
    );
    func.set_inst_loc(product, func.inst_loc(inst));
    *func.inst_data_mut(inst) = InstData::Binary {
        op: BinaryOp::Isub,
        args: [args[0], func.first_result(product)],
    };
    true
}

pub fn run(func: &mut Function) -> bool {
    let reused_quotients = reuse_quotients(func);
    let mut users: Vec<Vec<Inst>> = vec![Vec::new(); func.num_values()];
    let mut work: Vec<Inst> = func.layout_insts().map(|(_, i)| i).collect();
    for &inst in &work {
        func.inst_data(inst)
            .for_each_operand(|v| users[func.resolve(v).index()].push(inst));
    }
    // Start with definitions first within the usual layout; dependency
    // notifications make the final result independent of layout order.
    work.reverse();
    let mut queued = vec![true; func.num_insts()];
    let mut changed = reused_quotients;
    while let Some(inst) = work.pop() {
        queued[inst.index()] = false;
        if func.is_removed(inst) {
            continue;
        }
        if func.inst_results(inst).is_empty() {
            if let Some(data) = simplify_branch(func, inst) {
                // A terminator has no result users to notify, but its new
                // condition must notify it if that producer changes later.
                func.inst_data(inst).for_each_operand(|v| {
                    users[func.resolve(v).index()].retain(|&user| user != inst);
                });
                *func.inst_data_mut(inst) = data;
                func.inst_data(inst)
                    .for_each_operand(|v| users[func.resolve(v).index()].push(inst));
                changed = true;
            }
            continue;
        }
        let Some(fold) = simplify(func, inst, Some(&users)) else {
            continue;
        };
        let result = func.first_result(inst);
        let waiting: Vec<_> = func
            .inst_results(inst)
            .iter()
            .flat_map(|result| users[result.index()].iter().copied())
            .collect();
        match fold {
            Fold::Multi(constants) => {
                const_fold::replace_with_constants(func, inst, &constants);
                users.resize_with(func.num_values(), Vec::new);
                queued.resize(func.num_insts(), false);
            }
            Fold::Value(value) => {
                let value = func.resolve(value);
                if value == result {
                    continue;
                }
                func.replace_all_uses(result, value);
                func.remove_inst(inst);
                users[value.index()].extend(waiting.iter().copied());
            }
            Fold::Data(data) => {
                if *func.inst_data(inst) == data {
                    continue;
                }
                *func.inst_data_mut(inst) = data;
                // Strength reduction can create constants with fresh values.
                users.resize_with(func.num_values(), Vec::new);
                func.inst_data(inst)
                    .for_each_operand(|v| users[func.resolve(v).index()].push(inst));
                if queued.len() < func.num_insts() {
                    queued.resize(func.num_insts(), false);
                }
                // Revisit the rewritten instruction (e.g. shift by zero).
                queued[inst.index()] = true;
                work.push(inst);
            }
        }
        for user in waiting {
            if !queued[user.index()] {
                queued[user.index()] = true;
                work.push(user);
            }
        }
        changed = true;
    }
    changed
}

/// One visit, with no user graph, reassociation, reciprocal division expansion,
/// or consumer requeue. Used by the bounded Fast pipeline.
pub(crate) fn simplify_once(func: &mut Function, inst: Inst) -> bool {
    if func.inst_results(inst).is_empty() {
        return false;
    }
    let Some(fold) = simplify(func, inst, None) else {
        return false;
    };
    match fold {
        Fold::Value(value) => {
            let value = func.resolve(value);
            let result = func.first_result(inst);
            if value == result {
                return false;
            }
            func.replace_all_uses(result, value);
            func.remove_inst(inst);
        }
        Fold::Data(data) => {
            if *func.inst_data(inst) == data {
                return false;
            }
            *func.inst_data_mut(inst) = data;
        }
        Fold::Multi(constants) => const_fold::replace_with_constants(func, inst, &constants),
    }
    true
}
