//! FMA keeps encoded operand order and merges inactive bits independently.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
const MERGE: u8 = 12;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("packed AVX512 FMA requires AVX512F");
    }
    if args.len() != 6
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("packed AVX512 FMA signature mismatch");
    }
    if !iconst(func, args[5]).is_some_and(|control| matches!(control, 4 | 8..=11)) {
        return Err("AVX512 FMA control must be iconst.i32 4 or8..=11");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_fma(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        for (value, scratch) in [
            (args[0], MERGE),
            (args[2], FS0),
            (args[3], FS1),
            (args[4], FS2),
        ] {
            self.avx512_stage_vector(value, scratch);
        }
        self.avx512_stage_mask(args[1]);
        let mut opcode = EvexOpcode::parse(
            id.x86_avx512_fma_opcode()
                .expect("packed AVX512 FMA intrinsic"),
        )
        .expect("AVX512 FMA opcode");
        opcode.options.mask = Some(1);
        let control = iconst(self.func, args[5]).expect("validated FMA control");
        if control != 4 {
            opcode.options.rounding = Some(match control & 3 {
                0 => Rounding::Nearest,
                1 => Rounding::Down,
                2 => Rounding::Up,
                _ => Rounding::Zero,
            });
        }
        self.emit(
            Opcode::Evex(opcode),
            [
                asm_arg(simd_width(FS0, 512)),
                asm_arg(simd_width(FS1, 512)),
                asm_arg(simd_width(FS2, 512)),
            ],
        );
        // Hardware ties its inactive destination to arithmetic operand a. Copy
        // only active results into the separate merge value after the FMA, so
        // inactive arithmetic neither executes nor changes merge payloads.
        let merge_move = if self.ty(args[0]) == Type::Vector(keel_ir::VectorType::F64x8) {
            "vmovupd"
        } else {
            "vmovups"
        };
        self.emit(
            avx512_mask::opcode(merge_move, true),
            [
                asm_arg(simd_width(MERGE, 512)),
                asm_arg(simd_width(FS0, 512)),
            ],
        );
        let d = self.def_freg(result);
        if d != MERGE {
            self.emit(
                movs(self.ty(args[0])),
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(MERGE, 512))],
            );
        }
        self.def_done(result, d);
    }
}
