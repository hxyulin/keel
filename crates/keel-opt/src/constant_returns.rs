//! Propagate known return values across direct calls and remove trivial calls.

use crate::{ModuleContext, OptimizationContext, Pass};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InsertPoint, InstData, Module, Purpose, Type, ValueDef};

#[derive(Debug)]
pub(crate) struct ConstantReturns;

#[derive(Clone, Copy)]
enum Summary {
    Constant { ty: Type, imm: i64, trivial: bool },
    Parameter(usize),
}

impl Pass for ConstantReturns {
    fn name(&self) -> &'static str {
        "constant-returns"
    }

    fn description(&self) -> &'static str {
        "propagate constant and forwarded results of local calls and remove trivial calls"
    }

    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }

    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        let summaries: FxHashMap<_, _> = module
            .functions
            .iter()
            .filter_map(|f| Some((f.name.clone(), (f.sig.clone(), summarize(f)?))))
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
                    let (sig, summary) = summaries.get(&decl.name)?;
                    if decl.attrs.weak
                        || decl.attrs.noreturn
                        || decl.sig != *sig
                        || args.has_explicit_abi()
                    {
                        return None;
                    }
                    let result = f.first_result(inst);
                    if f.resolve(result) != result {
                        return None;
                    }
                    let arg = match *summary {
                        Summary::Constant { .. } => None,
                        Summary::Parameter(index) => Some(*args.get(index)?),
                    };
                    Some((inst, result, *summary, arg))
                })
                .collect();
            changed |= !sites.is_empty();
            context.changed.push(!sites.is_empty());
            for (inst, result, summary, arg) in sites {
                let (replacement, trivial) = match summary {
                    Summary::Constant { ty, imm, trivial } => {
                        let constant =
                            f.insert_inst(InsertPoint::Before(inst), InstData::Iconst { ty, imm });
                        (f.first_result(constant), trivial)
                    }
                    Summary::Parameter(_) => (arg.expect("matching argument"), true),
                };
                // Unless the whole body is trivial, keep its effects and
                // substitute only the value on its normal continuation.
                f.replace_all_uses(result, replacement);
                if trivial {
                    // Argument evaluation remains at the original call site.
                    f.remove_inst(inst);
                }
            }
        }
        Some(changed)
    }
}

fn summarize(f: &Function) -> Option<Summary> {
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
        || !matches!(
            result.ty,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
        )
        || result.purpose != Purpose::Normal
        || f.sig
            .params
            .iter()
            .any(|param| param.purpose != Purpose::Normal)
    {
        return None;
    }
    let entry = f.entry_block()?;
    let body = f.block_insts(entry);
    if let [ret] = body
        && f.blocks().count() == 1
        && let InstData::Return { args } = f.inst_data(*ret)
    {
        let result = f.resolve(args[0]);
        let index = f
            .block_params(entry)
            .iter()
            .position(|&param| param == result)?;
        return Some(Summary::Parameter(index));
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
    let (ty, imm) = constant?;
    let trivial = matches!(body, [a, b]
        if matches!(f.inst_data(*a), InstData::Iconst { .. })
            && matches!(f.inst_data(*b), InstData::Return { .. }))
        && f.blocks().count() == 1;
    Some(Summary::Constant { ty, imm, trivial })
}
