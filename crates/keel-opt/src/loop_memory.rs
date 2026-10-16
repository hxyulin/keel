//! Guarded load hoisting for small, unrotated two-block loops.
//! The initial pure integer header is cloned before entry. Its false edge
//! goes directly to the original exit; only its true edge executes the load.
//! Keeping the original header also keeps every subsequent iteration test.

use crate::memory_alias::AliasInfo;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::loops::LoopDepth;
use keel_ir::{
    BinaryOp, Block, BlockCall, Function, InsertPoint, Inst, InstData as D, Type, Value, ValueDef,
};
use std::collections::HashMap;

pub(crate) struct LoopMemory;
impl crate::Pass for LoopMemory {
    fn name(&self) -> &'static str {
        "loop-memory"
    }
    fn description(&self) -> &'static str {
        "hoist invariant ordinary loads through a cloned initial integer loop guard"
    }
    fn run(&self, f: &mut Function) -> bool {
        run(f)
    }
}

struct Candidate {
    header: Block,
    pre_term: Inst,
    incoming: BlockCall,
    exit: BlockCall,
    cond: Value,
    body_arm: usize,
    load: Inst,
    exit_values: Vec<Value>,
}

fn available(f: &Function, dom: &DominatorTree, pre: Block, v: Value) -> bool {
    let block = match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => f.inst_block(i),
        ValueDef::Param(b, _) => b,
        _ => return false,
    };
    dom.dominates(block, pre)
}

// Deliberately a small integer-only whitelist. No pointer comparisons,
// intrinsics, floating point, memory, or potentially trapping division.
fn guard_instruction(f: &Function, i: Inst) -> bool {
    let d = f.inst_data(i);
    if d.is_pinned() || d.may_trap() || f.inst_results(i).len() != 1 {
        return false;
    }
    let mut integer = f.value_type(f.first_result(i)).is_int();
    d.for_each_operand(|v| integer &= f.value_type(f.resolve(v)).is_int());
    integer && matches!(d, D::Iconst { .. } | D::Binary { .. } | D::Icmp { .. })
}

