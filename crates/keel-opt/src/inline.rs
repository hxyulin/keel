//! Bounded direct-call inlining of private definitions. Templates are frozen at
//! pass entry: newly cloned calls are not reconsidered, so growth and recursion
//! do not depend on module order. ABI by-value arguments get a fresh snapshot.
use crate::Pass;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{
    BlockCall, DebugScope, EntityRef, Function, InsertPoint, Inst, InstData, Intrinsic, Module,
    Purpose, Symbol, Type,
};
use std::collections::{HashMap, HashSet};

pub struct Inline;
impl Pass for Inline {
    fn name(&self) -> &'static str {
        "inline"
    }
    fn description(&self) -> &'static str {
        "inline bounded private direct-call functions"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
}
const MAX_INSTS: usize = 64;
const MAX_BLOCKS: usize = 8;
const MAX_STACK: u64 = 512;
const MAX_GROWTH: usize = 2048;
const MAX_CALLS: usize = 128;

fn eligible(f: &Function) -> bool {
    f.local
        && !f.sig.variadic
        && !f.sig.returns_twice
        && f.personality.is_none()
        && f.entry_block().is_some()
        && f.blocks().count() <= MAX_BLOCKS
        && f.layout_insts().count() <= MAX_INSTS
        && f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>() <= MAX_STACK
        && f.sig
            .params
            .iter()
            .all(|p| !matches!(&p.purpose, Purpose::ByVal(l) if l.size() > 128))
        && f.layout_insts().all(|(_, i)| match f.inst_data(i) {
            InstData::Call { func, .. } => !f.callee(*func).sig.returns_twice,
            InstData::CallIndirect { sig, .. } => !f.signature(*sig).returns_twice,
            InstData::Intrinsic { id, .. } => matches!(
                id,
                Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Debugtrap
            ),
            InstData::VaArg { .. }
            | InstData::Invoke { .. }
            | InstData::LandingPad { .. }
            | InstData::EhTypeid { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::InlineAsm { .. } => false,
            _ => true,
        })
}

pub fn run(module: &mut Module) -> bool {
    let definitions: HashMap<_, _> = module
        .functions
        .iter()
        .map(|f| (f.name.clone(), f))
        .collect();
    // Reject every member of a recursive direct-call cycle, not just self calls.
    let recursive = |name: &str| {
        let mut seen = HashSet::new();
        let mut pending = vec![name];
        while let Some(n) = pending.pop() {
            if !seen.insert(n) {
                continue;
            }
            if let Some(f) = definitions.get(n) {
                for (_, i) in f.layout_insts() {
                    let reference = match f.inst_data(i) {
                        InstData::Call { func, .. }
                        | InstData::ReturnCall { func, .. }
                        | InstData::Invoke { func, .. } => Some(*func),
                        _ => None,
                    };
                    if let Some(reference) = reference {
                        let callee = f.callee(reference).name.as_str();
                        if callee == name {
                            return true;
                        }
                        if definitions.contains_key(callee) {
                            pending.push(callee);
                        }
                    }
                }
            }
        }
        false
    };
    let templates: HashMap<_, _> = module
        .functions
        .iter()
        .filter(|f| eligible(f) && !recursive(&f.name))
        .map(|f| (f.name.clone(), f.clone()))
        .collect();
    let mut changed = false;
    for caller in &mut module.functions {
        if caller.sig.returns_twice || caller.personality.is_some() {
            continue;
        }
        let calls: Vec<_> = caller
            .layout_insts()
            .filter_map(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. }).then_some(i))
            .collect();
        let mut growth = 0;
        let mut count = 0;
        for call in calls {
            let InstData::Call { func, .. } = caller.inst_data(call) else {
                unreachable!()
            };
            let decl = caller.callee(*func);
            let Some(callee) = templates.get(&decl.name) else {
                continue;
            };
            if decl.sig != callee.sig || callee.name == caller.name {
                continue;
            }
            // Account for the CFG glue, byval loads/stores, and all cloned IR.
            let cost = callee.layout_insts().count()
                + 2
                + callee
                    .sig
                    .params
                    .iter()
                    .map(|p| match &p.purpose {
                        Purpose::ByVal(l) => l.size() as usize * 5 + 2,
                        _ => 0,
                    })
                    .sum::<usize>();
            if count == MAX_CALLS || growth + cost > MAX_GROWTH {
                continue;
            }
            inline_call(caller, call, callee);
            growth += cost;
            count += 1;
            changed = true;
        }
    }
    changed
}

