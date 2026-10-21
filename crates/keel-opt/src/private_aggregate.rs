//! Opt-in, bounded private two-word byval transport. No pipeline registration.
//!
//! Each call snapshots both words immediately before the call. Each byval
//! parameter gets its own fresh, naturally aligned callee home: arbitrary
//! mutation, address comparisons, forwarding and capture retain byval semantics.
//! This is deliberately not pointer substitution or a nocapture inference.
//! Sret is preserved verbatim; result transport optimization is out of scope.
use keel_ir::{
    AbiParam, BinaryOp, DataItem, Function, InsertPoint, InstData, Layout, MemoryFlags, Module,
    Purpose, Signature, Symbol, Type, Value,
};
use std::collections::{HashMap, HashSet};

const MAX_FUNCTION_INSTS: usize = 50_000;
const MAX_VALUES: usize = 100_000;
const MAX_MODULE_WORK: usize = 2_000_000;
const MAX_PARAMS: usize = 32;

#[derive(Clone)]
struct Plan {
    old: Signature,
    new: Signature,
    split: Vec<bool>,
}
fn pair(l: &Layout) -> bool {
    // Match structural shapes without expanding arbitrary layouts/leaves.
    matches!(l, Layout::Array(t, 2) if **t == Layout::Scalar(Type::I64))
        || matches!(l, Layout::Struct(fs) if fs.as_slice() == [Layout::Scalar(Type::I64), Layout::Scalar(Type::I64)])
}
fn plan(f: &Function) -> Option<Plan> {
    if !f.local
        || f.weak_odr
        || f.sig.variadic
        || f.sig.returns_twice
        || f.sig.params.len() > MAX_PARAMS
        || f.num_values() > MAX_VALUES
        || !f.debug_vars.is_empty()
        || !f.restrict_params.is_empty()
    {
        return None;
    }
    let entry = f.entry_block()?;
    // Entry arguments are replaced, so exclude entry backedges explicitly.
    if f.layout_insts().any(|(_, i)| {
        f.inst_data(i)
            .branch_dests()
            .iter()
            .any(|d| d.block == entry)
    }) {
        return None;
    }
    let split: Vec<_> = f
        .sig
        .params
        .iter()
        .map(|p| matches!(&p.purpose, Purpose::ByVal(l) if pair(l)))
        .collect();
    if !split.iter().any(|&b| b) {
        return None;
    }
    // No other aggregate shape is silently changed.
    if f.sig
        .params
        .iter()
        .any(|p| matches!(&p.purpose, Purpose::ByVal(l) if !pair(l)))
    {
        return None;
    }
    let mut new = f.sig.clone();
    new.params = f
        .sig
        .params
        .iter()
        .zip(&split)
        .flat_map(|(p, &s)| {
            if s {
                vec![AbiParam::new(Type::I64); 2]
            } else {
                vec![p.clone()]
            }
        })
        .collect();
    Some(Plan {
        old: f.sig.clone(),
        new,
        split,
    })
}
fn insert(f: &mut Function, at: InsertPoint, d: InstData) -> Value {
    let i = f.insert_inst(at, d);
    f.first_result(i)
}
fn offset(f: &mut Function, at: InsertPoint, base: Value) -> Value {
    let eight = insert(
        f,
        at,
        InstData::Iconst {
            ty: Type::I64,
            imm: 8,
        },
    );
    insert(
        f,
        at,
        InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, eight],
        },
    )
}
fn flags() -> MemoryFlags {
    MemoryFlags {
        align: 8,
        ..MemoryFlags::default()
    }
}

