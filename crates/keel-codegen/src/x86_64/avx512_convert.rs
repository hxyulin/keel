//! Raw masked conversions expose hardware rounding, indefinite results and SAE.
use super::*;
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
        let name = id
            .info()
            .name
            .strip_prefix("x86_avx512_")
            .expect("raw conversion name");
        let mut opcode = EvexOpcode::parse(name).expect("AVX512F conversion opcode");
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
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                movs(output),
                [asm_arg(simd(d, output)), asm_arg(simd(FS0, output))],
            );
        }
        self.def_done(result, d);
    }
}
