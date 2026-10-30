//! Packed-string instructions have implicit GPR and SIMD destinations. Only
//! reserved scratch views are clobbered; XMM0/YMM0 is restored before publishing.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_string(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let (name, index, kind) = id.x86_string_info().unwrap();
        for (arg, scratch) in args[..2].iter().copied().zip([FS1, FS2]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(Opcode::Movups, [asm_arg(xmm(scratch)), asm_arg(xmm(r))]);
            }
        }
        let save_width = self.simd_bits;
        let save_op = if self.wide_vectors {
            Opcode::Vmovups
        } else {
            Opcode::Movups
        };
        if kind == 1 {
            self.emit(
                save_op,
                [
                    asm_arg(simd_width(FS0, save_width)),
                    asm_arg(simd_width(0, save_width)),
                ],
            );
        }
        if index == 4 {
            for (arg, scratch) in args[2..4].iter().copied().zip([S0, S2]) {
                let ty = self.ty(arg);
                let r = self.use_reg(arg, scratch);
                if r != scratch {
                    self.emit(
                        Opcode::Mov,
                        [asm_arg(reg(scratch, ty)), asm_arg(reg(r, ty))],
                    );
                }
            }
        }
        self.emit(
            Opcode::parse(name).unwrap(),
            [
                asm_arg(xmm(FS1)),
                asm_arg(xmm(FS2)),
                asm_arg(iconst(self.func, args[index]).unwrap()),
            ],
        );
        if kind == 1 {
            self.emit(Opcode::Movups, [asm_arg(xmm(FS2)), asm_arg(xmm(0))]);
            self.emit(
                save_op,
                [
                    asm_arg(simd_width(0, save_width)),
                    asm_arg(simd_width(FS0, save_width)),
                ],
            );
        }
        match kind {
            0 => {
                let ty = self.func.value_type(result.unwrap());
                let d = self.def_reg(result);
                if d != S1 {
                    self.emit(Opcode::Mov, [asm_arg(reg(d, ty)), asm_arg(reg(S1, ty))]);
                }
                self.def_done(result, d);
            }
            1 => {
                let d = self.def_freg(result);
                if d != FS2 {
                    self.emit(Opcode::Movups, [asm_arg(xmm(d)), asm_arg(xmm(FS2))]);
                }
                self.def_done(result, d);
            }
            2 => {
                // Capture four flags with three GPR scratch views. MOVZX/MOVD
                // preserve flags, so CF can be saved in dead SIMD scratch first.
                for (cc, r) in [(2, S0), (4, S1), (8, S2)] {
                    self.emit(Opcode::Setcc(cc), [asm_arg(r8(r))]);
                }
                self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r8(S0))]);
                self.emit(Opcode::Movd, [asm_arg(xmm(FS1)), asm_arg(r32(S0))]);
                self.emit(Opcode::Setcc(0), [asm_arg(r8(S0))]);
                for r in [S0, S1, S2] {
                    self.emit(Opcode::Movzx, [asm_arg(r32(r)), asm_arg(r8(r))]);
                }
                for (r, shift) in [(S0, 11), (S1, 6), (S2, 7)] {
                    self.emit(Opcode::Shl, [asm_arg(r32(r)), asm_arg(shift)]);
                }
                self.emit(Opcode::Or, [asm_arg(r32(S2)), asm_arg(r32(S1))]);
                self.emit(Opcode::Or, [asm_arg(r32(S2)), asm_arg(r32(S0))]);
                self.emit(Opcode::Movd, [asm_arg(r32(S0)), asm_arg(xmm(FS1))]);
                self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S2))]);
                let d = self.def_reg(result);
                if d != S0 {
                    self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
                }
                self.def_done(result, d);
            }
            _ => unreachable!(),
        }
    }
}
