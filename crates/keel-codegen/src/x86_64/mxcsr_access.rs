//! Raw baseline MXCSR access. Private fixed frame slots never escape to user memory.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_mxcsr_access(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let slot = self.mxcsr_access.expect("MXCSR access frame slots");
        // Capture input before publishing the old result, including when the
        // allocator aliases input/output or both values spill. MOV/STMXCSR/
        // LDMXCSR leave integer flags unchanged; S0 is a reserved GPR.
        if id == Intrinsic::X86SseSwapMxcsr {
            let r = self.use_reg(args[0], S0);
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(self.frame(slot + 4), 32)), asm_arg(r32(r))],
            );
        }
        self.emit(Opcode::Stmxcsr, [asm_arg(sized(self.frame(slot), 32))]);
        if id == Intrinsic::X86SseSwapMxcsr {
            self.emit(Opcode::Ldmxcsr, [asm_arg(sized(self.frame(slot + 4), 32))]);
        }
        let d = self.def_reg(result);
        self.emit(
            Opcode::Mov,
            [asm_arg(r32(d)), asm_arg(sized(self.frame(slot), 32))],
        );
        self.def_done(result, d);
    }
}
