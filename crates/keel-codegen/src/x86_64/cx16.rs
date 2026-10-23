//! CMPXCHG16B has fixed inputs RDX:RAX and RCX:RBX, and returns RDX:RAX.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_cx16(&mut self, args: &[Value], result: Option<Value>) {
        // Capture the address before RBX or any fixed input can change.
        let base = self.use_reg(args[0], S3);
        if base != S3 {
            self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(r64(base))]);
        }
        let save = sized(self.frame(self.cx16_save.expect("planned CX16 slot")), 64);
        self.emit(Opcode::Mov, [asm_arg(save.clone()), asm_arg(r64(3))]);
        let [expected_lo, expected_hi] = self.halves(args[1]);
        let [desired_lo, desired_hi] = self.halves(args[2]);
        self.emit(Opcode::Mov, [asm_arg(r64(S0)), expected_lo]);
        self.emit(Opcode::Mov, [asm_arg(r64(S2)), expected_hi]);
        // Desired high may itself be allocated in RBX: consume it first.
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), desired_hi]);
        self.emit(Opcode::Mov, [asm_arg(r64(3)), desired_lo]);
        self.emit_locked(
            Opcode::parse("cmpxchg16b").unwrap(),
            [asm_arg(sized(memory(r64(S3), 0), 128))],
        );
        self.emit(Opcode::Mov, [asm_arg(r64(3)), asm_arg(save)]);
        self.pair_out(result);
    }
}
