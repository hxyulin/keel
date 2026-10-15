//! Monotone known-zero/known-one facts over scalar integer SSA values.
//!
//! Unknown is the conservative starting point, including loop parameters.
//! Facts flow through bit operations, constant shifts, casts, selects and all
//! incoming block edges. No edge is assumed executable or branch fact inferred.
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, Inst, InstData, IntCC, Type, UnaryOp, Value, ValueDef,
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Bits {
    zero: u128,
    one: u128,
}

fn mask(width: u32) -> u128 {
    u128::MAX >> (128 - width)
}
fn width(ty: Type) -> Option<u32> {
    ty.is_int().then(|| ty.bytes().unwrap() * 8)
}
fn intersect(a: Bits, b: Bits) -> Bits {
    Bits {
        zero: a.zero & b.zero,
        one: a.one & b.one,
    }
}
fn exact(value: u128, width: u32) -> Bits {
    Bits {
        zero: !value & mask(width),
        one: value & mask(width),
    }
}
fn bounds(bits: Bits, width: u32, signed: bool) -> (u128, u128) {
    let (zero, one) = if signed {
        let sign = 1u128 << (width - 1);
        (
            (bits.zero & !sign) | (bits.one & sign),
            (bits.one & !sign) | (bits.zero & sign),
        )
    } else {
        (bits.zero, bits.one)
    };
    (one, mask(width) & !zero)
}

fn comparison(cond: IntCC, a: Bits, b: Bits, width: u32) -> Option<bool> {
    if matches!(cond, IntCC::Eq | IntCC::Ne) {
        let same = a == b && a.zero | a.one == mask(width);
        let different = (a.zero & b.one) | (a.one & b.zero) != 0;
        return if same {
            Some(cond == IntCC::Eq)
        } else if different {
            Some(cond == IntCC::Ne)
        } else {
            None
        };
    }
    let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
    let ((lo_a, hi_a), (lo_b, hi_b)) = (bounds(a, width, signed), bounds(b, width, signed));
    match cond {
        IntCC::Slt | IntCC::Ult if hi_a < lo_b => Some(true),
        IntCC::Slt | IntCC::Ult if lo_a >= hi_b => Some(false),
        IntCC::Sle | IntCC::Ule if hi_a <= lo_b => Some(true),
        IntCC::Sle | IntCC::Ule if lo_a > hi_b => Some(false),
        IntCC::Sgt | IntCC::Ugt => {
            comparison(if signed { IntCC::Slt } else { IntCC::Ult }, b, a, width)
        }
        IntCC::Sge | IntCC::Uge => {
            comparison(if signed { IntCC::Sle } else { IntCC::Ule }, b, a, width)
        }
        _ => None,
    }
}

type Incoming = Vec<Vec<(Inst, usize)>>;

