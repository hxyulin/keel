//! Give the single-element path of a counted exit its own short continuation.
use crate::ir_match::{def, int_const};
use crate::{OptimizationContext, Pass, VectorTarget};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    BinaryOp, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, Type,
    Value,
};

pub(crate) struct SpecializeSingleCountExit;
const CLEANUP: &[&dyn Pass] = &[crate::DCE];

impl Pass for SpecializeSingleCountExit {
    fn name(&self) -> &'static str {
        "specialize-single-count-exit"
    }

    fn description(&self) -> &'static str {
        "give a guarded single-element exit its own short continuation"
    }

    fn cleanup_passes(&self) -> &'static [&'static dyn Pass] {
        CLEANUP
    }

    fn run(&self, f: &mut Function, context: &OptimizationContext) -> bool {
        if context.target != VectorTarget::Aarch64Neon
            || f.num_blocks() < 128
            || f.debug_loc.is_some()
            || f.debug_return.is_some()
            || !f.debug_vars.is_empty()
        {
            return false;
        }
        let site = f.blocks().find_map(|block| analyze(f, block));
        if let Some(site) = site {
            specialize(f, site);
            return true;
        }
        false
    }
}

struct Exit {
    gate: Inst,
    count: Value,
    copy: Block,
    shared: BlockCall,
    resume: Block,
}

fn small_block(f: &Function, block: Block, limit: usize, stores: bool) -> bool {
    let insts = f.block_insts(block);
    insts.len() <= limit
        && insts.iter().all(|&inst| {
            f.inst_loc(inst).is_none()
                && match f.inst_data(inst) {
                    InstData::Iconst { .. }
                    | InstData::Icmp { .. }
                    | InstData::Convert {
                        op: ConvertOp::Sext | ConvertOp::Uext,
                        ..
                    }
                    | InstData::Binary {
                        op: BinaryOp::Ptradd | BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::And,
                        ..
                    }
                    | InstData::Brif { .. }
                    | InstData::Jump { .. } => true,
                    InstData::Load { flags, .. } => !flags.volatile,
                    InstData::Store { flags, .. } => stores && !flags.volatile,
                    _ => false,
                }
        })
}

fn analyze(f: &Function, copy: Block) -> Option<Exit> {
    if !f.block_params(copy).is_empty() {
        return None;
    }
    let &term = f.block_insts(copy).last()?;
    let InstData::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    let InstData::Icmp {
        cond: IntCC::Sgt,
        args: [wide, one],
    } = *f.inst_data(def(f, *cond)?)
    else {
        return None;
    };
    if f.value_type(wide) != Type::I32 || int_const(f, one) != Some(1) {
        return None;
    }
    let InstData::Convert {
        op: ConvertOp::Sext,
        ty: Type::I32,
        arg: count,
    } = *f.inst_data(def(f, wide)?)
    else {
        return None;
    };
    let count = f.resolve(count);
    if f.value_type(count) != Type::I16 || !small_block(f, copy, 28, true) {
        return None;
    }
    let [fill, shared] = dests.as_ref();
    if fill.args.as_slice() != [wide] || fill.block == copy || shared.block == copy {
        return None;
    }
    let fill_term = *f.block_insts(fill.block).last()?;
    let InstData::Brif {
        dests: fill_dests, ..
    } = f.inst_data(fill_term)
    else {
        return None;
    };
    if fill_dests[0].block != fill.block || fill_dests[1].block != shared.block {
        return None;
    }

    let &[shared_param] = f.block_params(shared.block) else {
        return None;
    };
    if f.value_type(shared_param) != Type::I32
        || f.resolve(shared_param) != shared_param
        || !small_block(f, shared.block, 8, false)
    {
        return None;
    }
    let &shared_term = f.block_insts(shared.block).last()?;
    let InstData::Brif {
        dests: shared_dests,
        ..
    } = f.inst_data(shared_term)
    else {
        return None;
    };
    let [ret, resume] = shared_dests.each_ref().map(|d| d.block);
    let &[return_inst] = f.block_insts(ret) else {
        return None;
    };
    if !matches!(f.inst_data(return_inst), InstData::Return { args } if args.is_empty())
        || shared_dests.iter().any(|d| !d.args.is_empty())
        || !f.block_params(resume).is_empty()
        || !small_block(f, resume, 4, false)
        || !matches!(
            f.inst_data(*f.block_insts(resume).last()?),
            InstData::Jump { .. }
        )
    {
        return None;
    }
    let cfg = &crate::analysis::get(f).cfg;
    let &[pred] = cfg.preds(copy) else {
        return None;
    };
    if cfg.preds(shared.block).len() < 3 || cfg.preds(resume) != [shared.block] {
        return None;
    }
    let &gate = f.block_insts(pred).last()?;
    if !matches!(f.inst_data(gate), InstData::Brif { cond, dests }
        if f.resolve(*cond) == count && dests[0].block == copy && dests[1].block != copy)
    {
        return None;
    }

    // The fast edge bypasses the original tail. Its values may reach other
    // blocks through branch arguments, but no definition can escape directly.
    let region = [copy, shared.block, resume];
    for (block, inst) in f.layout_insts() {
        if region.contains(&block) {
            continue;
        }
        let mut escapes = false;
        f.inst_data(inst).for_each_operand(|value| {
            let value = f.resolve(value);
            escapes |= value == shared_param
                || def(f, value).is_some_and(|d| region.contains(&f.inst_block(d)));
        });
        if escapes {
            return None;
        }
    }
    Some(Exit {
        gate,
        count,
        copy,
        shared: shared.clone(),
        resume,
    })
}

