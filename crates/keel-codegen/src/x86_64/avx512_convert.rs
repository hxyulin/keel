//! Raw masked conversions expose hardware rounding, indefinite results and SAE.
use super::*;

fn mnemonic(id: Intrinsic) -> EvexMnemonic {
    use Intrinsic::*;
    match id {
        X86Avx512Vcvtdq2ps => EvexMnemonic::Vcvtdq2ps,
        X86Avx512Vcvtudq2ps => EvexMnemonic::Vcvtudq2ps,
        X86Avx512Vcvtdq2pd => EvexMnemonic::Vcvtdq2pd,
        X86Avx512Vcvtudq2pd => EvexMnemonic::Vcvtudq2pd,
        X86Avx512Vcvtps2dq => EvexMnemonic::Vcvtps2dq,
        X86Avx512Vcvtps2udq => EvexMnemonic::Vcvtps2udq,
        X86Avx512Vcvttps2dq => EvexMnemonic::Vcvttps2dq,
        X86Avx512Vcvttps2udq => EvexMnemonic::Vcvttps2udq,
        X86Avx512Vcvtpd2dq => EvexMnemonic::Vcvtpd2dq,
        X86Avx512Vcvtpd2udq => EvexMnemonic::Vcvtpd2udq,
        X86Avx512Vcvttpd2dq => EvexMnemonic::Vcvttpd2dq,
        X86Avx512Vcvttpd2udq => EvexMnemonic::Vcvttpd2udq,
        X86Avx512Vcvtps2pd => EvexMnemonic::Vcvtps2pd,
        X86Avx512Vcvtpd2ps => EvexMnemonic::Vcvtpd2ps,
        X86Avx512Vcvtph2ps => EvexMnemonic::Vcvtph2ps,
        X86Avx512Vcvtps2ph => EvexMnemonic::Vcvtps2ph,
        _ => unreachable!("AVX512F conversion intrinsic"),
    }
}
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("AVX512 conversion requires AVX512F");
    }
    if args.len() != 4
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("AVX512 conversion signature mismatch");
    }
    if !iconst(func, args[3]).is_some_and(|control| id.valid_x86_avx512_convert_control(control)) {
        return Err("invalid AVX512 conversion rounding/SAE control");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        for (value, scratch) in [(args[2], FS1), (args[0], FS0)] {
            let ty = self.ty(value);
            let source = self.use_freg(value, scratch);
            if source != scratch {
                self.emit(
                    movs(ty),
                    [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
                );
            }
        }
        self.avx512_stage_mask(args[1]);
        let control = iconst(self.func, args[3]).expect("validated conversion control");
        let mut opcode = EvexOpcode::new(mnemonic(id));
        opcode.options.mask = Some(1);
        if control & 8 != 0 {
            opcode.options.rounding = Some(
                if matches!(
                    id,
                    X86Avx512Vcvtdq2ps
                        | X86Avx512Vcvtudq2ps
                        | X86Avx512Vcvtps2dq
                        | X86Avx512Vcvtps2udq
                        | X86Avx512Vcvtpd2dq
                        | X86Avx512Vcvtpd2udq
                        | X86Avx512Vcvtpd2ps
                ) {
                    match control & 3 {
                        0 => Rounding::Nearest,
                        1 => Rounding::Down,
                        2 => Rounding::Up,
                        _ => Rounding::Zero,
                    }
                } else {
                    Rounding::Sae
                },
            );
        }
        let output = self.ty(args[0]);
        let input = self.ty(args[2]);
        if id == X86Avx512Vcvtps2ph {
            self.emit(
                Opcode::Evex(opcode),
                [
                    asm_arg(simd(FS0, output)),
                    asm_arg(simd(FS1, input)),
                    asm_arg(control & 7),
                ],
            );
        } else {
            self.emit(
                Opcode::Evex(opcode),
                [asm_arg(simd(FS0, output)), asm_arg(simd(FS1, input))],
            );
        }
        self.def_vector_from_fs0(result, output);
    }
}
