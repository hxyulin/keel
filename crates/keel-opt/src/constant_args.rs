//! Propagate literal arguments common to every direct call of a contained body.

use crate::Pass;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InsertPoint, InstData, Module, Purpose, Type, ValueDef};

pub struct ConstantArgs;
impl Pass for ConstantArgs {
    fn name(&self) -> &'static str {
        "constant-args"
    }
    fn description(&self) -> &'static str {
        "propagate common literal arguments into nonescaping local definitions"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module, &|_| true))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        _sites: Option<&mut Vec<crate::InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let result = run(module, enabled);
        changed.clear();
        changed.extend(module.functions.iter().map(|f| result && enabled(f)));
        Some(result)
    }
}

fn run(module: &mut Module, enabled: &dyn Fn(&Function) -> bool) -> bool {
    let roots = crate::assembly_symbols::roots(module);
    let candidates: FxHashMap<_, _> = module
        .functions
        .iter()
        .filter(|f| {
            enabled(f)
                && f.local
                && !f.attrs.weak
                && !f.weak_odr
                && !f.sig.variadic
                && !f.sig.returns_twice
                && !roots.contains(&f.name)
                && f.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        })
        .map(|f| (f.name.as_str(), &f.sig))
        .collect();
    let mut common: FxHashMap<String, Vec<Option<InstData>>> = FxHashMap::default();
    for f in &module.functions {
        for (_, inst) in f.layout_insts() {
            let (callee, args, explicit) = match f.inst_data(inst) {
                InstData::Call { func, args } | InstData::Invoke { func, args, .. } => {
                    (*func, &args[..], args.has_explicit_abi())
                }
                InstData::ReturnCall { func, args } => (*func, &args[..], false),
                _ => continue,
            };
            let decl = f.callee(callee);
            let Some(sig) = candidates.get(decl.name.as_str()) else {
                continue;
            };
            let valid = !explicit && *sig == &decl.sig && args.len() == sig.params.len();
            let constants: Vec<_> = sig
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if !valid || !(p.ty.is_int() && p.ty != Type::I128 || p.ty.is_float()) {
                        return None;
                    }
                    let value = f.resolve(args[i]);
                    let ValueDef::Result(inst, _) = f.value_def(value) else {
                        return None;
                    };
                    match f.inst_data(inst) {
                        data @ (InstData::Iconst { .. } | InstData::Fconst { .. }) => {
                            Some(data.clone())
                        }
                        _ => None,
                    }
                })
                .collect();
            common
                .entry(decl.name.clone())
                .and_modify(|prior| {
                    for (old, new) in prior.iter_mut().zip(&constants) {
                        if old != new {
                            *old = None;
                        }
                    }
                })
                .or_insert(constants);
        }
    }
    let mut changed = false;
    for f in &mut module.functions {
        let Some(constants) = common.get(&f.name) else {
            continue;
        };
        let Some(entry) = f.entry_block() else {
            continue;
        };
        let params = f.block_params(entry).to_vec();
        let Some(&first) = f.block_insts(entry).first() else {
            continue;
        };
        for (param, constant) in params.into_iter().zip(constants) {
            let Some(constant) = constant else {
                continue;
            };
            if f.resolve(param) != param
                || !f.layout_insts().any(|(_, i)| {
                    let mut used = false;
                    f.inst_data(i)
                        .for_each_operand(|v| used |= f.resolve(v) == param);
                    used
                })
            {
                continue;
            }
            let literal = f.insert_inst(InsertPoint::Before(first), constant.clone());
            f.replace_all_uses(param, f.first_result(literal));
            changed = true;
        }
    }
    changed
}
