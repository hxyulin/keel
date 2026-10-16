//! Exact-address, same-type memory forwarding within a basic block.
//!
//! A successful ordinary access establishes the bytes at that address until
//! a write or lifetime barrier. No alias disambiguation or speculative load
//! is needed. Distinct directly accessed restricted entry parameters also
//! disambiguate a store, under the definition-only C access contract.
//! Otherwise every write clears all facts, then an ordinary store establishes
//! just its own value. Volatile accesses never establish reusable facts.

use keel_ir::{Function, InstData, Type, Value};
use std::collections::HashMap;

pub fn run(func: &mut Function) -> bool {
    // No derivation/provenance analysis: any non-address use, escape, volatile
    // or atomic use disqualifies a root for this entire invocation. Comparing
    // pointer values is deliberately not an alias proof.
    let mut roots = std::collections::HashSet::new();
    if let Some(entry) = func.entry_block() {
        for &index in &func.restrict_params {
            if let Some(&value) = func.block_params(entry).get(index as usize) {
                roots.insert(func.resolve(value));
            }
        }
    }
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        match data {
            InstData::Load { flags, .. } if !flags.volatile => {}
            InstData::Store { value, flags, .. } if !flags.volatile => {
                roots.remove(&func.resolve(*value));
            }
            _ => data.for_each_operand(|v| {
                roots.remove(&func.resolve(v));
            }),
        }
    }
    let mut changed = false;
    let mut available: HashMap<(Value, Type), Value> = HashMap::new();
    let blocks: Vec<_> = func.blocks().collect();
    for block in blocks {
        available.clear();
        for inst in func.block_insts(block).to_vec() {
            match *func.inst_data(inst) {
                InstData::Load { ty, addr, flags } if !flags.volatile => {
                    let key = (func.resolve(addr), ty);
                    let result = func.inst_results(inst)[0];
                    if let Some(&value) = available.get(&key) {
                        func.replace_all_uses(result, func.resolve(value));
                        func.remove_inst(inst);
                        changed = true;
                    } else {
                        available.insert(key, result);
                    }
                }
                InstData::Store { value, addr, flags } if !flags.volatile => {
                    let addr = func.resolve(addr);
                    available.retain(|&(other, _), _| {
                        other != addr && roots.contains(&addr) && roots.contains(&other)
                    });
                    let value = func.resolve(value);
                    available.insert((func.resolve(addr), func.value_type(value)), value);
                }
                ref data if data.memory_effects().may_write() => available.clear(),
                _ => {}
            }
        }
    }
    changed
}
