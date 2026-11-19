//! Emit an incoming switch replica using the original allocation and table.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn emit_dispatch_tail(&mut self, jump: Inst) -> bool {
        let Some(&switch) = self.dispatch_tails.get(&jump) else {
            return false;
        };
        let InstData::Switch { arg, dests, cases } = self.func.inst_data(switch) else {
            unreachable!("planned switch");
        };
        let saved = (self.current_inst, self.allocation_index);
        self.current_inst = Some(switch);
        self.allocation_index = self.alloc.edits.instruction_index(switch);
        if let Some(index) = self.allocation_index {
            self.allocation_moves(index, true);
        }
        self.switch(switch, *arg, dests, cases, true);
        (self.current_inst, self.allocation_index) = saved;
        true
    }
}
