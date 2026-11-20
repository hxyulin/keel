//! Widen eight-byte forward copy and fill loops with guarded ranges.
use super::*;
use keel_ir::{MemoryFlags, StackSlot};

pub(crate) struct LoopCopy;

impl crate::Pass for LoopCopy {
    fn name(&self) -> &'static str {
        "loop-copy"
    }
    fn description(&self) -> &'static str {
        "widen guarded forward word copies and fills while preserving overlap"
    }
    fn family(&self) -> crate::PassFamily {
        crate::PassFamily::Vectorize
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        if f.debug_loc.is_some() || f.debug_return.is_some() || !f.debug_vars.is_empty() {
            return false;
        }
        let blocks = f.blocks().collect::<Vec<_>>();
        let mut changed = false;
        for block in blocks {
            if let Some(copy) = analyze(f, block) {
                widen(f, copy);
                changed = true;
            } else if let Some(fill) = analyze_fill(f, block) {
                widen_fill(f, fill);
                changed = true;
            }
        }
        changed
    }
    fn deterministic(&self) -> bool {
        true
    }
}

struct CopyLoop {
    scalar: Block,
    incoming: Inst,
    end: Value,
    load: MemoryFlags,
    store: MemoryFlags,
}

fn step(f: &Function, next: Value, base: Value) -> Option<Inst> {
    let inst = def(f, next)?;
    match *f.inst_data(inst) {
        D::Binary {
            op: B::Ptradd,
            args,
        } if f.resolve(args[0]) == base && int_const(f, args[1]) == Some(8) => Some(inst),
        _ => None,
    }
}

fn forward_bound(f: &Function, block: Block, cond: Value, next: Value) -> Option<(Inst, Value)> {
    let compare = def(f, cond)?;
    let D::Icmp { cond, args } = *f.inst_data(compare) else {
        return None;
    };
    let [lhs, rhs] = args.map(|v| f.resolve(v));
    let next = f.resolve(next);
    let end = match cond {
        IntCC::Ult if lhs == next => rhs,
        IntCC::Ugt if rhs == next => lhs,
        _ => return None,
    };
    (def(f, end).is_none_or(|i| f.inst_block(i) != block)).then_some((compare, end))
}

fn analyze(f: &Function, block: Block) -> Option<CopyLoop> {
    let [dst, src] = *f.block_params(block) else {
        return None;
    };
    if f.value_type(dst) != Type::Ptr || f.value_type(src) != Type::Ptr {
        return None;
    }
    let body = f.block_insts(block);
    let &term = body.last()?;
    let D::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    if dests[0].block != block || dests[1].block == block {
        return None;
    }
    let [next_dst, next_src] = *dests[0].args else {
        return None;
    };
    let advance_dst = step(f, next_dst, dst)?;
    let advance_src = step(f, next_src, src)?;
    let (compare, end) = forward_bound(f, block, *cond, next_dst)?;
    if end == dst || end == src {
        return None;
    }
    let safe = |flags: MemoryFlags| !flags.volatile && (flags.notrap || flags.trap_ub);
    let mut source = None;
    let mut destination = None;
    for &i in body {
        if f.inst_loc(i).is_some() {
            return None;
        }
        if [term, compare, advance_dst, advance_src].contains(&i) {
            continue;
        }
        match *f.inst_data(i) {
            D::Iconst { .. } => {}
            D::Load {
                ty: Type::I64,
                addr,
                flags,
            } if f.resolve(addr) == src && safe(flags) && source.is_none() => {
                source = Some((f.first_result(i), flags));
            }
            D::Store { addr, value, flags }
                if f.resolve(addr) == dst && safe(flags) && destination.is_none() =>
            {
                let (loaded, _) = source?;
                if f.resolve(value) != loaded {
                    return None;
                }
                destination = Some(flags);
            }
            _ => return None,
        }
    }
    let (_, load) = source?;
    let store = destination?;
    let incoming = loop_entry(f, block)?;
    Some(CopyLoop {
        scalar: block,
        incoming,
        end,
        load,
        store,
    })
}

