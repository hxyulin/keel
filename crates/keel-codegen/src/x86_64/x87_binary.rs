//! Raw D8, DC and DE arithmetic; physical operands follow Intel order.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_binary(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        use Intrinsic::*;
        let (name, second, pop) = match id {
            X86X87FaddSt1 => (Opcode::Fadd, true, false),
            X86X87Fadd => (Opcode::Fadd, false, false),
            X86X87FmulSt1 => (Opcode::Fmul, true, false),
            X86X87Fmul => (Opcode::Fmul, false, false),
            X86X87FsubSt1 => (op::FSUB, true, false),
            X86X87Fsub => (op::FSUB, false, false),
            X86X87FsubrSt1 => (op::FSUBR, true, false),
            X86X87Fsubr => (op::FSUBR, false, false),
            X86X87FdivSt1 => (op::FDIV, true, false),
            X86X87Fdiv => (op::FDIV, false, false),
            X86X87FdivrSt1 => (op::FDIVR, true, false),
            X86X87Fdivr => (op::FDIVR, false, false),
            X86X87Faddp => (op::FADDP, true, true),
            X86X87Fmulp => (Opcode::Fmulp, true, true),
            X86X87Fsubp => (Opcode::Fsubp, true, true),
            X86X87Fsubrp => (Opcode::Fsubrp, true, true),
            X86X87Fdivp => (op::FDIVP, true, true),
            X86X87Fdivrp => (op::FDIVRP, true, true),
            _ => unreachable!(),
        };
        self.f80_push(args[1]);
        self.f80_push(args[0]);
        let st = |number| Register {
            class: RegisterClass::X87,
            number,
            width: 80,
            high_byte: false,
        };
        // Explicit operands select D8, DC or DE, independently of algebra. Do not
        // swap source loads to mimic algebraic equivalence: NaNs and SW matter.
        self.emit(
            name,
            [
                asm_arg(st(if second { 1 } else { 0 })),
                asm_arg(st(if second { 0 } else { 1 })),
            ],
        );
        self.emit(op::FNSTSW, [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        if second && !pop {
            // DC leaves the original ST(0) above its destination. Discard it
            // after the snapshot, before materializing the actual ST(1) result.
            self.f80_pop_result(None);
        }
        self.f80_pop_result(Some(results[0]));
        if !second {
            self.f80_pop_result(None);
        }
        let status = results[1];
        let d = self.def_reg(Some(status));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(status), d);
    }
}
