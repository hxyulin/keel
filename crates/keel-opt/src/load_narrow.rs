//! Opt-in, explicitly little-endian narrowing of fault-free private stack reads.
//!
//! Generic pointer loads are deliberately unsupported: an ordinary load may
//! fault, and reducing its width can suppress that fault. See docs/load-narrow.md.

use crate::{Pass, VectorTarget};
use keel_ir::{BinaryOp, ConvertOp, Function, InsertPoint, InstData, Type, Value, ValueDef};
use std::collections::HashMap;

pub(crate) struct LoadNarrow;

impl Pass for LoadNarrow {
    fn name(&self) -> &'static str {
        "load-narrow"
    }

    fn description(&self) -> &'static str {
        "narrow fault-free private stack loads masked to a byte on explicit little-endian targets"
    }

    fn run(&self, _func: &mut Function) -> bool {
        // The target-independent entry point supplies no endianness contract.
        false
    }

    fn run_for_target(&self, func: &mut Function, target: VectorTarget) -> bool {
        match target {
            VectorTarget::Aarch64Neon | VectorTarget::X86Sse2 => run_little_endian(func),
            VectorTarget::Portable => false,
        }
    }
}

fn constant(func: &Function, value: Value) -> Option<i64> {
    let value = func.resolve(value);
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    if func.is_removed(inst) || func.first_result(inst) != value {
        return None;
    }
    match func.inst_data(inst) {
        InstData::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}

fn run_little_endian(func: &mut Function) -> bool {
    // Debug homes can be edited externally; do not reason about their privacy.
    if !func.debug_vars.is_empty() {
        return false;
    }
    let instructions: Vec<_> = func.layout_insts().map(|(_, inst)| inst).collect();
    // Bounded initial scope: no calls, atomics, inline asm, frame inspection,
    // dynamic stack operations, or volatile accesses anywhere in the function.
    if instructions.iter().any(|&inst| {
        let data = func.inst_data(inst);
        (!matches!(data, InstData::Load { .. } | InstData::Store { .. })
            && data.memory_effects() != keel_ir::MemoryEffects::None)
            || matches!(data, InstData::Load { flags, .. } | InstData::Store { flags, .. }
                if flags.volatile)
            || matches!(data, InstData::Intrinsic { .. })
    }) {
        return false;
    }

    // Map only owning result keys, never a resolved alias of a dead definition.
    // Each value records bytes remaining in its fixed allocation, so additions
    // are checked without relying on pointer wraparound or dereferenceability.
    let sizes: HashMap<_, _> = func.stack_slots().map(|(s, d)| (s, d.size)).collect();
    let mut addresses = HashMap::<Value, u32>::new();
    for &inst in &instructions {
        if let InstData::StackAddr { slot } = func.inst_data(inst) {
            let value = func.first_result(inst);
            if func.resolve(value) == value {
                addresses.insert(value, sizes[slot]);
            }
        }
    }
    loop {
        let before = addresses.len();
        for &inst in &instructions {
            if let InstData::Binary { op: BinaryOp::Ptradd, args } = *func.inst_data(inst)
                && let Some(&remaining) = addresses.get(&func.resolve(args[0]))
                && let Some(offset) = constant(func, args[1]).and_then(|n| u32::try_from(n).ok())
                && let Some(remaining) = remaining.checked_sub(offset)
            {
                let value = func.first_result(inst);
                if func.resolve(value) == value {
                    addresses.insert(value, remaining);
                }
            }
        }
        if addresses.len() == before {
            break;
        }
    }

    // Every memory address must be a proven in-bounds fixed-slot address.
    // Every use of such an address must be a plain access or a checked ptradd.
    // Thus there are no escaped, device, atomic, or concurrently shared objects.
    for &inst in &instructions {
        let data = func.inst_data(inst);
        let access = match data {
            InstData::Load { addr, ty, .. } => Some((*addr, *ty)),
            InstData::Store { addr, value, .. } => Some((*addr, func.value_type(*value))),
            _ => None,
        };
        if let Some((addr, ty)) = access {
            let Some(&remaining) = addresses.get(&func.resolve(addr)) else {
                return false;
            };
            if ty.bytes().is_none_or(|width| width > remaining) {
                return false;
            }
        }
        let mut escaped = false;
        data.for_each_operand(|value| {
            let value = func.resolve(value);
            if !addresses.contains_key(&value) {
                return;
            }
            let allowed = match data {
                InstData::Load { addr, .. } => func.resolve(*addr) == value,
                InstData::Store { addr, value: stored, .. } => {
                    func.resolve(*addr) == value && func.resolve(*stored) != value
                }
                InstData::Binary { op: BinaryOp::Ptradd, args } => {
                    func.resolve(args[0]) == value
                        && addresses.contains_key(&func.first_result(inst))
                }
                _ => false,
            };
            escaped |= !allowed;
        });
        if escaped {
            return false;
        }
    }

    // Count resolved operands, including operands in alias-bearing instructions
    // still in layout. Overcounting those dead uses is deliberately conservative.
    let mut uses = HashMap::<Value, usize>::new();
    for &inst in &instructions {
        func.inst_data(inst).for_each_operand(|value| {
            *uses.entry(func.resolve(value)).or_default() += 1;
        });
    }
    let mut changed = false;
    for inst in instructions {
        let InstData::Binary { op: BinaryOp::And, args } = *func.inst_data(inst) else {
            continue;
        };
        let result = func.first_result(inst);
        if func.resolve(result) != result || func.value_type(result) != Type::I64 {
            continue;
        }
        let value = if constant(func, args[1]) == Some(255) {
            func.resolve(args[0])
        } else if constant(func, args[0]) == Some(255) {
            func.resolve(args[1])
        } else {
            continue;
        };
        let ValueDef::Result(load, 0) = func.value_def(value) else {
            continue;
        };
        if func.is_removed(load)
            || func.first_result(load) != value
            || uses.get(&value) != Some(&1)
        {
            continue;
        }
        let InstData::Load { flags, ty: Type::I64, addr } = *func.inst_data(load) else {
            continue;
        };
        if flags.volatile || !flags.notrap {
            continue;
        }
        // Same address, same alignment/flags, same execution position/location.
        // Do not mutate Load's result type in place: Function forbids that.
        let narrow = func.insert_inst(InsertPoint::Before(load), InstData::Load {
            flags, ty: Type::I8, addr,
        });
        func.set_inst_loc(narrow, func.inst_loc(load));
        *func.inst_data_mut(inst) = InstData::Convert {
            op: ConvertOp::Uext, ty: Type::I64, arg: func.first_result(narrow),
        };
        func.remove_inst(load);
        changed = true;
    }
    changed
}

#[cfg(test)]
#[path = "../tests/load_narrow/mod.rs"]
mod tests;