/// Input must be verified whole-module IR. Returns rewritten definitions.
/// Run before stack-forward/stack-promote/DCE; callers may subsequently opt in
/// to internal-fast-calls. No defaults or calling conventions change here.
pub fn run(m: &mut Module) -> usize {
    if !m.files.is_empty() || !m.debug_types.is_empty() || !m.debug_globals.is_empty() {
        return 0;
    }
    let mut work = 0usize;
    let mut blocked = HashSet::new();
    let mut seen = HashSet::new();
    for d in &m.decls {
        blocked.insert(d.name.clone());
    }
    for d in &m.data {
        if let Some(init) = &d.init {
            for item in &init.items {
                if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item {
                    blocked.insert(symbol.clone());
                }
            }
        }
    }
    for f in &m.functions {
        if !seen.insert(f.name.clone()) {
            blocked.insert(f.name.clone());
        }
        if let Some(p) = &f.personality {
            blocked.insert(p.clone());
        }
        let mut n = 0;
        for (_, i) in f.layout_insts() {
            n += 1;
            work = work.saturating_add(1);
            f.inst_data(i)
                .for_each_operand(|_| work = work.saturating_add(1));
            if work > MAX_MODULE_WORK {
                return 0;
            }
            match f.inst_data(i) {
                // Textual symbol references and frame/nonlocal observers cannot
                // be reasoned about by this prototype. Abort transactionally.
                InstData::InlineAsm { .. } | InstData::Intrinsic { .. } => return 0,
                InstData::SymbolAddr {
                    symbol: Symbol::Func(id),
                } => {
                    blocked.insert(f.callee(*id).name.clone());
                }
                InstData::ReturnCall { func, .. } | InstData::Invoke { func, .. } => {
                    blocked.insert(f.callee(*func).name.clone());
                }
                InstData::Call { func, .. } if f.callee(*func).sig.returns_twice => return 0,
                InstData::CallIndirect { sig, .. } if f.signature(*sig).returns_twice => return 0,
                _ => {}
            }
        }
        // Huge callers are excluded too, since insertion uses linear Vec shifts.
        if n > MAX_FUNCTION_INSTS || f.num_values() > MAX_VALUES {
            return 0;
        }
    }
    let mut plans: HashMap<_, _> = m
        .functions
        .iter()
        .filter(|f| !blocked.contains(&f.name))
        .filter_map(|f| plan(f).map(|p| (f.name.clone(), p)))
        .collect();
    // Validate *all* stored contracts, including unused callee declarations,
    // before mutating anything. Unknown/mismatched callers block a definition.
    for f in &m.functions {
        for (_, d) in f.callees() {
            if plans.get(&d.name).is_some_and(|p| p.old != d.sig) {
                blocked.insert(d.name.clone());
            }
        }
    }
    plans.retain(|name, _| !blocked.contains(name));
    for f in &mut m.functions {
        // Rewrite calls before definitions so recursive calls snapshot original
        // byval homes as well. All inserted loads precede any callee writes.
        let calls: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| {
                if let InstData::Call { func, args } = f.inst_data(i) {
                    plans
                        .get(&f.callee(*func).name)
                        .map(|p| (i, args.clone(), p.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (i, args, p) in calls {
            let at = InsertPoint::Before(i);
            let mut new = Vec::new();
            for (v, split) in args.into_iter().zip(p.split) {
                if split {
                    let lo = insert(
                        f,
                        at,
                        InstData::Load {
                            ty: Type::I64,
                            addr: v,
                            flags: flags(),
                        },
                    );
                    let hi_addr = offset(f, at, v);
                    let hi = insert(
                        f,
                        at,
                        InstData::Load {
                            ty: Type::I64,
                            addr: hi_addr,
                            flags: flags(),
                        },
                    );
                    new.extend([lo, hi]);
                } else {
                    new.push(v);
                }
            }
            if let InstData::Call { args, .. } = f.inst_data_mut(i) {
                *args = new;
            }
        }
        let updates: Vec<_> = f
            .callees()
            .filter_map(|(id, d)| plans.get(&d.name).map(|p| (id, p.new.clone())))
            .collect();
        for (id, sig) in updates {
            f.callee_mut(id).sig = sig;
        }
        let Some(p) = plans.get(&f.name) else {
            continue;
        };
        let entry = f.entry_block().unwrap();
        let at = InsertPoint::Before(f.block_insts(entry)[0]);
        let old = f.block_params(entry).to_vec();
        for index in (0..old.len()).rev() {
            f.remove_block_param(entry, index);
        }
        for ((v, split), param) in old.into_iter().zip(&p.split).zip(&p.old.params) {
            if *split {
                let lo = f.append_block_param(entry, Type::I64);
                let hi = f.append_block_param(entry, Type::I64);
                let slot = f.create_stack_slot(16, 8);
                let base = insert(f, at, InstData::StackAddr { slot });
                f.insert_inst(
                    at,
                    InstData::Store {
                        addr: base,
                        value: lo,
                        flags: flags(),
                    },
                );
                let hi_addr = offset(f, at, base);
                f.insert_inst(
                    at,
                    InstData::Store {
                        addr: hi_addr,
                        value: hi,
                        flags: flags(),
                    },
                );
                if f.resolve(v) == v {
                    f.replace_all_uses(v, base);
                }
            } else {
                let new = f.append_block_param(entry, param.ty);
                // Dead/aliased original entry parameters need no substitution.
                if f.resolve(v) == v {
                    f.replace_all_uses(v, new);
                }
            }
        }
        f.sig = p.new.clone();
    }
    plans.len()
}
