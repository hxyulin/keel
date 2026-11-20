//! Shortcut a scalar xor-and-multiply recurrence when its high input bytes are zero.
use crate::ir_match::{def, int_const};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InstData as D, IntCC, Type, Value,
};

struct Chain {
    split: usize,
    last: usize,
    word: Value,
    prefix: Value,
    suffix_factor: i64,
}

fn binary(f: &Function, value: Value, op: B) -> Option<[Value; 2]> {
    match *f.inst_data(def(f, value)?) {
        D::Binary { op: found, args } if found == op => Some(args.map(|v| f.resolve(v))),
        _ => None,
    }
}

fn input_byte(f: &Function, byte: Value, shift: i64) -> Option<Value> {
    let shifted = if shift == 56 {
        byte
    } else if let Some([lhs, rhs]) = binary(f, byte, B::And) {
        if int_const(f, rhs) != Some(255) {
            return None;
        }
        lhs
    } else {
        let D::Convert {
            op: ConvertOp::Uext,
            ty: Type::I64,
            arg,
        } = *f.inst_data(def(f, byte)?)
        else {
            return None;
        };
        let D::Convert {
            op: ConvertOp::Trunc,
            ty: Type::I8,
            arg,
        } = *f.inst_data(def(f, arg)?)
        else {
            return None;
        };
        f.resolve(arg)
    };
    if shift == 0 {
        return Some(shifted);
    }
    let [word, amount] = binary(f, shifted, B::Ushr)?;
    (int_const(f, amount) == Some(shift)).then_some(word)
}

fn match_chain(f: &Function, block: Block, last: usize) -> Option<Chain> {
    let insts = f.block_insts(block);
    let last_inst = insts[last];
    let mut value = f.first_result(last_inst);
    if f.value_type(value) != Type::I64 || f.resolve(value) != value {
        return None;
    }
    let mut word = None;
    let mut multiplier = None;
    let mut prefix = None;
    for lane in (0..8).rev() {
        let shift = lane * 8;
        let inst = def(f, value)?;
        if f.inst_block(inst) != block {
            return None;
        }
        if shift == 8 {
            prefix = Some(value);
        }
        let [mix, factor] = binary(f, value, B::Imul)?;
        let factor = int_const(f, factor)?;
        if factor & 1 == 0
            || factor == 1
            || multiplier.replace(factor).is_some_and(|old| old != factor)
        {
            return None;
        }
        let [lhs, rhs] = binary(f, mix, B::Xor)?;
        let (prior, source) = [(lhs, rhs), (rhs, lhs)]
            .into_iter()
            .find_map(|(prior, byte)| {
                let source = input_byte(f, byte, shift)?;
                word.is_none_or(|expected| source == expected)
                    .then_some((prior, source))
            })?;
        word = Some(source);
        value = prior;
    }
    let word = word?;
    let prefix = prefix?;
    let split = insts.iter().position(|&i| Some(i) == def(f, prefix))? + 1;
    if split >= last || f.value_type(word) != Type::I64 {
        return None;
    }
    // All work that the fast path skips must be local, scalar arithmetic.
    // No result can escape except the final recurrence value.
    let skipped = &insts[split..=last];
    for &i in skipped {
        if f.inst_loc(i).is_some()
            || !matches!(
                f.inst_data(i),
                D::Iconst { .. }
                    | D::Convert {
                        op: ConvertOp::Uext | ConvertOp::Trunc,
                        ..
                    }
                    | D::Binary {
                        op: B::And | B::Ushr | B::Xor | B::Imul,
                        ..
                    }
            )
        {
            return None;
        }
    }
    for (_, inst) in f.layout_insts() {
        if skipped.contains(&inst) {
            continue;
        }
        let mut escapes = false;
        f.inst_data(inst).for_each_operand(|v| {
            escapes |= def(f, v).is_some_and(|i| i != last_inst && skipped.contains(&i));
        });
        if escapes {
            return None;
        }
    }
    Some(Chain {
        split,
        last,
        word,
        prefix,
        suffix_factor: multiplier?.wrapping_pow(6),
    })
}

fn shortcut(f: &mut Function, block: Block, chain: Chain) {
    let last = f.block_insts(block)[chain.last];
    let old = f.first_result(last);
    let cont = f.create_block();
    let slow = f.create_block();
    let fast = f.create_block();
    let result = f.append_block_param(cont, Type::I64);
    f.move_insts_to_end(block, chain.last + 1, cont);
    f.move_insts_to_end(block, chain.split, slow);
    let fallback = f.inst_data(last).clone();
    f.remove_inst(last);
    let slow_value = f.append_inst(slow, fallback);
    let max = f.append_inst(block, D::iconst(Type::I64, u16::MAX.into()));
    let high = f.append_inst(
        block,
        D::Icmp {
            cond: IntCC::Ugt,
            args: [chain.word, f.first_result(max)],
        },
    );
    f.append_inst(
        block,
        D::Brif {
            cond: f.first_result(high),
            dests: Box::new([
                BlockCall {
                    block: slow,
                    args: vec![],
                },
                BlockCall {
                    block: fast,
                    args: vec![],
                },
            ]),
        },
    );
    let factor = f.append_inst(fast, D::iconst(Type::I64, chain.suffix_factor));
    let fast_value = f.append_inst(
        fast,
        D::Binary {
            op: B::Imul,
            args: [chain.prefix, f.first_result(factor)],
        },
    );
    for (block, value) in [(slow, slow_value), (fast, fast_value)] {
        f.append_inst(
            block,
            D::Jump {
                dest: BlockCall {
                    block: cont,
                    args: vec![f.first_result(value)],
                },
            },
        );
    }
    f.replace_all_uses(old, result);
}

pub(crate) fn run(f: &mut Function) -> bool {
    if f.debug_loc.is_some() || f.debug_return.is_some() || !f.debug_vars.is_empty() {
        return false;
    }
    let mut changed = false;
    for block in f.blocks().collect::<Vec<_>>() {
        let insts = f.block_insts(block);
        if let Some(chain) = insts.iter().enumerate().find_map(|(pos, &inst)| {
            let &previous = insts.get(pos.checked_sub(1)?)?;
            if let D::Store { value, flags, .. } = f.inst_data(inst)
                && !flags.volatile
                && def(f, *value) == Some(previous)
                && matches!(f.inst_data(previous), D::Binary { op: B::Imul, .. })
            {
                return match_chain(f, block, pos - 1);
            }
            None
        }) {
            shortcut(f, block, chain);
            changed = true;
        }
    }
    changed
}
