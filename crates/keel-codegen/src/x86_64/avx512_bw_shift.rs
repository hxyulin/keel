//! Raw word counts are passed unchanged to saturating x86 shift instructions.
use super::*;
use keel_ir::intrinsic::X86Avx512BwShiftKind;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_bw_shift().expect("BW shift descriptor");
    if !features.avx512f || !features.avx512bw || (d.vector.bytes() < 64 && !features.avx512vl) {
        return Err(
            "BW word shifts require AVX512F plus AVX512BW, and narrow forms require AVX512VL",
        );
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("AVX512BW word-shift signature mismatch");
    }
    if d.kind == X86Avx512BwShiftKind::Immediate
        && !iconst(func, args[3]).is_some_and(|count| (0..=255).contains(&count))
    {
        return Err("AVX512BW immediate word shift count must be iconst.i32 in 0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_shift(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_bw_shift().expect("BW shift descriptor");
        let width = (d.vector.bytes() * 8) as u16;
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_vector(args[2], FS1);
        if d.kind != X86Avx512BwShiftKind::Immediate {
            self.avx512_stage_vector(args[3], FS2);
        }
        self.avx512_bw_stage_mask(args[1]);
        let count = if d.kind == X86Avx512BwShiftKind::Immediate {
            asm_arg(iconst(self.func, args[3]).expect("validated immediate"))
        } else {
            asm_arg(simd_width(
                FS2,
                if d.kind == X86Avx512BwShiftKind::Shared {
                    128
                } else {
                    width
                },
            ))
        };
        self.emit(
            avx512_mask::opcode(d.opcode, true),
            [
                asm_arg(simd_width(FS0, width)),
                asm_arg(simd_width(FS1, width)),
                count,
            ],
        );
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(Type::Vector(d.vector)),
                [
                    asm_arg(simd_width(destination, width)),
                    asm_arg(simd_width(FS0, width)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}
