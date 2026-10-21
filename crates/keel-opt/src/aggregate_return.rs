//! Opt-in, body-proven leaf-constructor result-home forwarding.
//!
//! No ABI-derived nocapture assumption: only a local, straight-line, write-only
//! constructor with a fully initialized result can qualify. Both homes remain
//! fixed, distinct, non-debug stack objects. Destination calls are permitted
//! only after the completed copy in the same block: the proven leaf cannot
//! observe the destination through another argument or a previously captured
//! pointer (no loads, calls, frame observers, or traps).
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, Module, Purpose, Signature,
    StackSlot, Value, ValueDef,
};
use std::collections::{HashMap, HashSet};

const MAX_INSTS: usize = 50_000;
const MAX_VALUES: usize = 100_000;
const WORK: usize = 2_000_000;
const MAX_BYTES: usize = 64;

fn constant(f: &Function, v: Value) -> Option<usize> {
    let ValueDef::Result(i, _) = f.value_def(f.resolve(v)) else {
        return None;
    };
    match f.inst_data(i) {
        InstData::Iconst { imm, .. } => usize::try_from(*imm).ok(),
        _ => None,
    }
}
fn pure(data: &InstData) -> bool {
    matches!(
        data,
        InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::Icmp { .. }
            | InstData::Select { .. }
            | InstData::Convert {
                op: ConvertOp::Uext | ConvertOp::Sext | ConvertOp::Trunc,
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
                    | BinaryOp::Sshr,
                ..
            }
    )
}
fn constructor(f: &Function) -> Option<(Signature, usize)> {
    if !f.local
        || f.sig.returns_twice
        || f.sig.variadic
        || !f.sig.returns.is_empty()
        || !f.debug_vars.is_empty()
        || f.blocks().count() != 1
    {
        return None;
    }
    let Purpose::Sret(layout) = &f.sig.params.first()?.purpose else {
        return None;
    };
    let size = usize::try_from(layout.size()).ok()?;
    if size == 0 || size > MAX_BYTES || f.num_values() > 512 {
        return None;
    }
    let block = f.entry_block()?;
    let insts = f.block_insts(block);
    if insts.len() > 128 {
        return None;
    }
    let mut addr = HashMap::from([(f.block_params(block)[0], 0usize)]);
    let mut written = vec![false; size];
    for &i in insts {
        let d = f.inst_data(i);
        let mut allowed_address = None;
        match d {
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } => {
                let off = addr
                    .get(&f.resolve(args[0]))?
                    .checked_add(constant(f, args[1])?)?;
                if off > size || f.resolve(f.first_result(i)) != f.first_result(i) {
                    return None;
                }
                addr.insert(f.first_result(i), off);
                allowed_address = Some(f.resolve(args[0]));
            }
            InstData::Store {
                addr: a,
                value,
                flags,
            } => {
                if flags.volatile {
                    return None;
                }
                let off = *addr.get(&f.resolve(*a))?;
                let end = off.checked_add(f.value_type(*value).bytes()? as usize)?;
                if end > size || written[off..end].iter().any(|&b| b) {
                    return None;
                }
                written[off..end].fill(true);
                allowed_address = Some(f.resolve(*a));
                if addr.contains_key(&f.resolve(*value)) {
                    return None;
                }
            }
            InstData::Return { args } if args.is_empty() => {}
            _ if pure(d) => {}
            _ => return None,
        }
        let mut bad = false;
        d.for_each_operand(|v| {
            let v = f.resolve(v);
            if addr.contains_key(&v) && Some(v) != allowed_address {
                bad = true;
            }
        });
        if bad {
            return None;
        }
    }
    written.iter().all(|&b| b).then(|| (f.sig.clone(), size))
}

