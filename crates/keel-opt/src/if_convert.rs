//! Bounded integer if-conversion for diamonds, direct-to-join triangles,
//! and nested shapes reduced from the inside out through private jump chains.
//! Arm instructions use an explicit nontrapping scalar allowlist; memory, calls, division, floating
//! point, exception operations and aliased definitions are never speculated.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, IntCC,
    Type, UnaryOp, Value, ValueDef,
};

const ARM_BUDGET: usize = 8;
const SELECT_BUDGET: usize = 8;
const BLOCK_BUDGET: usize = 8;

/// Remove `x ? clz/ctz(x) : width` diamonds without enabling general
/// if-conversion. Counts are defined at zero in IR. Each arm must contain
/// exactly its value and a jump, with no parameters or other predecessors.
pub(crate) fn fold_count_guards(f: &mut Function) -> bool {
    let mut changed = false;
    for head in f.blocks().collect::<Vec<_>>() {
        let Some(&term) = f.block_insts(head).last() else {
            continue;
        };
        let D::Brif { cond, dests } = f.inst_data(term).clone() else {
            continue;
        };
        let mut source = f.resolve(cond);
        let mut nonzero = 0;
        if let ValueDef::Result(test, 0) = f.value_def(source)
            && let D::Icmp {
                cond: cc @ (IntCC::Eq | IntCC::Ne),
                args: [a, b],
            } = *f.inst_data(test)
            && crate::ir_match::int_const(f, b) == Some(0)
        {
            source = f.resolve(a);
            nonzero = usize::from(cc == IntCC::Eq);
        }
        let arms = [dests[nonzero].block, dests[1 - nonzero].block];
        if arms[0] == arms[1]
            || arms.contains(&head)
            || dests.iter().any(|d| !d.args.is_empty())
            || arms
                .iter()
                .any(|&b| !f.block_params(b).is_empty() || f.block_insts(b).len() != 2)
        {
            continue;
        }
        let cfg = ControlFlowGraph::compute(f);
        if arms.iter().any(|&b| cfg.preds(b) != [head]) {
            continue;
        }
        let count = f.block_insts(arms[0])[0];
        let literal = f.block_insts(arms[1])[0];
        let D::Unary {
            op: UnaryOp::Clz | UnaryOp::Ctz,
            arg,
        } = *f.inst_data(count)
        else {
            continue;
        };
        if f.resolve(arg) != source || f.resolve(f.first_result(count)) != f.first_result(count) {
            continue;
        }
        let D::Iconst { imm, ty } = *f.inst_data(literal) else {
            continue;
        };
        if f.resolve(f.first_result(literal)) != f.first_result(literal)
            || Some(imm) != f.value_type(source).bytes().map(|n| i64::from(n * 8))
            || ty != f.value_type(f.first_result(count))
        {
            continue;
        }
        let jumps: Vec<_> = arms
            .iter()
            .map(|&b| f.inst_data(f.block_insts(b)[1]))
            .collect();
        let (D::Jump { dest: yes }, D::Jump { dest: no }) = (jumps[0], jumps[1]) else {
            continue;
        };
        if yes.block == head
            || arms.contains(&yes.block)
            || yes.block != no.block
            || yes.args.len() != 1
            || no.args.len() != 1
            || f.resolve(yes.args[0]) != f.first_result(count)
            || f.resolve(no.args[0]) != f.resolve(f.first_result(literal))
        {
            continue;
        }
        let dest = yes.clone();
        // Keep both definitions available to any dominated external use.
        // CFG cleanup will remove their now-unreachable, jump-only arms.
        f.move_inst(count, InsertPoint::Before(term));
        f.move_inst(literal, InsertPoint::Before(term));
        *f.inst_data_mut(term) = D::Jump { dest };
        changed = true;
    }
    changed
}

