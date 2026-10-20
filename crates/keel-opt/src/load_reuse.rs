//! Bounded availability of successful ordinary loads, without load motion.
//!
//! A reachable non-entry block inherits only its unique predecessor's completed
//! state. RPO visits that predecessor first; an unvisited predecessor, merge or
//! backedge cannot supply facts. Each inherited value therefore dominates its
//! replacement and was already successfully loaded on this execution. Siblings
//! receive separate snapshots. Block arguments are deliberately not translated.
//!
//! At most eight edges and 128 facts are carried, with no fixed point or path
//! exploration. Writes use the shared circular-footprint alias analysis; all
//! observable/lifetime barriers discard the state, including debugger edits.
use crate::memory_alias::{Address, AliasInfo};
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{EntityRef, Function, InstData, Type, Value, ValueDef};
use std::collections::HashMap;

pub struct LoadReuse;
impl crate::Pass for LoadReuse {
    fn name(&self) -> &'static str {
        "load-reuse"
    }
    fn description(&self) -> &'static str {
        "reuse successful ordinary loads across bounded unique-predecessor chains (opt-in)"
    }
    fn run(&self, func: &mut Function) -> bool {
        run(func)
    }
}

const MAX_EDGES: usize = 8;
const MAX_FACTS: usize = 128;
type Facts = HashMap<(Address, Type), (Value, Value)>;

fn run(func: &mut Function) -> bool {
    // A debugger may edit source variable homes between source locations.
    // Avoid inferring availability anywhere in functions with such homes.
    if !func.debug_vars.is_empty() {
        return false;
    }
    let cfg = ControlFlowGraph::compute(func);
    let aliases = AliasInfo::compute(func);
    let mut exits: Vec<Option<(usize, Facts)>> = vec![None; func.num_blocks()];
    let mut changed = false;
    for block in cfg.reverse_postorder(func) {
        let (depth, mut facts) = match cfg.preds(block) {
            [pred] if Some(block) != func.entry_block() => exits[pred.index()]
                .as_ref()
                .filter(|(depth, _)| *depth < MAX_EDGES)
                .map(|(depth, facts)| (depth + 1, facts.clone()))
                .unwrap_or_default(),
            _ => (0, Facts::new()),
        };
        for inst in func.block_insts(block).to_vec() {
            match *func.inst_data(inst) {
                InstData::Load { ty, addr, flags } if !flags.volatile => {
                    let key = (aliases.address(func, addr), ty);
                    let result = func.first_result(inst);
                    // Replacements leave the observable read in layout. Its
                    // redirected old result is neither a memory fact nor a
                    // result that this pass may redirect again.
                    if !matches!(func.value_def(result), ValueDef::Result(owner, 0) if owner == inst)
                    {
                        continue;
                    }
                    if let Some(&(value, _)) = facts.get(&key) {
                        func.replace_all_uses(result, func.resolve(value));
                        func.remove_inst(inst);
                        changed = true;
                    } else {
                        // Clearing at the cap is deterministic and keeps both
                        // snapshot copying and each store's alias scan bounded.
                        if facts.len() == MAX_FACTS {
                            facts.clear();
                        }
                        facts.insert(key, (result, addr));
                    }
                }
                InstData::Store { value, addr, flags } if !flags.volatile => {
                    let ty = func.value_type(func.resolve(value));
                    facts.retain(|&(_, other_ty), &mut (_, other)| {
                        match (ty.bytes(), other_ty.bytes()) {
                            (Some(bytes), Some(other_bytes)) => !aliases.may_alias(
                                func,
                                addr,
                                u64::from(bytes),
                                other,
                                u64::from(other_bytes),
                            ),
                            _ => false,
                        }
                    });
                }
                ref data if data.memory_effects().may_write() => facts.clear(),
                _ => {}
            }
        }
        exits[block.index()] = Some((depth, facts));
    }
    changed
}
