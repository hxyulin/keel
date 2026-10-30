//! Immutable inputs are staged before native BW operations use transient K1.
use super::*;
use keel_ir::intrinsic::X86Avx512BwMiscKind as Kind;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id
        .x86_avx512_bw_misc()
        .expect("BW miscellaneous descriptor");
    if !features.avx512f || !features.avx512bw || (d.result.bytes() < 64 && !features.avx512vl) {
        return Err("BW miscellaneous operations require F+BW; narrow forms also require VL");
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&arg, &ty)| func.value_type(func.resolve(arg)) != ty)
    {
        return Err("BW miscellaneous signature mismatch");
    }
    if d.kind == Kind::Dbsad && !iconst(func, args[4]).is_some_and(|n| (0..=255).contains(&n)) {
        return Err("BW DBSAD requires an imm8 constant");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_misc(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id
            .x86_avx512_bw_misc()
            .expect("BW miscellaneous descriptor");
        let ty = Type::Vector(d.result);
        let width = ptr(ty);
        let dst = simd_width(FS0, width);
        let a = simd_width(FS1, width);
        let b = simd_width(FS2, width);
        match d.kind {
            Kind::Abs => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_bw_stage_mask(args[1]);
                self.emit(
                    avx512_mask::opcode(d.opcode, true),
                    [asm_arg(dst), asm_arg(a)],
                );
            }
            Kind::Broadcast => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_bw_stage_mask(args[1]);
                let scalar = self.use_reg(args[2], S0);
                self.emit(
                    avx512_mask::opcode(d.opcode, true),
                    [asm_arg(dst), asm_arg(r32(scalar))],
                );
            }
            Kind::Blend => {
                self.avx512_stage_vector(args[1], FS1);
                self.avx512_stage_vector(args[2], FS2);
                self.avx512_bw_stage_mask(args[0]);
                self.emit(
                    avx512_mask::opcode(d.opcode, true),
                    [asm_arg(dst), asm_arg(a), asm_arg(b)],
                );
            }
            Kind::Sad => {
                self.avx512_stage_vector(args[0], FS1);
                self.avx512_stage_vector(args[1], FS2);
                // EVEX narrow selection must retain BW+VL rather than selecting legacy SAD.
                self.emit(
                    avx512_mask::opcode(d.opcode, false),
                    [asm_arg(dst), asm_arg(a), asm_arg(b)],
                );
            }
            Kind::Dbsad => {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_stage_vector(args[3], FS2);
                self.avx512_bw_stage_mask(args[1]);
                self.emit(
                    avx512_mask::opcode(d.opcode, true),
                    [
                        asm_arg(dst),
                        asm_arg(a),
                        asm_arg(b),
                        asm_arg(iconst(self.func, args[4]).expect("validated immediate")),
                    ],
                );
            }
        }
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(ty),
                [asm_arg(simd_width(destination, width)), asm_arg(dst)],
            );
        }
        self.def_done(result, destination);
    }
}
