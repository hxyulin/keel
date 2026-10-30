//! Pairwise operations use output-lane masks and retain immutable input vectors.
use super::*;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_bw_mul().expect("BW multiply descriptor");
    if !features.avx512f || !features.avx512bw || (d.result.bytes() < 64 && !features.avx512vl) {
        return Err(
            "BW multiply and average operations require AVX512F plus AVX512BW, and narrow forms require AVX512VL",
        );
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("AVX512BW multiply signature mismatch");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_mul(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_bw_mul().expect("BW multiply descriptor");
        let ty = Type::Vector(d.result);
        let width = ptr(ty);
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_vector(args[2], FS1);
        self.avx512_stage_vector(args[3], FS2);
        self.avx512_bw_stage_mask(args[1]);
        self.emit(
            avx512_mask::opcode(d.opcode, true),
            [
                asm_arg(simd_width(FS0, width)),
                asm_arg(simd_width(FS1, width)),
                asm_arg(simd_width(FS2, width)),
            ],
        );
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(ty),
                [
                    asm_arg(simd_width(destination, width)),
                    asm_arg(simd_width(FS0, width)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}
