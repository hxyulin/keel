//! Byte-aware forwarding in nonescaping fixed stack objects. Only integer
//! loads up to 64 bits are reconstructed from earlier stores in the same
//! block, or integer subranges of uniquely written dominating pieces. Exact
//! types can also forward from uniquely written dominating pieces. This
//! handles union temporaries exposed by aggregate helpers without assuming that
//! arbitrary pointers cannot alias.
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InsertPoint, Inst, InstData, StackSlot, Type, Value,
    ValueDef,
};

/// Opt-in composite pass: retain ordinary forwarding, then clean up the CFG
/// only when it directly exposes a previously nonconstant branch operand.
/// Existing constants and expressions needing another folding pass do not
/// request cleanup. The ordinary `run` path does not collect this snapshot.
pub fn run_with_cfg(func: &mut Function) -> bool {
    let conditional_inputs: Vec<_> = func
        .blocks()
        .filter_map(|block| {
            let &term = func.block_insts(block).last()?;
            let value = match *func.inst_data(term) {
                InstData::Brif { cond, .. } => cond,
                InstData::Switch { arg, .. } => arg,
                _ => return None,
            };
            crate::const_fold::constant(func, value)
                .is_none()
                .then_some(value)
        })
        .collect();
    let changed = run(func);
    if changed
        && conditional_inputs
            .iter()
            .any(|&value| crate::const_fold::constant(func, value).is_some())
    {
        crate::simplify_cfg::run(func);
    }
    changed
}

