//! Share a zero-flags copy of a recursive interpreter's dispatch body.
//! The original entry remains available to callers with a dynamic flag word.
use crate::ir_match::{def, int_const};
use crate::{ModuleContext, OptimizationContext, Pass, VectorTarget};
use keel_ir::{
    BinaryOp, CallConv, FuncDecl, Function, InsertPoint, Inst, InstData, Intrinsic, Module,
    Purpose, Type,
};

pub struct SpecializeZeroFlags;

impl Pass for SpecializeZeroFlags {
    fn name(&self) -> &'static str {
        "specialize-zero-flags"
    }

    fn description(&self) -> &'static str {
        "share a constant-zero flags version of a recursive dispatch loop"
    }

    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }

    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        Some(context.target == VectorTarget::X86Sse2 && run(module, context))
    }
}

fn recursive_dispatch(f: &Function) -> bool {
    if !f.local
        || f.attrs.weak
        || f.weak_odr
        || f.sig.cc != CallConv::C
        || f.sig.returns_twice
        || f.sig.variadic
        || f.personality.is_some()
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || f.sig
            .params
            .last()
            .is_none_or(|p| p.ty != Type::I32 || p.purpose != Purpose::Normal)
        || f.blocks().count() < 128
        || f.layout_insts().count() > 16_384
    {
        return false;
    }
    let Some(entry) = f.entry_block() else {
        return false;
    };
    let Some(&flags) = f.block_params(entry).last() else {
        return false;
    };
    if f.resolve(flags) != flags {
        return false;
    }
    let mut switches = 0;
    let mut zero_recursions = 0;
    let mut guarded = false;
    for (index, (_, inst)) in f.layout_insts().enumerate() {
        match f.inst_data(inst) {
            InstData::Switch { cases, .. } if cases.len() >= 24 => switches += 1,
            InstData::InlineAsm { .. }
            | InstData::Intrinsic {
                id: Intrinsic::FrameAddress,
                ..
            }
            | InstData::Invoke { .. } => return false,
            InstData::Call { func, args }
                if f.callee(*func).name == f.name
                    && f.callee(*func).sig == f.sig
                    && !args.has_explicit_abi()
                    && args.last().is_some_and(|&v| int_const(f, v) == Some(0)) =>
            {
                zero_recursions += 1;
            }
            InstData::Brif { cond, .. } if index < 256 => {
                if let Some(inst) = def(f, *cond)
                    && let InstData::Binary {
                        op: BinaryOp::And,
                        args,
                    } = f.inst_data(inst)
                {
                    guarded |=
                        [(args[0], args[1]), (args[1], args[0])]
                            .into_iter()
                            .any(|(flag, mask)| {
                                f.resolve(flag) == flags
                                    && int_const(f, mask)
                                        .is_some_and(|mask| (mask as u64).is_power_of_two())
                            });
                }
            }
            _ => {}
        }
    }
    switches > 0 && zero_recursions >= 2 && guarded
}

fn call_sites(caller: &Function, callee: &Function) -> (Vec<Inst>, bool) {
    let mut zero = Vec::new();
    let mut other = false;
    for (_, inst) in caller.layout_insts() {
        let InstData::Call { func, args } = caller.inst_data(inst) else {
            continue;
        };
        let called = caller.callee(*func);
        if called.name != callee.name || called.sig != callee.sig {
            continue;
        }
        if !args.has_explicit_abi()
            && args
                .last()
                .is_some_and(|&arg| int_const(caller, arg) == Some(0))
        {
            zero.push(inst);
        } else {
            other = true;
        }
    }
    (zero, other)
}

fn redirect(caller: &mut Function, callee: &FuncDecl, calls: &[Inst]) {
    if calls.is_empty() {
        return;
    }
    let reference = caller.import_function(&callee.name, callee.sig.clone());
    caller.set_callee_attrs(reference, callee.attrs.clone());
    for &site in calls {
        let InstData::Call { func, .. } = caller.inst_data_mut(site) else {
            unreachable!()
        };
        *func = reference;
    }
}

fn run(module: &mut Module, context: &mut ModuleContext<'_>) -> bool {
    if !module.assembly.is_empty() {
        return false;
    }
    let Some(index) = module
        .functions
        .iter()
        .position(|f| context.enabled(f) && recursive_dispatch(f))
    else {
        return false;
    };
    let original = &module.functions[index];
    let name = format!("{}.keel.flags0", original.name);
    if module.functions.iter().any(|f| f.name == name)
        || module
            .functions
            .iter()
            .any(|f| f.callees().any(|(_, d)| d.name == name))
        || module.decls.iter().any(|f| f.name == name)
        || module.data.iter().any(|d| d.name == name)
    {
        return false;
    }
    let calls: Vec<_> = module
        .functions
        .iter()
        .map(|f| {
            if context.enabled(f) {
                call_sites(f, original)
            } else {
                (Vec::new(), false)
            }
        })
        .collect();
    if !calls.iter().any(|(_, other)| *other) || calls[index].0.is_empty() {
        return false;
    }

    let mut cloned = original.clone();
    cloned.name = name;
    if !context.enabled(&cloned) {
        return false;
    }
    let entry = cloned.entry_block().unwrap();
    let flags = *cloned.block_params(entry).last().unwrap();
    let first = cloned.block_insts(entry)[0];
    let constant = cloned.insert_inst(InsertPoint::Before(first), InstData::iconst(Type::I32, 0));
    let literal = cloned.first_result(constant);
    for (_, inst) in original.layout_insts() {
        cloned.inst_data_mut(inst).for_each_operand_mut(|value| {
            if original.resolve(*value) == flags {
                *value = literal;
            }
        });
    }
    let clone_entry = FuncDecl {
        name: cloned.name.clone(),
        sig: cloned.sig.clone(),
        attrs: cloned.attrs.clone(),
    };
    redirect(&mut cloned, &clone_entry, &calls[index].0);
    for (caller, (sites, _)) in module.functions.iter_mut().zip(calls) {
        redirect(caller, &clone_entry, &sites);
    }
    module.functions.insert(index + 1, cloned);
    true
}
