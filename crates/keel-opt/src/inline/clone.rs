//! Copies a callee body into its caller at one call site.
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    BlockCall, DebugScope, EntityRef, Function, InsertPoint, Inst, InstData, Intrinsic, Purpose,
    StackSlot, Symbol, Type,
};

pub(super) fn byval_copy_chunks(size: u64) -> impl Iterator<Item = (u64, Type)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let remaining = size - offset;
        let (bytes, ty) = match remaining {
            0 => return None,
            1 => (1, Type::I8),
            2..=3 => (2, Type::I16),
            4..=7 => (4, Type::I32),
            _ => (8, Type::I64),
        };
        let chunk = (offset, ty);
        offset += bytes;
        Some(chunk)
    })
}

// Promotion leaves declarations in the slot arena. Cost and cloning must
// agree on surviving storage, including implicit ABI and debugger homes.
pub(super) fn inline_stack_slots(callee: &Function) -> Vec<bool> {
    let mut used = vec![false; callee.num_stack_slots()];
    for var in &callee.debug_vars {
        used[var.slot.index()] = true;
    }
    for (_, inst) in callee.layout_insts() {
        match *callee.inst_data(inst) {
            InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                used[slot.index()] = true;
            }
            _ => {}
        }
    }
    used
}

/// The callee slots that inlining gives lifetime markers: those it uses
/// that have no markers of their own and are no debug or `AggregateVaArg`
/// home, which nothing tracks through markers. Each comes with the callee's
/// first entry-block `stack_addr` of it, if any; the entry runs once per call,
/// since nothing branches to it. Otherwise inlining adds an address.
pub(super) fn slots_to_mark(callee: &Function) -> Vec<(StackSlot, Option<Inst>)> {
    if callee.num_stack_slots() == 0 {
        return Vec::new();
    }
    let mut mark = inline_stack_slots(callee);
    for var in &callee.debug_vars {
        mark[var.slot.index()] = false;
    }
    let entry = callee.entry_block().expect("an inlined callee has a body");
    let mut first = vec![None; callee.num_stack_slots()];
    for (block, inst) in callee.layout_insts() {
        let data = callee.inst_data(inst);
        if let Some(slot) = callee.lifetime_marker_slot(inst) {
            mark[slot.index()] = false;
        } else if let InstData::AggregateVaArg { home, .. } = *data {
            mark[home.index()] = false;
        } else if let InstData::StackAddr { slot } = *data
            && block == entry
            && first[slot.index()].is_none()
            && callee.resolve(callee.first_result(inst)) == callee.first_result(inst)
        {
            first[slot.index()] = Some(inst);
        }
    }
    callee
        .stack_slots()
        .filter(|(slot, _)| mark[slot.index()])
        .map(|(slot, _)| (slot, first[slot.index()]))
        .collect()
}

pub(super) fn two_functions(
    functions: &mut [Function],
    caller: usize,
    callee: usize,
) -> (&mut Function, &Function) {
    assert_ne!(caller, callee);
    if caller < callee {
        let (low, high) = functions.split_at_mut(callee);
        (&mut low[caller], &high[0])
    } else {
        let (low, high) = functions.split_at_mut(caller);
        (&mut high[0], &low[callee])
    }
}

