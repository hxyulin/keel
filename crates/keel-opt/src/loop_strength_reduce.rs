//! Replace invariant-times-induction products with same-width wrapping recurrences.
//! Invariant offsets are absorbed into existing recurrences without new state.
//! No pointer arithmetic is widened and no load or potentially trapping operation
//! is moved. Entry multiplication is pure, including on the zero-trip path.
use crate::analysis::Analysis;
use crate::counted_loop::{TwoBlockLoop, invariant};
use keel_ir::{
    BinaryOp as B, Block, Function, InsertPoint, Inst, InstData as D, Type, Value, ValueDef,
};

pub(crate) struct LoopStrengthReduce;
impl crate::Pass for LoopStrengthReduce {
    fn name(&self) -> &'static str {
        "loop-strength-reduce"
    }
    fn description(&self) -> &'static str {
        "replace invariant induction products with same-width wrapping recurrences"
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        run(f)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

fn def(f: &Function, v: Value) -> Option<Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, 0) => Some(i),
        _ => None,
    }
}
/// A small two-block loop whose body is the `brif`'s first destination.
fn shape(f: &Function, analysis: &Analysis, header: Block) -> Option<TwoBlockLoop> {
    TwoBlockLoop::find(f, analysis, header).filter(|l| {
        l.body_edge == 0 && f.block_params(header).len() <= 8 && f.block_insts(l.body).len() <= 128
    })
}
fn emit(f: &mut Function, before: Inst, data: D) -> Value {
    let i = f.insert_inst(InsertPoint::Before(before), data);
    f.first_result(i)
}

pub fn run(f: &mut Function) -> bool {
    let analysis = crate::analysis::get(f);
    let dom = &analysis.dom;
    let headers: Vec<_> = f.blocks().collect();
    let mut budget = 16;
    let mut changed = false;
    for header in headers {
        if budget == 0 {
            break;
        }
        let Some(TwoBlockLoop {
            body,
            pre,
            back,
            entry,
            ..
        }) = shape(f, &analysis, header)
        else {
            continue;
        };
        let params = f.block_params(header).to_vec();
        for (index, iv) in params.into_iter().enumerate() {
            if budget == 0 {
                break;
            }
            let ty = f.value_type(iv);
            if !matches!(ty, Type::I32 | Type::I64) || f.resolve(iv) != iv {
                continue;
            }
            let next = f.inst_data(back).branch_dests()[0].args[index];
            let Some(step_inst) = def(f, next) else {
                continue;
            };
            let D::Binary { op: B::Iadd, args } = *f.inst_data(step_inst) else {
                continue;
            };
            let literal = if f.resolve(args[0]) == iv {
                args[1]
            } else if f.resolve(args[1]) == iv {
                args[0]
            } else {
                continue;
            };
            let Some(literal_inst) = def(f, literal) else {
                continue;
            };
            let D::Iconst { ty: step_ty, imm } = *f.inst_data(literal_inst) else {
                continue;
            };
            if step_ty != ty {
                continue;
            }
            let seed = f.inst_data(entry).branch_dests()[0].args[index];
            let candidates = f.block_insts(body).to_vec();
            for product in candidates {
                if budget == 0 {
                    break;
                }
                let D::Binary { op: B::Imul, args } = *f.inst_data(product) else {
                    continue;
                };
                let result = f.first_result(product);
                if !matches!(f.value_def(result), ValueDef::Result(i, 0) if i == product) {
                    continue;
                }
                let stride = if f.resolve(args[0]) == iv {
                    args[1]
                } else if f.resolve(args[1]) == iv {
                    args[0]
                } else {
                    continue;
                };
                if f.value_type(stride) != ty || !invariant(f, dom, pre, stride) {
                    continue;
                }
                let initial = emit(
                    f,
                    entry,
                    D::Binary {
                        op: B::Imul,
                        args: [seed, stride],
                    },
                );
                let step = emit(f, entry, D::iconst(ty, imm));
                let delta = emit(
                    f,
                    entry,
                    D::Binary {
                        op: B::Imul,
                        args: [stride, step],
                    },
                );
                let recurrence = f.append_block_param(header, ty);
                let next_product = emit(
                    f,
                    back,
                    D::Binary {
                        op: B::Iadd,
                        args: [recurrence, delta],
                    },
                );
                f.inst_data_mut(entry).branch_dests_mut()[0]
                    .args
                    .push(initial);
                f.inst_data_mut(back).branch_dests_mut()[0]
                    .args
                    .push(next_product);
                f.replace_all_uses(result, recurrence);
                f.remove_inst(product);
                budget -= 1;
                changed = true;
            }
        }
    }
    changed | fold_offsets(f)
}

