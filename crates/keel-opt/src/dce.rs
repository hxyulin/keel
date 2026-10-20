//! Dead code elimination: mark what is needed, remove the rest.
//!
//! Pinned instructions (side effects, traps, terminators) are always
//! needed, except ordinary loads explicitly promised not to trap: their
//! reads have no observable effect when the result is dead. They remain
//! pinned for scheduling when live. A value is needed if a needed instruction
//! uses it, which makes its defining instruction needed in turn. Branch arguments are the
//! exception: passing a value to a block parameter only needs the value if
//! the parameter itself is needed. That way a loop variable that is only
//! ever passed around the loop (`jump block1(v7)` where `v7 = iadd v3, ..`
//! and nothing else reads `v3`) dies, parameter and all, which plain
//! "remove instructions with no uses" cannot see.

use keel_ir::{Block, EntityRef, Function, Inst, InstData, Value, ValueDef};

pub fn run(func: &mut Function) -> bool {
    // Every branch into each block: (branch instruction, destination index).
    let mut incoming: Vec<Vec<(Inst, usize)>> = vec![Vec::new(); func.num_blocks()];
    for (_, inst) in func.layout_insts() {
        for (i, dest) in func.inst_data(inst).branch_dests().iter().enumerate() {
            incoming[dest.block.index()].push((inst, i));
        }
    }

    let mut live_value = vec![false; func.num_values()];
    let mut live_inst = vec![false; func.num_insts()];
    let mut worklist: Vec<Value> = Vec::new();

    // Roots: the operands of pinned instructions, minus branch arguments.
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        let dead_read_eligible = matches!(data, InstData::Load { flags, .. }
            if flags.notrap && !flags.volatile);
        if data.is_pinned() && !dead_read_eligible {
            live_inst[inst.index()] = true;
            uses_except_branch_args(func, inst, |value| {
                enqueue(func, value, &mut live_value, &mut worklist);
            });
        }
    }
    // Entry parameters are the function's arguments, so they stay.
    if let Some(entry) = func.entry_block() {
        for &value in func.block_params(entry) {
            enqueue(func, value, &mut live_value, &mut worklist);
        }
    }

    while let Some(value) = worklist.pop() {
        match func.value_def(value) {
            ValueDef::Result(inst, _) => {
                if !std::mem::replace(&mut live_inst[inst.index()], true) {
                    uses_except_branch_args(func, inst, |value| {
                        enqueue(func, value, &mut live_value, &mut worklist);
                    });
                }
            }
            // A needed parameter needs the argument every branch passes it.
            ValueDef::Param(block, n) => {
                for &(branch, i) in &incoming[block.index()] {
                    let arg = func.inst_data(branch).branch_dests()[i].args[n as usize];
                    enqueue(func, arg, &mut live_value, &mut worklist);
                }
            }
            ValueDef::Alias(_) => unreachable!("resolved"),
        }
    }

    let mut changed = false;
    let dead_insts: Vec<Inst> = func
        .layout_insts()
        .map(|(_, inst)| inst)
        .filter(|inst| !live_inst[inst.index()])
        .collect();
    for inst in dead_insts {
        func.remove_inst(inst);
        changed = true;
    }

    let blocks: Vec<Block> = func.blocks().skip(1).collect();
    for block in blocks {
        // Back to front, so earlier indices stay valid.
        for n in (0..func.block_params(block).len()).rev() {
            let param = func.block_params(block)[n];
            if live_value[param.index()] {
                continue;
            }
            func.remove_block_param(block, n);
            for &(branch, i) in &incoming[block.index()] {
                let dests = func.inst_data_mut(branch).branch_dests_mut();
                dests[i].args.remove(n);
            }
            changed = true;
        }
    }
    changed
}

/// A resolved value enters the worklist once, even when many live roots or
/// incoming edges need it. Liveness is monotone throughout the analysis.
fn enqueue(func: &Function, value: Value, live: &mut [bool], work: &mut Vec<Value>) {
    let value = func.resolve(value);
    if !std::mem::replace(&mut live[value.index()], true) {
        work.push(value);
    }
}

/// Visits operands that are not block-call arguments.
fn uses_except_branch_args(func: &Function, inst: Inst, mut visit: impl FnMut(Value)) {
    match func.inst_data(inst) {
        InstData::Jump { .. } => {}
        InstData::Brif { cond, .. } => visit(*cond),
        InstData::Switch { arg, .. } => visit(*arg),
        data => data.for_each_operand(visit),
    }
}
