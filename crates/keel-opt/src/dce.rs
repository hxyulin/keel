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

use crate::scratch::Scratch;
use keel_ir::{Block, EntityRef, Function, Inst, InstData, Value, ValueDef};

pub fn run(func: &mut Function) -> bool {
    // Every branch into each block: (branch instruction, destination index),
    // packed by destination block. Only terminators branch.
    let mut incoming_start = Scratch::filled(func.num_blocks() + 1, 0u32);
    for block in func.blocks() {
        if let Some(&term) = func.block_insts(block).last() {
            for dest in func.inst_data(term).branch_dests() {
                incoming_start[dest.block.index() + 1] += 1;
            }
        }
    }
    for i in 1..incoming_start.len() {
        incoming_start[i] += incoming_start[i - 1];
    }
    let mut fill: Scratch<u32> = Scratch::new();
    fill.extend_from_slice(&incoming_start);
    let mut incoming_edges = Scratch::filled(
        *incoming_start.last().unwrap() as usize,
        (Inst::new(0), 0usize),
    );
    for block in func.blocks() {
        if let Some(&term) = func.block_insts(block).last() {
            for (i, dest) in func.inst_data(term).branch_dests().iter().enumerate() {
                let slot = &mut fill[dest.block.index()];
                incoming_edges[*slot as usize] = (term, i);
                *slot += 1;
            }
        }
    }
    let incoming = |block: Block| {
        let b = block.index();
        &incoming_edges[incoming_start[b] as usize..incoming_start[b + 1] as usize]
    };

    drop(fill);
    let mut live_value = Scratch::filled(func.num_values(), false);
    let mut live_inst = Scratch::filled(func.num_insts(), false);
    let mut worklist: Scratch<Value> = Scratch::new();

    // Roots: the operands of pinned instructions, minus branch arguments.
    for block in func.blocks() {
        for &inst in func.block_insts(block) {
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
                for &(branch, i) in incoming(block) {
                    let arg = func.inst_data(branch).branch_dests()[i].args[n as usize];
                    enqueue(func, arg, &mut live_value, &mut worklist);
                }
            }
            ValueDef::Alias(_) => unreachable!("resolved"),
        }
    }

    let mut changed = false;
    let mut dead_insts: Scratch<Inst> = Scratch::new();
    for block in func.blocks() {
        for &inst in func.block_insts(block) {
            if !live_inst[inst.index()] {
                dead_insts.push(inst);
            }
        }
    }
    for &inst in dead_insts.iter() {
        func.remove_inst(inst);
        changed = true;
    }

    let mut blocks: Scratch<Block> = Scratch::new();
    blocks.extend(func.blocks().skip(1));
    for &block in blocks.iter() {
        // Back to front, so earlier indices stay valid.
        for n in (0..func.block_params(block).len()).rev() {
            let param = func.block_params(block)[n];
            if live_value[param.index()] {
                continue;
            }
            func.remove_block_param(block, n);
            for &(branch, i) in incoming(block) {
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
