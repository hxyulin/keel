//! Share equivalent terminal blocks without adding values or changing effects.

use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    Block, CallContract, EntityRef, Function, InstData, Type, Value, ValueDef, ValueFacts,
};

#[derive(PartialEq, Eq, Hash)]
struct Key {
    params: Vec<Type>,
    instructions: Vec<(InstData, Option<CallContract>, Option<ValueFacts>)>,
}

const MAX_BLOCK_INSTS: usize = 64;

pub fn run(f: &mut Function) -> bool {
    if f.attrs.optnone
        || f.attrs.naked
        || f.attrs.interrupt
        || f.sig.returns_twice
        || f.personality.is_some()
        || f.debug_loc.is_some()
        || !f.debug_vars.is_empty()
        || f.num_values() > u32::MAX as usize / 2
        || f.layout_insts().any(|(_, i)| match f.inst_data(i) {
            InstData::InlineAsm { .. }
            | InstData::X86RflagsRegion { .. }
            | InstData::Invoke { .. }
            | InstData::LandingPad { .. } => true,
            InstData::Call { func, .. } => f.callee(*func).sig.returns_twice,
            InstData::CallIndirect { sig, .. } => f.signature(*sig).returns_twice,
            InstData::Intrinsic { id, .. } => {
                matches!(
                    id,
                    keel_ir::Intrinsic::X86RflagsRead | keel_ir::Intrinsic::X86RflagsSwap
                ) || id.x86_legacy_x87_image().is_some()
                    || id.x86_owned_x87_stack().is_some()
                    || id
                        .x86_string_port_io()
                        .is_some_and(|(_, _, _, owned)| owned)
            }
            _ => false,
        })
    {
        return false;
    }
    let mut candidates = vec![false; f.num_blocks()];
    for b in f.blocks().skip(1) {
        let insts = f.block_insts(b);
        candidates[b.index()] = insts.len() >= 2
            && insts.len() <= MAX_BLOCK_INSTS
            && insts.last().is_some_and(|&i| match f.inst_data(i) {
                InstData::Return { .. } | InstData::Trap => true,
                InstData::Jump { dest } => dest.block != b,
                _ => false,
            })
            && insts.iter().all(|&i| {
                let data = f.inst_data(i);
                !data.is_pinned()
                    || matches!(
                        data,
                        InstData::Load { .. }
                            | InstData::Store { .. }
                            | InstData::Call { .. }
                            | InstData::CallIndirect { .. }
                            | InstData::Return { .. }
                            | InstData::Jump { .. }
                            | InstData::Trap
                    )
            });
    }
    if !candidates.iter().any(|&v| v) {
        return false;
    }
    // Local definitions must not escape, including uses in unreachable blocks.
    for (b, i) in f.layout_insts() {
        f.inst_data(i).for_each_operand(|v| {
            let home = match f.value_def(f.resolve(v)) {
                ValueDef::Param(home, _) => home,
                ValueDef::Result(def, _) => f.inst_block(def),
                ValueDef::Alias(_) => unreachable!("resolved"),
            };
            if home != b {
                candidates[home.index()] = false;
            }
        });
    }
    let mut table = FxHashMap::default();
    let mut local = FxHashMap::default();
    let mut replacement: Vec<_> = (0..f.num_blocks()).map(Block::new).collect();
    let mut removed = Vec::new();
    for b in f.blocks().filter(|b| candidates[b.index()]) {
        local.clear();
        let mut next = f.num_values();
        for &v in f.block_params(b) {
            local.insert(v, Value::new(next));
            next += 1;
        }
        let mut key = Key {
            params: f.block_params(b).iter().map(|&v| f.value_type(v)).collect(),
            instructions: Vec::with_capacity(f.block_insts(b).len()),
        };
        for &i in f.block_insts(b) {
            for &v in f.inst_results(i) {
                local.insert(v, Value::new(next));
                next += 1;
            }
            let mut data = f.inst_data(i).clone();
            data.for_each_operand_mut(|v| {
                let resolved = f.resolve(*v);
                *v = local.get(&resolved).copied().unwrap_or(resolved);
            });
            key.instructions
                .push((data, f.call_contract(i).cloned(), f.inst_facts(i).copied()));
        }
        if let Some(&kept) = table.get(&key) {
            replacement[b.index()] = kept;
            removed.push(b);
        } else {
            table.insert(key, b);
        }
    }
    if removed.is_empty() {
        return false;
    }
    let terms: Vec<_> = f
        .blocks()
        .filter_map(|b| f.block_insts(b).last().copied())
        .collect();
    for term in terms {
        for dest in f.inst_data_mut(term).branch_dests_mut() {
            dest.block = replacement[dest.block.index()];
        }
    }
    f.remove_blocks(&removed);
    crate::simplify_cfg::run(f);
    crate::dce::run(f);
    true
}
