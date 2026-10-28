//! Canonical-address, same-type memory forwarding within a basic block.
//!
//! Successful ordinary accesses establish facts until an overlapping write or
//! barrier. Checked constant ptradd paths share canonical keys. Store retention
//! uses circular byte footprints and eligible definition-only restrict roots;
//! no address comparison, notrap or alignment annotation proves non-aliasing.

use crate::memory_alias::Address;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InstData, Type, Value, ValueDef};

pub fn run(func: &mut Function) -> bool {
    // Debugger edits to variable homes must remain visible to later reads.
    if !func.debug_vars.is_empty() {
        return false;
    }
    forward(func, Forwarding::RuntimeLoads)
}

/// Mandatory immediate operands are compile-time constants even with debug
/// variable homes. Resolve only those asm inputs; keep every runtime read and
/// its SSA result intact so debugger edits remain visible to ordinary uses.
pub(crate) fn finalize_asm_immediates(func: &mut Function) -> bool {
    forward(func, Forwarding::AsmImmediates)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Forwarding {
    RuntimeLoads,
    AsmImmediates,
}

fn forward(func: &mut Function, mode: Forwarding) -> bool {
    let aliases = crate::memory_alias::AliasInfo::compute(func);
    let mut changed = false;
    let mut available: FxHashMap<(Address, Type), (Value, Value)> = FxHashMap::default();
    let mut values: FxHashMap<Value, Value> = FxHashMap::default();
    let blocks: Vec<_> = func.blocks().collect();
    for block in blocks {
        available.clear();
        values.clear();
        for inst in func.block_insts(block).to_vec() {
            match *func.inst_data(inst) {
                InstData::Load { ty, addr, flags } if !flags.volatile => {
                    let key = (aliases.address(func, addr), ty);
                    let result = func.inst_results(inst)[0];
                    // Replacements retain observable reads in layout. Their
                    // redirected results are neither facts nor rewrite targets.
                    if !matches!(func.value_def(result), ValueDef::Result(owner, 0) if owner == inst)
                    {
                        continue;
                    }
                    if let Some(&(value, _)) = available.get(&key) {
                        let value = func.resolve(value);
                        if mode == Forwarding::RuntimeLoads {
                            func.replace_all_uses(result, value);
                            func.remove_inst(inst);
                            changed = true;
                        } else {
                            values.insert(result, value);
                        }
                    } else {
                        available.insert(key, (result, addr));
                    }
                }
                InstData::Store { value, addr, flags } if !flags.volatile => {
                    let addr = func.resolve(addr);
                    let value = func.resolve(value);
                    let value = values.get(&value).copied().unwrap_or(value);
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
                InstData::InlineAsm { ref asm, ref args } if mode == Forwarding::AsmImmediates => {
                    let inputs: Vec<_> = asm
                        .inputs
                        .iter()
                        .copied()
                        .zip(args.iter().copied())
                        .enumerate()
                        .filter(|(_, (operand, _))| {
                            matches!(
                                operand,
                                keel_ir::AsmOperand::Imm | keel_ir::AsmOperand::NumericImm
                            )
                        })
                        .collect();
                    let mut replacements = Vec::new();
                    for (index, (operand, argument)) in inputs {
                        let Some(value) =
                            asm_constant(func, argument, inst, &mut values, 0, &mut changed)
                        else {
                            continue;
                        };
                        let numeric = matches!(
                            crate::const_fold::constant_ref(func, value),
                            Some(InstData::Iconst { .. })
                        );
                        if func.resolve(argument) != value
                            && (numeric
                                || operand == keel_ir::AsmOperand::Imm
                                    && keel_ir::symbolic_immediate(func, value).is_some())
                        {
                            replacements.push((index, value));
                        }
                    }
                    let writes = func.inst_data(inst).memory_effects().may_write();
                    if let InstData::InlineAsm { args, .. } = func.inst_data_mut(inst) {
                        for (index, value) in replacements {
                            args[index] = value;
                            changed = true;
                        }
                    }
                    if writes {
                        available.clear();
                    }
                }
                ref data if data.memory_effects().may_write() => available.clear(),
                _ => {}
            }
        }
    }
    changed
}

/// Rematerialize only pure compile-time expressions consumed by a mandatory
/// immediate. The original instructions and ordinary uses remain unchanged.
fn asm_constant(
    func: &mut Function,
    value: Value,
    before: keel_ir::Inst,
    values: &mut FxHashMap<Value, Value>,
    depth: usize,
    changed: &mut bool,
) -> Option<Value> {
    let value = func.resolve(value);
    let known = values.get(&value).copied().unwrap_or(value);
    if matches!(
        crate::const_fold::constant_ref(func, known),
        Some(InstData::Iconst { .. })
    ) || keel_ir::symbolic_immediate(func, known).is_some()
    {
        return Some(known);
    }
    if depth == 64 {
        return None;
    }
    let ValueDef::Result(def, 0) = func.value_def(known) else {
        return None;
    };
    let mut data = func.inst_data(def).clone();
    if !matches!(
        data,
        InstData::Binary { .. } | InstData::Unary { .. } | InstData::Convert { .. }
    ) {
        return None;
    }
    let mut complete = true;
    data.for_each_operand_mut(|operand| {
        if let Some(constant) = asm_constant(func, *operand, before, values, depth + 1, changed) {
            *operand = constant;
        } else {
            complete = false;
        }
    });
    if !complete {
        return None;
    }
    let data = if let Some(constant @ InstData::Iconst { .. }) =
        crate::const_fold::fold(&data, |v| crate::const_fold::constant_ref(func, v))
    {
        constant
    } else if matches!(
        data,
        InstData::Binary {
            op: keel_ir::BinaryOp::Ptradd | keel_ir::BinaryOp::Iadd | keel_ir::BinaryOp::Isub,
            ..
        } | InstData::Convert {
            op: keel_ir::ConvertOp::PtrToInt | keel_ir::ConvertOp::IntToPtr,
            ..
        }
    ) {
        data
    } else {
        return None;
    };
    let materialized = func.insert_inst(keel_ir::InsertPoint::Before(before), data);
    *changed = true;
    func.set_inst_loc(materialized, func.inst_loc(def));
    let constant = func.first_result(materialized);
    if !matches!(
        crate::const_fold::constant_ref(func, constant),
        Some(InstData::Iconst { .. })
    ) && keel_ir::symbolic_immediate(func, constant).is_none()
    {
        func.remove_inst(materialized);
        return None;
    }
    values.insert(value, constant);
    Some(constant)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "dtype dt0 = base \"long\" signed 8
func @f() -> i64 {
    ss0 = stack_slot 8, align 8
    dvar \"x\" dt0 ss0
block0:
    v0 = iconst.i64 42
    v1 = stack_addr ss0
    store v0, v1
    v2 = load.i64 v1
    v3 = asm \"movq %1,%0\", \"=r,n\"(v2) -> i64
    return v2
}";

    #[test]
    fn mandatory_asm_constants_leave_debugger_home_reads_and_runtime_uses_intact() {
        let mut module = keel_ir::parse(SOURCE).unwrap();
        assert!(keel_ir::verify_module(&module).is_err());
        let function = &mut module.functions[0];
        assert!(finalize_asm_immediates(function));
        let asm = function
            .layout_insts()
            .find_map(|(_, inst)| {
                if let InstData::InlineAsm { args, .. } = function.inst_data(inst) {
                    Some(args[0])
                } else {
                    None
                }
            })
            .unwrap();
        assert!(matches!(
            crate::const_fold::constant_ref(function, asm),
            Some(InstData::Iconst { imm: 42, .. })
        ));
        let load = function
            .layout_insts()
            .find_map(|(_, inst)| {
                matches!(function.inst_data(inst), InstData::Load { .. }).then_some(inst)
            })
            .unwrap();
        let result = function.first_result(load);
        assert!(matches!(function.value_def(result), ValueDef::Result(owner, 0) if owner == load));
        let returned = function
            .layout_insts()
            .find_map(|(_, inst)| {
                if let InstData::Return { args } = function.inst_data(inst) {
                    Some(args[0])
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(returned, result);
        assert!(!run(function));
        assert!(!finalize_asm_immediates(function));
        keel_ir::verify_module(&module).unwrap();
    }

    #[test]
    fn mandatory_asm_constants_respect_memory_barriers_and_register_operands() {
        let barrier = SOURCE
            .replace("func @f", "decl @clobber()\nfunc @f")
            .replace("v2 = load", "call @clobber()\n    v2 = load");
        let register = SOURCE.replace("=r,n", "=r,r");
        for source in [barrier, register] {
            let mut module = keel_ir::parse(&source).unwrap();
            let before = module.to_string();
            assert!(!finalize_asm_immediates(&mut module.functions[0]));
            assert_eq!(module.to_string(), before);
        }
    }
}
