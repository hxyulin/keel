//! Encoded width governs VL legality; memory operands retain their shortened size.
use super::*;
use keel_ir::intrinsic::X86Avx512BwConvertKind;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id
        .x86_avx512_bw_convert()
        .expect("BW conversion descriptor");
    if !features.avx512f || !features.avx512bw || (d.encoded_bits < 512 && !features.avx512vl) {
        return Err(
            "BW conversions require AVX512F plus AVX512BW; encoded 128/256-bit forms require AVX512VL",
        );
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("AVX512BW conversion signature mismatch");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id
            .x86_avx512_bw_convert()
            .expect("BW conversion descriptor");
        let instruction = avx512_mask::opcode(d.opcode, true);
        match d.kind {
            X86Avx512BwConvertKind::Widen | X86Avx512BwConvertKind::Narrow => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_bw_stage_mask(args[1]);
                self.emit(
                    instruction,
                    [
                        asm_arg(simd(FS0, self.ty(args[0]))),
                        asm_arg(simd(FS1, self.ty(args[2]))),
                    ],
                );
            }
            X86Avx512BwConvertKind::WidenLoad | X86Avx512BwConvertKind::NarrowStore => {
                self.avx512_stage_vector(args[1], FS0);
                self.avx512_bw_stage_mask(args[2]);
                let base = self.use_reg(args[0], S1);
                let address = asm_arg(sized(memory(r64(base), 0), d.memory_bytes * 8));
                if d.kind == X86Avx512BwConvertKind::NarrowStore {
                    self.emit(instruction, [address, asm_arg(simd(FS0, self.ty(args[1])))]);
                    return;
                }
                self.emit(instruction, [asm_arg(simd(FS0, self.ty(args[1]))), address]);
            }
        }
        let ty = self.ty(result.expect("conversion value"));
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
