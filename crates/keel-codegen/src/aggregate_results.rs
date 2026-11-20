//! Keep register-returned integer pairs in SSA when the caller's result home
//! has no observers beyond same-block word loads.
use std::borrow::Cow;

use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    AbiParam, BinaryOp, Block, CallConv, FuncRef, Function, InsertPoint, Inst, InstData, Intrinsic,
    Layout, MemoryFlags, Purpose, StackSlot, Type, Value,
};

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
            sig.cc == CallConv::C
                && !sig.variadic
                && !sig.returns_twice
                && sig.returns.is_empty()
                && sig.x86_vector_abi.is_none()
                && callee.attrs.contract.parameters.is_empty()
                && callee.attrs.contract.parameter_ranges.is_empty()
                && matches!(sig.params.first().map(|p| &p.purpose),
                    Some(Purpose::Sret(layout)) if is_integer_pair(layout))
        })
        .map(|(id, _)| id)
        .collect();
    if candidates.is_empty() {
        return Cow::Borrowed(func);
    }

    let mut homes = FxHashMap::<StackSlot, Home>::default();
    let mut addresses = FxHashMap::<Value, (StackSlot, usize)>::default();
    let mut calls = Vec::<(Inst, FuncRef)>::new();
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
                    calls.push((inst, *func));
                }
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
    calls.retain(|(_, callee)| !blocked.contains(callee));
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
            InstData::Call { args, .. } if calls_by_inst.contains_key(&inst) => {
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
    let chosen: FxHashSet<_> = isolated.keys().map(|site| calls_by_inst[site]).collect();
    if chosen.is_empty() {
        return Cow::Borrowed(func);
    }
    let mut output = func.clone();
    for &callee in &chosen {
        let sig = &mut output.callee_mut(callee).sig;
        sig.params.remove(0);
        sig.returns = vec![AbiParam::new(Type::I64); 2];
    }

    // A function has one declaration per callee. Rewrite each use of the
    // changed declaration, materializing the result at the original address
    // whenever later instructions can still observe its storage.
    for (site, callee) in calls {
        if !chosen.contains(&callee) {
            continue;
        }
        let InstData::Call { args, .. } = func.inst_data(site) else {
            unreachable!()
        };
        let addr = args[0];
        let at = InsertPoint::Before(site);
        let new = output.insert_inst(
            at,
            InstData::Call {
                func: callee,
                args: args.iter().copied().skip(1).collect::<Vec<_>>().into(),
            },
        );
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