pub fn run(f: &mut Function) -> bool {
    let mut changed = false;
    // Visit children first, independent of layout order. Each original head is
    // considered once; newly exposed outer shapes retain the same arm budgets
    // (including selects already introduced by inner conversions).
    let heads = crate::analysis::get(f).dom.rpo().to_vec();
    for head in heads.into_iter().rev() {
        if !f.is_block_inserted(head) {
            continue;
        }
        let Some(&term) = f.block_insts(head).last() else {
            continue;
        };
        let D::Brif { cond, dests } = f.inst_data(term).clone() else {
            continue;
        };
        let analysis = crate::analysis::get(f);
        let cfg = &analysis.cfg;
        if !analysis.dom.is_reachable(head) {
            continue;
        }
        if dests[0].block == dests[1].block {
            changed |= fold_same_destination(f, term);
            continue;
        }
        let paths = [
            paths(f, cfg, head, &dests[0]),
            paths(f, cfg, head, &dests[1]),
        ];
        // The zero-block prefix represents a direct edge to the join. Longer
        // prefixes admit private forwarding blocks left by nested ternaries.
        let pair = paths[0]
            .iter()
            .flat_map(|left| paths[1].iter().map(move |right| (left, right)))
            .find(|(left, right)| {
                left.edge.block == right.edge.block
                    && left.edge.block != head
                    && canonical_params(f, left.edge.block)
                    && left.edge.args.len() <= SELECT_BUDGET
                    && left.insts.len() + right.insts.len() <= ARM_BUDGET
                    && left.blocks.len() + right.blocks.len() <= BLOCK_BUDGET
                    && !left.blocks.iter().any(|b| right.blocks.contains(b))
            });
        let Some((left, right)) = pair else { continue };
        let arms: Vec<_> = left.blocks.iter().chain(&right.blocks).copied().collect();
        // Moving definitions must not change their availability to other
        // users, including uses in unreachable blocks and noncanonical IR.
        let defs: FxHashSet<_> = arms
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
        let mut edges = [left.edge.clone(), right.edge.clone()];
        for (n, path) in [left, right].into_iter().enumerate() {
            // Each path was planned without mutation, using simultaneous
            // incoming-parameter substitution at every forwarding edge.
            for (i, d) in &path.insts {
                *f.inst_data_mut(*i) = d.clone();
                f.move_inst(*i, InsertPoint::Before(term));
            }
            // Resolve aliases even for the zero-block (direct) path.
            for v in &mut edges[n].args {
                *v = f.resolve(*v);
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
/// Fold parallel edges without speculating any instructions. Keep the same
/// scalar select budget and non-self-edge policy as general if-conversion.
pub(crate) fn fold_same_destination(f: &mut Function, term: Inst) -> bool {
    let D::Brif { cond, dests } = f.inst_data(term) else {
        return false;
    };
    if dests[0].block != dests[1].block
        || dests[0].block == f.inst_block(term)
        || !canonical_params(f, dests[0].block)
        || dests[0].args.len() > SELECT_BUDGET
    {
        return false;
    }
    let (cond, dests) = (*cond, dests.clone());
    let args = selects(f, term, cond, &dests[0].args, &dests[1].args);
    *f.inst_data_mut(term) = D::Jump {
        dest: BlockCall {
            block: dests[0].block,
            args,
        },
    };
    true
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
#[derive(Clone)]
struct Path {
    edge: BlockCall,
    blocks: Vec<Block>,
    insts: Vec<(Inst, D)>,
}
fn paths(f: &Function, cfg: &ControlFlowGraph, head: Block, edge: &BlockCall) -> Vec<Path> {
    let mut path = Path {
        edge: edge.clone(),
        blocks: vec![],
        insts: vec![],
    };
    let mut result = vec![path.clone()];
    let mut previous = head;
    let mut mapping = FxHashMap::default();
    while path.blocks.len() < BLOCK_BUDGET {
        let b = path.edge.block;
        if b == head
            || path.blocks.contains(&b)
            || Some(b) == f.entry_block()
            || cfg.preds(b) != [previous]
            || !canonical_params(f, b)
        {
            break;
        }
        let Some((&term, body)) = f.block_insts(b).split_last() else {
            break;
        };
        let D::Jump { dest } = f.inst_data(term) else {
            break;
        };
        if path.insts.len() + body.len() > ARM_BUDGET
            || body.iter().any(|&i| {
                f.inst_results(i).iter().any(|&v| f.resolve(v) != v) || !speculatable(f, i)
            })
        {
            break;
        }
        // Edge operands were already rewritten in the predecessor's scope.
        // Extend only after collecting all replacements, never by repeatedly
        // applying substitutions to operands (which breaks permutations).
        let incoming: Vec<_> = f
            .block_params(b)
            .iter()
            .copied()
            .zip(path.edge.args.iter().map(|&v| f.resolve(v)))
            .collect();
        mapping.extend(incoming);
        let rewrite = |v: Value| {
            let v = f.resolve(v);
            mapping.get(&v).copied().unwrap_or(v)
        };
        for &i in body {
            let mut d = f.inst_data(i).clone();
            d.for_each_operand_mut(|v| *v = rewrite(*v));
            path.insts.push((i, d));
        }
        path.edge = dest.clone();
        for v in &mut path.edge.args {
            *v = rewrite(*v);
        }
        path.blocks.push(b);
        previous = b;
        result.push(path.clone());
    }
    result
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
