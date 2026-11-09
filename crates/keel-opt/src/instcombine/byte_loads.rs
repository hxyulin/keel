//! Combine an exact two-byte little-endian assembly without crossing effects.
use super::{LazyUsers, Rule, Uses};
use crate::ir_match::{def, int_const};
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData as D, Type, Value, ValueDef,
};

// Bound recognition independently of function size.
const MAX_ADDRESS_DEPTH: usize = 8;
const MAX_WINDOW: usize = 16;

struct Byte {
    load: Inst,
    extend: Inst,
    addr: Value,
}

fn extended_byte(f: &Function, value: Value, ty: Type) -> Option<Byte> {
    let extend = def(f, value)?;
    let D::Convert {
        op: ConvertOp::Uext,
        ty: extended,
        arg,
    } = *f.inst_data(extend)
    else {
        return None;
    };
    if extended != ty {
        return None;
    }
    let load = def(f, arg)?;
    let D::Load {
        ty: Type::I8,
        addr,
        flags,
    } = *f.inst_data(load)
    else {
        return None;
    };
    if flags.volatile || !(flags.trap_ub || flags.notrap) {
        return None;
    }
    Some(Byte {
        load,
        extend,
        addr: f.resolve(addr),
    })
}

// Only checked literal offsets are peeled; dynamic address arithmetic remains
// an opaque SSA root. Never infer adjacency from alignment or pointer ordering.
fn address(f: &Function, mut value: Value) -> Option<(Value, i64)> {
    let mut offset = 0i64;
    for _ in 0..MAX_ADDRESS_DEPTH {
        value = f.resolve(value);
        let Some(inst) = def(f, value) else {
            return Some((value, offset));
        };
        let D::Binary {
            op: BinaryOp::Ptradd,
            args,
        } = *f.inst_data(inst)
        else {
            return Some((value, offset));
        };
        let Some(n) = int_const(f, args[1]) else {
            return Some((value, offset));
        };
        offset = offset.checked_add(n)?;
        value = args[0];
    }
    None
}

pub(super) fn run(f: &mut Function) -> bool {
    if !f.debug_vars.is_empty() {
        return false;
    }
    // Build use counts only after finding a candidate. Existing instcombine
    // user lists are built afterwards and include the new load's address use.
    let uses = LazyUsers::new(f);
    let mut changed = false;
    let roots: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| {
            matches!(
                f.inst_data(i),
                D::Binary {
                    op: BinaryOp::Or,
                    ..
                }
            )
            .then_some(i)
        })
        .collect();
    for root in roots {
        let D::Binary { args, .. } = *f.inst_data(root) else {
            continue;
        };
        if !matches!(f.value_def(f.first_result(root)), ValueDef::Result(owner, 0) if owner == root)
        {
            continue;
        }
        let ty = f.value_type(f.first_result(root));
        if !matches!(ty, Type::I32 | Type::I64) {
            continue;
        }
        for [low, shifted] in [args, [args[1], args[0]]] {
            let Some(shift) = def(f, shifted) else {
                continue;
            };
            let D::Binary {
                op: BinaryOp::Shl,
                args: shift_args,
            } = *f.inst_data(shift)
            else {
                continue;
            };
            if int_const(f, shift_args[1]) != Some(8) {
                continue;
            }
            let (Some(low), Some(high)) = (
                extended_byte(f, low, ty),
                extended_byte(f, shift_args[0], ty),
            ) else {
                continue;
            };
            let (Some((base, lo)), Some((other, hi))) =
                (address(f, low.addr), address(f, high.addr))
            else {
                continue;
            };
            if base != other || lo.checked_add(1) != Some(hi) {
                continue;
            }
            let chain = [low.load, low.extend, high.load, high.extend, shift];
            let block = f.inst_block(root);
            if chain.iter().any(|&i| {
                f.is_removed(i) || f.inst_block(i) != block
                    || !matches!(f.value_def(f.first_result(i)), ValueDef::Result(owner, 0) if owner == i)
            }) { continue; }
            let insts = f.block_insts(block);
            let Some(start) = insts.iter().position(|&i| i == low.load) else {
                continue;
            };
            let Some(end) = insts.iter().position(|&i| i == root) else {
                continue;
            };
            // The first load remains at its original position. Permit only
            // the matched chain, address additions and constants in the window.
            if end <= start
                || end - start > MAX_WINDOW
                || !insts[start..end].contains(&high.load)
                || insts[start..=end].iter().any(|&i| {
                    f.inst_loc(i).is_some()
                        || (!chain.contains(&i)
                            && i != root
                            && !matches!(
                                f.inst_data(i),
                                D::Iconst { .. }
                                    | D::Binary {
                                        op: BinaryOp::Ptradd,
                                        ..
                                    }
                            ))
                })
                || chain
                    .iter()
                    .any(|&i| !Uses::Users(&uses).single(f, f.first_result(i)))
            {
                continue;
            }
            let D::Load {
                flags: low_flags, ..
            } = *f.inst_data(low.load)
            else {
                unreachable!()
            };
            let D::Load {
                flags: high_flags, ..
            } = *f.inst_data(high.load)
            else {
                unreachable!()
            };
            if !(low_flags.trap_ub && high_flags.trap_ub || low_flags.notrap && high_flags.notrap) {
                continue;
            }
            let mut flags = low_flags;
            flags.notrap &= high_flags.notrap;
            flags.trap_ub &= high_flags.trap_ub;
            let load = f.insert_inst(
                InsertPoint::Before(low.load),
                D::Load {
                    ty: Type::I16,
                    addr: low.addr,
                    flags,
                },
            );
            *f.inst_data_mut(root) = D::Convert {
                op: ConvertOp::Uext,
                ty,
                arg: f.first_result(load),
            };
            for inst in chain {
                f.remove_inst(inst);
            }
            Rule::ByteLoads.hit();
            changed = true;
            break;
        }
    }
    changed
}
