//! Byte/word arithmetic and memory operations keep masks in scalar SSA.
use super::*;
use keel_ir::intrinsic::X86Avx512BwKind;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let descriptor = id.x86_avx512_bw_core().expect("BW core descriptor");
    if !features.avx512f
        || !features.avx512bw
        || (descriptor.vector.bytes() < 64 && !features.avx512vl)
    {
        return Err(
            "byte/word operations require AVX512F plus AVX512BW, and narrow forms require AVX512VL",
        );
    }
    let info = id.info();
    if args.len() != info.params.len()
        || args
            .iter()
            .zip(info.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("AVX512BW core signature mismatch");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn avx512_bw_stage_mask(&mut self, mask: Value) {
        let source = self.use_reg(mask, S0);
        let (name, register) = match self.ty(mask) {
            Type::I64 => ("kmovq", r64(source)),
            Type::I32 => ("kmovd", r32(source)),
            Type::I8 | Type::I16 => ("kmovw", r32(source)),
            _ => unreachable!("BW masks have their declared scalar lane width"),
        };
        self.emit(
            Opcode::parse(name).expect("BW mask move"),
            [asm_arg(avx512_mask::k1()), asm_arg(register)],
        );
    }
    pub(super) fn native_avx512_bw(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let descriptor = id.x86_avx512_bw_core().expect("BW core descriptor");
        let ty = Type::Vector(descriptor.vector);
        let width = ptr(ty);
        let instruction = avx512_mask::opcode(descriptor.opcode, true);
        match descriptor.kind {
            X86Avx512BwKind::Binary | X86Avx512BwKind::Move => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[2], FS1);
                if descriptor.kind == X86Avx512BwKind::Binary {
                    self.avx512_stage_vector(args[3], FS2);
                }
                self.avx512_bw_stage_mask(args[1]);
                if descriptor.kind == X86Avx512BwKind::Binary {
                    self.emit(
                        instruction,
                        [
                            asm_arg(simd_width(FS0, width)),
                            asm_arg(simd_width(FS1, width)),
                            asm_arg(simd_width(FS2, width)),
                        ],
                    );
                } else {
                    self.emit(
                        instruction,
                        [
                            asm_arg(simd_width(FS0, width)),
                            asm_arg(simd_width(FS1, width)),
                        ],
                    );
                }
            }
            X86Avx512BwKind::Load | X86Avx512BwKind::Store => {
                self.avx512_stage_vector(args[1], FS0);
                self.avx512_bw_stage_mask(args[2]);
                let base = self.use_reg(args[0], S1);
                let address = asm_arg(sized(memory(r64(base), 0), width));
                if descriptor.kind == X86Avx512BwKind::Store {
                    self.emit(instruction, [address, asm_arg(simd_width(FS0, width))]);
                    return;
                }
                self.emit(instruction, [asm_arg(simd_width(FS0, width)), address]);
            }
        }
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
