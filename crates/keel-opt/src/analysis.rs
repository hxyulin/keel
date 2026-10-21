//! A per-thread cache of each function's CFG, dominator tree and loop
//! depths, shared by the passes of one pipeline.
//!
//! Most passes ask for these analyses, and most runs between two requests
//! for the same function leave its CFG alone. An entry is found by the
//! function's address and reused when the function's [`Function::cfg_stamp`]
//! is the one it was computed for. Otherwise it is still reused after
//! checking every input of the computation against the function: its block
//! count, its layout, and each block's successors (read from its last
//! instruction, as [`ControlFlowGraph::compute`] does), since rewriting a
//! branch's arguments renews the stamp too. The dominator tree and loop
//! depths depend on nothing else. A stale entry, or one left by another
//! function at the same address, is recomputed, so a cached result always
//! equals a fresh one.

use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::FxHashMap;
use keel_ir::loops::LoopDepth;
use keel_ir::{Block, EntityRef, Function};
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

pub(crate) struct Analysis {
    pub cfg: ControlFlowGraph,
    pub dom: DominatorTree,
    loops: OnceCell<LoopDepth>,
    /// The latest stamp of the function known to have this CFG.
    stamp: Cell<u64>,
    num_blocks: usize,
    layout: Vec<Block>,
}

impl Analysis {
    /// Loop nesting depths, computed on first use.
    pub fn loops(&self, func: &Function) -> &LoopDepth {
        self.loops
            .get_or_init(|| LoopDepth::compute(func, &self.cfg, &self.dom))
    }

    /// Whether `func` still has the CFG this was computed from.
    fn matches(&self, func: &Function) -> bool {
        if func.num_blocks() != self.num_blocks {
            return false;
        }
        let mut layout = self.layout.iter();
        for block in func.blocks() {
            if layout.next() != Some(&block) {
                return false;
            }
            let succs = self.cfg.succs(block);
            let dests = match func.block_insts(block).last() {
                Some(&term) => func.inst_data(term).branch_dests(),
                None => &[],
            };
            // `succs` lists each destination block once, in branch order.
            let mut seen = 0;
            for dest in dests {
                if succs[..seen].contains(&dest.block) {
                    continue;
                }
                if succs.get(seen) != Some(&dest.block) {
                    return false;
                }
                seen += 1;
            }
            if seen != succs.len() {
                return false;
            }
        }
        layout.next().is_none()
    }
}

impl Analysis {
    /// Debug check: the cached analyses equal freshly computed ones.
    fn same_as_fresh(&self, func: &Function) -> bool {
        let cfg = ControlFlowGraph::compute(func);
        let dom = DominatorTree::compute(func, &cfg);
        (0..func.num_blocks()).map(Block::new).all(|b| {
            cfg.preds(b) == self.cfg.preds(b)
                && cfg.succs(b) == self.cfg.succs(b)
                && dom.idom(b) == self.dom.idom(b)
        }) && dom.rpo() == self.dom.rpo()
    }
}

thread_local! {
    static CACHE: RefCell<FxHashMap<usize, Rc<Analysis>>> = RefCell::default();
}

/// The CFG and dominator tree of `func` as it is now.
pub(crate) fn get(func: &Function) -> Rc<Analysis> {
    let key = func as *const Function as usize;
    let cached = CACHE.with(|cache| cache.borrow().get(&key).cloned());
    if let Some(analysis) = cached {
        if analysis.stamp.get() == func.cfg_stamp() || analysis.matches(func) {
            debug_assert!(analysis.matches(func) && analysis.same_as_fresh(func));
            analysis.stamp.set(func.cfg_stamp());
            return analysis;
        }
        // Release the stale arrays to the dominator pool before recomputing.
        CACHE.with(|cache| cache.borrow_mut().remove(&key));
        drop(analysis);
    }
    let mut cfg = ControlFlowGraph::compute(func);
    let mut dom = DominatorTree::compute(func, &cfg);
    cfg.shrink_to_fit();
    dom.shrink_to_fit();
    let analysis = Rc::new(Analysis {
        cfg,
        dom,
        loops: OnceCell::new(),
        stamp: Cell::new(func.cfg_stamp()),
        num_blocks: func.num_blocks(),
        layout: func.blocks().collect(),
    });
    CACHE.with(|cache| cache.borrow_mut().insert(key, analysis.clone()));
    analysis
}

/// Drops every cached analysis; the pipeline ends with this.
pub(crate) fn clear() {
    let entries = CACHE.with(|cache| std::mem::take(&mut *cache.borrow_mut()));
    drop(entries);
}