fn loop_entry(f: &Function, block: Block) -> Option<Inst> {
    let analysis = crate::analysis::get(f);
    let preds = analysis.cfg.preds(block);
    let [first, second] = *preds else {
        return None;
    };
    let pre = if first == block {
        second
    } else if second == block {
        first
    } else {
        return None;
    };
    let &incoming = f.block_insts(pre).last()?;
    if !matches!(f.inst_data(incoming), D::Jump { .. } | D::Brif { .. })
        || Some(block) == f.entry_block()
    {
        return None;
    }
    Some(incoming)
}

struct FillLoop {
    scalar: Block,
    incoming: Inst,
    end: Value,
    source: StackSlot,
    load: MemoryFlags,
    store: MemoryFlags,
}

fn analyze_fill(f: &Function, block: Block) -> Option<FillLoop> {
    let [dst] = *f.block_params(block) else {
        return None;
    };
    let body = f.block_insts(block);
    let &term = body.last()?;
    let D::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    if f.value_type(dst) != Type::Ptr || dests[0].block != block || dests[1].block == block {
        return None;
    }
    let [next_dst] = *dests[0].args else {
        return None;
    };
    let advance = step(f, next_dst, dst)?;
    let (compare, end) = forward_bound(f, block, *cond, next_dst)?;
    if end == dst {
        return None;
    }
    let mut read = None;
    let mut write = None;
    for &inst in body {
        if f.inst_loc(inst).is_some() {
            return None;
        }
        if [term, compare, advance].contains(&inst) {
            continue;
        }
        match *f.inst_data(inst) {
            D::Iconst { .. } | D::StackAddr { .. } => {}
            D::Load {
                ty: Type::I64,
                addr,
                flags,
            } if flags.notrap && !flags.volatile && read.is_none() => {
                let D::StackAddr { slot } = *f.inst_data(def(f, addr)?) else {
                    return None;
                };
                read = Some((slot, f.first_result(inst), flags));
            }
            D::Store { addr, value, flags }
                if f.resolve(addr) == dst
                    && !flags.volatile
                    && (flags.notrap || flags.trap_ub)
                    && write.is_none() =>
            {
                write = Some((f.resolve(value), flags));
            }
            _ => return None,
        }
    }
    let (source, loaded, load) = read?;
    let (stored, store) = write?;
    if loaded != stored || f.stack_slot(source).size < 8 {
        return None;
    }
    Some(FillLoop {
        scalar: block,
        incoming: loop_entry(f, block)?,
        end,
        source,
        load,
        store,
    })
}

fn widen_fill(f: &mut Function, c: FillLoop) {
    let guard = f.create_block();
    let prepare = f.create_block();
    let vector = f.create_block();
    let dst = f.append_block_param(guard, Type::Ptr);
    let d = f.append_block_param(vector, Type::Ptr);
    let source = emit(f, guard, D::StackAddr { slot: c.source });
    let dst_bits = pointer_bits(f, guard, dst);
    let src_bits = pointer_bits(f, guard, source);
    let end = pointer_bits(f, guard, c.end);
    let chunk = iconst(f, guard, Type::I64, 32);
    let limit = bin(f, guard, B::Isub, end, chunk);
    let valid_limit = cmp(f, guard, IntCC::Uge, end, chunk);
    let fits = cmp(f, guard, IntCC::Ult, dst_bits, limit);
    let valid_trip = bin(f, guard, B::And, valid_limit, fits);
    // Check the whole vector-written interval. The source can be before
    // the first output word or at or beyond the original end pointer.
    let word = iconst(f, guard, Type::I64, 8);
    let distance = bin(f, guard, B::Isub, dst_bits, src_bits);
    let before = cmp(f, guard, IntCC::Ule, src_bits, dst_bits);
    let separated = cmp(f, guard, IntCC::Uge, distance, word);
    let earlier = bin(f, guard, B::And, before, separated);
    let after = cmp(f, guard, IntCC::Uge, src_bits, end);
    let disjoint = bin(f, guard, B::Or, earlier, after);
    let safe = bin(f, guard, B::And, valid_trip, disjoint);
    branch(f, guard, safe, prepare, vec![], c.scalar, vec![dst]);
    let ty = Type::Vector(VectorType::I64x2);
    let pattern = emit(
        f,
        prepare,
        D::Load {
            ty: Type::I64,
            addr: source,
            flags: c.load,
        },
    );
    let fill = emit(f, prepare, D::VectorSplat { ty, arg: pattern });
    jump(f, prepare, vector, vec![dst]);

    let half = iconst(f, vector, Type::I64, 16);
    let high = bin(f, vector, B::Ptradd, d, half);
    let mut high_flags = c.store;
    high_flags.align = high_flags.align.min(16);
    f.append_inst(
        vector,
        D::Store {
            value: fill,
            addr: d,
            flags: c.store,
        },
    );
    f.append_inst(
        vector,
        D::Store {
            value: fill,
            addr: high,
            flags: high_flags,
        },
    );
    let next = bin(f, vector, B::Ptradd, d, chunk);
    let next_bits = pointer_bits(f, vector, next);
    let more = cmp(f, vector, IntCC::Ult, next_bits, limit);
    // Leave the last scalar iteration to produce the original exit values.
    branch(f, vector, more, vector, vec![next], c.scalar, vec![next]);
    for dest in f.inst_data_mut(c.incoming).branch_dests_mut() {
        if dest.block == c.scalar {
            dest.block = guard;
        }
    }
}

