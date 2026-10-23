//! Raw low status flags at an IR instruction boundary, not an ABI boundary.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_flags_state(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let exchange = id == Intrinsic::X86SahfExchange;
        if exchange {
            // Capture before LAHF changes AH, and before a potentially aliased
            // result is published. use_reg/rematerialize uses flag-neutral MOVs.
            let input = self.use_reg(args[0], S1);
            self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(input))]);
        }
        let ah = Register::parse("ah").unwrap();
        self.emit(Opcode::parse("lahf").unwrap(), []);
        // EDX/AH and AH/CL require no REX prefix. Never combine AH with an
        // allocated register (r8b..r15b, sil, dil, ...), even for publication.
        self.emit(Opcode::Movzx, [asm_arg(r32(S2)), asm_arg(ah)]);
        if exchange {
            self.emit(Opcode::Mov, [asm_arg(ah), asm_arg(r8(S1))]);
            self.emit(Opcode::parse("sahf").unwrap(), []);
        }
        // In particular, no AND/shift/XOR after SAHF: these would corrupt the
        // restored flags and OF. LAHF already supplies the constant bits.
        let d = self.def_reg(result);
        self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S2))]);
        self.def_done(result, d);
    }
}
