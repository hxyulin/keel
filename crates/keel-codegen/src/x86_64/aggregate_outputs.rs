//! Keep private copies of two-word integer arguments in SSA when SysV assigns
//! each word the same register or stack offset as an ordinary i64 argument.
use super::sysv::{self, ArgLoc, RegPiece, Slot};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    AbiParam, BinaryOp, Block, CallConv, FuncRef, Function, Inst, InstData, Intrinsic, Purpose,
    Signature, StackSlot, Type, Value,
};
use std::borrow::Cow;

struct Plan {
    signature: Signature,
    split: Vec<bool>,
    order: Vec<usize>,
}

impl Plan {
    fn new(old: &Signature) -> Option<Self> {
        if old.cc != CallConv::C
            || old.variadic
            || old.returns_twice
            || old.x86_vector_abi.is_some()
        {
            return None;
        }
        if !old.params.iter().any(is_pair) {
            return None;
        }
        let placement = |sig: &Signature| sysv::signature_arg_locs(sig, &super::param_kinds(sig));
        let before = placement(old);
        let split: Vec<_> = old
            .params
            .iter()
            .zip(&before.locs)
            .map(|(param, loc)| {
                is_pair(param)
                    && (matches!(loc, ArgLoc::InRegs(_))
                        || matches!(loc, ArgLoc::OnStack(_))
                            && usize::from(before.ints) == sysv::INT_ARGS.len())
            })
            .collect();
        if !split.contains(&true) {
            return None;
        }
        let on_stack =
            |loc: &ArgLoc| matches!(loc, ArgLoc::Value(Slot::Stack(_)) | ArgLoc::OnStack(_));
        let mut order: Vec<_> = (0..old.params.len()).collect();
        // A rolled-back pair can be placed on the stack before the last GP
        // register is assigned. In the backend's local signature, assign the
        // registers first so two separate scalar words cannot claim it.
        if split
            .iter()
            .zip(&before.locs)
            .any(|(&split, loc)| split && on_stack(loc))
        {
            order.sort_by_key(|&index| on_stack(&before.locs[index]));
        }
        let mut signature = old.clone();
        signature.params.clear();
        for &index in &order {
            if split[index] {
                signature
                    .params
                    .extend([AbiParam::new(Type::I64), AbiParam::new(Type::I64)]);
            } else {
                signature.params.push(old.params[index].clone());
            }
        }
        let after = placement(&signature);
        if before.stack != after.stack
            || before.stack_align != after.stack_align
            || before.ints != after.ints
            || before.floats != after.floats
        {
            return None;
        }
        let mut positions = after.locs.iter();
        for &index in &order {
            let loc = &before.locs[index];
            let low = positions.next()?;
            if !split[index] {
                if loc != low {
                    return None;
                }
                continue;
            }
            let high = positions.next()?;
            let same = match (loc, low, high) {
                (
                    ArgLoc::InRegs(regs),
                    ArgLoc::Value(Slot::Regs(lo, None)),
                    ArgLoc::Value(Slot::Regs(hi, None)),
                ) => {
                    regs.as_slice()
                        == [
                            RegPiece {
                                offset: 0,
                                loc: *lo,
                            },
                            RegPiece {
                                offset: 8,
                                loc: *hi,
                            },
                        ]
                }
                (
                    ArgLoc::OnStack(from),
                    ArgLoc::Value(Slot::Stack(lo)),
                    ArgLoc::Value(Slot::Stack(hi)),
                ) => from == lo && *hi == from + 8,
                _ => false,
            };
            if !same {
                return None;
            }
        }
        Some(Self {
            signature,
            split,
            order,
        })
    }
}

fn is_pair(param: &AbiParam) -> bool {
    matches!(&param.purpose, Purpose::ByVal(l) if crate::aggregate_results::is_integer_pair(l))
}

#[derive(Default)]
struct Home {
    insts: Vec<Inst>,
    block: Option<Block>,
    words: [Option<Value>; 2],
    usage: Option<((Inst, usize), [Value; 2])>,
    escaped: bool,
}

impl Home {
    fn observe(&mut self, block: Block, inst: Inst, index: usize) {
        if self.block == Some(block)
            && let [Some(low), Some(high)] = self.words
            && self.usage.is_none()
        {
            self.usage = Some(((inst, index), [low, high]));
        } else {
            self.escaped = true;
        }
    }
}