fn analyze(
    f: &Function,
    cfg: &ControlFlowGraph,
    dom: &DominatorTree,
    depth: &LoopDepth,
    alias: &AliasInfo,
    header: Block,
) -> Option<Candidate> {
    if depth.depth(header) != 1 || cfg.preds(header).len() != 2 {
        return None;
    }
    let insts = f.block_insts(header);
    if !(2..=17).contains(&insts.len()) {
        return None;
    }
    let &term = insts.last()?;
    let D::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    // Exactly one straight-line body/latch, with no conditional execution.
    let body_arm = dests.iter().position(|d| {
        let body = d.block;
        body != header
            && cfg.preds(body) == [header]
            && matches!(f.block_insts(body).last().map(|&i| f.inst_data(i)),
                Some(D::Jump { dest }) if dest.block == header)
    })?;
    let body = dests[body_arm].block;
    let exit = dests[1 - body_arm].clone();
    if exit.block == body
        || depth.depth(body) != 1
        || depth.depth(exit.block) != 0
        || !f.block_params(body).is_empty()
        || !dests[body_arm].args.is_empty()
    {
        return None;
    }
    // A bounded return exit also lets us repair direct uses of header
    // definitions (the frontend does not necessarily pass them as exit args).
    if cfg.preds(exit.block) != [header]
        || f.layout_insts()
            .flat_map(|(_, i)| f.inst_data(i).branch_dests())
            .filter(|d| d.block == exit.block)
            .count()
            != 1
        || f.block_insts(exit.block).len() > 32
        || !matches!(
            f.block_insts(exit.block).last().map(|&i| f.inst_data(i)),
            Some(D::Return { .. })
        )
    {
        return None;
    }
    let pre = *cfg.preds(header).iter().find(|&&b| b != body)?;
    if depth.depth(pre) != 0 || !dom.dominates(pre, header) {
        return None;
    }
    let &pre_term = f.block_insts(pre).last()?;
    let D::Jump { dest: incoming } = f.inst_data(pre_term) else {
        return None;
    };
    if incoming.block != header {
        return None;
    }
    // A simple comparison controls entry. Cloning all bounded header
    // instructions also maps values passed to the original false exit.
    let ValueDef::Result(cmp, _) = f.value_def(f.resolve(*cond)) else {
        return None;
    };
    if f.inst_block(cmp) != header || !matches!(f.inst_data(cmp), D::Icmp { .. }) {
        return None;
    }
    let D::Icmp { args: compare, .. } = *f.inst_data(cmp) else {
        unreachable!()
    };
    let D::Jump { dest: back } = f.inst_data(*f.block_insts(body).last()?) else {
        return None;
    };
    let counted = f
        .block_params(header)
        .iter()
        .zip(&back.args)
        .any(|(&iv, &next)| {
            if !matches!(f.value_type(iv), Type::I32 | Type::I64) {
                return false;
            }
            let iv = f.resolve(iv);
            if !compare
                .iter()
                .enumerate()
                .any(|(n, &v)| f.resolve(v) == iv && available(f, dom, pre, compare[1 - n]))
            {
                return false;
            }
            let ValueDef::Result(step, _) = f.value_def(f.resolve(next)) else {
                return false;
            };
            let D::Binary {
                op: BinaryOp::Iadd,
                args,
            } = *f.inst_data(step)
            else {
                return false;
            };
            args.iter().enumerate().any(|(n, &v)| {
                f.resolve(v) == iv
                    && matches!(f.value_def(f.resolve(args[1 - n])),
                ValueDef::Result(one, _) if matches!(f.inst_data(one), D::Iconst { imm: 1, .. }))
            })
        });
    if !counted {
        return None;
    }
    for &i in &insts[..insts.len() - 1] {
        if !guard_instruction(f, i) {
            return None;
        }
    }
    // All cloned operands must be initial header parameters, earlier header
    // results, or values available before the original incoming jump.
    let mut mapped: Vec<Value> = f
        .block_params(header)
        .iter()
        .map(|&v| f.resolve(v))
        .collect();
    for &i in &insts[..insts.len() - 1] {
        let mut valid = true;
        f.inst_data(i).for_each_operand(|v| {
            let v = f.resolve(v);
            valid &= mapped.contains(&v) || available(f, dom, pre, v);
        });
        if !valid {
            return None;
        }
        mapped.push(f.resolve(f.first_result(i)));
    }
    if !exit
        .args
        .iter()
        .all(|&v| mapped.contains(&f.resolve(v)) || available(f, dom, pre, v))
    {
        return None;
    }
    let mut exit_values = Vec::new();
    for (b, i) in f.layout_insts() {
        let mut valid = true;
        f.inst_data(i).for_each_operand(|v| {
            let v = f.resolve(v);
            if !mapped.contains(&v) || b == header || b == body {
                return;
            }
            if b != exit.block {
                valid = false;
                return;
            }
            if !exit_values.contains(&v) {
                exit_values.push(v);
            }
        });
        if !valid {
            return None;
        }
    }
    // One load bounds code growth and makes reapplication a no-op. Do not
    // cross an earlier load/store/trap, even if the candidate is notrap.
    let mut load = None;
    let body_insts = f.block_insts(body);
    if body_insts.len() > 128 {
        return None;
    }
    for &i in &body_insts[..body_insts.len() - 1] {
        match f.inst_data(i) {
            D::Load { flags, addr, ty } if !flags.volatile => {
                if load.is_some() || !available(f, dom, pre, *addr) || ty.bytes()? == 0 {
                    return None;
                }
                load = Some(i);
            }
            D::Store { flags, .. } if !flags.volatile && load.is_some() => {}
            d if !d.is_pinned() && !d.may_trap() => {}
            _ => return None,
        }
    }
    let load = load?;
    let D::Load { addr, ty, .. } = *f.inst_data(load) else {
        unreachable!()
    };
    let bytes = u64::from(ty.bytes()?);
    for &i in body_insts {
        if let D::Store {
            addr: other, value, ..
        } = *f.inst_data(i)
        {
            let width = u64::from(f.value_type(f.resolve(value)).bytes()?);
            if width == 0 || alias.may_alias(f, addr, bytes, other, width) {
                return None;
            }
        }
    }
    Some(Candidate {
        header,
        pre_term,
        incoming: incoming.clone(),
        exit,
        cond: *cond,
        body_arm,
        load,
        exit_values,
    })
}

