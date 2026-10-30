//! Raw DQ specials preserve MXCSR ordering and independent scalar merge/source roles.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
use keel_ir::intrinsic::X86Avx512DqSpecialKind as Kind;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_dq_special().expect("DQ special descriptor");
    if !features.avx512f
        || !features.avx512dq
        || (!d.scalar && d.vector.bytes() < 64 && !features.avx512vl)
    {
        return Err("DQ floating special requires F+DQ and narrow packed forms require VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("DQ special signature mismatch");
    }
    for index in [Some(d.immediate), d.sae].into_iter().flatten() {
        if !iconst(func, args[index]).is_some_and(|n| d.valid_control(index, n)) {
            return Err("invalid DQ special immediate or SAE control");
        }
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_dq_special(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_dq_special().expect("DQ special descriptor");
        let ty = Type::Vector(d.vector);
        let immediate = iconst(self.func, args[d.immediate]).expect("validated DQ immediate");
        let mut opcode = EvexOpcode::parse(d.opcode).expect("imported DQ special");
        opcode.options.mask = Some(1);
        if d.sae
            .is_some_and(|index| iconst(self.func, args[index]) == Some(8))
        {
            opcode.options.rounding = Some(Rounding::Sae);
        }
        if d.kind == Kind::Fpclass {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_mask(args[1]);
            self.emit(
                Opcode::Evex(opcode),
                [
                    asm_arg(avx512_mask::k1()),
                    asm_arg(simd(FS0, ty)),
                    asm_arg(immediate),
                ],
            );
            let dest = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").expect("mask transfer"),
                [asm_arg(r32(dest)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, dest);
            return;
        }
        self.avx512_stage_vector(args[0], FS0);
        // Keep original a.low intact for RANGE; scalar REDUCE uses a only
        // for upper-lane passthrough and computes its low result from b.
        self.avx512_stage_vector(args[2], FS1);
        if d.scalar || d.kind == Kind::Range {
            self.avx512_stage_vector(args[3], FS2);
        }
        self.avx512_stage_mask(args[1]);
        if d.scalar || d.kind == Kind::Range {
            self.emit(
                Opcode::Evex(opcode),
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                    asm_arg(simd(FS2, ty)),
                    asm_arg(immediate),
                ],
            );
        } else {
            self.emit(
                Opcode::Evex(opcode),
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                    asm_arg(immediate),
                ],
            );
        }
        let dest = self.def_freg(result);
        if dest != FS0 {
            self.emit(movs(ty), [asm_arg(simd(dest, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, dest);
    }
}