fn inline_call(caller: &mut Function, call: Inst, callee: &Function) {
    let InstData::Call { args, .. } = caller.inst_data(call).clone() else {
        unreachable!()
    };
    let origin = caller.inst_loc(call);
    let call_block = caller.inst_block(call);
    let continuation = caller.create_block();
    let results = caller.inst_results(call).to_vec();
    for old in results {
        let new = caller.append_block_param(continuation, caller.value_type(old));
        caller.replace_all_uses(old, new);
    }
    let tail: Vec<_> = caller
        .block_insts(call_block)
        .iter()
        .copied()
        .skip_while(|&i| i != call)
        .skip(1)
        .collect();
    for inst in tail {
        caller.move_inst(inst, InsertPoint::End(continuation));
    }
    let mut args: Vec<_> = args.into_iter().map(|v| caller.resolve(v)).collect();
    // Copy every byte, including padding. All snapshots precede callee effects.
    for (arg, param) in args.iter_mut().zip(&callee.sig.params) {
        if let Purpose::ByVal(layout) = &param.purpose {
            let slot = caller.create_stack_slot(layout.size() as u32, layout.align() as u32);
            let dst = caller.ins(InsertPoint::Before(call)).stack_addr(slot);
            let mut offset = 0;
            while offset < layout.size() {
                let bytes = [8, 4, 2, 1]
                    .into_iter()
                    .find(|&n| n <= layout.size() - offset)
                    .unwrap();
                let ty = match bytes {
                    8 => Type::I64,
                    4 => Type::I32,
                    2 => Type::I16,
                    _ => Type::I8,
                };
                let mut src_at = *arg;
                let mut dst_at = dst;
                if offset != 0 {
                    let n = caller
                        .ins(InsertPoint::Before(call))
                        .iconst(Type::I64, offset as i64);
                    src_at = caller.ins(InsertPoint::Before(call)).ptradd(*arg, n);
                    dst_at = caller.ins(InsertPoint::Before(call)).ptradd(dst, n);
                }
                let value = caller.ins(InsertPoint::Before(call)).load(ty, src_at);
                caller.ins(InsertPoint::Before(call)).store(value, dst_at);
                offset += bytes;
            }
            *arg = dst;
        }
    }
    let slots: HashMap<_, _> = callee
        .stack_slots()
        .map(|(slot, data)| (slot, caller.create_stack_slot(data.size, data.align)))
        .collect();
    let refs: HashMap<_, _> = callee
        .callees()
        .map(|(r, decl)| (r, caller.import_function(&decl.name, decl.sig.clone())))
        .collect();
    let data: HashMap<_, _> = callee
        .data_symbols()
        .map(|(r, name)| (r, caller.import_data(name)))
        .collect();
    let sigs: HashMap<_, _> = callee
        .signatures()
        .map(|(r, sig)| (r, caller.import_signature(sig.clone())))
        .collect();
    // IR has lexical scopes but no inline-subroutine metadata. Keep callee
    // source lines in a fresh lexical subtree beneath the caller's call site.
    let root = DebugScope::new(caller.debug_scopes.len());
    caller.debug_scopes.push(origin.and_then(|l| l.scope));
    let mut scopes = Vec::new();
    for parent in &callee.debug_scopes {
        let scope = DebugScope::new(caller.debug_scopes.len());
        caller
            .debug_scopes
            .push(Some(parent.map_or(root, |p| scopes[p.index()])));
        scopes.push(scope);
    }
    for var in &callee.debug_vars {
        let mut var = var.clone();
        var.slot = slots[&var.slot];
        var.scope = Some(var.scope.map_or(root, |s| scopes[s.index()]));
        // A callee parameter is a local variable in the caller, not a formal.
        var.param = false;
        caller.debug_vars.push(var);
    }
    let cfg = ControlFlowGraph::compute(callee);
    let dom = DominatorTree::compute(callee, &cfg);
    let mut blocks = HashMap::new();
    let mut values = HashMap::new();
    for &old in dom.rpo() {
        let new = caller.create_block();
        blocks.insert(old, new);
        for &v in callee.block_params(old) {
            values.insert(v, caller.append_block_param(new, callee.value_type(v)));
        }
    }
    for &block in dom.rpo() {
        let dest = blocks[&block];
        for &old in callee.block_insts(block) {
            let mut inst = callee.inst_data(old).clone();
            inst.for_each_operand_mut(|v| *v = values[&callee.resolve(*v)]);
            for edge in inst.branch_dests_mut() {
                edge.block = blocks[&edge.block];
            }
            match &mut inst {
                InstData::StackAddr { slot } => *slot = slots[slot],
                InstData::SymbolAddr { symbol } => {
                    *symbol = match *symbol {
                        Symbol::Func(r) => Symbol::Func(refs[&r]),
                        Symbol::Data(r) => Symbol::Data(data[&r]),
                    }
                }
                InstData::Call { func, .. } => *func = refs[func],
                InstData::CallIndirect { sig, .. } => *sig = sigs[sig],
                InstData::Return { args } => {
                    inst = InstData::Jump {
                        dest: BlockCall {
                            block: continuation,
                            args: args.clone(),
                        },
                    }
                }
                _ => {}
            }
            let new = caller.append_inst(dest, inst);
            for (&old_v, &new_v) in callee
                .inst_results(old)
                .iter()
                .zip(caller.inst_results(new))
            {
                values.insert(old_v, new_v);
            }
            let loc = callee
                .inst_loc(old)
                .map(|mut l| {
                    l.scope = Some(l.scope.map_or(root, |s| scopes[s.index()]));
                    l
                })
                .or(origin);
            caller.set_inst_loc(new, loc);
        }
    }
    caller.remove_inst(call);
    let jump = caller.append_inst(
        call_block,
        InstData::Jump {
            dest: BlockCall {
                block: blocks[&callee.entry_block().unwrap()],
                args,
            },
        },
    );
    caller.set_inst_loc(jump, origin);
}