pub(super) fn inline_call(caller: &mut Function, call: Inst, callee: &Function) {
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
    let position = caller
        .block_insts(call_block)
        .iter()
        .position(|&i| i == call)
        .expect("a call is in its block");
    caller.move_insts_to_end(call_block, position + 1, continuation);
    let mut args = args;
    args.iter_mut().for_each(|v| *v = caller.resolve(*v));
    // Each new slot lives from just before its first write until control
    // returns to the continuation. A path that never returns needs no end.
    let mut lifetimes = Vec::new();
    // Copy every byte, including padding. All snapshots precede callee effects.
    for (arg, param) in args.iter_mut().zip(&callee.sig.params) {
        if let Purpose::ByVal(layout) = &param.purpose {
            let slot = caller.create_stack_slot(layout.size() as u32, layout.align() as u32);
            let dst = caller.ins(InsertPoint::Before(call)).stack_addr(slot);
            caller
                .ins(InsertPoint::Before(call))
                .intrinsic(Intrinsic::LifetimeStart, &[dst]);
            lifetimes.push(dst);
            for (offset, ty) in byval_copy_chunks(layout.size()) {
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
            }
            *arg = dst;
        }
    }
    let used = inline_stack_slots(callee);
    let slots: FxHashMap<_, _> = callee
        .stack_slots()
        .filter(|(slot, _)| used[slot.index()])
        .map(|(slot, data)| (slot, caller.create_stack_slot(data.size, data.align)))
        .collect();
    let refs: FxHashMap<_, _> = callee
        .callees()
        .map(|(r, decl)| {
            let imported = caller.import_function(&decl.name, decl.sig.clone());
            // Keep contracts and facts on the caller's existing calls.
            caller.set_callee_attrs(imported, decl.attrs.clone());
            (r, imported)
        })
        .collect();
    let data: FxHashMap<_, _> = callee
        .data_symbols()
        .map(|(r, name)| (r, caller.import_data(name)))
        .collect();
    let sigs: FxHashMap<_, _> = callee
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
    let analysis = crate::analysis::get(callee);
    let dom = &analysis.dom;
    let mut blocks = FxHashMap::default();
    let mut values = FxHashMap::default();
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
                InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                    *slot = slots[slot]
                }
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
    for (&old, &new) in &values {
        if let Some(expected) = callee.explicit_value_expectation(old) {
            caller.set_value_expectation(new, expected);
        }
    }
    // Mark each callee slot that has no markers of its own, reusing the
    // callee's entry-block address when it has one.
    for (slot, address) in slots_to_mark(callee) {
        let address = match address {
            Some(old) => {
                let new = values[&callee.first_result(old)];
                let keel_ir::ValueDef::Result(def, _) = caller.value_def(new) else {
                    unreachable!("a cloned stack_addr is an instruction result")
                };
                let block = caller.inst_block(def);
                let insts = caller.block_insts(block);
                let next = insts[insts.iter().position(|&i| i == def).unwrap() + 1];
                caller
                    .ins(InsertPoint::Before(next))
                    .intrinsic(Intrinsic::LifetimeStart, &[new]);
                new
            }
            None => {
                let address = caller
                    .ins(InsertPoint::Before(call))
                    .stack_addr(slots[&slot]);
                caller
                    .ins(InsertPoint::Before(call))
                    .intrinsic(Intrinsic::LifetimeStart, &[address]);
                address
            }
        };
        lifetimes.push(address);
    }
    let head = caller.block_insts(continuation)[0];
    for &address in &lifetimes {
        caller
            .ins(InsertPoint::Before(head))
            .intrinsic(Intrinsic::LifetimeEnd, &[address]);
    }
    caller.remove_inst(call);
    let jump = caller.append_inst(
        call_block,
        InstData::Jump {
            dest: BlockCall {
                block: blocks[&callee.entry_block().unwrap()],
                args: args.to_vec(),
            },
        },
    );
    caller.set_inst_loc(jump, origin);
}

#[cfg(test)]
mod byval_tests {
    use super::*;

    use crate::inline::cost::{byval_copy_cost, code_size, inline_cost};
    use crate::inline::run_with_report;
    #[test]
    fn copy_cost_matches_real_emitted_ir_for_every_supported_size() {
        for size in 1..=128 {
            // i8 fields give every byte size without aggregate tail padding.
            let fields = vec!["i8"; size].join(", ");
            let mut module = keel_ir::parse(&format!(
                "func @h(ptr byval {{{fields}}}) local {{\nblock0(v0: ptr):\n return\n}}\nfunc @f(ptr) {{\nblock0(v0: ptr):\n call @h(v0)\n return\n}}"
            ))
            .unwrap();
            keel_ir::verify_module(&module).unwrap();
            let before = code_size(&module.functions[1]);
            assert!(run_with_report(&mut module).changed);
            keel_ir::verify_module(&module).unwrap();
            let caller = &module.functions[1];
            assert_eq!(
                code_size(caller) - before,
                1 + byval_copy_cost(size as u64),
                "net growth for size {size}",
            );
            // The snapshot lives from before its copy to the continuation.
            let markers = caller
                .layout_insts()
                .filter(|&(_, i)| caller.inst_data(i).is_lifetime_marker())
                .count();
            assert_eq!(markers, 2, "size {size}");
            // This block contains only the snapshot and the jump replacing
            // the call. Count real IR, independently of the chunk iterator.
            let insts: Vec<_> = caller
                .block_insts(caller.entry_block().unwrap())
                .iter()
                .copied()
                .filter(|&i| !caller.inst_data(i).is_lifetime_marker())
                .collect();
            assert!(matches!(
                caller.inst_data(*insts.last().unwrap()),
                InstData::Jump { .. }
            ));
            assert_eq!(byval_copy_cost(size as u64), insts.len() - 1, "size {size}");
            let copied_bytes: u64 = insts
                .iter()
                .filter_map(|&inst| match caller.inst_data(inst) {
                    InstData::Load { .. } => Some(
                        caller
                            .value_type(caller.inst_results(inst)[0])
                            .bytes()
                            .unwrap() as u64,
                    ),
                    _ => None,
                })
                .sum();
            assert_eq!(copied_bytes, size as u64);
        }
    }

