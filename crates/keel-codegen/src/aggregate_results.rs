//! Keep register-returned integer pairs in SSA when the caller's result home
//! has no observers beyond same-block word loads. Indirect calls use a
//! separate signature so other sites can still materialize the pair.
use std::borrow::Cow;

use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    AbiParam, BinaryOp, Block, CallConv, FuncRef, Function, InsertPoint, Inst, InstData, Intrinsic,
    Layout, MemoryFlags, Purpose, Signature, StackSlot, Type, Value,
};

#[derive(Clone, Copy)]
enum CallSite {
    Direct(FuncRef),
    Indirect,
}

fn is_register_pair_result(sig: &Signature) -> bool {
    sig.cc == CallConv::C
        && !sig.variadic
        && !sig.returns_twice
        && sig.returns.is_empty()
        && sig.x86_vector_abi.is_none()
        && matches!(
            sig.params.first().map(|p| &p.purpose),
            Some(Purpose::Sret(layout)) if is_integer_pair(layout)
        )
}

fn use_register_results(sig: &mut Signature) {
    sig.params.remove(0);
    sig.returns = vec![AbiParam::new(Type::I64); 2];
}

#[derive(Default)]
struct Home {
    base: Option<Inst>,
    upper: Option<Inst>,
    result: Option<(Inst, Block)>,
    loads: Vec<(Inst, usize)>,
    markers: Vec<Inst>,
    invalidated: bool,
    escaped: bool,
}

