//! Architectural unsigned counts: zero/sign-fill beyond the lane width.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_packed_shift(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let ty = self.ty(args[0]);
        let data = self.use_freg(args[0], FS0);
        if data != FS0 {
            self.emit(movs(ty), [asm_arg(simd(FS0, ty)), asm_arg(simd(data, ty))]);
        }
        if let Some((opcode, tier)) = id.x86_byte_shift() {
            let count = iconst(self.func, args[1]).unwrap();
            let opcode = Opcode::parse(opcode).unwrap();
            if tier == 0 {
                self.emit(opcode, [asm_arg(simd(FS0, ty)), asm_arg(count)]);
            } else {
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, ty)),
                        asm_arg(simd(FS0, ty)),
                        asm_arg(count),
                    ],
                );
            }
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
            }
            self.def_done(result, d);
            return;
        }
        let (opcode, per_lane, tier) = id.x86_packed_shift().unwrap();
        let count = if per_lane {
            let count = self.use_freg(args[1], FS1);
            if count != FS1 {
                self.emit(movs(ty), [asm_arg(simd(FS1, ty)), asm_arg(simd(count, ty))]);
            }
            simd(FS1, ty)
        } else {
            // The hardware consumes all low 64 count bits, even for word lanes.
            // MOVQ also zeros the unused upper half of its XMM destination.
            let count = self.use_reg(args[1], S0);
            self.emit(Opcode::Movq, [asm_arg(xmm(FS1)), asm_arg(r64(count))]);
            xmm(FS1)
        };
        let opcode = Opcode::parse(opcode).unwrap();
        if tier == 0 {
            self.emit(opcode, [asm_arg(simd(FS0, ty)), asm_arg(count)]);
        } else {
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS0, ty)),
                    asm_arg(count),
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
