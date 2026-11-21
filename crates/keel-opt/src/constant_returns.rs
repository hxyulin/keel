//! Propagate known return values across direct calls while preserving call effects.

use crate::{ModuleContext, OptimizationContext, Pass};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InsertPoint, InstData, Module, Purpose, Type, ValueDef};

#[derive(Debug)]
pub(crate) struct ConstantReturns;

impl Pass for ConstantReturns {
    fn name(&self) -> &'static str {
        "constant-returns"
    }

    fn description(&self) -> &'static str {
        "propagate uniform integer results of local calls while keeping their effects"
    }

    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }

    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        let summaries: FxHashMap<_, _> = module
            .functions
            .iter()
            .filter_map(|f| {
                let value = constant_return(f)?;
                Some((f.name.clone(), (f.sig.clone(), value)))
            })
            .collect();
        if summaries.is_empty() {
            return Some(false);
        }

        let mut changed = false;
        for f in &mut module.functions {
            if !context.enabled(f) {
                context.changed.push(false);
                continue;
            }
            let sites: Vec<_> = f
                .layout_insts()
                .filter_map(|(_, inst)| {
                    let InstData::Call { func, args } = f.inst_data(inst) else {
                        return None;
                    };
                    let decl = f.callee(*func);
                    let (sig, (ty, imm)) = summaries.get(&decl.name)?;
                    if decl.attrs.weak
                        || decl.attrs.noreturn
                        || decl.sig != *sig
                        || args.has_explicit_abi()
                    {
                        return None;
                    }
                    let result = f.first_result(inst);
                    (f.resolve(result) == result).then_some((inst, result, *ty, *imm))
                })
                .collect();
            changed |= !sites.is_empty();
            context.changed.push(!sites.is_empty());
            for (inst, result, ty, imm) in sites {
                let constant =
                    f.insert_inst(InsertPoint::Before(inst), InstData::Iconst { ty, imm });
                // The call may write memory, fault or never return. Substitute
                // only the value observed on its normal continuation.
                f.replace_all_uses(result, f.first_result(constant));
            }
        }
        Some(changed)
    }
}

fn constant_return(f: &Function) -> Option<(Type, i64)> {
    let [result] = f.sig.returns.as_slice() else {
        return None;
    };
    if !f.local
        || f.attrs.weak
        || f.weak_odr
        || f.attrs.naked
        || f.attrs.interrupt
        || f.attrs.noreturn
        || f.sig.variadic
        || f.sig.returns_twice
        || !matches!(result.ty, Type::I8 | Type::I16 | Type::I32 | Type::I64)
        || f.sig
            .params
            .iter()
            .any(|param| param.purpose != Purpose::Normal)
    {
        return None;
    }

    let mut constant = None;
    for (_, inst) in f.layout_insts() {
        match f.inst_data(inst) {
            // These instructions can return without a normal IR Return.
            InstData::InlineAsm { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. } => return None,
            InstData::Return { args } => {
                let ValueDef::Result(def, 0) = f.value_def(f.resolve(args[0])) else {
                    return None;
                };
                let InstData::Iconst { ty, imm } = *f.inst_data(def) else {
                    return None;
                };
                let value = (ty, imm);
                if constant.is_some_and(|previous| previous != value) {
                    return None;
                }
                constant = Some(value);
            }
            _ => {}
        }
    }
    constant
}
