//! Raw D8, DC and DE arithmetic; physical operands follow Intel order.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_binary(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        use Intrinsic::*;
        let (name, second, pop) = match id {
            X86X87FaddSt1 => ("fadd", true, false),
            X86X87Fadd => ("fadd", false, false),
            X86X87FmulSt1 => ("fmul", true, false),
            X86X87Fmul => ("fmul", false, false),
            X86X87FsubSt1 => ("fsub", true, false),
            X86X87Fsub => ("fsub", false, false),
            X86X87FsubrSt1 => ("fsubr", true, false),
            X86X87Fsubr => ("fsubr", false, false),
            X86X87FdivSt1 => ("fdiv", true, false),
            X86X87Fdiv => ("fdiv", false, false),
            X86X87FdivrSt1 => ("fdivr", true, false),
            X86X87Fdivr => ("fdivr", false, false),
            X86X87Faddp => ("faddp", true, true),
            X86X87Fmulp => ("fmulp", true, true),
            X86X87Fsubp => ("fsubp", true, true),
            X86X87Fsubrp => ("fsubrp", true, true),
            X86X87Fdivp => ("fdivp", true, true),
            X86X87Fdivrp => ("fdivrp", true, true),
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
            Opcode::parse(name).unwrap(),
            [
                asm_arg(st(if second { 1 } else { 0 })),
                asm_arg(st(if second { 0 } else { 1 })),
            ],
        );
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
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
