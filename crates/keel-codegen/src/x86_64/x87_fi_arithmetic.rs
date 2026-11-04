//! Exact signed integer-memory arithmetic, inheriting masked CW/PC/RC.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_fi_arithmetic(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        use Intrinsic::*;
        let (name, width) = match id {
            X86X87Fiadd16 => (op::FIADD, 16),
            X86X87Fiadd32 => (op::FIADD, 32),
            X86X87Fimul16 => (op::FIMUL, 16),
            X86X87Fimul32 => (op::FIMUL, 32),
            X86X87Fisub16 => (op::FISUB, 16),
            X86X87Fisub32 => (op::FISUB, 32),
            X86X87Fisubr16 => (op::FISUBR, 16),
            X86X87Fisubr32 => (op::FISUBR, 32),
            X86X87Fidiv16 => (op::FIDIV, 16),
            X86X87Fidiv32 => (op::FIDIV, 32),
            X86X87Fidivr16 => (op::FIDIVR, 16),
            X86X87Fidivr32 => (op::FIDIVR, 32),
            _ => unreachable!(),
        };
        let temporary = sized(
            self.slot(self.copy_temp.expect("FI arithmetic staging")),
            width,
        );
        // Capture both inputs before publishing either output. The reserved
        // staging home is independent of all allocator homes and explicit edits.
        let integer = self.use_reg(args[1], S0);
        self.emit(
            Opcode::Mov,
            [
                asm_arg(temporary.clone()),
                asm_arg(Register {
                    width,
                    ..r64(integer)
                }),
            ],
        );
        self.f80_push(args[0]);
        // The exact FI instruction preserves operand order, precision, rounding
        // and exceptions; FILD plus an algebraic recipe is not this contract.
        self.emit(name, [asm_arg(temporary)]);
        self.emit(op::FNSTSW, [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        self.f80_pop_result(Some(results[0]));
        let d = self.def_reg(Some(results[1]));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(results[1]), d);
    }
}
