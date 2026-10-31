//! VPMOV source width determines produced lanes and upper result zeroing.
use super::*;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_vl_narrow().expect("VL narrowing descriptor");
    if !features.avx512f || !features.avx512vl {
        return Err("VL narrowing conversions require AVX512F plus AVX512VL");
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("VL narrowing conversion signature mismatch");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_vl_narrow(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_vl_narrow().expect("VL narrowing descriptor");
        let instruction = avx512_mask::opcode(d.opcode, true);
        if d.store {
            self.avx512_stage_vector(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            self.emit(
                instruction,
                [
                    asm_arg(sized(memory(r64(base), 0), d.memory_bytes * 8)),
                    asm_arg(simd(FS0, self.ty(args[1]))),
                ],
            );
            return;
        }
        // Capture both immutable inputs before changing the tied destination.
        // Hardware clears bytes above the produced lane count, even with mask zero.
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_vector(args[2], FS1);
        self.avx512_stage_mask(args[1]);
        self.emit(
            instruction,
            [
                asm_arg(simd(FS0, self.ty(args[0]))),
                asm_arg(simd(FS1, self.ty(args[2]))),
            ],
        );
        let ty = self.ty(result.expect("narrowing conversion result"));
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(ty),
                [asm_arg(simd(destination, ty)), asm_arg(simd(FS0, ty))],
            );
        }
        self.def_done(result, destination);
    }
}
