//! Owned standard-format XSAVE64 images. See docs/x86-xstate-20261022.json.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_xstate(&mut self, id: Intrinsic, args: &[Value]) {
        // Only reserved GPRs: do not disturb the SIMD/x87 state being captured.
        // Materialize the pointer independently of indexed-address folding:
        // EAX and EDX must remain available for the entire 64-bit mask.
        let address = self.use_reg(args[0], S1);
        if address != S1 {
            self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(address))]);
        }
        let mask = self.use_reg(args[1], S0);
        if mask != S0 {
            self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(mask))]);
        }
        self.emit(Opcode::Mov, [asm_arg(r64(S2)), asm_arg(r64(S0))]);
        self.emit(Opcode::Shr, [asm_arg(r64(S2)), asm_arg(32)]);
        self.emit(
            Opcode::parse(if id == Intrinsic::X86Xsave64 {
                "xsave64"
            } else {
                "xrstor64"
            })
            .unwrap(),
            [asm_arg(memory(r64(S1), 0))],
        );
    }
}
