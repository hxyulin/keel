//! Opt-in, bounded private two-word byval transport. No default pipeline change.
//!
//! Each call snapshots both words immediately before the call. Each byval
//! parameter gets its own fresh, naturally aligned callee home: arbitrary
//! mutation, address comparisons, forwarding and capture retain byval semantics.
//! This is deliberately not pointer substitution or a nocapture inference.
//! Sret is preserved verbatim; result transport optimization is out of scope.
use keel_ir::{
    AbiParam, BinaryOp, DataItem, Function, InsertPoint, InstData, Layout, MemoryEffects,
    MemoryFlags, Module, Purpose, Signature, Symbol, Type, Value, ValueDef,
};
use std::collections::{HashMap, HashSet};

/// Explicit named/custom-pipeline entry point; absent from default pipelines.
pub struct PrivateAggregate;
impl crate::Pass for PrivateAggregate {
    fn name(&self) -> &'static str {
        "private-aggregate"
    }
    fn description(&self) -> &'static str {
        "scalarize contained private two-word byval transport"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module) != 0)
    }
}

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
    // Do not weaken an over-aligned access contract or add assumptions about
    // atomic/variadic facilities. Without address closure, reject the entire
    // definition even when the access may use an unrelated pointer.
    if f.layout_insts().any(|(_, i)| match f.inst_data(i) {
        InstData::Load { flags, .. } | InstData::Store { flags, .. } => flags.align > 8,
        InstData::AtomicLoad { .. }
        | InstData::AtomicStore { .. }
        | InstData::AtomicRmw { .. }
        | InstData::AtomicCas { .. }
        | InstData::VaArg { .. } => true,
        _ => false,
    }) {
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
                // Textual symbols disable the module; intrinsic/frame facilities
                // exclude the containing definition from this prototype.
                InstData::InlineAsm { .. } => return 0,
                InstData::Intrinsic { .. } => {
                    blocked.insert(f.name.clone());
                }
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
    let mut growth = 0usize;
    for f in &m.functions {
        let mut added = plans
            .get(&f.name)
            .map_or(0, |p| p.split.iter().filter(|&&b| b).count() * 5);
        for (_, i) in f.layout_insts() {
            if let InstData::Call { func, .. } = f.inst_data(i) {
                if let Some(p) = plans.get(&f.callee(*func).name) {
                    added = added.saturating_add(p.split.iter().filter(|&&b| b).count() * 4);
                }
            }
        }
        growth = growth.saturating_add(added);
        if added > 4096 || growth > 100_000 {
            return 0;
        }
    }
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
                    // Byval supplies bytes, not an explicit source-address
                    // alignment promise. Only the fresh home is known aligned.
                    let lo = insert(
                        f,
                        at,
                        InstData::Load {
                            ty: Type::I64,
                            addr: v,
                            flags: MemoryFlags::default(),
                        },
                    );
                    let hi_addr = offset(f, at, v);
                    let hi = insert(
                        f,
                        at,
                        InstData::Load {
                            ty: Type::I64,
                            addr: hi_addr,
                            flags: MemoryFlags::default(),
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
        let prefix = f.block_insts(entry).to_vec();
        let mut homes = HashMap::new();
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
                homes.insert(base, ([lo, hi], 0usize));
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
        // Forward exact entry-prefix i64 reads from the snapshot words. Stop at
        // the first effect that could mutate/capture a home; do not infer alias
        // freedom or nocapture from another parameter or call. Other reads and
        // arbitrary later mutations still use the independently rebuilt homes.
        for i in prefix.into_iter().take(128) {
            match f.inst_data(i).clone() {
                InstData::Load {
                    ty: Type::I64,
                    addr,
                    flags,
                } if !flags.volatile => {
                    if let Some(&(words, off @ (0 | 8))) = homes.get(&f.resolve(addr)) {
                        let result = f.first_result(i);
                        if f.resolve(result) == result {
                            f.replace_all_uses(result, words[off / 8]);
                            // This exact read is of our initialized fixed home,
                            // hence cannot fault. Ordinary DCE conservatively
                            // retains potentially trapping loads.
                            f.remove_inst(i);
                        }
                    }
                }
                InstData::Load { flags, .. } if !flags.volatile => {}
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args,
                } => {
                    if let Some(&(words, off)) = homes.get(&f.resolve(args[0])) {
                        if let ValueDef::Result(k, _) = f.value_def(f.resolve(args[1])) {
                            if let InstData::Iconst { imm, .. } = f.inst_data(k) {
                                if let Ok(delta) = usize::try_from(*imm) {
                                    if let Some(off) = off.checked_add(delta).filter(|&o| o <= 16) {
                                        let result = f.first_result(i);
                                        if f.resolve(result) == result {
                                            homes.insert(result, (words, off));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                data if data.memory_effects() == MemoryEffects::None => {}
                _ => break,
            }
        }
        f.sig = p.new.clone();
    }
    plans.len()
}