fn pointer_bits(f: &mut Function, block: Block, arg: Value) -> Value {
    emit(
        f,
        block,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg,
        },
    )
}

fn widen(f: &mut Function, c: CopyLoop) {
    let guard = f.create_block();
    let vector = f.create_block();
    let dst = f.append_block_param(guard, Type::Ptr);
    let src = f.append_block_param(guard, Type::Ptr);
    let d = f.append_block_param(vector, Type::Ptr);
    let s = f.append_block_param(vector, Type::Ptr);

    let dst_bits = pointer_bits(f, guard, dst);
    let src_bits = pointer_bits(f, guard, src);
    let end = pointer_bits(f, guard, c.end);
    let chunk = iconst(f, guard, Type::I64, 32);
    let limit = bin(f, guard, B::Isub, end, chunk);
    let distance = bin(f, guard, B::Isub, dst_bits, src_bits);
    let forward = cmp(f, guard, IntCC::Uge, dst_bits, src_bits);
    let separated = cmp(f, guard, IntCC::Uge, distance, chunk);
    let valid_limit = cmp(f, guard, IntCC::Uge, end, chunk);
    let chunk_fits = cmp(f, guard, IntCC::Ult, dst_bits, limit);
    let aliases_safe = bin(f, guard, B::And, forward, separated);
    let trip = bin(f, guard, B::And, valid_limit, chunk_fits);
    let safe = bin(f, guard, B::And, aliases_safe, trip);
    branch(
        f,
        guard,
        safe,
        vector,
        vec![dst, src],
        c.scalar,
        vec![dst, src],
    );

    let half = iconst(f, vector, Type::I64, 16);
    let high_dst = bin(f, vector, B::Ptradd, d, half);
    let high_src = bin(f, vector, B::Ptradd, s, half);
    let ty = Type::Vector(VectorType::I8x16);
    let mut high_load = c.load;
    high_load.align = high_load.align.min(16);
    let mut high_store = c.store;
    high_store.align = high_store.align.min(16);
    // The distance check makes the source and destination of each chunk disjoint.
    let low = emit(
        f,
        vector,
        D::Load {
            ty,
            addr: s,
            flags: c.load,
        },
    );
    let high = emit(
        f,
        vector,
        D::Load {
            ty,
            addr: high_src,
            flags: high_load,
        },
    );
    f.append_inst(
        vector,
        D::Store {
            value: low,
            addr: d,
            flags: c.store,
        },
    );
    f.append_inst(
        vector,
        D::Store {
            value: high,
            addr: high_dst,
            flags: high_store,
        },
    );
    let next_dst = bin(f, vector, B::Ptradd, d, chunk);
    let next_src = bin(f, vector, B::Ptradd, s, chunk);
    let next_bits = pointer_bits(f, vector, next_dst);
    let more = cmp(f, vector, IntCC::Ult, next_bits, limit);
    // Leave at least one scalar iteration to carry the original exit values.
    branch(
        f,
        vector,
        more,
        vector,
        vec![next_dst, next_src],
        c.scalar,
        vec![next_dst, next_src],
    );
    for dest in f.inst_data_mut(c.incoming).branch_dests_mut() {
        if dest.block == c.scalar {
            dest.block = guard;
        }
    }
}
