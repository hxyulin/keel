//! Opt-in recognition of the optimized C unsigned byte sign-extension diamond.
//!
//! The positive arm has already been forwarded to the merge by simplify-cfg.
//! This deliberately accepts only the measured i64/byte shape, not arbitrary
//! if-conversion. No instruction is moved or speculated. See the dedicated doc.
use keel_ir::{
    BinaryOp, Block, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, Type, Value, ValueDef,
};

pub struct SignExtendIdiom;
impl crate::Pass for SignExtendIdiom {
    fn name(&self) -> &'static str {
        "sign-extend-idiom"
    }
    fn description(&self) -> &'static str {
        "recognize the bounded unsigned i64 byte sign-extension diamond"
    }
    fn run(&self, func: &mut Function) -> bool {
        run(func)
    }
}

// A resolved definition must actually own this result. Dead aliased instructions
// can remain in layout: never interpret their old instruction as the new value.
fn definition(func: &Function, value: Value) -> Option<Inst> {
    let value = func.resolve(value);
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    (!func.is_removed(inst) && func.first_result(inst) == value).then_some(inst)
}
fn iconst(func: &Function, value: Value, expected: i64) -> bool {
    definition(func, value).is_some_and(|inst| {
        matches!(func.inst_data(inst),
        InstData::Iconst { ty: Type::I64, imm } if *imm == expected)
    })
}
fn uses(func: &Function, value: Value) -> usize {
    let mut count = 0;
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            count += usize::from(func.resolve(v) == value);
        });
    }
    count
}

struct Match {
    branch: Inst,
    arm: Block,
    merge: Block,
    source: Value,
}
fn recognize(func: &Function, header: Block) -> Option<Match> {
    let insts = func.block_insts(header);
    // Exact bounded source shape; this also excludes effects, traps, loads,
    // calls, debug intrinsics and unrelated work in the header.
    if insts.len() != 5 {
        return None;
    }
    let branch = *insts.last()?;
    let InstData::Brif { cond, dests } = func.inst_data(branch) else {
        return None;
    };
    let cmp = definition(func, *cond)?;
    let InstData::Icmp {
        cond: cc,
        args: [byte, bound],
    } = *func.inst_data(cmp)
    else {
        return None;
    };
    // Only exact unsigned predicates and bounds; the opposite polarity is
    // supported by exchanging destinations, never by guessing signedness.
    let positive = match cc {
        IntCC::Ult => 0,
        IntCC::Uge => 1,
        _ => return None,
    };
    let byte = func.resolve(byte);
    if !iconst(func, bound, 128) || func.value_type(byte) != Type::I64 {
        return None;
    }
    let mask = definition(func, byte)?;
    let InstData::Binary {
        op: BinaryOp::And,
        args: mask_args,
    } = *func.inst_data(mask)
    else {
        return None;
    };
    let source = if iconst(func, mask_args[1], 255) {
        mask_args[0]
    } else if iconst(func, mask_args[0], 255) {
        mask_args[1]
    } else {
        return None;
    };
    let source = func.resolve(source);
    if !matches!(func.value_def(source), ValueDef::Param(..)) {
        return None;
    }
    let direct = &dests[positive];
    let indirect = &dests[1 - positive];
    let merge = direct.block;
    let arm = indirect.block;
    if header == merge
        || header == arm
        || arm == merge
        || Some(arm) == func.entry_block()
        || Some(merge) == func.entry_block()
        || !indirect.args.is_empty()
        || !func.block_params(arm).is_empty()
        || direct.args.len() != 1
        || func.resolve(direct.args[0]) != byte
        || func.block_params(merge).len() != 1
        || func.value_type(func.block_params(merge)[0]) != Type::I64
    {
        return None;
    }
    let arm_insts = func.block_insts(arm);
    let [constant, sub, jump] = *arm_insts else {
        return None;
    };
    let InstData::Jump { dest } = func.inst_data(jump) else {
        return None;
    };
    if dest.block != merge || dest.args.len() != 1 {
        return None;
    }
    let result = func.resolve(dest.args[0]);
    if definition(func, result) != Some(sub) {
        return None;
    }
    let InstData::Binary {
        op: BinaryOp::Isub,
        args: [lhs, rhs],
    } = *func.inst_data(sub)
    else {
        return None;
    };
    if func.resolve(lhs) != byte
        || !iconst(func, rhs, 256)
        || definition(func, rhs) != Some(constant)
    {
        return None;
    }
    // Check real edges, including unreachable predecessors and duplicate
    // destinations; no global CFG rewrite or cleanup framework is needed.
    let mut arm_edges = 0;
    let mut merge_edges = 0;
    for (_, inst) in func.layout_insts() {
        for dest in func.inst_data(inst).branch_dests() {
            arm_edges += usize::from(dest.block == arm);
            merge_edges += usize::from(dest.block == merge);
        }
    }
    if arm_edges != 1 || merge_edges != 2 {
        return None;
    }
    // Every header instruction must be one of the four matched definitions.
    let mask_const = if iconst(func, mask_args[1], 255) {
        mask_args[1]
    } else {
        mask_args[0]
    };
    let expected = [
        mask,
        cmp,
        definition(func, bound)?,
        definition(func, mask_const)?,
    ];
    if insts[..4].iter().any(|i| !expected.contains(i))
        || expected.iter().any(|i| func.inst_block(*i) != header
            || func.resolve(func.first_result(*i)) != func.first_result(*i))
        || uses(func, byte) != 3 || uses(func, func.resolve(*cond)) != 1
        || uses(func, result) != 1 || uses(func, func.resolve(rhs)) != 1
        || uses(func, func.resolve(bound)) != 1 || uses(func, func.resolve(mask_const)) != 1
        // Deleting an arm with source locations would erase stepping points.
        || arm_insts.iter().any(|i| func.inst_loc(*i).is_some())
    {
        return None;
    }
    Some(Match {
        branch,
        arm,
        merge,
        source,
    })
}

pub fn run(func: &mut Function) -> bool {
    let mut changed = false;
    for block in func.blocks().collect::<Vec<_>>() {
        if !func.is_block_inserted(block) {
            continue;
        }
        let Some(m) = recognize(func, block) else {
            continue;
        };
        let trunc = func.insert_inst(
            InsertPoint::Before(m.branch),
            InstData::Convert {
                op: ConvertOp::Trunc,
                ty: Type::I8,
                arg: m.source,
            },
        );
        let sext = func.insert_inst(
            InsertPoint::Before(m.branch),
            InstData::Convert {
                op: ConvertOp::Sext,
                ty: Type::I64,
                arg: func.first_result(trunc),
            },
        );
        let loc = func.inst_loc(m.branch);
        func.set_inst_loc(trunc, loc);
        func.set_inst_loc(sext, loc);
        // Keep the merge parameter and its identity; replace only this incoming
        // argument. Both former paths are now this single existing-IR value.
        *func.inst_data_mut(m.branch) = InstData::Jump {
            dest: keel_ir::BlockCall {
                block: m.merge,
                args: vec![func.first_result(sext)],
            },
        };
        func.remove_block(m.arm);
        changed = true;
    }
    changed
}