/// Translate an existing recurrence rather than creating another one:
///     r = seed; use(r + offset); r += delta
/// becomes
///     r = seed + offset; use(r); r += delta.
/// This is an identity modulo the original width, including across signed wrap.
/// In particular, sext remains AFTER the wrapping add/recurrence, never outside
/// it. Only the pure seed add moves to entry; effects and guards stay in place.
fn fold_offsets(f: &mut Function) -> bool {
    // Each candidate checks all uses. Bound the work and avoid changing the
    // interpretation of values described by debugger metadata.
    if f.num_blocks() > 512
        || f.layout_insts().count() > 8192
        || f.debug_return.is_some()
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || !f.debug_scopes.is_empty()
        || f.layout_insts().any(|(_, i)| f.inst_loc(i).is_some())
    {
        return false;
    }
    let analysis = crate::analysis::get(f);
    let dom = &analysis.dom;
    let mut budget = 16;
    let mut changed = false;
    for header in f.blocks().collect::<Vec<_>>() {
        if budget == 0 {
            break;
        }
        let Some(TwoBlockLoop {
            body,
            pre,
            back,
            entry,
            ..
        }) = shape(f, &analysis, header)
        else {
            continue;
        };
        for (index, r) in f.block_params(header).to_vec().into_iter().enumerate() {
            if budget == 0 {
                break;
            }
            // Count attempted candidates too: the whole-function use scans
            // remain bounded even for functions with many rejected loops.
            budget -= 1;
            let ty = f.value_type(r);
            if !matches!(ty, Type::I32 | Type::I64) || f.resolve(r) != r {
                continue;
            }
            let next = f.inst_data(back).branch_dests()[0].args[index];
            let Some(update) = def(f, next) else { continue };
            if f.inst_block(update) != body || f.resolve(next) != next {
                continue;
            }
            let D::Binary { op: B::Iadd, args } = *f.inst_data(update) else {
                continue;
            };
            let delta = if f.resolve(args[0]) == r {
                args[1]
            } else if f.resolve(args[1]) == r {
                args[0]
            } else {
                continue;
            };
            // Literal steps may still be in the body before GCM. Every other
            // step must already dominate entry, so it cannot depend on r.
            if !invariant(f, dom, pre, delta)
                && !def(f, delta).is_some_and(|i| matches!(f.inst_data(i), D::Iconst { .. }))
            {
                continue;
            }
            let mut offset = None;
            let mut adds = Vec::new();
            let mut valid = true;
            for (_, i) in f.layout_insts() {
                // The next value must feed ONLY its own backedge slot. Its
                // numerical meaning changes along with the parameter.
                if i == back {
                    if f.inst_data(i).branch_dests()[0]
                        .args
                        .iter()
                        .enumerate()
                        .any(|(n, &v)| n != index && f.resolve(v) == next)
                    {
                        valid = false;
                    }
                } else {
                    f.inst_data(i)
                        .for_each_operand(|v| valid &= f.resolve(v) != next);
                }
                let mut uses_r = false;
                f.inst_data(i)
                    .for_each_operand(|v| uses_r |= f.resolve(v) == r);
                if !uses_r || i == update {
                    continue;
                }
                let D::Binary { op: B::Iadd, args } = *f.inst_data(i) else {
                    valid = false;
                    break;
                };
                let other = if f.resolve(args[0]) == r {
                    args[1]
                } else {
                    args[0]
                };
                let other = f.resolve(other);
                if f.inst_block(i) != body
                    || !invariant(f, dom, pre, other)
                    || offset.is_some_and(|o| o != other)
                    || f.resolve(f.first_result(i)) != f.first_result(i)
                {
                    valid = false;
                    break;
                }
                offset = Some(other);
                adds.push(i);
            }
            let Some(offset) = offset.filter(|_| valid) else {
                continue;
            };
            let seed = f.inst_data(entry).branch_dests()[0].args[index];
            let initial = emit(
                f,
                entry,
                D::Binary {
                    op: B::Iadd,
                    args: [seed, offset],
                },
            );
            f.inst_data_mut(entry).branch_dests_mut()[0].args[index] = initial;
            for i in adds {
                f.replace_all_uses(f.first_result(i), r);
                f.remove_inst(i);
            }
            changed = true;
        }
    }
    changed
}
