//! Bounded two-way unrolling of wrapping additive and affine reductions.
//! The original loop is retained as the scalar tail. Full runs this pass in its
//! epilogue, after loop-reduce-multi and before loop-rotate.
//! Accept zero-based signed/unsigned counted innermost loops at any nesting depth.
//! Run strength reduction (and alias cleanup) before unrolling, then rotation:
//! up to two extra same-width additive recurrences are forwarded sequentially.
//! Whole iterations are cloned in order, including both loads, before beginning
//! the next iteration; only the non-observable wrapping sum is reassociated.
use crate::counted_loop::{CountedLoop, TwoBlockLoop, invariant};
use crate::ir_match::{def, int_const};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, IntCC,
    Type, Value,
};

pub(crate) struct LoopUnroll;
impl crate::Pass for LoopUnroll {
    fn name(&self) -> &'static str {
        "loop-unroll"
    }
    fn description(&self) -> &'static str {
        "unroll bounded wrapping integer additive and constant-affine reductions"
    }
    fn family(&self) -> crate::PassFamily {
        crate::PassFamily::Unroll
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        run(f)
    }
    fn deterministic(&self) -> bool {
        true
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
    signed: bool,
    extras: Vec<(Value, Value, Value)>,
    affine: Option<(Inst, i64)>,
}
fn analyze(f: &Function, header: Block) -> Option<Candidate> {
    let params = f.block_params(header);
    if !(2..=4).contains(&params.len()) || f.block_insts(header).len() != 2 {
        return None;
    }
    let (iv, sum) = (params[0], params[1]);
    if f.resolve(sum) != sum || !matches!(f.value_type(sum), Type::I32 | Type::I64) {
        return None;
    }
    let mut analysis = None;
    let CountedLoop {
        shape:
            TwoBlockLoop {
                body,
                pre,
                body_edge: 0,
                back,
                entry: pre_jump,
                ..
            },
        bound,
        signed,
        ..
    } = CountedLoop::find(f, &mut analysis, header)?
    else {
        return None;
    };
    let analysis = analysis.expect("CountedLoop::find computed the analyses");
    let dom = &analysis.dom;
    let bound = f.resolve(bound);
    if !invariant(f, dom, pre, bound) {
        return None;
    }
    let D::Jump { dest } = f.inst_data(pre_jump) else {
        unreachable!()
    };
    if int_const(f, dest.args[0]) != Some(0) || !invariant(f, dom, pre, dest.args[1]) {
        return None;
    }
    let (start, seed) = (f.resolve(dest.args[0]), f.resolve(dest.args[1]));
    let initial = dest.args.clone();
    let insts = f.block_insts(body);
    // At most 20 instructions including the backedge; paired cloning adds <=54
    // instructions. At most 2 loads, 16 simultaneously live scalar values.
    if insts.len() > 20 {
        return None;
    }
    let D::Jump { dest } = f.inst_data(back) else {
        unreachable!()
    };
    let (next_iv, next_sum) = (f.resolve(dest.args[0]), f.resolve(dest.args[1]));
    let mut extras = Vec::new();
    for (index, &param) in params.iter().enumerate().skip(2) {
        let seed = f.resolve(initial[index]);
        let next = f.resolve(dest.args[index]);
        let update = def(f, next)?;
        let D::Binary { op: B::Iadd, args } = *f.inst_data(update) else {
            return None;
        };
        let stride = if f.resolve(args[0]) == param {
            args[1]
        } else if f.resolve(args[1]) == param {
            args[0]
        } else {
            return None;
        };
        if f.resolve(param) != param
            || !matches!(f.value_type(param), Type::I32 | Type::I64)
            || f.inst_block(update) != body
            || !invariant(f, dom, pre, stride)
            || !invariant(f, dom, pre, seed)
        {
            return None;
        }
        extras.push((param, seed, next));
    }
    let reduction = def(f, next_sum)?;
    let D::Binary { op: B::Iadd, args } = *f.inst_data(reduction) else {
        return None;
    };
    if f.inst_block(reduction) != body {
        return None;
    }
    // An affine recurrence s'=k*s+x can be split into parity streams with
    // coefficient k*k. No term may otherwise depend on the carried sum.
    let carry = |v| {
        let v = f.resolve(v);
        if v == sum {
            return Some(None);
        }
        let i = def(f, v)?;
        let D::Binary { op: B::Imul, args } = *f.inst_data(i) else {
            return None;
        };
        let constant = if f.resolve(args[0]) == sum {
            args[1]
        } else if f.resolve(args[1]) == sum {
            args[0]
        } else {
            return None;
        };
        (f.inst_block(i) == body).then_some(())?;
        Some(Some((i, int_const(f, constant)?)))
    };
    let affine = match (carry(args[0]), carry(args[1])) {
        (Some(carry), None) | (None, Some(carry)) => carry,
        _ => return None,
    };
    let mut loads = 0;
    let mut local = FxHashSet::default();
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
            D::Load { flags, ty, .. }
                if !flags.volatile
                    && (matches!(ty, Type::I32 | Type::I64)
                        || affine.is_some() && matches!(ty, Type::I8 | Type::I16)) =>
            {
                loads += 1;
            }
            _ => return None,
        }
        // An alias can leave its defining instruction in layout. Cloning it
        // under resolve(result) would overwrite the mapping for an invariant
        // or an earlier result and resurrect a dead computation.
        if f.inst_results(i).len() != 1
            || f.resolve(f.first_result(i)) != f.first_result(i)
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
            // The carried sum occurs only in its direct update or affine product;
            // its updated result cannot feed another operation in this body.
            if v == next_sum
                || v == sum && i != affine.map_or(reduction, |a| a.0)
                || affine.is_some_and(|a| v == f.first_result(a.0) && i != reduction)
                || !(params.contains(&v) || local.contains(&v) || invariant(f, dom, pre, v))
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
    let mut live = FxHashSet::from_iter([next_iv, next_sum]);
    live.extend(extras.iter().map(|e| e.2));
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
        signed,
        extras,
        affine,
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
    let squared = c.affine.map(|(_, coefficient)| {
        let i = f.insert_inst(
            InsertPoint::Before(c.pre_jump),
            D::iconst(sum_ty, coefficient.wrapping_mul(coefficient)),
        );
        f.first_result(i)
    });
    let pair_header = f.create_block();
    let pair_body = f.create_block();
    let combine = f.create_block();
    let iv = f.append_block_param(pair_header, iv_ty);
    let lane0 = f.append_block_param(pair_header, sum_ty);
    let lane1 = f.append_block_param(pair_header, sum_ty);
    let extra_params: Vec<_> = c
        .extras
        .iter()
        .map(|e| f.append_block_param(pair_header, f.value_type(e.0)))
        .collect();
    // For signed bounds, negative/zero bounds take the untouched scalar tail.
    // On every paired step 0<=iv<=bound<=SIGNED_MAX, so subtraction is exact.
    // iv<=bound is inductive: entry iv=0; a paired step occurs only when
    // bound-iv>=2. Hence subtraction and the two unit increments cannot wrap.
    let remaining = bin(f, pair_header, B::Isub, c.bound, iv);
    let two = emit(f, pair_header, D::iconst(iv_ty, 2));
    let enough = emit(
        f,
        pair_header,
        D::Icmp {
            cond: if c.signed { IntCC::Sge } else { IntCC::Uge },
            args: [remaining, two],
        },
    );
    f.append_inst(
        pair_header,
        D::Brif {
            cond: enough,
            dests: Box::new([
                BlockCall {
                    block: pair_body,
                    args: vec![],
                },
                BlockCall {
                    block: combine,
                    args: vec![],
                },
            ]),
        },
    );
    let originals = f.block_insts(c.body).to_vec();
    let mut next_iv = iv;
    let mut sums = [lane0, lane1];
    let mut extra_values = extra_params.clone();
    for lane in &mut sums {
        let mut map = FxHashMap::from_iter([(c.iv, next_iv), (c.sum, *lane)]);
        map.extend(c.extras.iter().zip(&extra_values).map(|(e, &v)| (e.0, v)));
        for &i in &originals[..originals.len() - 1] {
            let mut data = f.inst_data(i).clone();
            data.for_each_operand_mut(|v| {
                let resolved = f.resolve(*v);
                *v = map.get(&resolved).copied().unwrap_or(resolved);
            });
            if c.affine.is_some_and(|a| a.0 == i) {
                data = D::Binary {
                    op: B::Imul,
                    args: [*lane, squared.unwrap()],
                };
            }
            let v = emit(f, pair_body, data);
            map.insert(f.resolve(f.first_result(i)), v);
        }
        next_iv = map[&c.next_iv];
        *lane = map[&c.next_sum];
        extra_values = c.extras.iter().map(|e| map[&e.2]).collect();
    }
    let mut back = vec![next_iv, sums[0], sums[1]];
    back.extend(extra_values);
    jump(f, pair_body, pair_header, back);
    let weighted = if let Some((_, coefficient)) = c.affine {
        let factor = emit(f, combine, D::iconst(sum_ty, coefficient));
        bin(f, combine, B::Imul, lane0, factor)
    } else {
        lane0
    };
    let sum = bin(f, combine, B::Iadd, weighted, lane1);
    let mut tail = vec![iv, sum];
    tail.extend(extra_params);
    jump(f, combine, c.header, tail);
    // Insert zero before the preheader terminator, preserving all existing work.
    let zero_inst = f.insert_inst(InsertPoint::Before(c.pre_jump), D::iconst(sum_ty, 0));
    let zero = f.first_result(zero_inst);
    // Put the seed in the odd stream: k*even+odd then equals the original
    // seed even when the paired loop executes zero times, for any k.
    let mut entry = if c.affine.is_some() {
        vec![c.start, zero, c.seed]
    } else {
        vec![c.start, c.seed, zero]
    };
    entry.extend(c.extras.iter().map(|e| e.1));
    *f.inst_data_mut(c.pre_jump) = D::Jump {
        dest: BlockCall {
            block: pair_header,
            args: entry,
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
    {
        return false;
    }
    let headers: Vec<_> = f.blocks().collect();
    let mut changed = false;
    for header in headers {
        if let Some(c) = analyze(f, header) {
            // Cloning would drop the loop's debugger locations. Calls and
            // locations elsewhere in the function do not affect the loop.
            if [c.header, c.body].iter().any(|&b| {
                f.block_insts(b).iter().any(|&i| {
                    f.inst_loc(i).is_some()
                        || matches!(f.inst_data(i), D::Call { .. } | D::CallIndirect { .. })
                })
            }) {
                continue;
            }
            transform(f, c);
            changed = true;
        }
    }
    changed
}
