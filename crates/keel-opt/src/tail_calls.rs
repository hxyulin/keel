//! Recognize scalar tail calls only when the entire direct-call closure is
//! independent of stack frames. Unknown callees cannot establish this property.
use crate::Pass;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InstData, Module, Purpose, Signature, Type};

pub struct TailCalls;

impl Pass for TailCalls {
    fn name(&self) -> &'static str {
        "tail-calls"
    }
    fn description(&self) -> &'static str {
        "recognize frame-independent scalar direct tail calls"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
}

// Six single-register arguments fit both supported native ABIs, regardless of
// their integer/FP mixture. Exclude vectors, split values and aggregate homes.
fn scalar(ty: Type) -> bool {
    matches!(
        ty,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::F32 | Type::F64
    )
}

fn signature(sig: &Signature) -> bool {
    !sig.variadic
        && !sig.returns_twice
        && sig.x86_vector_abi.is_none()
        && sig.params.len() <= 6
        && sig.returns.len() <= 1
        && sig
            .params
            .iter()
            .chain(&sig.returns)
            .all(|p| p.purpose == Purpose::Normal && scalar(p.ty))
}

fn frame_independent_body(f: &Function) -> bool {
    if !signature(&f.sig)
        || f.attrs.optnone
        || f.attrs.naked
        || f.attrs.interrupt
        || f.personality.is_some()
    {
        return false;
    }
    // Use a positive list: a new instruction must establish that it cannot
    // observe the caller's frame before this pass starts accepting it.
    f.blocks()
        .all(|b| f.block_params(b).iter().all(|&v| scalar(f.value_type(v))))
        && f.layout_insts().all(|(_, i)| {
            f.inst_results(i).iter().all(|&v| scalar(f.value_type(v)))
                && matches!(
                    f.inst_data(i),
                    InstData::Iconst { .. }
                        | InstData::Fconst { .. }
                        | InstData::Unary { .. }
                        | InstData::Binary { .. }
                        | InstData::Overflow { .. }
                        | InstData::Select { .. }
                        | InstData::Convert { .. }
                        | InstData::Icmp { .. }
                        | InstData::Fcmp { .. }
                        | InstData::Jump { .. }
                        | InstData::Brif { .. }
                        | InstData::Switch { .. }
                        | InstData::Return { .. }
                        | InstData::Call { .. }
                        | InstData::ReturnCall { .. }
                        | InstData::Trap
                )
        })
}

fn run(module: &mut Module) -> bool {
    let definitions: FxHashMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.local && !f.attrs.weak && !f.weak_odr)
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    let mut safe: Vec<_> = module
        .functions
        .iter()
        .map(frame_independent_body)
        .collect();
    let mut callers = vec![Vec::new(); module.functions.len()];
    for (i, f) in module.functions.iter().enumerate() {
        for (_, inst) in f.layout_insts() {
            let callee = match f.inst_data(inst) {
                InstData::Call { func, args } | InstData::ReturnCall { func, args } => {
                    if args.has_explicit_abi() {
                        safe[i] = false;
                    }
                    f.callee(*func)
                }
                _ => continue,
            };
            if let Some(&j) = definitions.get(&callee.name)
                && !callee.attrs.weak
                && callee.sig == module.functions[j].sig
            {
                callers[j].push(i);
            } else {
                safe[i] = false;
            }
        }
    }
    // Propagate uncertainty through cycles as well as acyclic call chains.
    let mut work: Vec<_> = safe
        .iter()
        .enumerate()
        .filter_map(|(i, &s)| (!s).then_some(i))
        .collect();
    while let Some(i) = work.pop() {
        for &caller in &callers[i] {
            if safe[caller] {
                safe[caller] = false;
                work.push(caller);
            }
        }
    }
    let mut changed = false;
    for (index, f) in module.functions.iter_mut().enumerate() {
        if !safe[index] {
            continue;
        }
        let mut uses = FxHashMap::default();
        for (_, inst) in f.layout_insts() {
            f.inst_data(inst).for_each_operand(|v| {
                *uses.entry(f.resolve(v)).or_insert(0usize) += 1;
            });
        }
        let mut sites = Vec::new();
        for b in f.blocks() {
            let insts = f.block_insts(b);
            let [.., call, ret] = insts else {
                continue;
            };
            let InstData::Call { func, args } = f.inst_data(*call) else {
                continue;
            };
            let InstData::Return { args: results } = f.inst_data(*ret) else {
                continue;
            };
            if f.inst_results(*call)
                .iter()
                .all(|v| uses.get(v) == Some(&1))
                && f.callee(*func).sig == f.sig
                && f.call_contract(*call).is_none()
                && results.len() == f.inst_results(*call).len()
                && results
                    .iter()
                    .zip(f.inst_results(*call))
                    .all(|(&a, &b)| f.resolve(a) == f.resolve(b))
            {
                sites.push((*call, *ret, *func, args.clone()));
            }
        }
        for (call, ret, func, args) in sites {
            *f.inst_data_mut(ret) = InstData::ReturnCall { func, args };
            f.remove_inst(call);
            changed = true;
        }
    }
    changed
}
