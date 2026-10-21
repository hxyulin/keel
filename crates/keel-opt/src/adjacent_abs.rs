//! Bounded scalar absolute-value triangle conversion without moving memory.
//!
//! Only `x < 0 ? -x : x` with a private, single-ineg arm is accepted. Integer
//! negation is nontrapping and wraps at its original width, including MIN.
//! In particular, a difference of sign-extended i32 inputs stays i64: its
//! magnitude may be 4294967295. No load, address, or loop instruction moves.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{
    BlockCall, EntityRef, Function, InsertPoint, InstData as D, IntCC, Type, UnaryOp, Value,
    ValueDef,
};

pub fn run(f: &mut Function) -> bool {
    let mut changed = false;
    let heads: Vec<_> = f.blocks().collect();
    // Only this pass removes blocks: the arms it converts.
    let mut removed = vec![false; f.num_blocks()];
    for head in heads {
        if removed[head.index()] {
            continue;
        }
        let Some(&term) = f.block_insts(head).last() else {
            continue;
        };
        let D::Brif { cond, .. } = *f.inst_data(term) else {
            continue;
        };
        let Some(cmp) = defining_inst(f, cond) else {
            continue;
        };
        let D::Icmp {
            cond: IntCC::Slt,
            args: [x, zero],
        } = *f.inst_data(cmp)
        else {
            continue;
        };
        let x = f.resolve(x);
        if f.inst_block(cmp) != head
            || !matches!(
                f.value_type(x),
                Type::I8 | Type::I16 | Type::I32 | Type::I64
            )
            || !defining_inst(f, zero)
                .is_some_and(|i| matches!(f.inst_data(i), D::Iconst { imm: 0, .. }))
        {
            continue;
        }
        let D::Brif { dests, .. } = f.inst_data(term).clone() else {
            unreachable!("checked above");
        };
        let arm = dests[0].block;
        let join = dests[1].block;
        if arm == head
            || join == head
            || arm == join
            || Some(arm) == f.entry_block()
            || !dests[0].args.is_empty()
            || !f.block_params(arm).is_empty()
            || ControlFlowGraph::compute(f).preds(arm) != [head]
        {
            continue;
        }
        let [neg, jump] = *f.block_insts(arm) else {
            continue;
        };
        let D::Unary {
            op: UnaryOp::Ineg,
            arg,
        } = *f.inst_data(neg)
        else {
            continue;
        };
        let D::Jump { dest } = f.inst_data(jump) else {
            continue;
        };
        if f.resolve(arg) != x
            || dest.block != join
            || dest.args.len() != dests[1].args.len()
            || dest.args.len() > 8
        {
            continue;
        }
        let neg_value = f.inst_results(neg)[0];
        if f.resolve(neg_value) != neg_value {
            continue;
        }
        let mut args: Vec<_> = dests[1].args.iter().map(|&v| f.resolve(v)).collect();
        let differing: Vec<_> = dest
            .args
            .iter()
            .zip(&args)
            .enumerate()
            .filter(|(_, (a, b))| f.resolve(**a) != **b)
            .collect();
        let [(index, (yes, no))] = differing.as_slice() else {
            continue;
        };
        if f.resolve(**yes) != neg_value || **no != x {
            continue;
        }
        let index = *index;

        // Reuse the original ineg instruction, preserving its source location.
        // Moving only this pure operation cannot change read/fault/write order.
        f.move_inst(neg, InsertPoint::Before(term));
        let select = f.insert_inst(
            InsertPoint::Before(term),
            D::Select {
                args: [f.resolve(cond), neg_value, x],
            },
        );
        f.set_inst_loc(select, f.inst_loc(term));
        args[index] = f.inst_results(select)[0];
        *f.inst_data_mut(term) = D::Jump {
            dest: BlockCall { block: join, args },
        };
        f.remove_block(arm);
        removed[arm.index()] = true;
        changed = true;
    }
    changed
}

fn defining_inst(f: &Function, v: Value) -> Option<keel_ir::Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, 0) => Some(i),
        _ => None,
    }
}
