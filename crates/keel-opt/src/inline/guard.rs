//! Clone a scalar no-op guard while retaining the original slow call.
use crate::{InlineBudget, InlineSite, Pass, PassFamily};
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    BinaryOp, BlockCall, ConvertOp, Function, FunctionAttrs, InsertPoint, Inst, InstData, Module,
    Purpose, Signature, Value, ValueDef,
};

const MAX_GUARD_INSTS: usize = 8;
const MAX_MODULE_GROWTH: usize = 4096;
const MAX_CALLER_GROWTH: usize = 1024;
const MAX_MODULE_SITES: usize = 256;

pub struct InlineGuards;
impl Pass for InlineGuards {
    fn name(&self) -> &'static str {
        "inline-guards"
    }
    fn description(&self) -> &'static str {
        "inline scalar guards that prove a void call does nothing"
    }
    fn run(&self, _: &mut Function, _: &crate::OptimizationContext) -> bool {
        false
    }
    fn run_module(
        &self,
        module: &mut Module,
        context: &mut crate::ModuleContext<'_>,
    ) -> Option<bool> {
        Some(run(
            module,
            context.enabled,
            context.budget,
            context.sites.as_deref_mut(),
        ))
    }
    fn family(&self) -> PassFamily {
        PassFamily::Inline
    }
    fn uses_inline_budget(&self) -> bool {
        true
    }
}

struct Guard {
    signature: Signature,
    attrs: FunctionAttrs,
    parameters: Vec<Value>,
    code: Vec<(Value, InstData)>,
    condition: Value,
    execute_when_true: bool,
}

fn scalar(data: &InstData) -> bool {
    matches!(
        data,
        InstData::Iconst { .. }
            | InstData::Icmp { .. }
            | InstData::Convert {
                op: ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext,
                ..
            }
            | InstData::Binary {
                op: BinaryOp::Iadd
                    | BinaryOp::Isub
                    | BinaryOp::Imul
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Xor
                    | BinaryOp::Shl
                    | BinaryOp::Ushr
                    | BinaryOp::Sshr
                    | BinaryOp::Ptradd,
                ..
            }
    )
}

fn summarize(f: &Function) -> Option<Guard> {
    if super::eligibility::eligibility_reason(f, true).is_some()
        || f.attrs.alwaysinline
        || f.attrs.cold
        || f.attrs.noreturn
        || !f.sig.returns.is_empty()
        || f.sig.params.len() > 32
        || f.sig.params.iter().any(|p| p.purpose != Purpose::Normal)
        || f.num_insts() > 4096
        || f.num_values() > 4096
        || f.blocks().take(33).count() > 32
        || f.layout_insts().take(257).count() > 256
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || f.layout_insts().any(|(_, i)| f.inst_loc(i).is_some())
    {
        return None;
    }
    let entry = f.entry_block()?;
    let (&term, prefix) = f.block_insts(entry).split_last()?;
    let InstData::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    let empty = |b| {
        f.block_params(b).is_empty()
            && matches!(f.block_insts(b), [i] if matches!(f.inst_data(*i), InstData::Return { args } if args.is_empty()))
    };
    if !dests.iter().all(|d| d.args.is_empty()) || empty(dests[0].block) == empty(dests[1].block) {
        return None;
    }
    // Entry stores may initialize private aggregate transport homes. Prove the
    // entire prefix unobservable on the empty edge, not just the guard's slice.
    // External reads, volatile accesses, calls and escaping addresses decline.
    let mut addresses = FxHashMap::default();
    for &i in prefix {
        match f.inst_data(i) {
            InstData::StackAddr { slot } => {
                let result = f.first_result(i);
                if f.resolve(result) == result {
                    addresses.insert(result, (*slot, 0u64));
                }
            }
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } => {
                if let Some(&(slot, off)) = addresses.get(&f.resolve(args[0])) {
                    let delta = crate::ir_match::int_const(f, args[1])?;
                    let off = off.checked_add(u64::try_from(delta).ok()?)?;
                    let result = f.first_result(i);
                    if f.resolve(result) == result {
                        addresses.insert(result, (slot, off));
                    }
                }
            }
            InstData::Store { addr, value, flags } if !flags.volatile => {
                let &(slot, off) = addresses.get(&f.resolve(*addr))?;
                let end = off.checked_add(u64::from(f.value_type(*value).bytes()?))?;
                if end > u64::from(f.stack_slot(slot).size) {
                    return None;
                }
            }
            data if scalar(data) => {}
            _ => return None,
        }
    }
    let parameters = f.block_params(entry).to_vec();
    let mut code = Vec::new();
    fn collect(
        f: &Function,
        parameters: &[Value],
        value: Value,
        code: &mut Vec<(Value, InstData)>,
        fuel: &mut usize,
    ) -> Option<()> {
        let value = f.resolve(value);
        if parameters.contains(&value) || code.iter().any(|(v, _)| *v == value) {
            return Some(());
        }
        *fuel = fuel.checked_sub(1)?;
        let ValueDef::Result(i, 0) = f.value_def(value) else {
            return None;
        };
        let data = f.inst_data(i);
        if Some(f.inst_block(i)) != f.entry_block() || !scalar(data) {
            return None;
        }
        let mut operands = Vec::new();
        data.for_each_operand(|v| operands.push(v));
        for operand in operands {
            collect(f, parameters, operand, code, fuel)?;
        }
        let mut data = data.clone();
        data.for_each_operand_mut(|v| *v = f.resolve(*v));
        code.push((value, data));
        Some(())
    }
    let mut fuel = MAX_GUARD_INSTS;
    collect(f, &parameters, *cond, &mut code, &mut fuel)?;
    Some(Guard {
        signature: f.sig.clone(),
        attrs: f.attrs.clone(),
        parameters,
        code,
        condition: f.resolve(*cond),
        execute_when_true: empty(dests[1].block),
    })
}

