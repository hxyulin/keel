//! Bounded availability of successful ordinary loads, without load motion.
//!
//! A reachable non-entry block inherits completed predecessor states, retaining
//! only identical values present on every incoming path. An unvisited
//! predecessor or backedge cannot supply facts. Each inherited value dominates its
//! replacement and was already successfully loaded on this execution. Siblings
//! receive separate snapshots. Block arguments are deliberately not translated.
//!
//! At most eight edges and 128 facts are carried, with no fixed point or path
//! exploration. Writes use the shared circular-footprint alias analysis; all
//! observable/lifetime barriers discard the state, including debugger edits.
//! Exact stores also publish their value, but do not establish readability.
//! Store-to-load forwarding requires a prior successful read of that exact
//! field, or an explicit notrap promise on the later load. Stores stay pinned.
use crate::memory_alias::{Address, AliasInfo};
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{EntityRef, Function, InstData, Type, Value, ValueDef};

pub struct LoadReuse;
impl crate::Pass for LoadReuse {
    fn name(&self) -> &'static str {
        "load-reuse"
    }
    fn description(&self) -> &'static str {
        "reuse successful reads and exact stored values across bounded CFG chains"
    }
    fn run(&self, func: &mut Function) -> bool {
        run(func)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

const MAX_EDGES: usize = 8;
const MAX_FACTS: usize = 128;
const MAX_PREDS: usize = 8;
#[derive(Clone, Copy)]
struct Fact {
    value: Value,
    addr: Value,
    readable: bool,
}
type Facts = FxHashMap<(Address, Type), Fact>;

fn incoming(
    func: &Function,
    cfg: &ControlFlowGraph,
    block: keel_ir::Block,
    exits: &[Option<(usize, Facts)>],
) -> (usize, Facts) {
    let preds = cfg.preds(block);
    if Some(block) == func.entry_block() || preds.is_empty() || preds.len() > MAX_PREDS {
        return (0, Facts::default());
    }
    let mut depth = 0;
    // Refuse the entire merge if any predecessor has not completed, including
    // loop backedges. Never run a fixed point or invent phi memory values.
    for pred in preds {
        let Some((pred_depth, _)) = &exits[pred.index()] else {
            return (0, Facts::default());
        };
        depth = depth.max(*pred_depth);
    }
    if depth >= MAX_EDGES {
        return (0, Facts::default());
    }
    let mut facts = exits[preds[0].index()].as_ref().unwrap().1.clone();
    for pred in &preds[1..] {
        let other = &exits[pred.index()].as_ref().unwrap().1;
        facts.retain(|key, fact| {
            if let Some(other) = other.get(key)
                && func.resolve(fact.value) == func.resolve(other.value)
            {
                fact.readable &= other.readable;
                true
            } else {
                false
            }
        });
    }
    (depth + 1, facts)
}

fn run(func: &mut Function) -> bool {
    // A debugger may edit source variable homes between source locations.
    // Avoid inferring availability anywhere in functions with such homes.
    if !func.debug_vars.is_empty() {
        return false;
    }
    let analysis = crate::analysis::get(func);
    let cfg = &analysis.cfg;
    let aliases = AliasInfo::compute(func);
    let mut exits: Vec<Option<(usize, Facts)>> = vec![None; func.num_blocks()];
    let mut changed = false;
    for &block in analysis.dom.rpo() {
        let (depth, mut facts) = incoming(func, cfg, block, &exits);
        // Only the visited load is removed; the next one moves into its place.
        let mut i = 0;
        while let Some(&inst) = func.block_insts(block).get(i) {
            i += 1;
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
                    if let Some(fact) = facts.get(&key)
                        && (fact.readable || flags.notrap)
                    {
                        func.replace_all_uses(result, func.resolve(fact.value));
                        func.remove_inst(inst);
                        i -= 1;
                        changed = true;
                    } else {
                        // Clearing at the cap is deterministic and keeps both
                        // snapshot copying and each store's alias scan bounded.
                        if facts.len() == MAX_FACTS {
                            facts.clear();
                        }
                        facts.insert(
                            key,
                            Fact {
                                value: result,
                                addr,
                                readable: true,
                            },
                        );
                    }
                }
                InstData::Store { value, addr, flags } if !flags.volatile => {
                    let value = func.resolve(value);
                    let ty = func.value_type(value);
                    let key = (aliases.address(func, addr), ty);
                    let readable = facts.get(&key).is_some_and(|fact| fact.readable);
                    facts.retain(
                        |&(_, other_ty), fact| match (ty.bytes(), other_ty.bytes()) {
                            (Some(bytes), Some(other_bytes)) => !aliases.may_alias(
                                func,
                                addr,
                                u64::from(bytes),
                                fact.addr,
                                u64::from(other_bytes),
                            ),
                            _ => false,
                        },
                    );
                    if facts.len() == MAX_FACTS {
                        facts.clear();
                    }
                    facts.insert(
                        key,
                        Fact {
                            value,
                            addr,
                            readable,
                        },
                    );
                }
                ref data if data.memory_effects().may_write() => facts.clear(),
                _ => {}
            }
        }
        exits[block.index()] = Some((depth, facts));
    }
    changed
}
