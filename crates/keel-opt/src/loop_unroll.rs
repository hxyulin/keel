//! Bounded two-way unrolling of wrapping integer additive reductions.
//! The original loop is retained as the scalar tail. No pipeline enables this pass.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::loops::LoopDepth;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, IntCC,
    Type, Value, ValueDef,
};
use std::collections::{HashMap, HashSet};

pub(crate) struct LoopUnroll;
impl crate::Pass for LoopUnroll {
    fn name(&self) -> &'static str {
        "loop-unroll"
    }
    fn description(&self) -> &'static str {
        "unroll bounded wrapping integer additive reductions with two accumulators"
    }
    fn run(&self, f: &mut Function) -> bool {
        run(f)
    }
}

struct Candidate {
    header: Block,
    body: Block,
    pre_jump: Inst,
    iv: Value,
    sum: Value,
    start: Value,
    seed: Value,
    bound: Value,
    next_iv: Value,
    next_sum: Value,
}
fn def(f: &Function, v: Value) -> Option<Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => Some(i),
        _ => None,
    }
}
fn constant(f: &Function, v: Value) -> Option<i64> {
    match f.inst_data(def(f, v)?) {
        D::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}
fn invariant(f: &Function, dom: &DominatorTree, pre: Block, v: Value) -> bool {
    let b = match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => f.inst_block(i),
        ValueDef::Param(b, _) => b,
        _ => return false,
    };
    dom.dominates(b, pre)
}
fn analyze(f: &Function, header: Block) -> Option<Candidate> {
    let [iv, sum] = *f.block_params(header) else {
        return None;
    };
    if f.resolve(iv) != iv
        || f.resolve(sum) != sum
        || !matches!(f.value_type(iv), Type::I32 | Type::I64)
        || !matches!(f.value_type(sum), Type::I32 | Type::I64)
    {
        return None;
    }
    let [compare, branch] = *f.block_insts(header) else {
        return None;
    };
    let D::Icmp { cond, args } = *f.inst_data(compare) else {
        return None;
    };
    let bound = match (cond, f.resolve(args[0]) == iv, f.resolve(args[1]) == iv) {
        (IntCC::Ult, true, false) => f.resolve(args[1]),
        (IntCC::Ugt, false, true) => f.resolve(args[0]),
        _ => return None,
    };
    let D::Brif { cond, dests } = f.inst_data(branch) else {
        return None;
    };
    if f.resolve(*cond) != f.first_result(compare) {
        return None;
    }
    let body = dests[0].block;
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
    let depth = LoopDepth::compute(f, &cfg, &dom);
    if body == header
        || dests[1].block == header
        || dests[1].block == body
        || cfg.preds(body) != [header]
        || !f.block_params(body).is_empty()
        || !dests[0].args.is_empty()
        || depth.depth(header) != 1
        || depth.depth(body) != 1
        || !dom.dominates(header, body)
    {
        return None;
    }
    let preds = cfg.preds(header);
    if preds.len() != 2 {
        return None;
    }
    let pre = *preds.iter().find(|&&b| b != body)?;
    if pre == header || depth.depth(pre) != 0 || !invariant(f, &dom, pre, bound) {
        return None;
    }
    let pre_jump = *f.block_insts(pre).last()?;
    let D::Jump { dest } = f.inst_data(pre_jump) else {
        return None;
    };
    if dest.block != header
        || dest.args.len() != 2
        || constant(f, dest.args[0]) != Some(0)
        || !invariant(f, &dom, pre, dest.args[1])
    {
        return None;
    }
    let (start, seed) = (f.resolve(dest.args[0]), f.resolve(dest.args[1]));
    let insts = f.block_insts(body);
    // At most 20 instructions including the backedge; paired cloning adds <=54
    // instructions. At most 2 loads, 16 simultaneously live scalar values.
    if insts.len() > 20 {
        return None;
    }
    let back = *insts.last()?;
    let D::Jump { dest } = f.inst_data(back) else {
        return None;
    };
    if dest.block != header || dest.args.len() != 2 {
        return None;
    }
    let (next_iv, next_sum) = (f.resolve(dest.args[0]), f.resolve(dest.args[1]));
    let step = def(f, next_iv)?;
    let D::Binary { op: B::Iadd, args } = *f.inst_data(step) else {
        return None;
    };
    if f.inst_block(step) != body
        || !((f.resolve(args[0]) == iv && constant(f, args[1]) == Some(1))
            || (f.resolve(args[1]) == iv && constant(f, args[0]) == Some(1)))
    {
        return None;
    }
    let reduction = def(f, next_sum)?;
    let D::Binary { op: B::Iadd, args } = *f.inst_data(reduction) else {
        return None;
    };
    if f.inst_block(reduction) != body || (f.resolve(args[0]) == sum) == (f.resolve(args[1]) == sum)
    {
        return None;
    }
    let mut loads = 0;
    let mut local = HashSet::new();
    for &i in &insts[..insts.len() - 1] {
        match f.inst_data(i) {
            D::Iconst { ty, .. } if ty.is_int() => {}
            D::Binary {
                op:
                    B::Iadd
                    | B::Isub
                    | B::Imul
                    | B::And
                    | B::Or
                    | B::Xor
                    | B::Shl
                    | B::Ushr
                    | B::Sshr
                    | B::Ptradd,
                ..
            } => {}
            D::Convert {
                op: ConvertOp::Uext | ConvertOp::Sext | ConvertOp::Trunc,
                ty,
                ..
            } if ty.is_int() => {}
            D::Load { flags, ty, .. } if !flags.volatile && matches!(ty, Type::I32 | Type::I64) => {
                loads += 1;
            }
            _ => return None,
        }
        if f.inst_results(i).len() != 1
            || !matches!(
                f.value_type(f.first_result(i)),
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
            )
        {
            return None;
        }
        let mut ok = true;
        f.inst_data(i).for_each_operand(|v| {
            let v = f.resolve(v);
            // The carried sum may occur only at the final additive update;
            // its updated result cannot feed another operation in this body.
            if v == next_sum
                || v == sum && i != reduction
                || !(v == iv || v == sum || local.contains(&v) || invariant(f, &dom, pre, v))
            {
                ok = false;
            }
        });
        if !ok {
            return None;
        }
        local.insert(f.resolve(f.first_result(i)));
    }
    if !(1..=2).contains(&loads) {
        return None;
    }
    // No body-defined value can bypass the tail's definitions on an exit.
    for (b, i) in f.layout_insts() {
        if b != body {
            let mut escapes = false;
            f.inst_data(i)
                .for_each_operand(|v| escapes |= local.contains(&f.resolve(v)));
            if escapes {
                return None;
            }
        }
    }
    let mut live = HashSet::from([next_iv, next_sum]);
    for &i in insts[..insts.len() - 1].iter().rev() {
        for v in f.inst_results(i) {
            live.remove(&f.resolve(*v));
        }
        f.inst_data(i).for_each_operand(|v| {
            live.insert(f.resolve(v));
        });
        if live.len() > 16 {
            return None;
        }
    }
    Some(Candidate {
        header,
        body,
        pre_jump,
        iv,
        sum,
        start,
        seed,
        bound,
        next_iv,
        next_sum,
    })
}
fn emit(f: &mut Function, b: Block, d: D) -> Value {
    let i = f.append_inst(b, d);
    f.first_result(i)
}
fn bin(f: &mut Function, b: Block, op: B, a: Value, c: Value) -> Value {
    emit(f, b, D::Binary { op, args: [a, c] })
}
fn jump(f: &mut Function, b: Block, to: Block, args: Vec<Value>) {
    f.append_inst(
        b,
        D::Jump {
            dest: BlockCall { block: to, args },
        },
    );
}
fn transform(f: &mut Function, c: Candidate) {
    let iv_ty = f.value_type(c.iv);
    let sum_ty = f.value_type(c.sum);
    let pair_header = f.create_block();
    let pair_body = f.create_block();
    let combine = f.create_block();
    let iv = f.append_block_param(pair_header, iv_ty);
    let lane0 = f.append_block_param(pair_header, sum_ty);
    let lane1 = f.append_block_param(pair_header, sum_ty);
    // iv<=bound is inductive: entry iv=0; a paired step occurs only when
    // bound-iv>=2. Hence subtraction and the two unit increments cannot wrap.
    let remaining = bin(f, pair_header, B::Isub, c.bound, iv);
    let two = emit(f, pair_header, D::Iconst { ty: iv_ty, imm: 2 });
    let enough = emit(
        f,
        pair_header,
        D::Icmp {
            cond: IntCC::Uge,
            args: [remaining, two],
        },
    );
    f.append_inst(
        pair_header,
        D::Brif {
            cond: enough,
            dests: [
                BlockCall {
                    block: pair_body,
                    args: vec![],
                },
                BlockCall {
                    block: combine,
                    args: vec![],
                },
            ],
        },
    );
    let originals = f.block_insts(c.body).to_vec();
    let mut next_iv = iv;
    let mut sums = [lane0, lane1];
    for lane in &mut sums {
        let mut map = HashMap::from([(c.iv, next_iv), (c.sum, *lane)]);
        for &i in &originals[..originals.len() - 1] {
            let mut data = f.inst_data(i).clone();
            data.for_each_operand_mut(|v| {
                let resolved = f.resolve(*v);
                *v = map.get(&resolved).copied().unwrap_or(resolved);
            });
            let v = emit(f, pair_body, data);
            map.insert(f.resolve(f.first_result(i)), v);
        }
        next_iv = map[&c.next_iv];
        *lane = map[&c.next_sum];
    }
    jump(f, pair_body, pair_header, vec![next_iv, sums[0], sums[1]]);
    let sum = bin(f, combine, B::Iadd, lane0, lane1);
    jump(f, combine, c.header, vec![iv, sum]);
    // Insert zero before the preheader terminator, preserving all existing work.
    let zero_inst = f.insert_inst(
        InsertPoint::Before(c.pre_jump),
        D::Iconst { ty: sum_ty, imm: 0 },
    );
    let zero = f.first_result(zero_inst);
    *f.inst_data_mut(c.pre_jump) = D::Jump {
        dest: BlockCall {
            block: pair_header,
            args: vec![c.start, c.seed, zero],
        },
    };
}
fn run(f: &mut Function) -> bool {
    // Variable location ranges cannot describe the extra accumulator. Skip debug
    // functions entirely rather than publishing misleading locations/ranges.
    if f.debug_return.is_some()
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || !f.debug_scopes.is_empty()
        || f.layout_insts().any(|(_, i)| f.inst_loc(i).is_some())
    {
        return false;
    }
    let headers: Vec<_> = f.blocks().collect();
    let mut changed = false;
    for header in headers {
        if let Some(c) = analyze(f, header) {
            transform(f, c);
            changed = true;
        }
    }
    changed
}
