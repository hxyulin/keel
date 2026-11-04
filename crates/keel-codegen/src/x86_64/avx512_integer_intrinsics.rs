//! Raw AVX512F integer controls retain hardware count and mask semantics.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("AVX512 integer intrinsic requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 integer intrinsic signature mismatch");
    }
    if let Some(index) = id.x86_avx512_integer_control()
        && !iconst(func, args[index]).is_some_and(|control| (0..256).contains(&control))
    {
        return Err("AVX512 integer control must be iconst.i32 in0..=255");
    }
    Ok(())
}

fn name(id: Intrinsic) -> EvexMnemonic {
    use Intrinsic::*;
    match id {
        X86Avx512MaskTernlogd => EvexMnemonic::Vpternlogd,
        X86Avx512MaskTernlogq => EvexMnemonic::Vpternlogq,
        X86Avx512MaskRold => EvexMnemonic::Vprold,
        X86Avx512MaskRolq => EvexMnemonic::Vprolq,
        X86Avx512MaskRord => EvexMnemonic::Vprord,
        X86Avx512MaskRorq => EvexMnemonic::Vprorq,
        X86Avx512MaskRolvd => EvexMnemonic::Vprolvd,
        X86Avx512MaskRolvq => EvexMnemonic::Vprolvq,
        X86Avx512MaskRorvd => EvexMnemonic::Vprorvd,
        X86Avx512MaskRorvq => EvexMnemonic::Vprorvq,
        X86Avx512MaskSllvd => EvexMnemonic::Vpsllvd,
        X86Avx512MaskSllvq => EvexMnemonic::Vpsllvq,
        X86Avx512MaskSrlvd => EvexMnemonic::Vpsrlvd,
        X86Avx512MaskSrlvq => EvexMnemonic::Vpsrlvq,
        X86Avx512MaskSravd => EvexMnemonic::Vpsravd,
        X86Avx512MaskSravq => EvexMnemonic::Vpsravq,
        X86Avx512Testd => EvexMnemonic::Vptestmd,
        X86Avx512Testq => EvexMnemonic::Vptestmq,
        X86Avx512Testnd => EvexMnemonic::Vptestnmd,
        X86Avx512Testnq => EvexMnemonic::Vptestnmq,
        X86Avx512MaskAbsd => EvexMnemonic::Vpabsd,
        X86Avx512MaskAbsq => EvexMnemonic::Vpabsq,
        X86Avx512MaskMuldq => EvexMnemonic::Vpmuldq,
        X86Avx512MaskMuludq => EvexMnemonic::Vpmuludq,
        _ => unreachable!("AVX512 integer intrinsic"),
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_integer_intrinsic(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let opcode = name(id);
        if id.is_x86_avx512_integer_test() {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[1], FS1);
            self.avx512_stage_mask(args[2]);
            self.emit(
                avx512_mask::opcode(opcode, true),
                [
                    asm_arg(avx512_mask::k1()),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                ],
            );
            let destination = self.def_reg(result);
            self.emit(
                op::KMOVW,
                [asm_arg(r32(destination)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, destination);
            return;
        }
        let output = if id.x86_avx512_integer_control() == Some(5) {
            // Ternary logic overwrites its first input. Compute in scratch,
            // then merge separately so the passthrough can be independent of A.
            self.avx512_stage_vector(args[2], FS0);
            self.avx512_stage_vector(args[3], FS1);
            self.avx512_stage_vector(args[4], FS2);
            self.emit(
                avx512_mask::opcode(opcode, false),
                [
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(simd_width(FS2, 512)),
                    asm_arg(iconst(self.func, args[5]).expect("verified ternary control")),
                ],
            );
            self.avx512_stage_vector(args[0], FS1);
            self.avx512_stage_mask(args[1]);
            self.emit(
                avx512_mask::opcode(
                    if id.info().returns[0] == Type::Vector(keel_ir::VectorType::I64x8) {
                        EvexMnemonic::Vmovdqu64
                    } else {
                        EvexMnemonic::Vmovdqu32
                    },
                    true,
                ),
                [asm_arg(simd_width(FS1, 512)), asm_arg(simd_width(FS0, 512))],
            );
            FS1
        } else {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            if args.len() == 4 && id.x86_avx512_integer_control().is_none() {
                self.avx512_stage_vector(args[3], FS2);
            }
            self.avx512_stage_mask(args[1]);
            if let Some(index) = id.x86_avx512_integer_control() {
                self.emit(
                    avx512_mask::opcode(opcode, true),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(iconst(self.func, args[index]).expect("verified rotation control")),
                    ],
                );
            } else if args.len() == 4 {
                self.emit(
                    avx512_mask::opcode(opcode, true),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(simd_width(FS2, 512)),
                    ],
                );
            } else {
                self.emit(
                    avx512_mask::opcode(opcode, true),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS1, 512))],
                );
            }
            FS0
        };
        let destination = self.def_freg(result);
        if destination != output {
            self.emit(
                Opcode::Vmovups,
                [
                    asm_arg(simd_width(destination, 512)),
                    asm_arg(simd_width(output, 512)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}