pub(super) fn promote(func: &Function, debug: bool) -> Cow<'_, Function> {
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
    let mut plans: FxHashMap<_, _> = func
        .callees()
        .filter_map(|(id, callee)| Plan::new(&callee.sig).map(|plan| (id, plan)))
        .collect();
    if plans.is_empty() {
        return Cow::Borrowed(func);
    }

    let mut homes = FxHashMap::<StackSlot, Home>::default();
    let mut addresses = FxHashMap::<Value, (StackSlot, usize)>::default();
    let mut calls = Vec::<(Inst, FuncRef)>::new();
    for (_, inst) in func.layout_insts() {
        match func.inst_data(inst) {
            InstData::InlineAsm { .. }
            | InstData::Intrinsic {
                id: Intrinsic::FrameAddress,
                ..
            } => {
                return Cow::Borrowed(func);
            }
            data if crate::lowering::returns_twice(func, data) => return Cow::Borrowed(func),
            InstData::Invoke { func, .. } | InstData::ReturnCall { func, .. } => {
                plans.remove(func);
            }
            InstData::Call { func: callee, args } if plans.contains_key(callee) => {
                if args.has_explicit_abi() || args.len() != plans[callee].split.len() {
                    plans.remove(callee);
                } else {
                    calls.push((inst, *callee));
                }
            }
            InstData::StackAddr { slot } => {
                let storage = func.stack_slot(*slot);
                let value = func.first_result(inst);
                if storage.size == 16 && storage.align >= 8 && func.resolve(value) == value {
                    homes.entry(*slot).or_default().insts.push(inst);
                    addresses.insert(value, (*slot, 0));
                }
            }
            _ => {}
        }
    }
    calls.retain(|(_, callee)| plans.contains_key(callee));
    if calls.is_empty() || homes.is_empty() {
        return Cow::Borrowed(func);
    }

    for (_, inst) in func.layout_insts() {
        if let InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, offset],
        } = *func.inst_data(inst)
            && let Some(&(slot, 0)) = addresses.get(&func.resolve(base))
            && crate::fold::iconst(func, offset) == Some(8)
        {
            let value = func.first_result(inst);
            if func.resolve(value) == value {
                homes.get_mut(&slot).unwrap().insts.push(inst);
                addresses.insert(value, (slot, 1));
            }
        }
    }

    for (block, inst) in func.layout_insts() {
        if let Some(slot) = func.lifetime_marker_slot(inst)
            && let Some(home) = homes.get_mut(&slot)
        {
            home.words = [None; 2];
            home.insts.push(inst);
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
            InstData::Store { addr, value, flags }
                if !flags.volatile && flags.align <= 8 && func.value_type(*value) == Type::I64 =>
            {
                if let Some(&(slot, word)) = addresses.get(&func.resolve(*addr)) {
                    let home = homes.get_mut(&slot).unwrap();
                    if home.block != Some(block) {
                        home.words = [None; 2];
                        home.block = Some(block);
                    }
                    home.words[word] = Some(func.resolve(*value));
                    home.insts.push(inst);
                }
            }
            InstData::Call { func: callee, args } if plans.contains_key(callee) => {
                for (index, &value) in args.iter().enumerate() {
                    if let Some(&(slot, word)) = addresses.get(&func.resolve(value)) {
                        let home = homes.get_mut(&slot).unwrap();
                        if word == 0 && plans[callee].split[index] {
                            home.observe(block, inst, index);
                        } else {
                            home.escaped = true;
                        }
                    }
                }
            }
            data => data.for_each_operand(&mut escape),
        }
    }

    let isolated: FxHashMap<_, _> = homes
        .into_values()
        .filter_map(|home| {
            if !home.escaped {
                let (key, words) = home.usage?;
                Some((key, (words, home.insts)))
            } else {
                None
            }
        })
        .collect();
    let blocked: FxHashSet<_> = calls
        .iter()
        .filter(|(site, callee)| {
            plans[callee]
                .split
                .iter()
                .enumerate()
                .any(|(index, &split)| split && !isolated.contains_key(&(*site, index)))
        })
        .map(|(_, callee)| *callee)
        .collect();
    let mut calls = calls
        .into_iter()
        .filter(|(_, callee)| !blocked.contains(callee))
        .peekable();
    if calls.peek().is_none() {
        return Cow::Borrowed(func);
    }
    let mut output = func.clone();
    let mut rewritten = FxHashSet::default();
    // A function has one declaration per callee, so every call to a changed
    // declaration must cross the exact same physical ABI as before.
    for (site, callee) in calls {
        let plan = &plans[&callee];
        if rewritten.insert(callee) {
            output.callee_mut(callee).sig = plan.signature.clone();
        }
        let InstData::Call { args, .. } = func.inst_data(site) else {
            unreachable!()
        };
        let mut values = Vec::with_capacity(plan.signature.params.len());
        for &index in &plan.order {
            if plan.split[index] {
                let (words, insts) = &isolated[&(site, index)];
                values.extend(words);
                for &inst in insts {
                    output.remove_inst(inst);
                }
            } else {
                values.push(args[index]);
            }
        }
        let InstData::Call { args, .. } = output.inst_data_mut(site) else {
            unreachable!()
        };
        *args = values.into();
    }
    Cow::Owned(output)
}
