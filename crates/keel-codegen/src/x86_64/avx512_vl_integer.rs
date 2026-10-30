//! Pure VL integers use three reserved vector scratches and a scalar SSA mask.
use super::*;
use keel_ir::intrinsic::X86Avx512VlIntegerKind as Kind;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_vl_integer().expect("VL integer descriptor");
    if !features.avx512f || !features.avx512vl {
        return Err("VL integer operation requires AVX512F plus AVX512VL");
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("VL integer signature mismatch");
    }
    if d.kind == Kind::Ternary && !iconst(func, args[5]).is_some_and(|n| (0..=255).contains(&n)) {
        return Err("VL ternary truth table must be iconst.i32 in 0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_vl_integer(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_vl_integer().expect("VL integer descriptor");
        let width = (d.vector.bytes() * 8) as u16;
        if d.kind == Kind::Test {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[1], FS1);
            self.avx512_stage_mask(args[2]);
            self.emit(
                avx512_mask::opcode(d.opcode, true),
                [
                    asm_arg(avx512_mask::k1()),
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS1, width)),
                ],
            );
            let dst = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").unwrap(),
                [asm_arg(r32(dst)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, dst);
            return;
        }
        // Compute first, then stage the independent merge. This keeps even a
        // three-input destructive ternary operation within XMM13..15.
        self.avx512_stage_vector(args[2], FS0);
        if d.kind != Kind::Abs {
            self.avx512_stage_vector(args[3], FS1);
        }
        match d.kind {
            Kind::Abs => self.emit(
                avx512_mask::opcode(d.opcode, false),
                [
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS0, width)),
                ],
            ),
            Kind::Multiply => self.emit(
                avx512_mask::opcode(d.opcode, false),
                [
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS0, width)),
                    asm_arg(simd_width(FS1, width)),
                ],
            ),
            Kind::Ternary => {
                self.avx512_stage_vector(args[4], FS2);
                self.emit(
                    avx512_mask::opcode(d.opcode, false),
                    [
                        asm_arg(simd_width(FS0, width)),
                        asm_arg(simd_width(FS1, width)),
                        asm_arg(simd_width(FS2, width)),
                        asm_arg(iconst(self.func, args[5]).expect("validated ternary control")),
                    ],
                );
            }
            Kind::Test => unreachable!(),
        }
        self.avx512_stage_vector(args[0], FS1);
        self.avx512_stage_mask(args[1]);
        self.emit(
            avx512_mask::opcode(
                if d.vector.lane_type() == Type::I32 {
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
                movs(Type::Vector(d.vector)),
                [
                    asm_arg(simd_width(dst, width)),
                    asm_arg(simd_width(FS1, width)),
                ],
            );
        }
        self.def_done(result, dst);
    }
}
