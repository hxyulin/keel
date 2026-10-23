//! Exact runtime sign-bit controls, including legacy implicit XMM0 preservation.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_sign_blend(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (opcode, tier) = id.x86_sign_blend().unwrap();
        let ty = self.ty(args[0]);
        // Capture all three SSA inputs before touching implicit XMM0 or an output.
        for (&arg, scratch) in args.iter().zip([FS0, FS1, FS2]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(
                    movs(ty),
                    [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
                );
            }
        }
        let opcode = Opcode::parse(opcode).unwrap();
        if tier == 0 {
            // The dedicated parallel-copy temporary is not an SSA spill home.
            // Save all YMM0 bits in wide functions, even for a 128-bit blend.
            let width = if self.wide_vectors { 256 } else { 128 };
            let save = if self.wide_vectors {
                Opcode::Vmovups
            } else {
                Opcode::Movups
            };
            let slot = self.copy_temp.expect("vector instruction has copy storage");
            let temp = sized(self.copy_slot(slot), width);
            self.emit(save, [asm_arg(temp.clone()), asm_arg(simd_width(0, width))]);
            self.emit(Opcode::Movups, [asm_arg(xmm(0)), asm_arg(xmm(FS2))]);
            self.emit(
                opcode,
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(xmm(0))],
            );
            self.emit(save, [asm_arg(simd_width(0, width)), asm_arg(temp)]);
        } else {
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                    asm_arg(simd(FS2, ty)),
                ],
            );
        }
        let dest = self.def_freg(result);
        if dest != FS0 {
            self.emit(movs(ty), [asm_arg(simd(dest, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, dest);
    }
}
