//! Scalar SSA masks live in GPRs. K1 is scratch between each mask transfer and use.
use super::*;
use keel_asm_ir::x86::evex::EvexOpcode;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("AVX512 mask intrinsic requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("AVX512 mask intrinsic signature mismatch");
    }
    if id.is_x86_avx512_compare() && !iconst(func, args[2]).is_some_and(|n| (0..8).contains(&n)) {
        return Err("AVX512 integer predicate must be iconst.i32 in0..=7");
    }
    Ok(())
}

fn k1() -> Register {
    Register {
        class: RegisterClass::Mask,
        number: 1,
        high_byte: false,
        width: 64,
    }
}
fn opcode(name: &str, masked: bool) -> Opcode {
    let mut op = EvexOpcode::parse(name).expect("registered AVX512 mask opcode");
    if masked {
        op.options.mask = Some(1);
    }
    Opcode::Evex(op)
}
impl FunctionCompiler<'_> {
    fn avx512_stage_vector(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                movs(self.ty(value)),
                [
                    asm_arg(simd_width(scratch, 512)),
                    asm_arg(simd_width(source, 512)),
                ],
            );
        }
    }
    fn avx512_stage_mask(&mut self, value: Value) {
        let source = self.use_reg(value, S0);
        self.emit(
            Opcode::parse("kmovw").expect("AVX512F mask move"),
            [asm_arg(k1()), asm_arg(r32(source))],
        );
    }
    pub(super) fn native_avx512_mask(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        if id.is_x86_avx512_compare() {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[1], FS1);
            let name = match id {
                X86Avx512Cmpd => "vpcmpd",
                X86Avx512Cmpq => "vpcmpq",
                X86Avx512Cmpud => "vpcmpud",
                X86Avx512Cmpuq => "vpcmpuq",
                _ => unreachable!(),
            };
            self.emit(
                opcode(name, false),
                [
                    asm_arg(k1()),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(iconst(self.func, args[2]).expect("validated predicate")),
                ],
            );
            let d = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").expect("AVX512F mask move"),
                [asm_arg(r32(d)), asm_arg(k1())],
            );
            self.def_done(result, d);
            return;
        }
        let load = matches!(id, X86Avx512MaskLoadd | X86Avx512MaskLoadq);
        let store = matches!(id, X86Avx512MaskStored | X86Avx512MaskStoreq);
        if load || store {
            self.avx512_stage_vector(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let mem = sized(memory(r64(base), 0), 512);
            let name = if matches!(id, X86Avx512MaskLoadq | X86Avx512MaskStoreq) {
                "vmovdqu64"
            } else {
                "vmovdqu32"
            };
            if store {
                self.emit(
                    opcode(name, true),
                    [asm_arg(mem), asm_arg(simd_width(FS0, 512))],
                );
                return;
            }
            self.emit(
                opcode(name, true),
                [asm_arg(simd_width(FS0, 512)), asm_arg(mem)],
            );
        } else {
            // Capture all immutable SSA operands before modifying the merge destination.
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            self.avx512_stage_vector(args[3], FS2);
            self.avx512_stage_mask(args[1]);
            let name = match id {
                X86Avx512MaskAddd => "vpaddd",
                X86Avx512MaskAddq => "vpaddq",
                X86Avx512MaskSubd => "vpsubd",
                X86Avx512MaskSubq => "vpsubq",
                _ => unreachable!("AVX512 mask arithmetic"),
            };
            self.emit(
                opcode(name, true),
                [
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(simd_width(FS2, 512)),
                ],
            );
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                movs(self.ty(args[if load { 1 } else { 0 }])),
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(FS0, 512))],
            );
        }
        self.def_done(result, d);
    }
}
