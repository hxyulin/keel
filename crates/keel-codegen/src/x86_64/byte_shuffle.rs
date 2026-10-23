//! Runtime byte controls select within 128-bit halves, with sign-bit zeroing.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_byte_shuffle(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let ty = self.ty(args[0]);
        for (&arg, scratch) in args.iter().zip([FS0, FS1]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        if id == Intrinsic::X86Ssse3Pshufb128 {
            self.emit(
                Opcode::parse("pshufb").unwrap(),
                [asm_arg(simd(FS0, ty)), asm_arg(simd(FS1, ty))],
            );
        } else {
            self.emit(
                Opcode::parse("vpshufb").unwrap(),
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                ],
            );
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}