    #[test]
    fn dead_declarations_do_not_discount_required_byval_snapshots() {
        let mut module = keel_ir::parse(
            "func @helper(ptr byval [2 x i64]) local {
 ss0 = stack_slot 32, align 8
block0(v0: ptr):
 return
}
func @caller(ptr) {
block0(v0: ptr):
 call @helper(v0)
 return
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(
            inline_cost(&module.functions[0]),
            (1 + byval_copy_cost(16), 16)
        );
        let (caller, callee) = two_functions(&mut module.functions, 1, 0);
        let call = caller.block_insts(caller.entry_block().unwrap())[0];
        inline_call(caller, call, callee);
        assert_eq!(caller.num_stack_slots(), 1);
        assert_eq!(caller.stack_slots().next().unwrap().1.size, 16);
        assert_eq!(
            caller
                .layout_insts()
                .filter(|&(_, i)| matches!(caller.inst_data(i), InstData::Load { .. }))
                .count(),
            2,
            "the potentially trapping byval source reads remain"
        );
        keel_ir::verify_module(&module).unwrap();
    }

    #[test]
    fn implicit_aggregate_va_arg_home_is_charged_and_remapped() {
        let mut module = keel_ir::parse(
            "func @helper(ptr) -> ptr local {
 ss0 = stack_slot 16, align 8
 ss1 = stack_slot 16, align 8
 ss2 = stack_slot 16, align 8
block0(v0: ptr):
 v1 = va_arg_aggregate [2 x i64], v0, ss2
 return v1
}
func @caller(ptr) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = call @helper(v0)
 return v1
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(inline_cost(&module.functions[0]), (2, 16));
        // Unlike a scalar va_arg, this aggregate operation has an explicit
        // activation-local home, which must survive without a StackAddr.
        crate::run_passes(&mut module, &["inline"]).unwrap();
        let caller = &module.functions[1];
        assert_eq!(caller.num_stack_slots(), 2);
        let home = caller
            .layout_insts()
            .find_map(|(_, i)| match caller.inst_data(i) {
                InstData::AggregateVaArg { home, .. } => Some(*home),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            home.index(),
            1,
            "the old home ss2 gets a fresh caller identity"
        );
        assert_eq!(caller.stack_slots().nth(home.index()).unwrap().1.size, 16);
        keel_ir::verify_module(&module).unwrap();
    }

    #[test]
    fn copy_chunks_keep_wide_loads_and_odd_tails() {
        for (size, expected) in [
            (1, vec![Type::I8]),
            (7, vec![Type::I32, Type::I16, Type::I8]),
            (8, vec![Type::I64]),
            (9, vec![Type::I64, Type::I8]),
            (15, vec![Type::I64, Type::I32, Type::I16, Type::I8]),
            (16, vec![Type::I64, Type::I64]),
            (128, vec![Type::I64; 16]),
        ] {
            assert_eq!(
                byval_copy_chunks(size)
                    .map(|(_, ty)| ty)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        // Empty aggregates are rejected by IR verification; preserve the
        // emitter's stack_addr-only behavior if invoked on an empty layout.
        assert_eq!(byval_copy_chunks(0).count(), 0);
        assert_eq!(byval_copy_cost(0), 1);
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn importing_an_existing_callee_preserves_its_ordered_policy_once() {
        let mut module = keel_ir::parse("decl @work(i64) -> i64 cold optimize(\"Os\")\nfunc @helper(i64) -> i64 local {\nblock0(v0:i64):\n v1 = call @work(v0)\n return v1\n}\nfunc @caller(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @work(v0)\n v2 = call @helper(v1)\n return v2\n}\n").unwrap();
        let (caller, callee) = two_functions(&mut module.functions, 1, 0);
        let call = caller
            .layout_insts()
            .find_map(|(_, inst)| match caller.inst_data(inst) {
                InstData::Call { func, .. } if caller.callee(*func).name == "helper" => Some(inst),
                _ => None,
            })
            .unwrap();
        inline_call(caller, call, callee);
        let work = caller.callees().find(|(_, d)| d.name == "work").unwrap().1;
        assert_eq!(work.attrs.optimize.iter().count(), 1);
        keel_ir::verify_module(&module).unwrap();
    }
}