pub(crate) fn promote(func: &Function, debug: bool) -> Cow<'_, Function> {
    if debug
        || func.num_stack_slots() == 0
        || func.num_values() > 100_000
        || func.debug_loc.is_some()
        || !func.debug_vars.is_empty()
        || func.personality.is_some()
        || func.sig.returns_twice
    {
        return Cow::Borrowed(func);
    }
    // SysV and the AArch64 ABIs return this layout in their two integer
    // result registers without passing a hidden address. Two i64 results
    // leave the other arguments in their original places.
    let candidates: FxHashSet<_> = func
        .callees()
        .filter(|(_, callee)| {
            let sig = &callee.sig;
            is_register_pair_result(sig)
                && callee.attrs.contract.parameters.is_empty()
                && callee.attrs.contract.parameter_ranges.is_empty()
        })
        .map(|(id, _)| id)
        .collect();
    if candidates.is_empty()
        && !func
            .signatures()
            .any(|(_, sig)| is_register_pair_result(sig))
    {
        return Cow::Borrowed(func);
    }

    let mut homes = FxHashMap::<StackSlot, Home>::default();
    let mut addresses = FxHashMap::<Value, (StackSlot, usize)>::default();
    let mut calls = Vec::<(Inst, CallSite)>::new();
    let mut blocked = FxHashSet::default();
    for (_, inst) in func.layout_insts() {
        match func.inst_data(inst) {
            InstData::InlineAsm { .. }
            | InstData::AggregateVaArg { .. }
            | InstData::Intrinsic {
                id: Intrinsic::FrameAddress,
                ..
            } => {
                return Cow::Borrowed(func);
            }
            data if crate::lowering::returns_twice(func, data) => {
                return Cow::Borrowed(func);
            }
            InstData::Invoke { func, .. } | InstData::ReturnCall { func, .. } => {
                blocked.insert(*func);
            }
            InstData::Call { func, args } if candidates.contains(func) => {
                if args.has_explicit_abi() {
                    blocked.insert(*func);
                } else {
                    calls.push((inst, CallSite::Direct(*func)));
                }
            }
            InstData::CallIndirect { sig, args, .. }
                if !args.has_explicit_abi() && is_register_pair_result(func.signature(*sig)) =>
            {
                calls.push((inst, CallSite::Indirect));
            }
            InstData::StackAddr { slot } => {
                let storage = func.stack_slot(*slot);
                let value = func.first_result(inst);
                if storage.size == 16 && storage.align >= 8 && func.resolve(value) == value {
                    let home = homes.entry(*slot).or_default();
                    home.escaped |= home.base.replace(inst).is_some();
                    addresses.insert(value, (*slot, 0));
                }
            }
            _ => {}
        }
    }
    calls.retain(|(_, site)| !matches!(site, CallSite::Direct(callee) if blocked.contains(callee)));
    if calls.is_empty() || homes.is_empty() {
        return Cow::Borrowed(func);
    }

    for (_, inst) in func.layout_insts() {
        let InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, offset],
        } = *func.inst_data(inst)
        else {
            continue;
        };
        if let Some(&(slot, 0)) = addresses.get(&func.resolve(base))
            && crate::fold::iconst(func, offset) == Some(8)
        {
            let value = func.first_result(inst);
            if func.resolve(value) == value {
                let home = homes.get_mut(&slot).unwrap();
                home.escaped |= home.upper.replace(inst).is_some();
                addresses.insert(value, (slot, 1));
            }
        }
    }

    let calls_by_inst: FxHashMap<_, _> = calls.iter().copied().collect();
    for (block, inst) in func.layout_insts() {
        if let Some(slot) = func.lifetime_marker_slot(inst) {
            if let Some(home) = homes.get_mut(&slot) {
                home.markers.push(inst);
                home.invalidated |= home.result.is_some();
            }
            continue;
        }
        let mut escape = |value| {
            if let Some(&(slot, _)) = addresses.get(&func.resolve(value)) {
                homes.get_mut(&slot).unwrap().escaped = true;
            }
        };
        match func.inst_data(inst) {
            InstData::Binary {
                op: BinaryOp::Ptradd,
                ..
            } if addresses.contains_key(&func.first_result(inst)) => {}
            data @ (InstData::Call { args, .. } | InstData::CallIndirect { args, .. })
                if calls_by_inst.contains_key(&inst) =>
            {
                if let InstData::CallIndirect { callee, .. } = data {
                    escape(*callee);
                }
                if let Some(&(slot, 0)) = addresses.get(&func.resolve(args[0])) {
                    let home = homes.get_mut(&slot).unwrap();
                    home.escaped |= home.result.replace((inst, block)).is_some();
                }
                for &value in args.iter().skip(usize::from(matches!(
                    addresses.get(&func.resolve(args[0])),
                    Some((_, 0))
                ))) {
                    if let Some(&(slot, _)) = addresses.get(&func.resolve(value)) {
                        homes.get_mut(&slot).unwrap().escaped = true;
                    }
                }
            }
            InstData::Load {
                ty: Type::I64,
                addr,
                flags,
            } if !flags.volatile && flags.align <= 8 => {
                if let Some(&(slot, word)) = addresses.get(&func.resolve(*addr)) {
                    let home = homes.get_mut(&slot).unwrap();
                    let value = func.first_result(inst);
                    if !home.invalidated
                        && home.result.is_some_and(|(_, from)| from == block)
                        && func.resolve(value) == value
                    {
                        home.loads.push((inst, word));
                    } else {
                        home.escaped = true;
                    }
                }
            }
            data => data.for_each_operand(&mut escape),
        }
    }

    let isolated: FxHashMap<_, _> = homes
        .into_values()
        .filter(|home| !home.escaped && !home.loads.is_empty())
        .filter_map(|home| home.result.map(|(site, _)| (site, home)))
        .collect();
    if isolated.is_empty() {
        return Cow::Borrowed(func);
    }
    let chosen: FxHashSet<_> = isolated
        .keys()
        .filter_map(|site| match calls_by_inst[site] {
            CallSite::Direct(callee) => Some(callee),
            CallSite::Indirect => None,
        })
        .collect();
    let mut output = func.clone();
    for &callee in &chosen {
        use_register_results(&mut output.callee_mut(callee).sig);
    }

    // A function has one declaration per direct callee. Rewrite each use of
    // a changed declaration, materializing the result at the original address
    // whenever later instructions can still observe its storage.
    for (site, call) in calls {
        let (addr, data) = match (call, func.inst_data(site)) {
            (CallSite::Direct(callee), InstData::Call { args, .. }) => {
                if !chosen.contains(&callee) {
                    continue;
                }
                (
                    args[0],
                    InstData::Call {
                        func: callee,
                        args: args.iter().copied().skip(1).collect::<Vec<_>>().into(),
                    },
                )
            }
            (CallSite::Indirect, InstData::CallIndirect { sig, callee, args }) => {
                if !isolated.contains_key(&site) {
                    continue;
                }
                // Other call sites may still need the original result home.
                let mut sig = func.signature(*sig).clone();
                use_register_results(&mut sig);
                (
                    args[0],
                    InstData::CallIndirect {
                        sig: output.import_signature(sig),
                        callee: *callee,
                        args: args.iter().copied().skip(1).collect::<Vec<_>>().into(),
                    },
                )
            }
            _ => unreachable!("recorded call site"),
        };
        let at = InsertPoint::Before(site);
        let new = output.insert_inst(at, data);
        let [low, high] = *output.inst_results(new) else {
            unreachable!("two-word result");
        };
        if let Some(home) = isolated.get(&site) {
            for &(load, word) in &home.loads {
                output.replace_all_uses(output.first_result(load), [low, high][word]);
                output.remove_inst(load);
            }
            for &marker in &home.markers {
                output.remove_inst(marker);
            }
            if let Some(upper) = home.upper {
                output.remove_inst(upper);
            }
            output.remove_inst(home.base.unwrap());
        } else {
            let flags = MemoryFlags::default();
            output.insert_inst(
                at,
                InstData::Store {
                    value: low,
                    addr,
                    flags,
                },
            );
            let offset = output.insert_inst(at, InstData::iconst(Type::I64, 8));
            let upper = output.insert_inst(
                at,
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args: [addr, output.first_result(offset)],
                },
            );
            output.insert_inst(
                at,
                InstData::Store {
                    value: high,
                    addr: output.first_result(upper),
                    flags,
                },
            );
        }
        output.remove_inst(site);
    }
    Cow::Owned(output)
}

