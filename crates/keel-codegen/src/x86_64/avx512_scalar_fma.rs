//! Scalar FMA retains hardware upper lanes while merging the low lane independently.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("scalar AVX512 FMA requires AVX512F");
    }
    if args.len() != 6
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("scalar AVX512 FMA signature mismatch");
    }
    if !iconst(func, args[5]).is_some_and(|control| matches!(control, 4 | 8..=11)) {
        return Err("AVX512 FMA control must be iconst.i32 4 or8..=11");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    fn scalar_fma_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                Opcode::Movups,
                [asm_arg(xmm(scratch)), asm_arg(xmm(source))],
            );
        }
    }
    pub(super) fn native_avx512_scalar_fma(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        for (value, scratch) in [(args[2], FS0), (args[3], FS1), (args[4], FS2)] {
            self.scalar_fma_stage(value, scratch);
        }
        self.avx512_stage_mask(args[1]);
        let name = id
            .x86_avx512_scalar_fma_opcode()
            .expect("scalar FMA intrinsic");
        let mut op = EvexOpcode::parse(name).expect("scalar FMA opcode");
        op.options.mask = Some(1);
        let control = iconst(self.func, args[5]).expect("validated FMA rounding");
        if control != 4 {
            op.options.rounding = Some(match control & 3 {
                0 => Rounding::Nearest,
                1 => Rounding::Down,
                2 => Rounding::Up,
                _ => Rounding::Zero,
            });
        }
        self.emit(
            Opcode::Evex(op),
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
        );
        // The arithmetic sources have been consumed. Reuse FS1 for the independent
        // merge and retain the FMA's upper lanes through VMOVSS/SD source one.
        self.scalar_fma_stage(args[0], FS1);
        let move_name = if self.ty(args[0]) == Type::Vector(keel_ir::VectorType::F32x4) {
            "vmovss"
        } else {
            "vmovsd"
        };
        self.emit(
            avx512_mask::opcode(move_name, true),
            [asm_arg(xmm(FS1)), asm_arg(xmm(FS0)), asm_arg(xmm(FS0))],
        );
        let d = self.def_freg(result);
        if d != FS1 {
            self.emit(Opcode::Movups, [asm_arg(xmm(d)), asm_arg(xmm(FS1))]);
        }
        self.def_done(result, d);
    }
}
