//! Exact long-mode FLAGS observation. See docs/targets/x86_64/system/README.md.
use super::*;

pub(super) const SWAP_BLOCKER: &str = "x86_rflags_swap requires a verified owned-FLAGS region: IR has no FLAGS resource or proof of DF/TF/AC and execution-state restoration before calls/return";

impl FunctionCompiler<'_> {
    pub(super) fn native_rflags_read(&mut self, result: Option<Value>) {
        // Intrinsics disable frameless lowering: CFA is rbp+16 throughout this
        // transient eight-byte push, including the PC between PUSHFQ and POP.
        // No allocatable GPR, IR stack slot, or outgoing argument is overwritten.
        debug_assert!(!self.frameless);
        self.emit(Opcode::parse("pushfq").unwrap(), []);
        self.emit(Opcode::Pop, [asm_arg(r64(S0))]);
        let d = self.def_reg(result);
        self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
        self.def_done(result, d);
    }
}
