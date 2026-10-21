//! Bounded post-allocation experiment. No SSA or allocation identities change.
use super::*;

const MAX_CLONES: usize = 8;
const MAX_TABLE_ENTRIES: usize = 1024;

pub(super) fn plan(
    func: &Function,
    module: &Module,
    alloc: &Allocation,
    selection: &crate::select::Selection,
    enabled: bool,
) -> HashMap<Inst, Inst> {
    let mut sites = HashMap::new();
    if !enabled
        || !module.files.is_empty()
        || !selection.constraints().is_empty()
        || func.layout_insts().any(|(_, inst)| {
            func.inst_loc(inst).is_some()
                || matches!(func.inst_data(inst), InstData::InlineAsm { .. })
        })
    {
        return sites;
    }
    let supported = |inst| {
        !selection.covered(inst)
            && alloc.edits.instruction_index(inst).is_none_or(|i| {
                let edit = alloc.edits.instruction(i);
                // Terminators cannot execute after transfers. Operand use
                // overrides, before transfers and edge copies are replayed.
                edit.after.is_empty() && edit.defs.is_empty()
            })
    };
    let mut entries = 0;
    for &block in &alloc.order {
        for &jump in func.block_insts(block) {
            let InstData::Jump { dest } = func.inst_data(jump) else {
                continue;
            };
            let [switch] = func.block_insts(dest.block) else {
                continue;
            };
            let InstData::Switch { arg, cases, .. } = func.inst_data(*switch) else {
                continue;
            };
            let ty = func.value_type(func.resolve(*arg));
            if !matches!(ty, Type::I32 | Type::I64)
                || !(4..=128).contains(&cases.len())
                || !supported(jump)
                || !supported(*switch)
            {
                continue;
            }
            let mask = if ty == Type::I32 {
                u32::MAX as u64
            } else {
                u64::MAX
            };
            let min = cases.iter().map(|&c| c as u64 & mask).min().unwrap();
            let max = cases.iter().map(|&c| c as u64 & mask).max().unwrap();
            let span = max - min;
            if span >= 3 * cases.len() as u64 || span >= 4096 {
                continue;
            }
            let cost = span as usize + 1;
            if sites.len() == MAX_CLONES || entries + cost > MAX_TABLE_ENTRIES {
                continue;
            }
            sites.insert(jump, *switch);
            entries += cost;
        }
    }
    sites
}

impl FunctionCompiler<'_> {
    /// Called after the original Jump's parallel edge copies. That establishes
    /// every dispatch block parameter, including ones used directly downstream.
    pub(super) fn emit_dispatch_tail(&mut self, jump: Inst) -> bool {
        let Some(&switch) = self.dispatch_tails.get(&jump) else {
            return false;
        };
        let InstData::Switch { arg, dests, cases } = self.func.inst_data(switch).clone() else {
            unreachable!("planned switch")
        };
        let saved = (self.current_inst, self.allocation_index, self.next_block);
        self.current_inst = Some(switch);
        self.allocation_index = self.alloc.edits.instruction_index(switch);
        if let Some(index) = self.allocation_index {
            self.allocation_moves(index, true);
        }
        // next_block is the physical next block at the Jump emission site.
        // Dense switches always transfer explicitly, including every case pad.
        self.switch(switch, arg, &dests, &cases, Some(jump));
        (self.current_inst, self.allocation_index, self.next_block) = saved;
        true
    }
}
