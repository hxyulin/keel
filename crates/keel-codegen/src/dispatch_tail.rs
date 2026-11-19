//! Replicate dense dispatch at its incoming jumps while sharing the table.
use crate::lowering::SwitchPlan;
use crate::regalloc::Allocation;
use crate::select::Selection;
use keel_ir::fxhash::FxHashMap as HashMap;
use keel_ir::{Function, Inst, InstData, Module, Type};

const MAX_SITES: usize = 128;

pub(crate) fn plan(
    func: &Function,
    module: &Module,
    alloc: &Allocation,
    selection: &Selection,
    enabled: bool,
) -> HashMap<Inst, Inst> {
    if !enabled
        || func.num_blocks() < 16
        || !module.files.is_empty()
        || func.debug_loc.is_some()
        || func.debug_return.is_some()
        || !func.debug_vars.is_empty()
        || func.layout_insts().any(|(_, inst)| {
            func.inst_loc(inst).is_some()
                || matches!(func.inst_data(inst), InstData::InlineAsm { .. })
        })
    {
        return HashMap::default();
    }

    let supported = |inst| {
        !selection.covered(inst)
            && !selection.has_constraint(inst)
            && alloc.edits.instruction_index(inst).is_none_or(|index| {
                let edit = alloc.edits.instruction(index);
                edit.after.is_empty() && edit.defs.is_empty()
            })
    };

    // Plan each target once. A one-instruction switch block has no other
    // work to replay after the predecessor has copied its block arguments.
    let mut switches = HashMap::default();
    for &block in &alloc.order {
        let [switch] = func.block_insts(block) else {
            continue;
        };
        let InstData::Switch { arg, cases, .. } = func.inst_data(*switch) else {
            continue;
        };
        let ty = func.value_type(func.resolve(*arg));
        let mask = match ty {
            Type::I32 => u32::MAX as u64,
            Type::I64 => u64::MAX,
            _ => continue,
        };
        if (64..=256).contains(&cases.len())
            && SwitchPlan::new(cases, mask).jump_table()
            && supported(*switch)
        {
            switches.insert(block, *switch);
        }
    }

    let mut sites = HashMap::default();
    if !switches.is_empty() {
        for &block in &alloc.order {
            let Some(&jump) = func.block_insts(block).last() else {
                continue;
            };
            if let InstData::Jump { dest } = func.inst_data(jump)
                && let Some(&switch) = switches.get(&dest.block)
                && supported(jump)
            {
                sites.insert(jump, switch);
                if sites.len() == MAX_SITES {
                    break;
                }
            }
        }
    }
    sites
}
