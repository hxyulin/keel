//! VL widening reads an immutable XMM source, then applies its independent merge.
use super::*;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_vl_widen().expect("VL widen descriptor");
    if !features.avx512f || !features.avx512vl {
        return Err("VL widening requires AVX512F plus AVX512VL");
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("VL widening signature mismatch");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_vl_widen(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_vl_widen().expect("VL widen descriptor");
        let width = (d.destination.bytes() * 8) as u16;
        self.avx512_stage_vector(args[2], FS0);
        self.emit(
            avx512_mask::opcode(d.opcode, false),
            [
                asm_arg(simd_width(FS0, width)),
                asm_arg(simd_width(FS0, 128)),
            ],
        );
        self.avx512_stage_vector(args[0], FS1);
        self.avx512_stage_mask(args[1]);
        self.emit(
            avx512_mask::opcode(
                if d.destination.lane_type() == Type::I32 {
                    "vmovdqa32"
                } else {
                    "vmovdqa64"
                },
                true,
            ),
            [
                asm_arg(simd_width(FS1, width)),
                asm_arg(simd_width(FS0, width)),
            ],
        );
        let dst = self.def_freg(result);
        if dst != FS1 {
            self.emit(
                movs(Type::Vector(d.destination)),
                [
                    asm_arg(simd_width(dst, width)),
                    asm_arg(simd_width(FS1, width)),
                ],
            );
        }
        self.def_done(result, dst);
    }
}