/// Run after inlining/scalar cleanup and before small-memory expansion/DCE.
/// Returns the number of eliminated full copies. Input must be verified IR.
/// Whole-module debug mode deliberately disables this prototype.
pub fn run(module: &mut Module) -> usize {
    if !module.files.is_empty() || !module.debug_types.is_empty() {
        return 0;
    }
    let constructors: HashMap<_, _> = module
        .functions
        .iter()
        .filter_map(|f| constructor(f).map(|proof| (f.name.clone(), proof)))
        .collect();
    let mut changes = 0;
    for f in &mut module.functions {
        changes += forward(f, &constructors);
    }
    changes
}

fn forward(f: &mut Function, constructors: &HashMap<String, (Signature, usize)>) -> usize {
    if f.num_values() > MAX_VALUES || !f.debug_vars.is_empty() {
        return 0;
    }
    let insts: Vec<_> = f
        .layout_insts()
        .take(MAX_INSTS + 1)
        .map(|(_, i)| i)
        .collect();
    if insts.len() > MAX_INSTS {
        return 0;
    }
    // Reject all lifetime/nonlocal/frame facilities, including invoke. Ordinary
    // calls are allowed only because the two homes have precise use closure.
    if insts.iter().any(|&i| match f.inst_data(i) {
        InstData::Intrinsic { .. } | InstData::InlineAsm { .. } | InstData::Invoke { .. } => true,
        InstData::Call { func, .. } => f.callee(*func).sig.returns_twice,
        InstData::CallIndirect { sig, .. } => f.signature(*sig).returns_twice,
        _ => false,
    }) {
        return 0;
    }
    let mut scan_cost = insts.len();
    for &i in &insts {
        f.inst_data(i)
            .for_each_operand(|_| scan_cost = scan_cost.saturating_add(1));
    }
    if scan_cost > WORK {
        return 0;
    }
    let slots: HashMap<_, _> = f.stack_slots().collect();
    let mut addresses = HashMap::<Value, (StackSlot, usize)>::new();
    // One layout walk, no fixed point. Unrecognized/cyclic/late aliases reject
    // via the use-closure scan rather than being guessed to be nonescaping.
    for &i in &insts {
        let result = match f.inst_data(i) {
            InstData::StackAddr { slot } => Some((*slot, 0)),
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } => addresses.get(&f.resolve(args[0])).and_then(|&(s, o)| {
                o.checked_add(constant(f, args[1])?)
                    .filter(|&end| end <= slots[&s].size as usize)
                    .map(|o| (s, o))
            }),
            _ => None,
        };
        if let Some(a) = result {
            let v = f.first_result(i);
            if f.resolve(v) == v {
                addresses.insert(v, a);
            }
        }
    }
    let mut budget = WORK;
    let mut changed = 0;
    let mut removed = HashSet::new();
    for block in f.blocks().collect::<Vec<_>>() {
        let body = f.block_insts(block).to_vec();
        for (position, &call) in body.iter().enumerate() {
            let InstData::Call { func, args } = f.inst_data(call) else {
                continue;
            };
            let decl = f.callee(*func);
            let Some((sig, size)) = constructors.get(&decl.name) else {
                continue;
            };
            if &decl.sig != sig {
                continue;
            }
            let Some(&(source, 0)) = args.first().and_then(|v| addresses.get(&f.resolve(*v)))
            else {
                continue;
            };
            if args.iter().skip(1).any(|v| {
                addresses
                    .get(&f.resolve(*v))
                    .is_some_and(|&(s, _)| s == source)
            }) {
                continue;
            }
            if slots[&source].size as usize != *size {
                continue;
            }
            let mut loads = HashMap::<Value, (usize, usize, Inst)>::new();
            let mut erase = HashSet::new();
            let mut destination = None;
            let mut copied = vec![false; *size];
            let mut copy_end = position;
            // Exact local copy, with at most 128 intervening address/constant
            // instructions. No effect or potentially trapping instruction moves.
            for (copy_position, &i) in body.iter().enumerate().skip(position + 1).take(128) {
                if removed.contains(&i) {
                    break;
                }
                match f.inst_data(i) {
                    InstData::Iconst { .. } | InstData::StackAddr { .. } => {}
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        ..
                    } if addresses.contains_key(&f.first_result(i)) => {}
                    InstData::Load { addr, flags, ty } if !flags.volatile => {
                        let Some(&(s, off)) = addresses.get(&f.resolve(*addr)) else {
                            break;
                        };
                        let Some(width) = ty.bytes().map(|n| n as usize) else {
                            break;
                        };
                        if s != source
                            || off + width > *size
                            || f.resolve(f.first_result(i)) != f.first_result(i)
                        {
                            break;
                        }
                        loads.insert(f.first_result(i), (off, width, i));
                    }
                    InstData::Store { addr, value, flags } if !flags.volatile => {
                        let Some(&(s, off)) = addresses.get(&f.resolve(*addr)) else {
                            break;
                        };
                        let Some(&(src_off, width, load)) = loads.get(&f.resolve(*value)) else {
                            break;
                        };
                        if s == source
                            || destination.is_some_and(|d| d != s)
                            || off != src_off
                            || off + width > *size
                            || copied[off..off + width].iter().any(|&b| b)
                        {
                            break;
                        }
                        destination = Some(s);
                        copied[off..off + width].fill(true);
                        erase.insert(load);
                        erase.insert(i);
                    }
                    _ => break,
                }
                if copied.iter().all(|&b| b) {
                    copy_end = copy_position;
                    break;
                }
            }
            let Some(dest) = destination else { continue };
            if !copied.iter().all(|&b| b)
                || slots[&dest].size as usize != *size
                || slots[&dest].align < slots[&source].align
            {
                continue;
            }
            // Charge the full scan before doing it; fixed cap per function.
            if budget < scan_cost {
                return changed;
            }
            budget -= scan_cost;
            let later: HashSet<_> = body[copy_end + 1..].iter().copied().collect();
            let mut closed = true;
            for &i in &insts {
                if removed.contains(&i) {
                    continue;
                }
                let d = f.inst_data(i);
                d.for_each_operand(|v| {
                    let v = f.resolve(v);
                    if loads.contains_key(&v) && !erase.contains(&i) {
                        closed = false;
                    }
                    let Some(&(s, off)) = addresses.get(&v) else {
                        return;
                    };
                    if s != source && s != dest {
                        return;
                    }
                    let allowed = match d {
                        InstData::Binary {
                            op: BinaryOp::Ptradd,
                            args,
                        } => f.resolve(args[0]) == v && addresses.contains_key(&f.first_result(i)),
                        InstData::Load { addr, flags, ty } => {
                            !flags.volatile
                                && f.resolve(*addr) == v
                                && off + ty.bytes().unwrap_or(u32::MAX) as usize <= *size
                                && (s == dest || erase.contains(&i))
                        }
                        InstData::Store { addr, value, flags } => {
                            !flags.volatile
                                && f.resolve(*addr) == v
                                && f.resolve(*value) != v
                                && off + f.value_type(*value).bytes().unwrap_or(u32::MAX) as usize
                                    <= *size
                                && s == dest
                        }
                        InstData::Call { args, .. } => {
                            (s == dest && later.contains(&i))
                                || (i == call && s == source && f.resolve(args[0]) == v)
                        }
                        _ => false,
                    };
                    if !allowed {
                        closed = false;
                    }
                });
            }
            if !closed {
                continue;
            }
            let addr_inst = f.insert_inst(
                InsertPoint::Before(call),
                InstData::StackAddr { slot: dest },
            );
            let addr = f.first_result(addr_inst);
            addresses.insert(addr, (dest, 0));
            if let InstData::Call { args, .. } = f.inst_data_mut(call) {
                args[0] = addr;
            }
            for &i in &erase {
                f.remove_inst(i);
            }
            removed.extend(erase);
            changed += 1;
        }
    }
    changed
}
