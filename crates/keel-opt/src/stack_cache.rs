//! Cache fixed stack fields in SSA after an entry-block effect boundary.
//!
//! Unlike private-slot promotion, this permits a previously captured address.
//! Every original store remains in place: arbitrary reads may still alias the
//! object. The suffix must contain no other writers or memory barriers. Initial
//! reads are confined to valid, in-bounds fixed slots; Keel's uninitialized
//! memory is stable, so this does not invent values or speculate faults.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::loops::LoopDepth;
use keel_ir::{
    BinaryOp, EntityRef, Function, InsertPoint, InstData as D, MemoryFlags, StackSlot, Type, Value,
    ValueDef,
};
use std::collections::HashMap;

pub(crate) struct StackCache;
impl crate::Pass for StackCache {
    fn name(&self) -> &'static str {
        "stack-cache"
    }
    fn description(&self) -> &'static str {
        "cache loop stack reads after entry effects while retaining observable stores (opt-in)"
    }
    fn run(&self, f: &mut Function) -> bool {
        run(f)
    }
}

type Field = (StackSlot, usize, Type);

pub fn run(f: &mut Function) -> bool {
    if f.stack_slots().next().is_none() {
        return false;
    }
    let blocks: Vec<_> = f.blocks().take(65).collect();
    let instructions: Vec<_> = f.layout_insts().take(2049).collect();
    if blocks.is_empty()
        || blocks.len() > 64
        || instructions.len() > 2048
        || !f.debug_vars.is_empty()
        || blocks.iter().any(|&b| f.block_params(b).len() > 16)
    {
        return false;
    }
    if instructions.iter().any(|&(_, i)| match f.inst_data(i) {
        D::Invoke { .. } | D::LandingPad { .. } | D::InlineAsm { .. } => true,
        D::Call { func, .. } => f.callee(*func).sig.returns_twice,
        D::CallIndirect { sig, .. } => f.signature(*sig).returns_twice,
        D::Intrinsic { id, .. } => matches!(
            id,
            keel_ir::Intrinsic::FrameAddress
                | keel_ir::Intrinsic::StackSave
                | keel_ir::Intrinsic::StackRestore
                | keel_ir::Intrinsic::StackAlloc
                | keel_ir::Intrinsic::Debugtrap
        ),
        _ => false,
    }) {
        return false;
    }
    let cfg = ControlFlowGraph::compute(f);
    let entry = f.entry_block().unwrap();
    if !cfg.preds(entry).is_empty() || cfg.reverse_postorder(f).len() != blocks.len() {
        return false;
    }
    let sizes: HashMap<_, _> = f.stack_slots().map(|(s, d)| (s, d.size as usize)).collect();
    let mut addresses = HashMap::new();
    for &(_, i) in &instructions {
        if let D::StackAddr { slot } = *f.inst_data(i)
            && f.resolve(f.first_result(i)) == f.first_result(i)
        {
            addresses.insert(f.first_result(i), (slot, 0usize));
        }
    }
    // Bound address discovery rather than chasing arbitrary pointer chains.
    for _ in 0..8 {
        let before = addresses.len();
        for &(_, i) in &instructions {
            if let D::Binary {
                op: BinaryOp::Ptradd,
                args,
            } = *f.inst_data(i)
                && let Some(&(slot, offset)) = addresses.get(&f.resolve(args[0]))
                && let Some(n) = constant(f, args[1]).and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(n).filter(|&n| n <= sizes[&slot])
                && f.resolve(f.first_result(i)) == f.first_result(i)
            {
                addresses.insert(f.resolve(f.first_result(i)), (slot, end));
            }
        }
        if addresses.len() == before {
            break;
        }
    }
    let field = |addr: Value, ty: Type| -> Option<Field> {
        if !matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr) {
            return None;
        }
        let &(slot, offset) = addresses.get(&f.resolve(addr))?;
        (offset.checked_add(width(ty))? <= sizes[&slot]).then_some((slot, offset, ty))
    };
    let barrier = |i| match *f.inst_data(i) {
        D::Store { addr, value, flags } if !flags.volatile => {
            field(addr, f.value_type(f.resolve(value))).is_none()
        }
        ref d => d.memory_effects().may_write(),
    };
    // The entry has no predecessors: no execution can return across this cut.
    let entry_insts = f.block_insts(entry);
    let cut = entry_insts
        .iter()
        .rposition(|&i| barrier(i))
        .map_or(0, |p| p + 1);
    let Some(&anchor) = entry_insts.get(cut) else {
        return false;
    };
    if instructions.iter().any(|&(b, i)| b != entry && barrier(i)) {
        return false;
    }
    let suffix: Vec<_> = entry_insts[cut..]
        .iter()
        .copied()
        .map(|i| (entry, i))
        .chain(instructions.iter().copied().filter(|&(b, _)| b != entry))
        .collect();
    let mut accesses = HashMap::new();
    let mut fields = Vec::new();
    for &(_, i) in &suffix {
        let key = match *f.inst_data(i) {
            D::Load { addr, ty, flags } if !flags.volatile => field(addr, ty),
            D::Store { addr, value, flags } if !flags.volatile => {
                field(addr, f.value_type(f.resolve(value)))
            }
            _ => None,
        };
        if let Some(key) = key {
            accesses.insert(i, key);
            if !fields.contains(&key) {
                fields.push(key);
            }
        }
    }
    if fields.len() > 32 {
        return false;
    }
    let dom = DominatorTree::compute(f, &cfg);
    let loops = LoopDepth::compute(f, &cfg, &dom);
    let selected: Vec<_> = fields
        .iter()
        .copied()
        .filter(|&(s, o, t)| {
            !fields.iter().any(|&(other, offset, ty)| {
                other == s
                    && (offset, ty) != (o, t)
                    && o < offset + width(ty)
                    && offset < o + width(t)
            }) && !suffix.iter().any(|&(_, i)| {
                accesses.get(&i) == Some(&(s, o, t))
                    && matches!(f.inst_data(i), D::Load { .. })
                    && f.resolve(f.first_result(i)) != f.first_result(i)
            }) && suffix.iter().any(|&(b, i)| {
                loops.depth(b) > 0
                    && accesses.get(&i) == Some(&(s, o, t))
                    && matches!(f.inst_data(i), D::Load { .. })
            })
        })
        .take(8)
        .collect();
    if selected.is_empty() {
        return false;
    }
    for key @ (slot, offset, ty) in selected {
        let addr_inst = f.insert_inst(InsertPoint::Before(anchor), D::StackAddr { slot });
        let mut addr = f.first_result(addr_inst);
        if offset != 0 {
            let off = f.insert_inst(
                InsertPoint::Before(anchor),
                D::Iconst {
                    ty: Type::I64,
                    imm: offset as i64,
                },
            );
            let add = f.insert_inst(
                InsertPoint::Before(anchor),
                D::Binary {
                    op: BinaryOp::Ptradd,
                    args: [addr, f.first_result(off)],
                },
            );
            addr = f.first_result(add);
        }
        let init = f.insert_inst(
            InsertPoint::Before(anchor),
            D::Load {
                ty,
                addr,
                flags: MemoryFlags {
                    notrap: true,
                    ..MemoryFlags::default()
                },
            },
        );
        let initial = f.first_result(init);
        let mut incoming = vec![None; f.num_blocks()];
        let mut outgoing = vec![None; f.num_blocks()];
        for &b in &blocks {
            incoming[b.index()] = Some(if b == entry {
                initial
            } else {
                f.append_block_param(b, ty)
            });
        }
        for &b in &blocks {
            let mut value = incoming[b.index()].unwrap();
            for &(owner, i) in &suffix {
                if owner != b || accesses.get(&i) != Some(&key) {
                    continue;
                }
                match *f.inst_data(i) {
                    D::Load { .. } => {
                        f.replace_all_uses(f.first_result(i), f.resolve(value));
                        f.remove_inst(i);
                    }
                    D::Store { value: v, .. } => value = f.resolve(v),
                    _ => unreachable!(),
                }
            }
            outgoing[b.index()] = Some(value);
        }
        for &b in &blocks {
            let term = *f.block_insts(b).last().unwrap();
            for dest in f.inst_data_mut(term).branch_dests_mut() {
                dest.args.push(outgoing[b.index()].unwrap());
            }
        }
    }
    true
}

fn width(ty: Type) -> usize {
    ty.bytes().unwrap() as usize
}
fn constant(f: &Function, v: Value) -> Option<i64> {
    if let ValueDef::Result(i, _) = f.value_def(f.resolve(v))
        && let D::Iconst { imm, .. } = *f.inst_data(i)
    {
        Some(imm)
    } else {
        None
    }
}