fn infer(func: &Function, value: Value, facts: &[Bits], incoming: &Incoming) -> Bits {
    let get = |v| facts[func.resolve(v).index()];
    let w = width(func.value_type(value)).unwrap();
    let m = mask(w);
    match func.value_def(value) {
        ValueDef::Param(block, index) => {
            if Some(block) == func.entry_block() {
                return Bits::default();
            }
            incoming[block.index()]
                .iter()
                .map(|&(inst, edge)| {
                    get(func.inst_data(inst).branch_dests()[edge].args[index as usize])
                })
                .reduce(intersect)
                .unwrap_or_default()
        }
        ValueDef::Result(inst, result) => match *func.inst_data(inst) {
            InstData::Iconst { imm, .. } => exact(imm as i128 as u128, w),
            InstData::Icmp { cond, args } => {
                let (a, b) = (get(args[0]), get(args[1]));
                let arg_width = width(func.value_type(func.resolve(args[0])));
                match arg_width.and_then(|w| comparison(cond, a, b, w)) {
                    Some(value) => exact(u128::from(value), w),
                    None => Bits {
                        zero: m & !1,
                        one: 0,
                    },
                }
            }
            InstData::Fcmp { .. } => Bits {
                zero: m & !1,
                one: 0,
            },
            InstData::Overflow { .. } if result == 1 => Bits {
                zero: m & !1,
                one: 0,
            },
            InstData::Unary {
                op: UnaryOp::Bnot,
                arg,
            } => {
                let a = get(arg);
                Bits {
                    zero: a.one,
                    one: a.zero,
                }
            }
            InstData::Binary { op, args } => {
                let (a, b) = (get(args[0]), get(args[1]));
                match op {
                    BinaryOp::And => Bits {
                        zero: a.zero | b.zero,
                        one: a.one & b.one,
                    },
                    BinaryOp::Or => Bits {
                        zero: a.zero & b.zero,
                        one: a.one | b.one,
                    },
                    BinaryOp::Xor => Bits {
                        zero: (a.zero & b.zero) | (a.one & b.one),
                        one: (a.zero & b.one) | (a.one & b.zero),
                    },
                    BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr
                        if (b.zero | b.one) & u128::from(w - 1) == u128::from(w - 1) =>
                    {
                        let shift = (b.one & u128::from(w - 1)) as u32;
                        let high = m & !(m >> shift);
                        match op {
                            BinaryOp::Shl => Bits {
                                zero: ((a.zero << shift)
                                    | if shift == 0 { 0 } else { (1u128 << shift) - 1 })
                                    & m,
                                one: a.one << shift & m,
                            },
                            BinaryOp::Ushr => Bits {
                                zero: (a.zero >> shift) | high,
                                one: a.one >> shift,
                            },
                            _ => {
                                let sign = 1 << (w - 1);
                                Bits {
                                    zero: (a.zero >> shift)
                                        | if a.zero & sign != 0 { high } else { 0 },
                                    one: (a.one >> shift)
                                        | if a.one & sign != 0 { high } else { 0 },
                                }
                            }
                        }
                    }
                    _ => Bits::default(),
                }
            }
            InstData::Convert { op, arg, .. } => {
                let Some(input_width) = width(func.value_type(func.resolve(arg))) else {
                    return Bits::default();
                };
                let a = get(arg);
                match op {
                    ConvertOp::Trunc => Bits {
                        zero: a.zero & m,
                        one: a.one & m,
                    },
                    ConvertOp::Uext => Bits {
                        zero: a.zero | (m & !mask(input_width)),
                        one: a.one,
                    },
                    ConvertOp::Sext => {
                        let high = m & !mask(input_width);
                        let sign = 1 << (input_width - 1);
                        Bits {
                            zero: a.zero | if a.zero & sign != 0 { high } else { 0 },
                            one: a.one | if a.one & sign != 0 { high } else { 0 },
                        }
                    }
                    _ => Bits::default(),
                }
            }
            InstData::Select { args: [cond, a, b] } => {
                let c = get(cond);
                if c.one != 0 {
                    get(a)
                } else if c.zero == mask(width(func.value_type(func.resolve(cond))).unwrap()) {
                    get(b)
                } else {
                    intersect(get(a), get(b))
                }
            }
            _ => Bits::default(),
        },
        ValueDef::Alias(_) => unreachable!("worklist contains resolved definitions"),
    }
}

fn analyze(func: &Function) -> Vec<Bits> {
    let mut facts = vec![Bits::default(); func.num_values()];
    let mut users = vec![Vec::new(); func.num_values()];
    let mut incoming = vec![Vec::new(); func.num_blocks()];
    let mut work = Vec::new();
    for block in func.blocks() {
        work.extend(
            func.block_params(block)
                .iter()
                .copied()
                .filter(|&v| width(func.value_type(v)).is_some() && func.resolve(v) == v),
        );
    }
    for (_, inst) in func.layout_insts() {
        let results = func.inst_results(inst);
        let integer_result = |v: Value| width(func.value_type(v)).is_some() && func.resolve(v) == v;
        work.extend(results.iter().copied().filter(|&v| integer_result(v)));
        let data = func.inst_data(inst);
        // Arithmetic we do not model cannot gain facts from its operands.
        // Do not build or traverse dependencies that can never refine it.
        if matches!(
            data,
            InstData::Icmp { .. }
                | InstData::Select { .. }
                | InstData::Unary {
                    op: UnaryOp::Bnot,
                    ..
                }
                | InstData::Binary {
                    op: BinaryOp::And
                        | BinaryOp::Or
                        | BinaryOp::Xor
                        | BinaryOp::Shl
                        | BinaryOp::Ushr
                        | BinaryOp::Sshr,
                    ..
                }
                | InstData::Convert {
                    op: ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext,
                    ..
                }
        ) {
            data.for_each_operand(|arg| {
                users[func.resolve(arg).index()]
                    .extend(results.iter().copied().filter(|&v| integer_result(v)));
            });
        }
        for (edge, dest) in func.inst_data(inst).branch_dests().iter().enumerate() {
            incoming[dest.block.index()].push((inst, edge));
            for (&arg, &param) in dest.args.iter().zip(func.block_params(dest.block)) {
                if width(func.value_type(param)).is_some() && func.resolve(param) == param {
                    users[func.resolve(arg).index()].push(param);
                }
            }
        }
    }
    let mut queued = vec![false; func.num_values()];
    for &v in &work {
        queued[v.index()] = true;
    }
    work.reverse();
    while let Some(value) = work.pop() {
        queued[value.index()] = false;
        let inferred = infer(func, value, &facts, &incoming);
        let old = facts[value.index()];
        let new = Bits {
            zero: old.zero | inferred.zero,
            one: old.one | inferred.one,
        };
        debug_assert_eq!(new.zero & new.one, 0);
        if new != old {
            facts[value.index()] = new;
            for &user in &users[value.index()] {
                if !queued[user.index()] {
                    queued[user.index()] = true;
                    work.push(user);
                }
            }
        }
    }
    facts
}

