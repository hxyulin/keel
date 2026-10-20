//! Bounded integer if-conversion. Only complete, single-predecessor diamonds
//! and parallel edges to the same join are admitted. Arm instructions use an
//! explicit nontrapping scalar allowlist; memory, calls, division, floating
//! point, exception operations and aliased definitions are never speculated.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, Type,
    UnaryOp, Value,
};
use std::collections::{HashMap, HashSet};

const ARM_BUDGET: usize = 8;
const SELECT_BUDGET: usize = 8;

pub fn run(f: &mut Function) -> bool {
    let mut changed = false;
    // At most one decision per original block; recompute CFG after mutations.
    for head in f.blocks().collect::<Vec<_>>() {
        if !f.is_block_inserted(head) {
            continue;
        }
        let Some(&term) = f.block_insts(head).last() else {
            continue;
        };
        let D::Brif { cond, dests } = f.inst_data(term).clone() else {
            continue;
        };
        let cfg = ControlFlowGraph::compute(f);
        if !cfg.reverse_postorder(f).contains(&head) {
            continue;
        }
        if dests[0].block == dests[1].block {
            let join = dests[0].block;
            if join == head || !canonical_params(f, join) || dests[0].args.len() > SELECT_BUDGET {
                continue;
            }
            let args = selects(f, term, cond, &dests[0].args, &dests[1].args);
            *f.inst_data_mut(term) = D::Jump {
                dest: BlockCall { block: join, args },
            };
            changed = true;
            continue;
        }
        let arms = [dests[0].block, dests[1].block];
        if arms.contains(&head)
            || arms.iter().any(|&b| {
                cfg.preds(b) != [head] || Some(b) == f.entry_block() || !canonical_params(f, b)
            })
        {
            continue;
        }
        let Some((left, left_insts)) = arm(f, arms[0]) else {
            continue;
        };
        let Some((right, right_insts)) = arm(f, arms[1]) else {
            continue;
        };
        if left.block != right.block
            || left.block == head
            || arms.contains(&left.block)
            || !canonical_params(f, left.block)
            || left_insts.len() + right_insts.len() > ARM_BUDGET
            || left.args.len() > SELECT_BUDGET
        {
            continue;
        }
        // Moving definitions must not change their availability to other
        // users, including uses in unreachable blocks and noncanonical IR.
        let defs: HashSet<_> = arms
            .iter()
            .flat_map(|&b| {
                f.block_params(b).iter().copied().chain(
                    f.block_insts(b)
                        .iter()
                        .flat_map(|&i| f.inst_results(i).iter().copied()),
                )
            })
            .collect();
        if f.layout_insts().any(|(b, i)| {
            if arms.contains(&b) {
                return false;
            }
            let mut outside = false;
            f.inst_data(i)
                .for_each_operand(|v| outside |= defs.contains(&f.resolve(v)));
            outside
        }) {
            continue;
        }
        let mut edges = [left, right];
        for n in 0..2 {
            // Substitute the arm's incoming parameters simultaneously, before
            // moving instructions. Outgoing join arguments observe the same
            // substitution, including empty arms and permuted edge operands.
            let mapping: HashMap<_, _> = f
                .block_params(arms[n])
                .iter()
                .copied()
                .zip(dests[n].args.iter().map(|&v| f.resolve(v)))
                .collect();
            let rewrite = |v: Value| {
                let v = f.resolve(v);
                mapping.get(&v).copied().unwrap_or(v)
            };
            let instructions = if n == 0 { &left_insts } else { &right_insts };
            let rewritten: Vec<_> = instructions
                .iter()
                .map(|&i| {
                    let mut d = f.inst_data(i).clone();
                    d.for_each_operand_mut(|v| *v = rewrite(*v));
                    (i, d)
                })
                .collect();
            for v in &mut edges[n].args {
                *v = rewrite(*v);
            }
            for (i, d) in rewritten {
                *f.inst_data_mut(i) = d;
                f.move_inst(i, InsertPoint::Before(term));
            }
        }
        let args = selects(f, term, cond, &edges[0].args, &edges[1].args);
        *f.inst_data_mut(term) = D::Jump {
            dest: BlockCall {
                block: edges[0].block,
                args,
            },
        };
        for b in arms {
            f.remove_block(b);
        }
        changed = true;
    }
    changed
}
fn canonical_params(f: &Function, b: Block) -> bool {
    f.block_params(b).iter().all(|&v| {
        f.resolve(v) == v
            && matches!(
                f.value_type(v),
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
            )
    })
}
fn arm(f: &Function, b: Block) -> Option<(BlockCall, Vec<Inst>)> {
    let (&term, body) = f.block_insts(b).split_last()?;
    let D::Jump { dest } = f.inst_data(term) else {
        return None;
    };
    if body.len() > ARM_BUDGET
        || body
            .iter()
            .any(|&i| f.inst_results(i).iter().any(|&v| f.resolve(v) != v) || !speculatable(f, i))
    {
        return None;
    }
    Some((dest.clone(), body.to_vec()))
}
fn speculatable(f: &Function, i: Inst) -> bool {
    // Keep the scalar subset explicit instead of equating "pure" with safe
    // speculation, so newly added IR operations cannot silently enter it.
    if f.inst_results(i).iter().any(|&v| {
        !matches!(
            f.value_type(v),
            Type::I8 | Type::I16 | Type::I32 | Type::I64
        )
    }) {
        return false;
    }
    match f.inst_data(i) {
        D::Iconst { .. } | D::Icmp { .. } | D::Select { .. } => true,
        D::Binary { op, .. } => matches!(
            op,
            B::Iadd
                | B::Isub
                | B::Imul
                | B::And
                | B::Or
                | B::Xor
                | B::Shl
                | B::Ushr
                | B::Sshr
                | B::Rotl
                | B::Rotr
                | B::Umulhi
                | B::Smulhi
        ),
        D::Unary { op, .. } => matches!(
            op,
            UnaryOp::Ineg
                | UnaryOp::Bnot
                | UnaryOp::Clz
                | UnaryOp::Ctz
                | UnaryOp::Popcnt
                | UnaryOp::Bswap
        ),
        D::Convert { op, .. } => matches!(op, ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc),
        _ => false,
    }
}
fn selects(f: &mut Function, at: Inst, cond: Value, yes: &[Value], no: &[Value]) -> Vec<Value> {
    let cond = f.resolve(cond);
    yes.iter()
        .zip(no)
        .map(|(&a, &b)| {
            let (a, b) = (f.resolve(a), f.resolve(b));
            if a == b {
                a
            } else {
                f.ins(InsertPoint::Before(at)).select(cond, a, b)
            }
        })
        .collect()
}
