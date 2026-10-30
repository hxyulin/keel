//! Narrow raw conversions use MXCSR and never carry ER/SAE decorators.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f || !features.avx512vl {
        return Err("narrow conversion requires AVX512F and AVX512VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("AVX512VL conversion signature mismatch");
    }
    if args.len() == 4 && !iconst(func, args[3]).is_some_and(|n| (0..=255).contains(&n)) {
        return Err("AVX512VL half conversion requires an imm8");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_vl_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, width, input, output) = id
            .x86_avx512_vl_convert()
            .expect("VL conversion descriptor");
        let input = Type::Vector(input);
        let output = Type::Vector(output);
        // Encoded length follows the descriptor. Narrowing can produce XMM
        // from YMM, while widening can consume only the low half of XMM.
        debug_assert_eq!(
            width as u32,
            input.bytes().unwrap().max(output.bytes().unwrap()) * 8
        );
        self.avx512_stage_vector(args[2], FS1);
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_mask(args[1]);
        let opcode = avx512_mask::opcode(name, true);
        if name == "vcvtps2ph" {
            // Bits 3..7 are ignored by hardware, not SAE decorators.
            let control = iconst(self.func, args[3]).expect("validated imm8");
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, output)),
                    asm_arg(simd(FS1, input)),
                    asm_arg(control),
                ],
            );
        } else {
            self.emit(
                opcode,
                [asm_arg(simd(FS0, output)), asm_arg(simd(FS1, input))],
            );
        }
        // The conversion itself clears the architectural short-result tail,
        // including when every meaningful mask bit is clear.
        let dest = self.def_freg(result);
        if dest != FS0 {
            self.emit(
                movs(output),
                [asm_arg(simd(dest, output)), asm_arg(simd(FS0, output))],
            );
        }
        self.def_done(result, dest);
    }
}
