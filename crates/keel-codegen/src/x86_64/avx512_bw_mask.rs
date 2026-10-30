//! BW mask results stay scalar SSA values; K1 is transient scratch.
use super::*;
use keel_ir::intrinsic::X86Avx512BwMaskKind;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_bw_mask().expect("BW mask descriptor");
    if !features.avx512f || !features.avx512bw || (d.vector.bytes() < 64 && !features.avx512vl) {
        return Err(
            "BW mask operations require AVX512F plus AVX512BW, and narrow forms require AVX512VL",
        );
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("AVX512BW mask signature mismatch");
    }
    if d.kind == X86Avx512BwMaskKind::Compare
        && !iconst(func, args[3]).is_some_and(|predicate| (0..=7).contains(&predicate))
    {
        return Err("AVX512BW comparison predicate must be iconst.i32 in 0..=7");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_mask(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_bw_mask().expect("BW mask descriptor");
        let width = (d.vector.bytes() * 8) as u16;
        let predicate = avx512_mask::k1();
        match d.kind {
            X86Avx512BwMaskKind::Compare | X86Avx512BwMaskKind::Test => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[1], FS1);
                self.avx512_bw_stage_mask(args[2]);
                let instruction = avx512_mask::opcode(d.opcode, true);
                if d.kind == X86Avx512BwMaskKind::Compare {
                    self.emit(
                        instruction,
                        [
                            asm_arg(predicate),
                            asm_arg(simd_width(FS0, width)),
                            asm_arg(simd_width(FS1, width)),
                            asm_arg(iconst(self.func, args[3]).expect("validated predicate")),
                        ],
                    );
                } else {
                    self.emit(
                        instruction,
                        [
                            asm_arg(predicate),
                            asm_arg(simd_width(FS0, width)),
                            asm_arg(simd_width(FS1, width)),
                        ],
                    );
                }
            }
            X86Avx512BwMaskKind::ToMask => {
                self.avx512_stage_vector(args[0], FS0);
                self.emit(
                    avx512_mask::opcode(d.opcode, false),
                    [asm_arg(predicate), asm_arg(simd_width(FS0, width))],
                );
            }
            X86Avx512BwMaskKind::FromMask => {
                self.avx512_bw_stage_mask(args[0]);
                self.emit(
                    avx512_mask::opcode(d.opcode, false),
                    [asm_arg(simd_width(FS0, width)), asm_arg(predicate)],
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
                return;
            }
        }
        let destination = self.def_reg(result);
        let (opcode, register) = match d.mask {
            Type::I64 => ("kmovq", r64(destination)),
            Type::I32 => ("kmovd", r32(destination)),
            Type::I8 | Type::I16 => ("kmovw", r32(destination)),
            _ => unreachable!("BW lane mask type"),
        };
        self.emit(
            Opcode::parse(opcode).expect("BW mask result move"),
            [asm_arg(register), asm_arg(predicate)],
        );
        self.def_done(result, destination);
    }
}
