//! Exact-address, same-type memory forwarding within a basic block.
//!
//! A successful ordinary access establishes the bytes at that address until
//! a write or lifetime barrier. No alias disambiguation or speculative load
//! is needed: every write clears all facts, then an ordinary store establishes
//! just its own value. Volatile accesses never establish reusable facts.

use keel_ir::{Function, InstData, Type, Value};
use std::collections::HashMap;

pub fn run(func: &mut Function) -> bool {
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
                    available.clear();
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