pub fn run(func: &mut Function) -> bool {
    let facts = analyze(func);
    let get = |v| facts[func.resolve(v).index()];
    let mut replacements = Vec::new();
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        if data.is_pinned()
            || func.inst_results(inst).len() != 1
            || matches!(data, InstData::Iconst { .. })
        {
            continue;
        }
        let value = func.first_result(inst);
        if func.resolve(value) != value {
            continue;
        }
        let ty = func.value_type(value);
        let Some(w) = width(ty) else {
            continue;
        };
        let fact = facts[value.index()];
        if let InstData::Binary { op, args } = *data {
            for side in [0, 1] {
                let (a, b) = (get(args[side]), get(args[1 - side]));
                let identity = match op {
                    BinaryOp::And => (mask(w) & !a.zero) & !b.one == 0,
                    BinaryOp::Or => (mask(w) & !a.one) & !b.zero == 0,
                    _ => false,
                };
                if identity {
                    replacements.push((inst, Some(func.resolve(args[side])), None));
                    break;
                }
            }
            if replacements.last().is_some_and(|r| r.0 == inst) {
                continue;
            }
        }
        if fact.zero | fact.one == mask(w) {
            let imm = if w <= 64 {
                (((fact.one as u64) << (64 - w)) as i64) >> (64 - w)
            } else {
                let imm = fact.one as i64;
                if imm as i128 as u128 != fact.one {
                    continue;
                }
                imm
            };
            replacements.push((inst, None, Some(InstData::Iconst { ty, imm })));
        }
    }
    let changed = !replacements.is_empty();
    for (inst, value, data) in replacements {
        if let Some(value) = value {
            func.replace_all_uses(func.first_result(inst), value);
            func.remove_inst(inst);
        } else if let Some(data) = data {
            *func.inst_data_mut(inst) = data;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_four_bit_abstract_comparison_is_sound_for_all_concrete_values() {
        let states: Vec<_> = (0..81)
            .map(|mut encoding| {
                let mut bits = Bits::default();
                for bit in 0..4 {
                    match encoding % 3 {
                        1 => bits.zero |= 1 << bit,
                        2 => bits.one |= 1 << bit,
                        _ => {}
                    }
                    encoding /= 3;
                }
                let concrete: Vec<u128> = (0..16)
                    .filter(|v| v & bits.zero == 0 && v & bits.one == bits.one)
                    .collect();
                (bits, concrete)
            })
            .collect();
        for (a, values_a) in &states {
            for (b, values_b) in &states {
                for cond in [
                    IntCC::Eq,
                    IntCC::Ne,
                    IntCC::Ult,
                    IntCC::Ule,
                    IntCC::Ugt,
                    IntCC::Uge,
                    IntCC::Slt,
                    IntCC::Sle,
                    IntCC::Sgt,
                    IntCC::Sge,
                ] {
                    if let Some(proven) = comparison(cond, *a, *b, 4) {
                        for &x in values_a {
                            for &y in values_b {
                                let signed =
                                    |v: u128| if v & 8 != 0 { v as i32 - 16 } else { v as i32 };
                                let expected = match cond {
                                    IntCC::Eq => x == y,
                                    IntCC::Ne => x != y,
                                    IntCC::Ult => x < y,
                                    IntCC::Ule => x <= y,
                                    IntCC::Ugt => x > y,
                                    IntCC::Uge => x >= y,
                                    IntCC::Slt => signed(x) < signed(y),
                                    IntCC::Sle => signed(x) <= signed(y),
                                    IntCC::Sgt => signed(x) > signed(y),
                                    IntCC::Sge => signed(x) >= signed(y),
                                };
                                assert_eq!(proven, expected, "{a:?} {cond:?} {b:?}, x={x}, y={y}");
                            }
                        }
                    }
                }
            }
        }
    }
}