impl Guard {
    fn matches(&self, f: &Function, value: Value, expected: Value, arguments: &[Value]) -> bool {
        let value = f.resolve(value);
        if let Some(index) = self.parameters.iter().position(|&p| p == expected) {
            return value == f.resolve(arguments[index]);
        }
        let Some((_, expected_data)) = self.code.iter().find(|(v, _)| *v == expected) else {
            return false;
        };
        let ValueDef::Result(i, 0) = f.value_def(value) else {
            return false;
        };
        let actual = f.inst_data(i);
        let mut operands = Vec::new();
        actual.for_each_operand(|v| operands.push(v));
        let mut mapped = expected_data.clone();
        let mut index = 0;
        let mut equal = true;
        mapped.for_each_operand_mut(|v| {
            if let Some(&actual) = operands.get(index) {
                equal &= self.matches(f, actual, *v, arguments);
                *v = actual;
            } else {
                equal = false;
            }
            index += 1;
        });
        equal && mapped == *actual
    }

    fn apply(&self, f: &mut Function, call: Inst, arguments: &[Value]) {
        let mut values: FxHashMap<_, _> = self
            .parameters
            .iter()
            .copied()
            .zip(arguments.iter().map(|&v| f.resolve(v)))
            .collect();
        for (value, data) in &self.code {
            let mut data = data.clone();
            data.for_each_operand_mut(|v| *v = values[v]);
            let i = f.insert_inst(InsertPoint::Before(call), data);
            f.set_inst_loc(i, f.inst_loc(call));
            values.insert(*value, f.first_result(i));
        }
        let block = f.inst_block(call);
        let position = f
            .block_insts(block)
            .iter()
            .position(|&i| i == call)
            .unwrap();
        let continuation = f.create_block();
        f.move_insts_to_end(block, position + 1, continuation);
        let slow = f.create_block();
        f.move_insts_to_end(block, position, slow);
        let destinations = [slow, continuation];
        let dests = Box::new(std::array::from_fn(|i| BlockCall {
            block: destinations[i ^ usize::from(!self.execute_when_true)],
            args: Vec::new(),
        }));
        f.insert_inst(
            InsertPoint::End(block),
            InstData::Brif {
                cond: values[&self.condition],
                dests,
            },
        );
        f.insert_inst(
            InsertPoint::End(slow),
            InstData::Jump {
                dest: BlockCall {
                    block: continuation,
                    args: Vec::new(),
                },
            },
        );
    }
}

fn run(
    module: &mut Module,
    enabled: &dyn Fn(&Function) -> bool,
    budget: &mut InlineBudget,
    mut sites: Option<&mut Vec<InlineSite>>,
) -> bool {
    let summaries: FxHashMap<_, _> = module
        .functions
        .iter()
        .filter_map(|f| summarize(f).map(|guard| (f.name.clone(), guard)))
        .collect();
    if summaries.is_empty() {
        return false;
    }
    let mut growth = 0;
    let mut count = 0;
    for f in &mut module.functions {
        if !enabled(f)
            || f.sig.returns_twice
            || f.personality.is_some()
            || f.attrs.naked
            || f.attrs.interrupt
            || f.debug_loc.is_some()
            || !f.debug_vars.is_empty()
        {
            continue;
        }
        if !f.layout_insts().any(|(_, i)| {
            matches!(f.inst_data(i), InstData::Call { func, .. } if summaries.contains_key(&f.callee(*func).name))
        }) {
            continue;
        }
        let cfg = ControlFlowGraph::compute(f);
        let calls: Vec<_> = f
            .layout_insts()
            .filter_map(|(block, call)| {
                let InstData::Call { func, args } = f.inst_data(call) else {
                    return None;
                };
                let decl = f.callee(*func);
                let guard = summaries.get(&decl.name)?;
                if args.has_explicit_abi()
                    || decl.sig != guard.signature
                    || decl.attrs.noinline
                    || decl.attrs.optnone
                    || decl.attrs.noreturn
                    || f.attrs.fast_math != guard.attrs.fast_math
                    || !f
                        .attrs
                        .target_features
                        .permits_inlining(&guard.attrs.target_features)
                {
                    return None;
                }
                // A repeated invocation must not wrap the retained slow call again.
                if let [pred] = cfg.preds(block)
                    && let Some(&term) = f.block_insts(*pred).last()
                    && let InstData::Brif { cond, dests } = f.inst_data(term)
                    && dests[0].block != dests[1].block
                    && dests[usize::from(!guard.execute_when_true)].block == block
                    && guard.matches(f, *cond, guard.condition, args)
                {
                    return None;
                }
                Some((call, guard, args.to_vec(), decl.name.clone()))
            })
            .collect();
        let mut caller_growth = 0;
        for (call, guard, arguments, name) in calls {
            let added = guard.code.len() + 2;
            if count == MAX_MODULE_SITES
                || growth + added > MAX_MODULE_GROWTH
                || caller_growth + added > MAX_CALLER_GROWTH
                || budget.used + added > budget.limit
            {
                continue;
            }
            guard.apply(f, call, &arguments);
            if let Some(sites) = sites.as_deref_mut() {
                sites.push(InlineSite::new(
                    f,
                    &name,
                    call,
                    false,
                    "guard_prefix_inlined",
                ));
            }
            caller_growth += added;
            growth += added;
            budget.used += added;
            count += 1;
        }
    }
    growth != 0
}
