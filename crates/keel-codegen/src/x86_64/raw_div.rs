//! Explicit 2w/w DIV/IDIV. Generic division retains its normalized IR traps.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_raw_div(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        let (width, signed) = id.x86_raw_div().expect("raw division dispatch");
        // These registers are reserved by the allocator. Capture all inputs
        // before writing any result home, including homes shared with inputs.
        for (value, scratch) in [(args[0], S2), (args[1], S0), (args[2], S1)] {
            let r = self.use_reg(value, scratch);
            if r != scratch {
                self.emit(Opcode::Mov, [asm_arg(r64(scratch)), asm_arg(r64(r))]);
            }
        }
        if width == 8 {
            self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r8(S0))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S2)), asm_arg(r8(S2))]);
            self.emit(Opcode::Shl, [asm_arg(r32(S2)), asm_arg(8)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S2))]);
        }
        // The sole principal instruction: hardware raises #DE itself, even
        // when neither result is used. No tests, helper calls, or UD2.
        self.emit(
            if signed { Opcode::Idiv } else { Opcode::Div },
            [asm_arg(Register { width, ..r64(S1) })],
        );
        if width == 8 {
            // Copy AX then shift: never encode AH alongside a REX prefix.
            self.emit(Opcode::Movzx, [asm_arg(r32(S2)), asm_arg(r16(S0))]);
            self.emit(Opcode::Shr, [asm_arg(r32(S2)), asm_arg(8)]);
        }
        let results = self.func.inst_results(inst);
        for (value, scratch) in [(results[0], S0), (results[1], S2)] {
            // Normalize narrow results before the allocator's 64-bit spill.
            if width < 32 {
                self.emit(
                    Opcode::Movzx,
                    [
                        asm_arg(r32(scratch)),
                        asm_arg(Register {
                            width,
                            ..r64(scratch)
                        }),
                    ],
                );
            }
            let d = self.def_reg(Some(value));
            if d != scratch {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(scratch))]);
            }
            self.def_done(Some(value), d);
        }
    }
}
