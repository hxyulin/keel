//! Explicit architectural unary/status operations, separate from generic F80.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_basic(&mut self, inst: Inst, id: Intrinsic, arg: Value) {
        use Intrinsic::*;
        let (name, status_only) = match id {
            X86X87Fabs => (op::FABS, false),
            X86X87Fchs => (op::FCHS, false),
            X86X87Fsqrt => (op::FSQRT, false),
            X86X87Frndint => (op::FRNDINT, false),
            X86X87Ftst => (op::FTST, true),
            X86X87Fxam => (op::FXAM, true),
            _ => unreachable!(),
        };
        // The input is resident before any output/home may overwrite it.
        self.f80_push(arg);
        self.emit(name, []);
        self.emit(op::FNSTSW, [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        let status = if status_only {
            self.f80_pop_result(None);
            results[0]
        } else {
            self.f80_pop_result(Some(results[0]));
            results[1]
        };
        // FSTP does not clobber AX; its FP effects follow the returned snapshot.
        let d = self.def_reg(Some(status));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(status), d);
    }
}
