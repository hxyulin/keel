//! Hardware constants inherit RC; never replace these with pre-rounded literals.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_x87_constant(&mut self, inst: Inst, id: Intrinsic) {
        let name = match id {
            Intrinsic::X86X87Fldpi => "fldpi",
            Intrinsic::X86X87Fldl2e => "fldl2e",
            Intrinsic::X86X87Fldl2t => "fldl2t",
            Intrinsic::X86X87Fldlg2 => "fldlg2",
            Intrinsic::X86X87Fldln2 => "fldln2",
            _ => unreachable!(),
        };
        self.emit(Opcode::parse(name).unwrap(), []);
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        self.f80_pop_result(Some(results[0]));
        let d = self.def_reg(Some(results[1]));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(results[1]), d);
    }
}