pub(crate) fn is_integer_pair(layout: &Layout) -> bool {
    matches!(layout, Layout::Array(t, 2) if **t == Layout::Scalar(Type::I64))
        || matches!(layout, Layout::Struct(fs) if fs.as_slice() == [Layout::Scalar(Type::I64), Layout::Scalar(Type::I64)])
}

/// Return the two words directly when the virtual result address is only
/// used by exact scalar stores in return blocks.
pub(crate) fn promote_exits(
    func: &Function,
    debug: bool,
    goal: crate::CodegenGoal,
) -> Cow<'_, Function> {
    if debug
        || goal != crate::CodegenGoal::Speed
        || !is_register_pair_result(&func.sig)
        || func.num_values() > 10_000
        || func.debug_loc.is_some()
        || !func.debug_vars.is_empty()
        || func.debug_return.is_some()
        || func.personality.is_some()
        || func.attrs.optnone
        || func.attrs.naked
        || func.attrs.interrupt
        || !func.attrs.contract.parameters.is_empty()
        || !func.attrs.contract.parameter_ranges.is_empty()
        || !func.restrict_params.is_empty()
    {
        return Cow::Borrowed(func);
    }
    let Some(entry) = func.entry_block() else {
        return Cow::Borrowed(func);
    };
    let low = func.block_params(entry)[0];
    if func.resolve(low) != low {
        return Cow::Borrowed(func);
    }

    let mut upper = FxHashMap::default();
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        if matches!(
            data,
            InstData::InlineAsm { .. }
                | InstData::Intrinsic {
                    id: Intrinsic::FrameAddress,
                    ..
                }
                | InstData::ReturnCall { .. }
                | InstData::ReturnCallIndirect { .. }
        ) || crate::lowering::returns_twice(func, data)
            || data.branch_dests().iter().any(|dest| dest.block == entry)
        {
            return Cow::Borrowed(func);
        }
        if let InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, offset],
        } = *data
            && func.resolve(base) == low
            && crate::fold::iconst(func, offset) == Some(8)
        {
            let high = func.first_result(inst);
            if func.resolve(high) != high {
                return Cow::Borrowed(func);
            }
            upper.insert(high, inst);
        }
    }

    let mut exits = Vec::new();
    for block in func.blocks() {
        let mut words = [None, None];
        for &inst in func.block_insts(block) {
            let data = func.inst_data(inst);
            match data {
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    ..
                } if upper.contains_key(&func.first_result(inst)) => {}
                InstData::Store { addr, value, flags }
                    if func.resolve(*addr) == low || upper.contains_key(&func.resolve(*addr)) =>
                {
                    let word = usize::from(func.resolve(*addr) != low);
                    if flags.volatile
                        || flags.align > 8
                        || func.value_type(*value) != Type::I64
                        || words[word].replace((inst, *value)).is_some()
                    {
                        return Cow::Borrowed(func);
                    }
                }
                InstData::Return { .. } => {
                    let [Some(lo), Some(hi)] = words else {
                        return Cow::Borrowed(func);
                    };
                    exits.push((inst, lo, hi));
                    words = [None, None];
                }
                _ => {
                    let mut escaped = false;
                    data.for_each_operand(|operand| {
                        let operand = func.resolve(operand);
                        escaped |= operand == low || upper.contains_key(&operand);
                    });
                    if escaped {
                        return Cow::Borrowed(func);
                    }
                }
            }
        }
        if words.iter().any(Option::is_some) {
            return Cow::Borrowed(func);
        }
    }
    if exits.is_empty() {
        return Cow::Borrowed(func);
    }

    let mut output = func.clone();
    use_register_results(&mut output.sig);
    output.remove_block_param(entry, 0);
    for (ret, (low_store, low_value), (high_store, high_value)) in exits {
        output.replace_inst(
            ret,
            InstData::Return {
                args: vec![low_value, high_value],
            },
        );
        output.remove_inst(low_store);
        output.remove_inst(high_store);
    }
    for inst in upper.into_values() {
        output.remove_inst(inst);
    }
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use super::promote_exits;
    use crate::CodegenGoal;
    use std::borrow::Cow;

    #[test]
    fn a_register_return_home_must_have_no_observers() {
        let source = "decl @observe(ptr) cc(c)\n\
            func @pair(ptr sret {i64,i64},i64,i64) cc(c) {\n\
            block0(v0:ptr,v1:i64,v2:i64):\n\
                v3 = iconst.i64 8\n\
                v4 = ptradd v0,v3\n\
                store v1,v0\n\
                store v2,v4\n\
                return\n\
            }";
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(matches!(
            promote_exits(&module.functions[0], false, CodegenGoal::Speed),
            Cow::Owned(_),
        ));

        for replacement in ["store volatile v1,v0", "call @observe(v0)\nstore v1,v0"] {
            let module = keel_ir::parse(&source.replace("store v1,v0", replacement)).unwrap();
            keel_ir::verify_module(&module).unwrap();
            assert!(
                matches!(
                    promote_exits(&module.functions[0], false, CodegenGoal::Speed),
                    Cow::Borrowed(_),
                ),
                "{replacement}",
            );
        }
    }
}
