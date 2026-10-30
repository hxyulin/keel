//! Raw masked FP operations retain MXCSR ordering and execute only active lanes.
use super::avx512_mask::{k1, opcode};
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_float_mask(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        if id.is_x86_avx512_float_compare() {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[1], FS1);
            self.avx512_stage_mask(args[2]);
            let name = if id == X86Avx512Cmpps {
                "vcmpps"
            } else {
                "vcmppd"
            };
            self.emit(
                opcode(name, true),
                [
                    asm_arg(k1()),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(iconst(self.func, args[3]).expect("validated FP predicate")),
                ],
            );
            let d = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").expect("AVX512F mask move"),
                [asm_arg(r32(d)), asm_arg(k1())],
            );
            self.def_done(result, d);
            return;
        }
        let load = matches!(id, X86Avx512MaskLoadps | X86Avx512MaskLoadpd);
        let store = matches!(id, X86Avx512MaskStoreps | X86Avx512MaskStorepd);
        if load || store {
            self.avx512_stage_vector(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let at = sized(memory(r64(base), 0), 512);
            let name = if matches!(id, X86Avx512MaskLoadps | X86Avx512MaskStoreps) {
                "vmovups"
            } else {
                "vmovupd"
            };
            if store {
                self.emit(
                    opcode(name, true),
                    [asm_arg(at), asm_arg(simd_width(FS0, 512))],
                );
                return;
            }
            self.emit(
                opcode(name, true),
                [asm_arg(simd_width(FS0, 512)), asm_arg(at)],
            );
        } else {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            if args.len() == 4 {
                self.avx512_stage_vector(args[3], FS2);
            }
            self.avx512_stage_mask(args[1]);
            let name = match id {
                X86Avx512MaskAddps => "vaddps",
                X86Avx512MaskAddpd => "vaddpd",
                X86Avx512MaskSubps => "vsubps",
                X86Avx512MaskSubpd => "vsubpd",
                X86Avx512MaskMulps => "vmulps",
                X86Avx512MaskMulpd => "vmulpd",
                X86Avx512MaskDivps => "vdivps",
                X86Avx512MaskDivpd => "vdivpd",
                X86Avx512MaskSqrtps => "vsqrtps",
                X86Avx512MaskSqrtpd => "vsqrtpd",
                _ => unreachable!("raw masked AVX512 FP operation"),
            };
            if args.len() == 4 {
                self.emit(
                    opcode(name, true),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(simd_width(FS2, 512)),
                    ],
                );
            } else {
                self.emit(
                    opcode(name, true),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS1, 512))],
                );
            }
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                Opcode::parse("vmovups").expect("AVX move"),
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(FS0, 512))],
            );
        }
        self.def_done(result, d);
    }
}
