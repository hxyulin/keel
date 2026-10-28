//! Remove proven empty calls and functions made unreachable by optimization.

use crate::Pass;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{CallConv, FuncDecl, Function, InstData, Module, Purpose};

pub struct EmptyCalls;
impl Pass for EmptyCalls {
    fn name(&self) -> &'static str {
        "empty-calls"
    }
    fn description(&self) -> &'static str {
        "remove void calls to bound definitions consisting only of a return"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(remove_empty_calls(module, &|_| true))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        _sites: Option<&mut Vec<crate::InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let result = remove_empty_calls(module, enabled);
        changed.clear();
        changed.resize(module.functions.len(), result);
        Some(result)
    }
}

// Prove termination as well as absence of effects. In particular, loops, loads,
// stack operations, assembly and recursive calls cannot inherit this proof.
fn empty(f: &Function) -> bool {
    !f.attrs.weak
        && !f.weak_odr
        && (f.local || f.attrs.noninterposable || f.attrs.no_semantic_interposition)
        && !f.attrs.noreturn
        && !f.sig.returns_twice
        && !f.sig.variadic
        && f.sig.returns.is_empty()
        && f.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        && f.blocks().count() == 1 && f.entry_block().is_some_and(|b| {
        let instructions = f.block_insts(b);
        instructions.len() == 1
            && matches!(f.inst_data(instructions[0]), InstData::Return { args } if args.is_empty())
    })
}

fn remove_empty_calls(module: &mut Module, enabled: &dyn Fn(&Function) -> bool) -> bool {
    let summaries: FxHashMap<_, _> = module
        .functions
        .iter()
        .filter(|f| empty(f))
        .map(|f| (f.name.clone(), f.sig.clone()))
        .collect();
    let mut changed = false;
    for f in &mut module.functions {
        if !enabled(f) {
            continue;
        }
        let dead: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                InstData::Call { func, args } => {
                    let callee = f.callee(*func);
                    (!args.has_explicit_abi()
                        && !callee.attrs.noreturn
                        && summaries.get(&callee.name) == Some(&callee.sig))
                    .then_some(i)
                }
                _ => None,
            })
            .collect();
        for i in dead {
            // Argument evaluation remains in the caller. DCE may remove only
            // operands that independently satisfy its effect/trap contract.
            f.remove_inst(i);
            changed = true;
        }
    }
    changed
}

pub struct UnusedFunctions;
impl Pass for UnusedFunctions {
    fn name(&self) -> &'static str {
        "unused-functions"
    }
    fn description(&self) -> &'static str {
        "discard local function bodies unreachable from emitted roots"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(remove_unused_functions(module, &|_| true))
    }
    fn run_module_for_functions(
        &self,
        module: &mut Module,
        enabled: &dyn Fn(&Function) -> bool,
        _sites: Option<&mut Vec<crate::InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        let result = remove_unused_functions(module, enabled);
        changed.clear();
        changed.resize(module.functions.len(), result);
        Some(result)
    }
}

fn remove_unused_functions(module: &mut Module, enabled: &dyn Fn(&Function) -> bool) -> bool {
    let mut live = crate::assembly_symbols::roots(module);
    live.extend(
        module
            .functions
            .iter()
            .filter(|f| !enabled(f))
            .map(|f| f.name.clone()),
    );
    // Fast declarations are invalid without their local definition. Keep
    // these bodies until callee-table compaction can remove stale imports.
    live.extend(
        module
            .functions
            .iter()
            .filter(|f| f.sig.cc == CallConv::Fast)
            .map(|f| f.name.clone()),
    );
    let functions: FxHashMap<_, _> = module
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let mut work: Vec<_> = live.iter().cloned().collect();
    while let Some(name) = work.pop() {
        let Some(f) = functions.get(name.as_str()) else {
            continue;
        };
        for (_, i) in f.layout_insts() {
            let name = match f.inst_data(i) {
                InstData::Call { func, .. }
                | InstData::ReturnCall { func, .. }
                | InstData::Invoke { func, .. } => Some(f.callee(*func).name.as_str()),
                InstData::SymbolAddr { symbol } => Some(f.symbol_name(*symbol)),
                _ => None,
            };
            if let Some(name) = name
                && live.insert(name.to_owned())
            {
                work.push(name.to_owned());
            }
        }
    }
    let before = module.functions.len();
    for f in &module.functions {
        if !live.contains(&f.name) {
            let mut attrs = f.attrs.clone();
            attrs.section = None;
            if let Some(decl) = module.decls.iter_mut().find(|d| d.name == f.name) {
                decl.attrs = attrs;
            } else {
                module.decls.push(FuncDecl {
                    name: f.name.clone(),
                    sig: f.sig.clone(),
                    attrs,
                });
            }
        }
    }
    module.functions.retain(|f| live.contains(&f.name));
    let changed = before != module.functions.len();
    if changed {
        module.sync_function_attrs();
    }
    changed
}