fn mapped(f: &Function, values: &HashMap<Value, Value>, v: Value) -> Value {
    let v = f.resolve(v);
    values.get(&v).copied().unwrap_or(v)
}

fn transform(f: &mut Function, mut c: Candidate) {
    let mut values: HashMap<_, _> = f
        .block_params(c.header)
        .iter()
        .zip(&c.incoming.args)
        .map(|(&v, &a)| (f.resolve(v), f.resolve(a)))
        .collect();
    let header = f.block_insts(c.header).to_vec();
    for &i in &header[..header.len() - 1] {
        let mut data = f.inst_data(i).clone();
        data.for_each_operand_mut(|v| *v = mapped(f, &values, *v));
        let copy = f.insert_inst(InsertPoint::Before(c.pre_term), data);
        f.set_inst_loc(copy, f.inst_loc(i));
        values.insert(f.resolve(f.first_result(i)), f.first_result(copy));
    }
    let mut exit_replacements = HashMap::new();
    for &v in &c.exit_values {
        let param = f.append_block_param(c.exit.block, f.value_type(v));
        exit_replacements.insert(v, param);
        c.exit.args.push(v);
    }
    for i in f.block_insts(c.exit.block).to_vec() {
        let mut data = f.inst_data(i).clone();
        data.for_each_operand_mut(|v| *v = mapped(f, &exit_replacements, *v));
        *f.inst_data_mut(i) = data;
    }
    if let D::Brif { dests, .. } = f.inst_data_mut(*header.last().unwrap()) {
        dests[1 - c.body_arm].args.extend(&c.exit_values);
    }
    let entered = f.create_block();
    // Move, rather than copy: result identities, flags, and source locations
    // survive. The direct false exit is essential to result dominance.
    f.move_inst(c.load, InsertPoint::End(entered));
    let jump = f.append_inst(entered, D::Jump { dest: c.incoming });
    f.set_inst_loc(jump, f.inst_loc(c.pre_term));
    let mut exit = c.exit;
    for v in &mut exit.args {
        *v = mapped(f, &values, *v);
    }
    let yes = BlockCall {
        block: entered,
        args: Vec::new(),
    };
    let dests = if c.body_arm == 0 {
        [yes, exit]
    } else {
        [exit, yes]
    };
    *f.inst_data_mut(c.pre_term) = D::Brif {
        cond: mapped(f, &values, c.cond),
        dests,
    };
}

fn run(f: &mut Function) -> bool {
    // Bounded whole-function analysis, and at most eight small loop changes.
    // Stack-variable debug descriptions are conservatively outside this
    // initial subset. Ordinary instruction locations/scopes are copied intact.
    if !f.debug_vars.is_empty() || f.num_blocks() > 512 || f.layout_insts().count() > 8192 {
        return false;
    }
    let mut changed = false;
    for _ in 0..8 {
        let cfg = ControlFlowGraph::compute(f);
        let dom = DominatorTree::compute(f, &cfg);
        let depth = LoopDepth::compute(f, &cfg, &dom);
        let alias = AliasInfo::compute(f);
        let candidate = dom
            .rpo()
            .iter()
            .find_map(|&h| analyze(f, &cfg, &dom, &depth, &alias, h));
        let Some(candidate) = candidate else {
            break;
        };
        transform(f, candidate);
        changed = true;
    }
    changed
}
