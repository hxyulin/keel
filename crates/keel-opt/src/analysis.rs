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
        // Linear scans stay bounded for ordinary branches. Only wide switches
        // need a per-destination marker table, shared across this check's blocks.
        const SMALL_SUCCESSOR_LIMIT: usize = 8;
        let mut visited = Vec::new();
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
            let wide = succs.len() > SMALL_SUCCESSOR_LIMIT;
            if wide && visited.is_empty() {
                visited.resize(self.num_blocks, None);
            }
            let mut seen = 0;
            for dest in dests {
                let duplicate = if wide {
                    let Some(marker) = visited.get_mut(dest.block.index()) else {
                        return false;
                    };
                    marker.replace(block) == Some(block)
                } else {
                    succs[..seen].contains(&dest.block)
                };
                if duplicate {
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
    /// Audit the cached analyses against independent fresh computations.
    #[cfg(any(test, feature = "audit-analysis"))]
    fn same_as_fresh(&self, func: &Function) -> bool {
        let cfg = ControlFlowGraph::compute(func);
        let dom = DominatorTree::compute(func, &cfg);
        (0..func.num_blocks()).map(Block::new).all(|b| {
            cfg.preds(b) == self.cfg.preds(b)
                && cfg.succs(b) == self.cfg.succs(b)
                && dom.idom(b) == self.dom.idom(b)
        }) && dom.rpo() == self.dom.rpo()
            && self.loops.get().is_none_or(|cached| {
                let fresh = LoopDepth::compute(func, &cfg, &dom);
                (0..func.num_blocks())
                    .map(Block::new)
                    .all(|block| cached.depth(block) == fresh.depth(block))
            })
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
            #[cfg(feature = "audit-analysis")]
            assert!(
                analysis.same_as_fresh(func),
                "cached analyses differ from fresh computations"
            );
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

/// Checks that a cached entry carrying `func`'s stamp still has its CFG.
/// [`get`] trusts an equal stamp, and checking there on every hit would make
/// per-block callers quadratic; the pass manager checks after each pass run.
#[cfg(debug_assertions)]
pub(crate) fn debug_check_stamp(func: &Function) {
    let key = func as *const Function as usize;
    let cached = CACHE.with(|cache| cache.borrow().get(&key).cloned());
    if let Some(analysis) = cached {
        assert!(
            analysis.stamp.get() != func.cfg_stamp() || analysis.matches(func),
            "cached CFG inputs changed without a new stamp"
        );
    }
}

/// Drops every cached analysis; the pipeline ends with this.
pub(crate) fn clear() {
    let entries = CACHE.with(|cache| std::mem::take(&mut *cache.borrow_mut()));
    drop(entries);
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_ir::{BlockCall, InsertPoint, InstData, Signature};

    #[test]
    fn appended_terminator_invalidates_cached_cfg() {
        clear();
        let mut function = Function::new("append", Signature::new(&[], &[]));
        let entry = function.create_block();
        let exit = function.create_block();
        let before = get(&function);
        assert!(before.cfg.succs(entry).is_empty());
        function.insert_inst(
            InsertPoint::End(entry),
            InstData::Jump {
                dest: BlockCall {
                    block: exit,
                    args: vec![],
                },
            },
        );
        let after = get(&function);
        assert_eq!(after.cfg.succs(entry), &[exit]);
        assert_eq!(after.cfg.preds(exit), &[entry]);
        clear();
    }
    fn switch_function(width: usize) -> Function {
        let cases = (0..width * 2)
            .map(|i| format!("{i}: case{}", 1 + i % width))
            .collect::<Vec<_>>()
            .join(", ");
        let mut source = format!(
            "func @switches(i64) {{\nentry(v0:i64):\n brif v0,left,right\nleft:\n switch v0,case1,[{cases}]\nright:\n switch v0,case{width},[{cases}]\n"
        );
        for i in 1..=width {
            source.push_str(&format!("case{i}:\n return\n"));
        }
        source.push_str("}\n");
        let mut module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        module.functions.remove(0)
    }

    #[test]
    fn duplicate_switch_targets_preserve_order_and_cache_validity() {
        for width in [2, 8, 9, 256] {
            clear();
            let mut function = switch_function(width);
            let left = function.blocks().nth(1).unwrap();
            let term = *function.block_insts(left).last().unwrap();
            let before = get(&function);
            assert!(before.matches(&function));
            assert!(before.same_as_fresh(&function));

            // Changing a duplicate occurrence leaves the ordered CFG intact.
            let dests = function.inst_data_mut(term).branch_dests_mut();
            dests[width + 1].block = dests[width].block;
            keel_ir::verify(&function).unwrap();
            assert!(before.matches(&function));
            assert!(Rc::ptr_eq(&before, &get(&function)));
            assert!(before.same_as_fresh(&function));

            // The destination set is unchanged, but first-occurrence order moves.
            function.inst_data_mut(term).branch_dests_mut().swap(0, 2);
            assert!(!before.matches(&function));
            let reordered = get(&function);
            assert!(!Rc::ptr_eq(&before, &reordered));
            assert!(reordered.same_as_fresh(&function));

            // Removing the final unique destination must also invalidate reuse.
            let dests = function.inst_data_mut(term).branch_dests_mut();
            let removed = *reordered.cfg.succs(left).last().unwrap();
            let replacement = reordered.cfg.succs(left)[0];
            for dest in dests {
                if dest.block == removed {
                    dest.block = replacement;
                }
            }
            assert!(!reordered.matches(&function));
            let reduced = get(&function);
            assert!(!Rc::ptr_eq(&reordered, &reduced));
            assert!(reduced.same_as_fresh(&function));

            // Reintroducing a target must reject the smaller cached successor set.
            function.inst_data_mut(term).branch_dests_mut()[width + 1].block = removed;
            assert!(!reduced.matches(&function));
            let expanded = get(&function);
            assert!(!Rc::ptr_eq(&reduced, &expanded));
            assert!(expanded.same_as_fresh(&function));
            clear();
        }
    }

    fn loop_function() -> Function {
        keel_ir::parse("func @loop(i32, i32) {\nentry(v0:i32,v1:i32):\n jump body(v0)\nbody(v2:i32):\n brif v0,body(v2),exit\nexit:\n return\n}")
            .unwrap().functions.remove(0)
    }

    #[test]
    fn unchanged_and_argument_only_edits_reuse_cfg_and_lazy_loops() {
        clear();
        let mut function = loop_function();
        let before = get(&function);
        let body = function.blocks().nth(1).unwrap();
        assert_eq!(before.loops(&function).depth(body), 1);
        assert!(Rc::ptr_eq(&before, &get(&function)));
        assert!(before.same_as_fresh(&function));
        let entry = function.entry_block().unwrap();
        let jump = function.block_insts(entry)[0];
        let other = function.block_params(entry)[1];
        let old_stamp = function.cfg_stamp();
        let InstData::Jump { dest } = function.inst_data_mut(jump) else {
            unreachable!()
        };
        dest.args[0] = other;
        assert_ne!(old_stamp, function.cfg_stamp());
        let after = get(&function);
        assert!(Rc::ptr_eq(&before, &after));
        assert!(std::ptr::eq(
            before.loops(&function),
            after.loops(&function)
        ));
        assert!(after.same_as_fresh(&function));
        clear();
    }

    #[test]
    fn successor_and_layout_edits_replace_cached_analyses() {
        clear();
        let mut function = loop_function();
        let before = get(&function);
        let body = function.blocks().nth(1).unwrap();
        let exit = function.blocks().nth(2).unwrap();
        assert_eq!(before.loops(&function).depth(body), 1);
        let branch = function.block_insts(body)[0];
        *function.inst_data_mut(branch) = InstData::Jump {
            dest: BlockCall {
                block: exit,
                args: vec![],
            },
        };
        let after = get(&function);
        assert!(!Rc::ptr_eq(&before, &after));
        assert_eq!(after.loops(&function).depth(body), 0);
        assert!(after.same_as_fresh(&function));
        let entry = function.entry_block().unwrap();
        let jump = function.block_insts(entry)[0];
        *function.inst_data_mut(jump) = InstData::Jump {
            dest: BlockCall {
                block: exit,
                args: vec![],
            },
        };
        function.remove_block(body);
        let removed = get(&function);
        assert!(!Rc::ptr_eq(&after, &removed));
        assert!(removed.same_as_fresh(&function));
        clear();
    }

    #[test]
    fn replacement_at_same_address_only_reuses_equivalent_cfg() {
        clear();
        let mut slot = Box::new(loop_function());
        let before = get(&slot);
        before.loops(&slot);
        // Unchanged clones share a stamp, which promises the same CFG inputs.
        *slot = slot.as_ref().clone();
        assert!(Rc::ptr_eq(&before, &get(&slot)));
        assert!(before.same_as_fresh(&slot));
        *slot = keel_ir::parse("func @replacement() {\nentry:\n return\n}")
            .unwrap()
            .functions
            .remove(0);
        let after = get(&slot);
        assert!(!Rc::ptr_eq(&before, &after));
        after.loops(&slot);
        assert!(after.same_as_fresh(&slot));
        clear();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "cached CFG inputs changed without a new stamp")]
    fn stamp_check_rejects_an_entry_whose_cfg_differs() {
        clear();
        let mut function = Function::new("stale", Signature::new(&[], &[]));
        function.create_block();
        let stale = get(&function);
        let entry = function.create_block();
        function.insert_inst(InsertPoint::End(entry), InstData::Return { args: vec![] });
        // Stand in for a CFG change that did not renew the stamp.
        stale.stamp.set(function.cfg_stamp());
        debug_check_stamp(&function);
    }
}
