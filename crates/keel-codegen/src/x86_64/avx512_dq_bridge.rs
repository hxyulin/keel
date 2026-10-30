//! DQ mask bridges transfer sign bits or expand mask bits to integer lanes.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let (_, shape) = id
        .x86_avx512_dq_bridge()
        .expect("DQ mask bridge descriptor");
    if !features.avx512f || !features.avx512dq || (shape.bytes() < 64 && !features.avx512vl) {
        return Err("DQ mask bridge requires AVX512F+DQ and narrow forms require AVX512VL");
    }
    if args.len() != 1 || func.value_type(func.resolve(args[0])) != id.info().params[0] {
        return Err("DQ mask bridge signature mismatch");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_dq_bridge(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, shape) = id
            .x86_avx512_dq_bridge()
            .expect("DQ mask bridge descriptor");
        let ty = Type::Vector(shape);
        if id.info().params[0].is_vector() {
            let source = self.use_freg(args[0], FS0);
            self.emit(
                avx512_mask::opcode(name, false),
                [asm_arg(avx512_mask::k1()), asm_arg(simd(source, ty))],
            );
            let dest = self.def_reg(result);
            // VPMOVD2M/Q2M clear unused mask bits. KMOVW publishes all at once.
            self.emit(
                Opcode::parse("kmovw").expect("mask transfer"),
                [asm_arg(r32(dest)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, dest);
        } else {
            self.avx512_stage_mask(args[0]);
            let dest = self.def_freg(result);
            self.emit(
                avx512_mask::opcode(name, false),
                [asm_arg(simd(dest, ty)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, dest);
        }
    }
}
