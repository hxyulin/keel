//! Exact ten-byte architectural packed BCD memory operations.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_x87_bcd(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        let load = id == Intrinsic::X86X87Fbld;
        // Materialize the pointer before any definition can reuse its home.
        let address = self.use_reg(args[if load { 0 } else { 1 }], S1);
        if !load {
            self.f80_push(args[0]);
        }
        self.emit(
            Opcode::parse(if load { "fbld" } else { "fbstp" }).unwrap(),
            [asm_arg(sized(memory(r64(address), 0), 80))],
        );
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        if load {
            self.f80_pop_result(Some(results[0]));
        }
        let result = results[usize::from(load)];
        let d = self.def_reg(Some(result));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(result), d);
    }
}