pub fn run(func: &mut Function) -> bool {
    // Every rewrite below starts from a slot address.
    if func.num_stack_slots() == 0 {
        return false;
    }
    // Propagate addresses only along constant in-bounds ptradd chains. Any
    // other use, including storing/passing/returning an address, escapes it.
    let mut addresses = FxHashMap::default();
    // One scan gathers the slot addresses and pointer offsets, and the
    // frame observers and stack marks that make this pass give up.
    let mut offsets = Vec::new();
    // These operations can inspect the frame or invalidate captured state,
    // even without receiving a fixed-slot address as an explicit argument.
    let mut observer = false;
    // A saved SP can itself become a frame observer if inspected, converted,
    // passed or used as an address. Only opaque marks used directly by restore
    // qualify; do not infer this from the absence of fixed-slot address escapes.
    let mut marks = FxHashSet::default();
    let mut aliased_mark = false;
    let mut restore = false;
    for (_, inst) in func.layout_insts() {
        match *func.inst_data(inst) {
            InstData::StackAddr { slot } => {
                let value = func.first_result(inst);
                if func.resolve(value) == value {
                    addresses.insert(value, (slot, 0usize));
                }
            }
            InstData::Binary {
                op: BinaryOp::Ptradd,
                ..
            } => offsets.push(inst),
            InstData::InlineAsm { .. } => observer = true,
            InstData::Intrinsic { id, .. } => match id {
                keel_ir::Intrinsic::FrameAddress => observer = true,
                keel_ir::Intrinsic::StackSave => {
                    let mark = func.first_result(inst);
                    aliased_mark |= func.resolve(mark) != mark;
                    marks.insert(mark);
                }
                keel_ir::Intrinsic::StackRestore => restore = true,
                _ => {}
            },
            InstData::Call { func: callee, .. } | InstData::Invoke { func: callee, .. } => {
                observer |= func.callee(callee).sig.returns_twice
            }
            InstData::CallIndirect { sig, .. } => observer |= func.signature(sig).returns_twice,
            _ => {}
        }
    }
    // Every rewrite below starts from a fixed slot address.
    if addresses.is_empty() || observer || aliased_mark {
        return false;
    }
    let sizes: FxHashMap<_, _> = func
        .stack_slots()
        .map(|(s, d)| (s, d.size as usize))
        .collect();
    loop {
        let before = addresses.len();
        for &inst in &offsets {
            if let InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } = *func.inst_data(inst)
                && let Some(&(slot, offset)) = addresses.get(&func.resolve(args[0]))
                && let Some(n) = constant(func, args[1]).and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(n).filter(|&n| n <= sizes[&slot])
                && func.resolve(func.first_result(inst)) == func.first_result(inst)
            {
                addresses.insert(func.first_result(inst), (slot, end));
            }
        }
        if addresses.len() == before {
            break;
        }
    }
    // Debugger-visible homes may be edited at a breakpoint. Their reads
    // must continue to observe memory even when the program never escapes
    // the address itself.
    let mut escaped: FxHashSet<_> = func.debug_vars.iter().map(|v| v.slot).collect();
    // The ABI instruction writes this home without a StackAddr SSA operand.
    escaped.extend(func.layout_insts().filter_map(|(_, i)| match func.inst_data(i) {
        InstData::AggregateVaArg { home, .. } => Some(*home), _ => None,
    }));
    // Without marks, any restore is of an unknown stack pointer.
    if marks.is_empty() && restore {
        return false;
    }
    // Nothing changes the function before the rewrite loop, so the later
    // scans revisit the same instructions in the same order.
    for (_, inst) in func.layout_insts().filter(|_| !marks.is_empty()) {
        let data = func.inst_data(inst);
        if let InstData::Intrinsic {
            id: keel_ir::Intrinsic::StackRestore,
            args,
        } = data
            && !marks.contains(&func.resolve(args[0]))
        {
            return false;
        }
        let mut observed = false;
        data.for_each_operand(|v| {
            if marks.contains(&func.resolve(v))
                && !matches!(
                    data,
                    InstData::Intrinsic {
                        id: keel_ir::Intrinsic::StackRestore,
                        ..
                    }
                )
            {
                observed = true;
            }
        });
        if observed {
            return false;
        }
    }
    // Opaque stack marks delimit dynamic lifetimes. Restrict such functions
    // to local intervals: no dominance facts
    // may cross a mark/restore, even through another block. Keep their stores
    // as well; this path deliberately makes no whole-function lifetime proof.
    let local_only = !marks.is_empty() || restore;

    // Most operands are not addresses; a dense bit spares their lookups.
    let mut is_address = vec![0u64; func.num_values().div_ceil(64)];
    for v in addresses.keys() {
        is_address[v.index() / 64] |= 1 << (v.index() % 64);
    }
    for (_, inst) in func.layout_insts() {
        let data = func.inst_data(inst);
        data.for_each_operand(|v| {
            let v = func.resolve(v);
            if is_address[v.index() / 64] & (1 << (v.index() % 64)) == 0 {
                return;
            }
            let Some(&(slot, offset)) = addresses.get(&v) else {
                return;
            };
            let allowed = match data {
                InstData::Load { addr, flags, ty } => {
                    !flags.volatile
                        && func.resolve(*addr) == v
                        && offset + ty.bytes().unwrap_or(8) as usize <= sizes[&slot]
                }
                InstData::Store { addr, value, flags } => {
                    !flags.volatile
                        && func.resolve(*addr) == v
                        && func.resolve(*value) != v
                        && offset + func.value_type(*value).bytes().unwrap_or(8) as usize
                            <= sizes[&slot]
                }
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args,
                } => func.resolve(args[0]) == v && addresses.contains_key(&func.first_result(inst)),
                _ => false,
            };
            if !allowed {
                escaped.insert(slot);
            }
        });
    }
    // Only accesses to a nonescaping slot are forwarded or removed.
    if addresses.values().all(|(slot, _)| escaped.contains(slot)) {
        return false;
    }
    // Count all writers, including writes in unreachable blocks and writes
    // of a different type. A cross-block fact is eligible only if every byte
    // in its range has exactly one syntactic writer in the whole function.
    // This avoids phi nodes and iterative memory dataflow at joins/loops.
    let mut writers = FxHashMap::<(StackSlot, usize), usize>::default();
    let mut stores = Vec::new();
    for block in func.blocks().filter(|_| !local_only) {
        for (position, &inst) in func.block_insts(block).iter().enumerate() {
            if let InstData::Store { addr, value, .. } = *func.inst_data(inst)
                && let Some(&(slot, offset)) = addresses.get(&func.resolve(addr))
                && !escaped.contains(&slot)
            {
                let value = func.resolve(value);
                let ty = func.value_type(value);
                let width = ty.bytes().unwrap_or(8) as usize;
                for n in offset..offset + width {
                    *writers.entry((slot, n)).or_default() += 1;
                }
                stores.push(((slot, offset, ty), (block, position, value), width));
            }
        }
    }
    let unique: FxHashMap<_, _> = stores
        .into_iter()
        .filter(|&((slot, offset, _), _, width)| {
            (offset..offset + width).all(|n| writers[&(slot, n)] == 1)
        })
        .map(|(key, fact, _)| (key, fact))
        .collect();
    // A uniquely written integer piece also defines its integer subranges.
    // Enumerate at most 20 proper subranges per eight-byte store, retaining
    // the same whole-range writer proof. Never reinterpret floats or pointers.
    let mut subranges = FxHashMap::default();
    for (&(slot, offset, ty), &(block, position, value)) in &unique {
        let Some(width) = integer_width(ty) else {
            continue;
        };
        for subty in [Type::I8, Type::I16, Type::I32] {
            let subwidth = integer_width(subty).unwrap();
            if subwidth >= width {
                continue;
            }
            for source in 0..=width - subwidth {
                subranges.insert(
                    (slot, offset + source, subty),
                    (block, position, value, source),
                );
            }
        }
    }
    // Avoid building dominance for functions with only local candidates.
    let cross_block = func.blocks().any(|block| {
        func.block_insts(block).iter().any(|&inst| {
            if let InstData::Load { addr, ty, .. } = *func.inst_data(inst)
                && let Some(&(slot, offset)) = addresses.get(&func.resolve(addr))
            {
                unique
                    .get(&(slot, offset, ty))
                    .is_some_and(|&(store_block, _, _)| store_block != block)
                    || subranges
                        .get(&(slot, offset, ty))
                        .is_some_and(|&(store_block, _, _, _)| store_block != block)
            } else {
                false
            }
        })
    });
    let analysis = cross_block.then(|| crate::analysis::get(func));
    let dominators = analysis.as_ref().map(|analysis| &analysis.dom);
    let mut changed = false;
    let mut remaining_loads = FxHashSet::default();
    // Per-block state and the block's instruction snapshot, reused.
    let mut bytes: FxHashMap<(StackSlot, usize), (Value, usize)> = FxHashMap::default();
    let mut exact: FxHashMap<(StackSlot, usize, Type), Value> = FxHashMap::default();
    let mut insts = Vec::new();
    for block in func.blocks().collect::<Vec<_>>() {
        bytes.clear();
        exact.clear();
        insts.clear();
        insts.extend_from_slice(func.block_insts(block));
        for (position, &inst) in insts.iter().enumerate() {
            match *func.inst_data(inst) {
                InstData::Store { addr, value, .. } => {
                    let Some(&(slot, offset)) = addresses.get(&func.resolve(addr)) else {
                        continue;
                    };
                    if escaped.contains(&slot) {
                        continue;
                    }
                    let value = func.resolve(value);
                    let ty = func.value_type(value);
                    let width = ty.bytes().unwrap_or(8) as usize;
                    exact.retain(|&(s, o, t), _| {
                        s != slot
                            || o + t.bytes().unwrap_or(8) as usize <= offset
                            || offset + width <= o
                    });
                    exact.insert((slot, offset, ty), value);
                    for n in 0..width {
                        bytes.remove(&(slot, offset + n));
                    }
                    if integer_width(ty).is_some() {
                        for n in 0..width {
                            bytes.insert((slot, offset + n), (value, n));
                        }
                    }
                }
                InstData::Load { addr, ty, .. } => {
                    let Some(&(slot, offset)) = addresses.get(&func.resolve(addr)) else {
                        continue;
                    };
                    // A live instruction may already have an aliased result
                    // in an in-memory Function. Never overwrite that alias.
                    if escaped.contains(&slot)
                        || func.resolve(func.first_result(inst)) != func.first_result(inst)
                    {
                        remaining_loads.insert(slot);
                        continue;
                    }
                    let global = unique.get(&(slot, offset, ty)).and_then(
                        |&(store_block, store_position, value)| {
                            let dominates = if store_block == block {
                                store_position < position
                            } else {
                                dominators.as_ref()?.dominates(store_block, block)
                            };
                            dominates.then_some(value)
                        },
                    );
                    let forwarded = exact
                        .get(&(slot, offset, ty))
                        .copied()
                        .or(global)
                        .or_else(|| {
                            let width = integer_width(ty)?;
                            let parts = (0..width)
                                .map(|n| bytes.get(&(slot, offset + n)).copied())
                                .collect::<Option<Vec<_>>>()?;
                            Some(reconstruct(func, inst, ty, &parts))
                        })
                        .or_else(|| {
                            let &(store_block, store_position, value, source) =
                                subranges.get(&(slot, offset, ty))?;
                            let dominates = if store_block == block {
                                store_position < position
                            } else {
                                dominators.as_ref()?.dominates(store_block, block)
                            };
                            if !dominates {
                                return None;
                            }
                            let parts: Vec<_> = (0..integer_width(ty)?)
                                .map(|n| (value, source + n))
                                .collect();
                            Some(reconstruct(func, inst, ty, &parts))
                        });
                    if let Some(value) = forwarded {
                        func.replace_all_uses(func.first_result(inst), value);
                        func.remove_inst(inst);
                        changed = true;
                    } else {
                        remaining_loads.insert(slot);
                    }
                }
                // Nonescaping fixed slots cannot be touched by an ordinary
                // call. Stack lifetime operations, however, are barriers.
                InstData::Intrinsic {
                    id: keel_ir::Intrinsic::StackSave | keel_ir::Intrinsic::StackRestore,
                    ..
                } => {
                    bytes.clear();
                    exact.clear();
                }
                _ => {}
            }
        }
    }
    // Discard private stores only when every read of that object has
    // disappeared, across all blocks. Debug homes were excluded above.
    for (_, inst) in func.layout_insts().collect::<Vec<_>>() {
        if let InstData::Store { addr, .. } = *func.inst_data(inst)
            && let Some(&(slot, _)) = addresses.get(&func.resolve(addr))
            && !local_only
            && !escaped.contains(&slot)
            && !remaining_loads.contains(&slot)
        {
            func.remove_inst(inst);
            changed = true;
        }
    }
    changed
}
fn integer_width(ty: Type) -> Option<usize> {
    match ty {
        Type::I8 => Some(1),
        Type::I16 => Some(2),
        Type::I32 => Some(4),
        Type::I64 => Some(8),
        _ => None,
    }
}
fn constant(f: &Function, v: Value) -> Option<i64> {
    if let ValueDef::Result(i, _) = f.value_def(f.resolve(v))
        && let InstData::Iconst { imm, .. } = f.inst_data(i)
    {
        return Some(*imm);
    }
    None
}
fn reconstruct(f: &mut Function, at: Inst, ty: Type, parts: &[(Value, usize)]) -> Value {
    let mut start = 0;
    let mut result = None;
    while start < parts.len() {
        let (value, source) = parts[start];
        let mut end = start + 1;
        while end < parts.len() && parts[end] == (value, source + end - start) {
            end += 1;
        }
        let mut value = f.resolve(value);
        let from = f.value_type(value);
        if source > 0 {
            let shift = f
                .ins(InsertPoint::Before(at))
                .iconst(from, (source * 8) as i64);
            value = f.ins(InsertPoint::Before(at)).ushr(value, shift);
        }
        if from != ty {
            let op = if integer_width(from).unwrap() > parts.len() {
                ConvertOp::Trunc
            } else {
                ConvertOp::Uext
            };
            let i = f.insert_inst(
                InsertPoint::Before(at),
                InstData::Convert { op, ty, arg: value },
            );
            value = f.first_result(i);
        }
        if end - start < parts.len() {
            let mask = f
                .ins(InsertPoint::Before(at))
                .iconst(ty, ((1u64 << ((end - start) * 8)) - 1) as i64);
            value = f.ins(InsertPoint::Before(at)).and(value, mask);
        }
        if start > 0 {
            let shift = f
                .ins(InsertPoint::Before(at))
                .iconst(ty, (start * 8) as i64);
            value = f.ins(InsertPoint::Before(at)).shl(value, shift);
        }
        result = Some(match result {
            None => value,
            Some(previous) => f.ins(InsertPoint::Before(at)).or(previous, value),
        });
        start = end;
    }
    result.unwrap()
}

#[cfg(test)]
mod tests {
    use keel_ir::InstData;

    #[test]
    fn aliased_live_load_result_is_not_overwritten() {
        let mut m = keel_ir::parse(
            "func @f(i64, i64) -> i64 {
     ss0 = stack_slot 8, align 8
    block0(v0: i64, v1: i64):
     v2 = stack_addr ss0
     store v0, v2
     jump block1
    block1:
     v3 = load.i64 v2
     return v3
    }",
        )
        .unwrap();
        let f = &mut m.functions[0];
        let load = f
            .layout_insts()
            .find_map(|(_, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(i))
            .unwrap();
        let result = f.first_result(load);
        let replacement = f.block_params(f.entry_block().unwrap())[1];
        f.replace_all_uses(result, replacement);
        // Such in-memory states need not pass verification. Preserve the alias
        // exactly, rather than installing the stored value over its definition.
        super::run(&mut m.functions[0]);
        assert_eq!(m.functions[0].resolve(result), replacement);
    }
}