fn clone_body(
    f: &mut Function,
    old: Block,
    dest: Block,
    values: &mut FxHashMap<Value, Value>,
) -> InstData {
    let insts = f.block_insts(old).to_vec();
    let (&last, body) = insts.split_last().expect("matched block has a terminator");
    for &inst in body {
        let mut data = f.inst_data(inst).clone();
        data.for_each_operand_mut(|v| remap(f, v, values));
        let new = f.append_inst(dest, data);
        if let Some(&v) = f.inst_results(inst).first() {
            values.insert(v, f.first_result(new));
        }
    }
    let mut term = f.inst_data(last).clone();
    term.for_each_operand_mut(|v| remap(f, v, values));
    term
}

fn remap(f: &Function, value: &mut Value, values: &FxHashMap<Value, Value>) {
    let old = f.resolve(*value);
    *value = values.get(&old).copied().unwrap_or(old);
}

fn specialize(f: &mut Function, exit: Exit) {
    let fallback = f.create_block();
    let single = f.create_block();
    let resume = f.create_block();
    let old_gate = f.inst_data(exit.gate).clone();
    f.append_inst(fallback, old_gate);

    let at = InsertPoint::Before(exit.gate);
    let one = f.insert_inst(at, InstData::iconst(Type::I16, 1));
    let test = f.insert_inst(
        at,
        InstData::Icmp {
            cond: IntCC::Ne,
            args: [exit.count, f.first_result(one)],
        },
    );
    *f.inst_data_mut(exit.gate) = InstData::Brif {
        cond: f.first_result(test),
        dests: Box::new([
            BlockCall {
                block: fallback,
                args: vec![],
            },
            BlockCall {
                block: single,
                args: vec![],
            },
        ]),
    };

    // Both loads and stores stay in order. The same path would do no
    // padding when the count is one, so omit the test and its fill edge.
    let mut values = FxHashMap::default();
    clone_body(f, exit.copy, single, &mut values);
    let shared_param = f.block_params(exit.shared.block)[0];
    let mut arg = exit.shared.args[0];
    remap(f, &mut arg, &values);
    values.insert(shared_param, arg);
    let mut term = clone_body(f, exit.shared.block, single, &mut values);
    term.branch_dests_mut()[1].block = resume;
    f.append_inst(single, term);
    let next = clone_body(f, exit.resume, resume, &mut values);
    f.append_inst(resume, next);
}
