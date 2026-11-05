//! Final private transport for contained direct calls, including removal of
//! C-only extension contracts. Backends may specialize eligible private leaves.
//! An address-taken function that is also called directly may become fast
//! behind a local C thunk, a single `return_call`, which every address then
//! names. `cc(fast)` makes no promise to stay callable from C.
use crate::Pass;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    ArgExt, CallConv, DataItem, Function, InsertPoint, InstData, Module, Purpose, Signature,
    Symbol, Type, Value,
};

pub struct InternalFastCalls;
impl Pass for InternalFastCalls {
    fn name(&self) -> &'static str {
        "internal-fast-calls"
    }
    fn description(&self) -> &'static str {
        "use private fast transport without C-only narrow extensions"
    }
    fn run(&self, _: &mut Function, _: &crate::OptimizationContext) -> bool {
        false
    }
    fn run_module(
        &self,
        module: &mut Module,
        context: &mut crate::ModuleContext<'_>,
    ) -> Option<bool> {
        context.whole_module(module, run)
    }
}

/// Whether a C thunk for this signature can be a single `return_call` on
/// both native ABIs: at most six single-register arguments, and results
/// with no extension, so the fast callee's results are the thunk's.
fn thunkable(sig: &Signature) -> bool {
    let register = |p: &keel_ir::AbiParam| {
        matches!(
            p.ty,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::F32 | Type::F64 | Type::Ptr
        )
    };
    sig.params.len() <= 6
        && sig.params.iter().all(register)
        && sig
            .returns
            .iter()
            .all(|r| register(r) && r.ext == ArgExt::None)
}

fn run(module: &mut Module) -> bool {
    let mut blocked = FxHashSet::default();
    // Functions whose address is taken only by `symbol_addr` or a data
    // pointer with no addend. They may become fast behind a C thunk.
    let mut addressed = FxHashSet::default();
    let mut called = FxHashSet::default();
    for object in &module.data {
        if let Some(init) = &object.init {
            for item in &init.items {
                match item {
                    DataItem::Ptr { symbol, addend: 0 } => {
                        addressed.insert(symbol.clone());
                    }
                    DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } => {
                        blocked.insert(symbol.clone());
                    }
                    _ => {}
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
                InstData::Call { func, args } | InstData::Invoke { func, args, .. } => {
                    let name = &f.callee(*func).name;
                    if args.has_explicit_abi() {
                        blocked.insert(name.clone());
                    } else {
                        called.insert(name.clone());
                    }
                }
                _ => {}
            }
            match f.inst_data(i) {
                // Arbitrary assembly may refer to local symbols textually.
                InstData::InlineAsm { .. } => return false,
                InstData::SymbolAddr {
                    symbol: Symbol::Func(callee),
                } => {
                    addressed.insert(f.callee(*callee).name.clone());
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
    // Module assembly may name a function textually, bypassing its thunk.
    if !module.assembly.is_empty() {
        blocked.extend(addressed.drain());
    }
    let mut taken: FxHashSet<String> = module
        .functions
        .iter()
        .map(|f| f.name.clone())
        .chain(module.decls.iter().map(|d| d.name.clone()))
        .chain(module.data.iter().map(|d| d.name.clone()))
        .collect();
    let mut signatures = FxHashMap::default();
    // Address-taken functions made fast: their name, attributes and C
    // signature, and their thunk's name.
    let mut thunks = Vec::new();
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
        if addressed.contains(&f.name) {
            // Only direct calls gain from fast transport; a thunk alone
            // would only add a branch to every indirect call.
            if !called.contains(&f.name) || !thunkable(&f.sig) {
                continue;
            }
            let mut thunk = format!("{}.c", f.name);
            let mut n = 1;
            while taken.contains(&thunk) {
                thunk = format!("{}.c{n}", f.name);
                n += 1;
            }
            taken.insert(thunk.clone());
            thunks.push((f.name.clone(), f.attrs.clone(), f.sig.clone(), thunk));
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
    if thunks.is_empty() {
        return !signatures.is_empty();
    }
    // Every address now names the thunk, so addresses stay equal to each
    // other and every indirect caller sees the C convention.
    let redirect: FxHashMap<&str, (&Signature, &str)> = thunks
        .iter()
        .map(|(name, _, sig, thunk)| (name.as_str(), (sig, thunk.as_str())))
        .collect();
    for object in &mut module.data {
        if let Some(init) = &mut object.init {
            for item in &mut init.items {
                if let DataItem::Ptr { symbol, addend: 0 } = item
                    && let Some((_, thunk)) = redirect.get(symbol.as_str())
                {
                    *symbol = (*thunk).to_owned();
                }
            }
        }
    }
    for f in &mut module.functions {
        let sites: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                InstData::SymbolAddr {
                    symbol: Symbol::Func(callee),
                } => redirect
                    .get(f.callee(*callee).name.as_str())
                    .map(|&r| (i, r)),
                _ => None,
            })
            .collect();
        for (i, (sig, thunk)) in sites {
            let callee = f.import_function(thunk, sig.clone());
            *f.inst_data_mut(i) = InstData::SymbolAddr {
                symbol: Symbol::Func(callee),
            };
        }
    }
    for (name, attrs, sig, thunk) in thunks {
        let mut f = Function::new(thunk, sig.clone());
        f.local = true;
        let block = f.create_block();
        let args: Vec<Value> = sig
            .params
            .iter()
            .map(|p| f.append_block_param(block, p.ty))
            .collect();
        let callee = f.import_function(&name, signatures[&name].clone());
        // A callee entry carries its definition's attributes.
        f.callee_mut(callee).attrs = attrs;
        f.ins(InsertPoint::End(block)).build(InstData::ReturnCall {
            func: callee,
            args: args.into(),
        });
        module.functions.push(f);
    }
    true
}
