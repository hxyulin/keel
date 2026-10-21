//! Scalar predication of the exact carried running-balance recurrence.
//!
//! The input load stays in its original block. Only total integer operations
//! from four private arms move after that load. Both the balance and sum remain
//! loop-carried; this pass does not introduce a vector reduction.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, IntCC,
    Type, Value, ValueDef,
};
use std::collections::HashSet;

pub fn run(f: &mut Function) -> bool {
    let mut changed = false;
    let mut cfg = None;
    let heads: Vec<_> = f.blocks().collect();
    for head in heads {
        if convert(f, head, &mut cfg) {
            changed = true;
            // Conversion removes private arms. Later candidates must see the
            // new edges, while nonmatching heads can share one lazy snapshot.
            cfg = None;
        }
    }
    changed
}

fn convert(f: &mut Function, head: Block, cfg: &mut Option<ControlFlowGraph>) -> bool {
    if !f.is_block_inserted(head) {
        return false;
    }
    let Some(&term) = f.block_insts(head).last() else {
        return false;
    };
    let D::Brif { cond, dests } = f.inst_data(term).clone() else {
        return false;
    };
    if dests.iter().any(|d| !d.args.is_empty()) {
        return false;
    }
    let (positive, negative) = (dests[0].block, dests[1].block);
    if positive == negative {
        return false;
    }
    let Some(input_cmp) = def(f, cond) else {
        return false;
    };
    let D::Icmp {
        cond: IntCC::Sgt,
        args: [input, zero],
    } = *f.inst_data(input_cmp)
    else {
        return false;
    };
    if f.inst_block(input_cmp) != head || constant(f, zero, Type::I32) != Some(0) {
        return false;
    }
    let Some(input_load) = def(f, input) else {
        return false;
    };
    if f.inst_block(input_load) != head
        || !matches!(f.inst_data(input_load), D::Load { ty: Type::I32, flags, .. } if !flags.volatile)
    {
        return false;
    }
    if f.block_params(positive).len() != 0 || f.block_params(negative).len() != 0 {
        return false;
    }
    // Reject ordinary branches using local instruction checks before paying
    // for whole-function CFG construction.
    let cfg = cfg.get_or_insert_with(|| ControlFlowGraph::compute(f));
    if cfg.preds(positive) != [head] || cfg.preds(negative) != [head] {
        return false;
    }

    let [widen, credit, pos_jump] = f.block_insts(positive) else {
        return false;
    };
    let D::Convert {
        op: ConvertOp::Uext,
        ty: Type::I64,
        arg,
    } = *f.inst_data(*widen)
    else {
        return false;
    };
    if f.resolve(arg) != f.resolve(input) {
        return false;
    }
    let D::Binary {
        op: B::Iadd,
        args: [balance, amount],
    } = *f.inst_data(*credit)
    else {
        return false;
    };
    if f.resolve(amount) != f.first_result(*widen) || f.value_type(f.resolve(balance)) != Type::I64
    {
        return false;
    }
    let D::Jump { dest: pos_edge } = f.inst_data(*pos_jump) else {
        return false;
    };
    if pos_edge.args.len() != 1 || f.resolve(pos_edge.args[0]) != f.first_result(*credit) {
        return false;
    }
    let join = pos_edge.block;
    if f.block_params(join).len() != 1 || f.value_type(f.block_params(join)[0]) != Type::I64 {
        return false;
    }

    let [limit, compare, neg_branch] = f.block_insts(negative) else {
        return false;
    };
    if constant_inst(f, *limit, Type::I64) != Some(17) {
        return false;
    }
    let D::Icmp {
        cond: IntCC::Ugt,
        args: [tested, bound],
    } = *f.inst_data(*compare)
    else {
        return false;
    };
    if f.resolve(tested) != f.resolve(balance) || f.resolve(bound) != f.first_result(*limit) {
        return false;
    }
    let D::Brif {
        cond: debit_cond,
        dests: neg_dests,
    } = f.inst_data(*neg_branch).clone()
    else {
        return false;
    };
    if f.resolve(debit_cond) != f.first_result(*compare)
        || neg_dests.iter().any(|d| !d.args.is_empty())
    {
        return false;
    }
    let (debit, empty) = (neg_dests[0].block, neg_dests[1].block);
    if debit == empty || cfg.preds(debit) != [negative] || cfg.preds(empty) != [negative] {
        return false;
    }
    if !f.block_params(debit).is_empty() || !f.block_params(empty).is_empty() {
        return false;
    }
    let [debit_limit, subtract, debit_jump] = f.block_insts(debit) else {
        return false;
    };
    if constant_inst(f, *debit_limit, Type::I64) != Some(17) {
        return false;
    }
    let D::Binary {
        op: B::Isub,
        args: [old, decrement],
    } = *f.inst_data(*subtract)
    else {
        return false;
    };
    if f.resolve(old) != f.resolve(balance) || f.resolve(decrement) != f.first_result(*debit_limit)
    {
        return false;
    }
    let D::Jump { dest: debit_edge } = f.inst_data(*debit_jump) else {
        return false;
    };
    if debit_edge.block != join
        || debit_edge.args.len() != 1
        || f.resolve(debit_edge.args[0]) != f.first_result(*subtract)
    {
        return false;
    }
    let [empty_value, empty_jump] = f.block_insts(empty) else {
        return false;
    };
    if constant_inst(f, *empty_value, Type::I64) != Some(0) {
        return false;
    }
    let D::Jump { dest: empty_edge } = f.inst_data(*empty_jump) else {
        return false;
    };
    if empty_edge.block != join
        || empty_edge.args.len() != 1
        || f.resolve(empty_edge.args[0]) != f.first_result(*empty_value)
    {
        return false;
    }
    if cfg.preds(join).len() != 3
        || ![positive, debit, empty]
            .iter()
            .all(|b| cfg.preds(join).contains(b))
    {
        return false;
    }

    // Confirm the second carried value is the wrapping sum of the new balance.
    let [sum_add, step, index_add, latch] = f.block_insts(join) else {
        return false;
    };
    let D::Binary {
        op: B::Iadd,
        args: [sum, new_balance],
    } = *f.inst_data(*sum_add)
    else {
        return false;
    };
    if f.resolve(new_balance) != f.block_params(join)[0]
        || constant_inst(f, *step, Type::I64) != Some(1)
    {
        return false;
    }
    let D::Binary {
        op: B::Iadd,
        args: [index, one],
    } = *f.inst_data(*index_add)
    else {
        return false;
    };
    if f.resolve(one) != f.first_result(*step) {
        return false;
    }
    let D::Jump { dest: loop_edge } = f.inst_data(*latch) else {
        return false;
    };
    let header = loop_edge.block;
    if f.block_params(header).len() != 3 || loop_edge.args.len() != 3 {
        return false;
    }
    if f.resolve(loop_edge.args[0]) != f.first_result(*index_add)
        || f.resolve(loop_edge.args[1]) != f.block_params(join)[0]
        || f.resolve(loop_edge.args[2]) != f.first_result(*sum_add)
        || f.resolve(index) != f.block_params(header)[0]
        || f.resolve(balance) != f.block_params(header)[1]
        || f.resolve(sum) != f.block_params(header)[2]
    {
        return false;
    }
    let Some(&header_term) = f.block_insts(header).last() else {
        return false;
    };
    let D::Brif {
        dests: header_dests,
        ..
    } = f.inst_data(header_term)
    else {
        return false;
    };
    if header_dests[0].block != head
        || !header_dests[0].args.is_empty()
        || cfg.preds(head) != [header]
    {
        return false;
    }

    let removed = [positive, negative, debit, empty];
    let defs: HashSet<Value> = removed
        .iter()
        .flat_map(|&b| {
            f.block_insts(b)
                .iter()
                .flat_map(|&i| f.inst_results(i).iter().copied())
        })
        .collect();
    let moved: HashSet<Inst> = [
        *widen,
        *credit,
        *limit,
        *compare,
        *debit_limit,
        *subtract,
        *empty_value,
    ]
    .into_iter()
    .collect();
    if moved
        .iter()
        .any(|&i| f.resolve(f.first_result(i)) != f.first_result(i))
    {
        return false;
    }
    if f.layout_insts().any(|(b, i)| {
        if removed.contains(&b) || i == term {
            return false;
        }
        let mut outside = false;
        f.inst_data(i)
            .for_each_operand(|v| outside |= defs.contains(&f.resolve(v)));
        outside
    }) {
        return false;
    }

    let operations = [
        *widen,
        *credit,
        *limit,
        *compare,
        *debit_limit,
        *subtract,
        *empty_value,
    ];
    let (cond, credit_value, debit_cond, debit_value, zero_value) = (
        f.resolve(cond),
        f.first_result(*credit),
        f.first_result(*compare),
        f.first_result(*subtract),
        f.first_result(*empty_value),
    );
    let debit_loc = f.inst_loc(*neg_branch);
    let credit_loc = f.inst_loc(term);
    for i in operations {
        f.move_inst(i, InsertPoint::Before(term));
    }
    let debit_or_zero =
        f.ins(InsertPoint::Before(term))
            .select(debit_cond, debit_value, zero_value);
    let selected = f
        .ins(InsertPoint::Before(term))
        .select(cond, credit_value, debit_or_zero);
    f.set_inst_loc(def(f, debit_or_zero).unwrap(), debit_loc);
    f.set_inst_loc(def(f, selected).unwrap(), credit_loc);
    *f.inst_data_mut(term) = D::Jump {
        dest: BlockCall {
            block: join,
            args: vec![selected],
        },
    };
    for b in removed {
        f.remove_block(b);
    }
    true
}

fn def(f: &Function, v: Value) -> Option<Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, 0) => Some(i),
        _ => None,
    }
}
fn constant(f: &Function, v: Value, ty: Type) -> Option<i64> {
    let i = def(f, v)?;
    (f.value_type(f.resolve(v)) == ty)
        .then(|| match f.inst_data(i) {
            D::Iconst { ty: t, imm } if *t == ty => Some(*imm),
            _ => None,
        })
        .flatten()
}
fn constant_inst(f: &Function, i: Inst, ty: Type) -> Option<i64> {
    match f.inst_data(i) {
        D::Iconst { ty: t, imm } if *t == ty => Some(*imm),
        _ => None,
    }
}
