//! Promote definitely initialized, exact, disjoint fields of nonescaping fixed
//! slots, including integer fields covered by wider byte-splat initialization.
//! No uninitialized SSA values are invented: a forward must analysis
//! proves every load initialized on every path, including zero-trip loops.
//! All frame observers and exception/nonlocal continuations are excluded.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{
    BinaryOp, EntityRef, Function, InsertPoint, InstData, StackSlot, Type, Value, ValueDef,
};
use std::collections::{HashMap, HashSet};

type Field = (StackSlot, usize, Type);

pub fn run(f: &mut Function) -> bool {
    let blocks: Vec<_> = f.blocks().collect();
    if blocks.is_empty() || blocks.len() > 256 {
        return false;
    }
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    if insts.len() > 8192 {
        return false;
    }
    if insts.iter().any(|&i| match f.inst_data(i) {
        InstData::Invoke { .. } | InstData::LandingPad { .. } | InstData::InlineAsm { .. } => true,
        InstData::Call { func, .. } => f.callee(*func).sig.returns_twice,
        InstData::CallIndirect { sig, .. } => f.signature(*sig).returns_twice,
        InstData::Intrinsic { id, .. } => matches!(
            id,
            keel_ir::Intrinsic::FrameAddress
                | keel_ir::Intrinsic::StackSave
                | keel_ir::Intrinsic::StackRestore
        ),
        _ => false,
    }) {
        return false;
    }
    let cfg = ControlFlowGraph::compute(f);
    let rpo = cfg.reverse_postorder(f);
    let entry = rpo[0];
    // Avoid fabricating edge values for unreachable predecessors. Cleanup can
    // remove those blocks before this pass. Entry backedges need an entry phi.
    if rpo.len() != blocks.len() || !cfg.preds(entry).is_empty() {
        return false;
    }
    let sizes: HashMap<_, _> = f.stack_slots().map(|(s, d)| (s, d.size as usize)).collect();
    let mut addresses = HashMap::new();
    for &i in &insts {
        if let InstData::StackAddr { slot } = *f.inst_data(i) {
            let v = f.first_result(i);
            if f.resolve(v) == v {
                addresses.insert(v, (slot, 0usize));
            }
        }
    }
    loop {
        let before = addresses.len();
        for &i in &insts {
            if let InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } = *f.inst_data(i)
                && let Some(&(slot, offset)) = addresses.get(&f.resolve(args[0]))
                && let Some(n) = constant(f, args[1]).and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(n).filter(|&n| n <= sizes[&slot])
                && f.resolve(f.first_result(i)) == f.first_result(i)
            {
                addresses.insert(f.first_result(i), (slot, end));
            }
        }
        if before == addresses.len() {
            break;
        }
    }
    let mut escaped: HashSet<_> = f.debug_vars.iter().map(|v| v.slot).collect();
    let mut accesses: HashMap<_, Field> = HashMap::new();
    let mut fields = Vec::new();
    for &i in &insts {
        let d = f.inst_data(i);
        d.for_each_operand(|v| {
            let v = f.resolve(v);
            let Some(&(slot, offset)) = addresses.get(&v) else {
                return;
            };
            let ty = match d {
                InstData::Load { addr, flags, ty }
                    if !flags.volatile
                        && f.resolve(*addr) == v
                        && f.resolve(f.first_result(i)) == f.first_result(i) =>
                {
                    Some(*ty)
                }
                InstData::Store { addr, value, flags }
                    if !flags.volatile && f.resolve(*addr) == v && f.resolve(*value) != v =>
                {
                    Some(f.value_type(*value))
                }
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args,
                } if f.resolve(args[0]) == v && addresses.contains_key(&f.first_result(i)) => {
                    return;
                }
                _ => None,
            };
            // Scalar integer and pointer fields only; exact type is part of
            // the key. Atomic, intrinsic, call, return and address comparisons
            // escape the entire object. No libc/nocapture assumptions.
            if let Some(ty @ (Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr)) = ty
                && offset
                    .checked_add(width(ty))
                    .is_some_and(|end| end <= sizes[&slot])
            {
                let field = (slot, offset, ty);
                accesses.insert(i, field);
                if !fields.contains(&field) {
                    fields.push(field);
                }
            } else {
                escaped.insert(slot);
            }
        });
    }
    if fields.len() > 64 {
        return false;
    }
    // A repeated byte has the same representation at every offset on either
    // endianness. Do not reinterpret arbitrary integer bits or pointer values.
    // Keep these stores in memory when they initialize a narrower field: their
    // other bytes may still be read by an unpromoted field (including padding).
    let splats: HashMap<_, _> = accesses
        .keys()
        .filter_map(|&i| {
            let InstData::Store { value, .. } = *f.inst_data(i) else {
                return None;
            };
            splat_byte(f, value).map(|byte| (i, byte))
        })
        .collect();
    let eligible: Vec<_> = fields
        .iter()
        .copied()
        .filter(|&(slot, offset, ty)| {
            !escaped.contains(&slot)
                && !accesses.iter().any(|(i, &(s, o, t))| {
                    s == slot
                        && (o, t) != (offset, ty)
                        && offset < o + width(t)
                        && o < offset + width(ty)
                        && !(ty.is_int()
                            && splats.contains_key(i)
                            && o <= offset
                            && offset + width(ty) <= o + width(t))
                })
        })
        .collect();
    let mut changed = false;
    let mut retained_initializers = HashSet::new();
    for field in eligible {
        let covers = |i: &_| {
            let &(s, o, t) = accesses.get(i)?;
            (field.2.is_int()
                && s == field.0
                && o <= field.1
                && field.1 + width(field.2) <= o + width(t))
            .then(|| splats.get(i).copied())
            .flatten()
        };
        let relevant = |i: &_| accesses.get(i) == Some(&field) || covers(i).is_some();
        let mut stores = vec![false; f.num_blocks()];
        let mut has_load = false;
        for &b in &blocks {
            for &i in f.block_insts(b) {
                if relevant(&i) {
                    stores[b.index()] |= matches!(f.inst_data(i), InstData::Store { .. });
                    has_load |= matches!(f.inst_data(i), InstData::Load { .. });
                }
            }
        }
        if !has_load {
            continue;
        }
        // Greatest fixed point: false at entry, true elsewhere initially.
        // Starting false everywhere would lose initialized loop recurrences.
        let mut initialized = vec![true; f.num_blocks()];
        initialized[entry.index()] = false;
        loop {
            let mut progress = false;
            for &b in &rpo[1..] {
                let value = cfg
                    .preds(b)
                    .iter()
                    .all(|p| initialized[p.index()] || stores[p.index()]);
                if initialized[b.index()] != value {
                    initialized[b.index()] = value;
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
        let valid = blocks.iter().all(|&b| {
            let mut defined = initialized[b.index()];
            for &i in f.block_insts(b) {
                if !relevant(&i) {
                    continue;
                }
                match f.inst_data(i) {
                    InstData::Store { .. } => defined = true,
                    InstData::Load { .. } if !defined => return false,
                    _ => {}
                }
            }
            true
        });
        if !valid {
            continue;
        }
        let mut incoming = vec![None; f.num_blocks()];
        for &b in &blocks {
            if initialized[b.index()] {
                incoming[b.index()] = Some(f.append_block_param(b, field.2));
            }
        }
        let mut outgoing = vec![None; f.num_blocks()];
        for &b in &blocks {
            let mut value = incoming[b.index()];
            for i in f.block_insts(b).to_vec() {
                if !relevant(&i) {
                    continue;
                }
                if accesses.get(&i) != Some(&field) {
                    let byte = covers(&i).unwrap();
                    retained_initializers.insert(i);
                    let loc = f.inst_loc(i);
                    let init = f
                        .ins(InsertPoint::Before(i))
                        .loc(loc)
                        .build(InstData::Iconst {
                            ty: field.2,
                            imm: repeated_byte(byte, field.2),
                        });
                    value = Some(f.first_result(init));
                    continue;
                }
                match *f.inst_data(i) {
                    InstData::Store { value: v, .. } => value = Some(f.resolve(v)),
                    InstData::Load { .. } => f.replace_all_uses(f.first_result(i), value.unwrap()),
                    _ => unreachable!(),
                }
                f.remove_inst(i);
            }
            outgoing[b.index()] = value;
        }
        for &b in &blocks {
            let term = *f.block_insts(b).last().unwrap();
            // Include every syntactic edge, even duplicate brif/switch edges.
            for dest in f.inst_data_mut(term).branch_dests_mut() {
                if incoming[dest.block.index()].is_some() {
                    dest.args.push(outgoing[b.index()].unwrap());
                }
            }
        }
        changed = true;
    }
    // Once all overlapping reads have moved to SSA, the covering initializer
    // is dead too. Otherwise keep its complete original byte footprint.
    for i in retained_initializers {
        let &(slot, offset, ty) = &accesses[&i];
        let still_read = accesses.iter().any(|(&other, &(s, o, t))| {
            s == slot
                && o < offset + width(ty)
                && offset < o + width(t)
                && !f.is_removed(other)
                && matches!(f.inst_data(other), InstData::Load { .. })
        });
        if !still_read {
            f.remove_inst(i);
        }
    }
    changed
}
fn constant(f: &Function, v: Value) -> Option<i64> {
    if let ValueDef::Result(i, _) = f.value_def(f.resolve(v))
        && let InstData::Iconst { imm, .. } = f.inst_data(i)
    {
        Some(*imm)
    } else {
        None
    }
}

fn width(ty: Type) -> usize {
    ty.bytes().unwrap_or(8) as usize
}

fn repeated_byte(byte: u8, ty: Type) -> i64 {
    ((u64::MAX / 255 * u64::from(byte)) >> (64 - width(ty) * 8)) as i64
}

fn splat_byte(f: &Function, value: Value) -> Option<u8> {
    let ty = f.value_type(value);
    if !matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64) {
        return None;
    }
    let imm = constant(f, value)?;
    let byte = imm as u8;
    let mask = u64::MAX >> (64 - width(ty) * 8);
    ((imm as u64 & mask) == repeated_byte(byte, ty) as u64).then_some(byte)
}
