//! Final private transport for contained direct calls, including removal of
//! C-only extension contracts. Backends may specialize eligible private leaves.
use crate::Pass;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{ArgExt, CallConv, DataItem, Function, InstData, Module, Purpose, Symbol};

pub struct InternalFastCalls;
impl Pass for InternalFastCalls {
    fn name(&self) -> &'static str {
        "internal-fast-calls"
    }
    fn description(&self) -> &'static str {
        "use private fast transport without C-only narrow extensions"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
}

fn run(module: &mut Module) -> bool {
    let mut blocked = FxHashSet::default();
    for object in &module.data {
        if let Some(init) = &object.init {
            for item in &init.items {
                if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item {
                    blocked.insert(symbol.clone());
                }
            }
        }
    }
    for f in &module.functions {
        if let Some(personality) = &f.personality {
            blocked.insert(personality.clone());
        }
        for (_, i) in f.layout_insts() {
            match f.inst_data(i) {
                InstData::Call { func, args } | InstData::Invoke { func, args, .. }
                    if args.has_explicit_abi() =>
                {
                    blocked.insert(f.callee(*func).name.clone());
                }
                _ => {}
            }
            match f.inst_data(i) {
                // Arbitrary assembly may refer to local symbols textually.
                InstData::InlineAsm { .. } => return false,
                InstData::SymbolAddr {
                    symbol: Symbol::Func(callee),
                } => {
                    blocked.insert(f.callee(*callee).name.clone());
                }
                InstData::ReturnCall { func: callee, .. } => {
                    // Tail contracts compare result extension attributes. Do
                    // not change either endpoint in this initial experiment.
                    blocked.insert(f.name.clone());
                    blocked.insert(f.callee(*callee).name.clone());
                }
                InstData::ReturnCallIndirect { .. } => {
                    blocked.insert(f.name.clone());
                }
                _ => {}
            }
        }
    }
    let mut signatures = FxHashMap::default();
    for f in &mut module.functions {
        if !f.local
            || f.attrs.constructor
            || f.attrs.destructor
            || f.attrs.used
            || f.attrs.retain
            || f.attrs.section.is_some()
            || f.attrs.weak
            || f.weak_odr
            || f.sig.cc != CallConv::C
            || f.sig.x86_vector_abi.is_some()
            || f.sig.variadic
            || f.sig.returns_twice
            || f.sig.returns.len() > 1
            || blocked.contains(&f.name)
            || f.sig
                .params
                .iter()
                .chain(&f.sig.returns)
                .any(|p| p.purpose != Purpose::Normal)
        {
            continue;
        }
        f.sig.cc = CallConv::Fast;
        for p in f.sig.params.iter_mut().chain(&mut f.sig.returns) {
            p.ext = ArgExt::None;
        }
        signatures.insert(f.name.clone(), f.sig.clone());
    }
    for f in &mut module.functions {
        let updates: Vec<_> = f
            .callees()
            .filter_map(|(id, d)| signatures.get(&d.name).map(|s| (id, s.clone())))
            .collect();
        for (id, sig) in updates {
            f.callee_mut(id).sig = sig;
        }
    }
    !signatures.is_empty()
}
