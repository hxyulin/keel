//! Canonical-address, same-type memory forwarding within a basic block.
//!
//! Successful ordinary accesses establish facts until an overlapping write or
//! barrier. Checked constant ptradd paths share canonical keys. Store retention
//! uses circular byte footprints and eligible definition-only restrict roots;
//! no address comparison, notrap or alignment annotation proves non-aliasing.

use crate::memory_alias::Address;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InstData, Type, Value};

pub fn run(func: &mut Function) -> bool {
    let aliases = crate::memory_alias::AliasInfo::compute(func);
    let mut changed = false;
    let mut available: FxHashMap<(Address, Type), (Value, Value)> = FxHashMap::default();
    let blocks: Vec<_> = func.blocks().collect();
    for block in blocks {
        available.clear();
        for inst in func.block_insts(block).to_vec() {
            match *func.inst_data(inst) {
                InstData::Load { ty, addr, flags } if !flags.volatile => {
                    let key = (aliases.address(func, addr), ty);
                    let result = func.inst_results(inst)[0];
                    if let Some(&(value, _)) = available.get(&key) {
                        func.replace_all_uses(result, func.resolve(value));
                        func.remove_inst(inst);
                        changed = true;
                    } else {
                        available.insert(key, (result, addr));
                    }
                }
                InstData::Store { value, addr, flags } if !flags.volatile => {
                    let addr = func.resolve(addr);
                    let value = func.resolve(value);
                    let ty = func.value_type(value);
                    available.retain(|&(_, other_ty), &mut (_, other)| {
                        match (ty.bytes(), other_ty.bytes()) {
                            (Some(bytes), Some(other_bytes)) => !aliases.may_alias(
                                func,
                                addr,
                                u64::from(bytes),
                                other,
                                u64::from(other_bytes),
                            ),
                            _ => false, // target-dependent pointer widths stay conservative
                        }
                    });
                    available.insert((aliases.address(func, addr), ty), (value, addr));
                }
                ref data if data.memory_effects().may_write() => available.clear(),
                _ => {}
            }
        }
    }
    changed
}
