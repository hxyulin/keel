//! Scalar SSA masks live in GPRs. K1 is scratch between each mask transfer and use.
use super::*;
use keel_asm_ir::x86::evex::EvexOpcode;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f
        || (id.is_x86_avx512_vl_core()
            || id.x86_avx512_vl_float_arithmetic().is_some()
            || id.x86_avx512_vl_integer_move().is_some())
            && !features.avx512vl
    {
        return Err(
            "masked vector operation requires AVX512F and, for 128/256-bit packed forms, AVX512VL",
        );
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 mask intrinsic signature mismatch");
    }
    let name = id.x86_avx512_mask_opcode().expect("masked vector opcode");
    if name.starts_with("vcmp") || name.starts_with("vpcmp") {
        let index = if args.len() == 3 { 2 } else { 3 };
        let maximum = if name.starts_with("vpcmp") { 7 } else { 31 };
        if !iconst(func, args[index]).is_some_and(|value| (0..=maximum).contains(&value)) {
            return Err("AVX512 comparison requires a constant predicate in its declared range");
        }
        if args.len() == 5 && !matches!(iconst(func, args[4]), Some(4 | 8)) {
            return Err("AVX512 comparison SAE control must be iconst.i32 4 or 8");
        }
    }
    Ok(())
}

pub(super) fn k1() -> Register {
    Register {
        class: RegisterClass::Mask,
        number: 1,
        high_byte: false,
        width: 64,
    }
}
pub(super) fn opcode(name: &str, masked: bool) -> Opcode {
    let mut op = EvexOpcode::parse(name).expect("registered AVX512 mask opcode");
    if masked {
        op.options.mask = Some(1);
    }
    Opcode::Evex(op)
}
impl FunctionCompiler<'_> {
    pub(super) fn avx512_stage_vector(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            let ty = self.ty(value);
            self.emit(
                movs(ty),
                [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
            );
        }
    }
    pub(super) fn avx512_stage_mask(&mut self, value: Value) {
        let source = self.use_reg(value, S0);
        // KMOVW belongs to AVX512F and covers every mask in this family.
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
        self.native_masked_vector(id, args, result);
    }
    pub(super) fn native_masked_vector(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let info = id.info();
        let ty = *info
            .params
            .iter()
            .find(|ty| ty.is_vector())
            .expect("masked vector operand");
        let width = (ty.bytes().expect("vector width") * 8) as u16;
        let name = id.x86_avx512_mask_opcode().expect("masked vector opcode");
        let compare = name.starts_with("vcmp") || name.starts_with("vpcmp");
        if compare {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[1], FS1);
            let masked = args.len() != 3;
            if masked {
                self.avx512_stage_mask(args[2]);
            }
            let mut instruction = opcode(name, masked);
            if args.len() == 5 && iconst(self.func, args[4]) == Some(8) {
                let Opcode::Evex(ref mut op) = instruction else {
                    unreachable!()
                };
                op.options.rounding = Some(keel_asm_ir::x86::evex::Rounding::Sae);
            }
            let index = if masked { 3 } else { 2 };
            self.emit(
                instruction,
                [
                    asm_arg(k1()),
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS1, width)),
                    asm_arg(iconst(self.func, args[index]).expect("validated predicate")),
                ],
            );
            let destination = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").expect("AVX512F mask move"),
                [asm_arg(r32(destination)), asm_arg(k1())],
            );
            self.def_done(result, destination);
            return;
        }
        if info.params[0] == Type::Ptr {
            self.avx512_stage_vector(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let address = asm_arg(sized(memory(r64(base), 0), width));
            if info.returns.is_empty() {
                self.emit(
                    opcode(name, true),
                    [address, asm_arg(simd_width(FS0, width))],
                );
                return;
            }
            self.emit(
                opcode(name, true),
                [asm_arg(simd_width(FS0, width)), address],
            );
        } else {
            // Capture all sources before writing an overlapping merge destination.
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            if args.len() == 4 {
                self.avx512_stage_vector(args[3], FS2);
            }
            self.avx512_stage_mask(args[1]);
            if args.len() == 4 {
                self.emit(
                    opcode(name, true),
                    [
                        asm_arg(simd_width(FS0, width)),
                        asm_arg(simd_width(FS1, width)),
                        asm_arg(simd_width(FS2, width)),
                    ],
                );
            } else {
                self.emit(
                    opcode(name, true),
                    [
                        asm_arg(simd_width(FS0, width)),
                        asm_arg(simd_width(FS1, width)),
                    ],
                );
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
