//! Legacy AES-NI only. Stage sources in reserved scratch registers before
//! touching the destructive destination, preserving live/coalesced/spilled SSA.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_aes(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let unary = matches!(
            id,
            Intrinsic::X86Aesimc128 | Intrinsic::X86Aeskeygenassist128
        );
        let ty = self.ty(args[0]);
        for (&arg, scratch) in args.iter().take(if unary { 1 } else { 2 }).zip([FS0, FS1]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(
                    movs(ty),
                    [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
                );
            }
        }
        let op = Opcode::parse(id.x86_aes_opcode().unwrap()).unwrap();
        let dst = asm_arg(simd(FS0, ty));
        let src = asm_arg(simd(if unary { FS0 } else { FS1 }, ty));
        if id == Intrinsic::X86Aeskeygenassist128 {
            let control = iconst(self.func, args[1]).unwrap();
            self.emit(op, [dst, src, asm_arg(control)]);
        } else {
            self.emit(op, [dst, src]);
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}
