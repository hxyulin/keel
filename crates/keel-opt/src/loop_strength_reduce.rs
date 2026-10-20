//! Replace invariant-times-induction products with same-width wrapping recurrences.
//! No pointer arithmetic is widened and no load or potentially trapping operation
//! is moved. Entry multiplication is pure, including on the zero-trip path.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
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
    fn run(&self, f: &mut Function) -> bool {
        run(f)
    }
}

fn def(f: &Function, v: Value) -> Option<Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, 0) => Some(i),
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
fn emit(f: &mut Function, before: Inst, data: D) -> Value {
    let i = f.insert_inst(InsertPoint::Before(before), data);
    f.first_result(i)
}

pub fn run(f: &mut Function) -> bool {
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
    let headers: Vec<_> = f.blocks().collect();
    let mut budget = 16;
    let mut changed = false;
    for header in headers {
        if budget == 0 {
            break;
        }
        let Some(&branch) = f.block_insts(header).last() else {
            continue;
        };
        let D::Brif { dests, .. } = f.inst_data(branch) else {
            continue;
        };
        let body = dests[0].block;
        if body == header
            || dests[1].block == header
            || dests[1].block == body
            || !dests[0].args.is_empty()
            || !f.block_params(body).is_empty()
            || cfg.preds(body) != [header]
            || cfg.preds(header).len() != 2
            || f.block_params(header).len() > 8
            || f.block_insts(body).len() > 128
        {
            continue;
        }
        let Some(&back) = f.block_insts(body).last() else {
            continue;
        };
        let D::Jump { dest } = f.inst_data(back) else {
            continue;
        };
        if dest.block != header {
            continue;
        }
        let Some(&pre) = cfg.preds(header).iter().find(|&&b| b != body) else {
            continue;
        };
        if pre == header || !dom.dominates(pre, header) {
            continue;
        }
        let Some(&entry) = f.block_insts(pre).last() else {
            continue;
        };
        let D::Jump { dest } = f.inst_data(entry) else {
            continue;
        };
        if dest.block != header {
            continue;
        }
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
                if f.value_type(stride) != ty || !invariant(f, &dom, pre, stride) {
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
                let step = emit(f, entry, D::Iconst { ty, imm });
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
    changed
}
