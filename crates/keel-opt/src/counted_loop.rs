//! The two-block loops that strength reduction, unrolling and vectorization
//! rewrite, matched once for all three.

use crate::analysis::Analysis;
use crate::ir_match::{def, int_const};
use keel_ir::dominators::DominatorTree;
use keel_ir::{BinaryOp, Block, Function, Inst, InstData, IntCC, Type, Value, ValueDef};
use std::rc::Rc;

/// A natural loop of a header and one body block:
///
/// ```text
/// pre:    ... jump header(seeds)
/// header: ... brif c, body, exit      (the body on either edge)
/// body:   ... jump header(nexts)
/// ```
///
/// The body is the only latch, has no parameters, and is entered only from
/// the header, by an edge without arguments. The preheader is the header's
/// only other predecessor. The exit is neither block of the loop.
pub(crate) struct TwoBlockLoop {
    pub body: Block,
    pub pre: Block,
    /// Which destination of the header's `brif` enters the body.
    pub body_edge: usize,
    /// The body's jump back to the header.
    pub back: Inst,
    /// The preheader's jump to the header.
    pub entry: Inst,
}

impl TwoBlockLoop {
    pub fn find(f: &Function, analysis: &Analysis, header: Block) -> Option<Self> {
        let &branch = f.block_insts(header).last()?;
        let InstData::Brif { dests, .. } = f.inst_data(branch) else {
            return None;
        };
        // The body's own shape rejects almost every block before the loop
        // analysis is computed.
        let body_edge = (0..2).find(|&edge| {
            let body = dests[edge].block;
            body != header
                && dests[1 - edge].block != body
                && dests[edge].args.is_empty()
                && f.block_params(body).is_empty()
                && analysis.cfg.preds(body) == [header]
                && f.block_insts(body).last().is_some_and(
                    |&i| matches!(f.inst_data(i), InstData::Jump { dest } if dest.block == header),
                )
        })?;
        let body = dests[body_edge].block;
        let back = *f.block_insts(body).last()?;
        let lp = analysis.loops(f).loop_at(header)?;
        let (&[latch], 2, Some(pre)) = (lp.latches, lp.blocks.len(), lp.preheader) else {
            return None;
        };
        let &entry = f.block_insts(pre).last()?;
        // The preheader's only successor is the header.
        if latch != body || !matches!(f.inst_data(entry), InstData::Jump { .. }) {
            return None;
        }
        Some(Self {
            body,
            pre,
            body_edge,
            back,
            entry,
        })
    }
}

/// A [`TwoBlockLoop`] counted by its header's first parameter: the header
/// holds only integer literals before `icmp iv, bound` and the `brif` on it,
/// the body is entered while `iv` is below `bound`, and the body steps `iv`
/// by one.
pub(crate) struct CountedLoop {
    pub shape: TwoBlockLoop,
    /// The induction variable, an `i32` or `i64` header parameter.
    pub iv: Value,
    /// The compare operand `iv` is tested against, unresolved.
    pub bound: Value,
    /// Whether the compare is signed.
    pub signed: bool,
    /// The `iv + 1` the back edge passes.
    pub step: Inst,
}

impl CountedLoop {
    /// `analysis` is filled with the function's analyses only once the
    /// header passes its local checks, so most blocks never need them.
    pub fn find(f: &Function, analysis: &mut Option<Rc<Analysis>>, header: Block) -> Option<Self> {
        let &iv = f.block_params(header).first()?;
        if f.resolve(iv) != iv || !matches!(f.value_type(iv), Type::I32 | Type::I64) {
            return None;
        }
        let (literals, &[compare, branch]) = f.block_insts(header).split_last_chunk::<2>()?;
        if !literals
            .iter()
            .all(|&i| matches!(f.inst_data(i), InstData::Iconst { .. }))
        {
            return None;
        }
        let InstData::Icmp { cond, args } = *f.inst_data(compare) else {
            return None;
        };
        let InstData::Brif { cond: test, .. } = *f.inst_data(branch) else {
            return None;
        };
        if f.resolve(test) != f.first_result(compare) {
            return None;
        }
        let (bound, signed, body_edge) =
            match (cond, f.resolve(args[0]) == iv, f.resolve(args[1]) == iv) {
                (IntCC::Ult, true, false) => (args[1], false, 0),
                (IntCC::Ugt, false, true) => (args[0], false, 0),
                (IntCC::Slt, true, false) => (args[1], true, 0),
                (IntCC::Sgt, false, true) => (args[0], true, 0),
                (IntCC::Uge, true, false) => (args[1], false, 1),
                (IntCC::Ule, false, true) => (args[0], false, 1),
                (IntCC::Sge, true, false) => (args[1], true, 1),
                (IntCC::Sle, false, true) => (args[0], true, 1),
                _ => return None,
            };
        let analysis = analysis.get_or_insert_with(|| crate::analysis::get(f));
        let shape = TwoBlockLoop::find(f, analysis, header)?;
        if shape.body_edge != body_edge {
            return None;
        }
        let InstData::Jump { dest } = f.inst_data(shape.back) else {
            unreachable!("TwoBlockLoop::find checked the back edge")
        };
        let step = def(f, dest.args[0])?;
        let InstData::Binary {
            op: BinaryOp::Iadd,
            args,
        } = *f.inst_data(step)
        else {
            return None;
        };
        if f.inst_block(step) != shape.body
            || !((f.resolve(args[0]) == iv && int_const(f, args[1]) == Some(1))
                || (f.resolve(args[1]) == iv && int_const(f, args[0]) == Some(1)))
        {
            return None;
        }
        Some(Self {
            shape,
            iv,
            bound,
            signed,
            step,
        })
    }
}

/// Whether `v` is defined in a block that dominates the preheader `pre`, so
/// it is available throughout the loop.
pub(crate) fn invariant(f: &Function, dom: &DominatorTree, pre: Block, v: Value) -> bool {
    let b = match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => f.inst_block(i),
        ValueDef::Param(b, _) => b,
        _ => return false,
    };
    dom.dominates(b, pre)
}
