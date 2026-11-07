//! Bounded direct-call versioning of local constant function-pointer tables.
use crate::ir_match::{def, int_const};
use crate::{ModuleContext, OptimizationContext, Pass};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp as B, BlockCall, ConvertOp, DataInit, DataItem, DataObject, Function, FunctionAttrs,
    Inst, InstData as D, Module, Purpose, Signature, Symbol, Type, Value,
};

const TARGET_LIMIT: usize = 8;
const SITE_LIMIT: usize = 8;

pub(crate) struct Devirtualize;
impl Pass for Devirtualize {
    fn name(&self) -> &'static str {
        "devirtualize-const-table"
    }
    fn description(&self) -> &'static str {
        "version bounded calls through local immutable small-leaf function tables"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, m: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        if context.layout.is_none_or(|l| l.pointer_bytes() != 8) || !m.assembly.is_empty() {
            return Some(false);
        }
        let names: FxHashSet<_> = m
            .data
            .iter()
            .filter_map(table_init)
            .flat_map(|init| init.items.iter())
            .filter_map(|item| match item {
                DataItem::Ptr { symbol, .. } => Some(symbol.as_str()),
                _ => None,
            })
            .collect();
        if names.is_empty() {
            return Some(false);
        }
        let definitions: FxHashMap<_, _> = m
            .functions
            .iter()
            .filter(|f| {
                names.contains(f.name.as_str())
                    && !f.attrs.noinline
                    && !f.attrs.optnone
                    && !f.attrs.naked
                    && !f.attrs.interrupt
                    && f.blocks().take(2).count() == 1
                    && f.layout_insts().take(13).count() <= 12
                    && f.layout_insts().all(|(_, i)| {
                        matches!(
                            f.inst_data(i),
                            D::Iconst { .. }
                                | D::Unary { .. }
                                | D::Binary { .. }
                                | D::Convert { .. }
                                | D::Icmp { .. }
                                | D::Select { .. }
                                | D::Return { .. }
                        )
                    })
                    && f.local
                    && !f.weak_odr
                    && !f.attrs.weak
                    && !f.attrs.noreturn
                    && !f.sig.variadic
                    && !f.sig.returns_twice
                    && f.sig.params.iter().chain(&f.sig.returns).all(|p| {
                        p.purpose == Purpose::Normal
                            && matches!(
                                p.ty,
                                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                            )
                    })
                    && f.sig.returns.len() <= 1
            })
            .map(|f| {
                (
                    f.name.clone(),
                    Target {
                        name: f.name.clone(),
                        sig: f.sig.clone(),
                        attrs: f.attrs.clone(),
                    },
                )
            })
            .collect();
        let tables: FxHashMap<_, Vec<Target>> = m
            .data
            .iter()
            .filter_map(|d| {
                let init = table_init(d)?;
                let targets: Option<Vec<_>> = init
                    .items
                    .iter()
                    .map(|i| match i {
                        DataItem::Ptr { symbol, addend: 0 } => definitions.get(symbol).cloned(),
                        _ => None,
                    })
                    .collect();
                Some((d.name.clone(), targets?))
            })
            .collect();
        if tables.is_empty()
            || m.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), D::InlineAsm { .. }))
            })
        {
            return Some(false);
        }
        let mut changed = false;
        for f in &mut m.functions {
            let mut dirty = false;
            if context.enabled(f) {
                let sites: Vec<_> = f
                    .layout_insts()
                    .filter_map(|(_, i)| plan(f, i, &tables))
                    .take(SITE_LIMIT)
                    .collect();
                for site in sites {
                    rewrite(f, site);
                    dirty = true;
                }
            }
            context.changed.push(dirty);
            changed |= dirty;
        }
        Some(changed)
    }
}
fn table_init(data: &DataObject) -> Option<&DataInit> {
    let init = data.init.as_ref()?;
    (data.local
        && !data.weak
        && !data.weak_odr
        && !data.thread_local
        && init.constant
        && init.align >= 8
        && !init.items.is_empty()
        && init.items.len() <= TARGET_LIMIT
        && init
            .items
            .iter()
            .all(|item| matches!(item, DataItem::Ptr { addend: 0, .. })))
    .then_some(init)
}
#[derive(Clone)]
struct Target {
    name: String,
    sig: Signature,
    attrs: FunctionAttrs,
}
struct Site {
    call: Inst,
    load: Inst,
    index: Value,
    targets: Vec<Target>,
}
fn plan(f: &Function, call: Inst, tables: &FxHashMap<String, Vec<Target>>) -> Option<Site> {
    let D::CallIndirect { sig, callee, args } = f.inst_data(call) else {
        return None;
    };
    if args.has_explicit_abi() || f.inst_results(call).iter().any(|&v| f.resolve(v) != v) {
        return None;
    }
    let D::Load {
        ty: Type::Ptr,
        flags,
        addr,
    } = *f.inst_data(def(f, *callee)?)
    else {
        return None;
    };
    if flags.volatile || flags.align > 8 {
        return None;
    }
    let D::Binary {
        op: B::Ptradd,
        args: [base, offset],
    } = *f.inst_data(def(f, addr)?)
    else {
        return None;
    };
    let D::SymbolAddr {
        symbol: symbol @ Symbol::Data(_),
    } = *f.inst_data(def(f, base)?)
    else {
        return None;
    };
    let table = tables.get(f.symbol_name(symbol))?;
    let D::Binary { op, args } = *f.inst_data(def(f, offset)?) else {
        return None;
    };
    let index = match op {
        B::Shl if int_const(f, args[1]) == Some(3) => args[0],
        B::Imul if int_const(f, args[1]) == Some(8) => args[0],
        B::Imul if int_const(f, args[0]) == Some(8) => args[1],
        _ => return None,
    };
    if f.value_type(index) != Type::I64 {
        return None;
    }
    let masked = match *f.inst_data(def(f, index)?) {
        D::Convert {
            op: ConvertOp::Uext | ConvertOp::Sext,
            arg,
            ..
        } if f.value_type(arg) == Type::I32 => arg,
        _ => index,
    };
    let D::Binary { op: B::And, args } = *f.inst_data(def(f, masked)?) else {
        return None;
    };
    let maximum = int_const(f, args[0]).or_else(|| int_const(f, args[1]))?;
    if maximum < 0 || maximum as usize >= table.len() {
        return None;
    }
    let targets = &table[..=maximum as usize];
    if targets.iter().any(|t| {
        t.sig != *f.signature(*sig) || f.callees().any(|(_, d)| d.name == t.name && d.sig != t.sig)
    }) {
        return None;
    }
    Some(Site {
        call,
        load: def(f, *callee)?,
        index: f.resolve(index),
        targets: targets.to_vec(),
    })
}
fn rewrite(f: &mut Function, site: Site) {
    let Site {
        call,
        load,
        index,
        targets,
    } = site;
    // The defined, eight-byte-aligned object contains every masked index.
    // Stride and access width are both eight, so this access cannot fault.
    if let D::Load { flags, .. } = f.inst_data_mut(load) {
        flags.notrap = true;
        flags.trap_ub = false;
    }
    let D::CallIndirect { args, .. } = f.inst_data(call).clone() else {
        unreachable!()
    };
    let contract = f.call_contract(call).cloned();
    let loc = f.inst_loc(call);
    let head = f.inst_block(call);
    let at = f
        .block_insts(head)
        .iter()
        .position(|&i| i == call)
        .expect("planned call is inserted");
    let join = f.create_block();
    for value in f.inst_results(call).to_vec() {
        let param = f.append_block_param(join, f.value_type(value));
        f.replace_all_uses(value, param);
    }
    f.move_insts_to_end(head, at + 1, join);
    f.remove_inst(call);
    let mut dests = Vec::new();
    for target in targets {
        let block = f.create_block();
        let existing = f
            .callees()
            .find(|(_, d)| d.name == target.name)
            .map(|(id, _)| id);
        let callee = existing.unwrap_or_else(|| {
            let id = f.import_function(&target.name, target.sig);
            f.set_callee_attrs(id, target.attrs);
            id
        });
        let direct = f.append_inst(
            block,
            D::Call {
                func: callee,
                args: args.clone(),
            },
        );
        f.set_inst_loc(direct, loc);
        if let Some(contract) = &contract {
            f.set_call_contract(direct, contract.clone())
                .expect("direct call preserves the original signature");
        }
        let values = f.inst_results(direct).to_vec();
        let jump = f.append_inst(
            block,
            D::Jump {
                dest: BlockCall {
                    block: join,
                    args: values,
                },
            },
        );
        f.set_inst_loc(jump, loc);
        dests.push(BlockCall {
            block,
            args: vec![],
        });
    }
    // The nonnegative AND mask proves every index is within this target set.
    // Zero is the default; all remaining values receive explicit cases.
    let cases = (1..dests.len() as i64).collect();
    let switch = f.append_inst(
        head,
        D::Switch {
            arg: index,
            dests,
            cases,
        },
    );
    f.set_inst_loc(switch, loc);
}
