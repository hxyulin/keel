//! Register-only intrinsic lowering. Stage both live operands before destroying FS0.
use super::*;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if id == Intrinsic::X86Pclmulqdq128 && !features.pclmulqdq {
        return Err("x86_pclmulqdq128 requires PCLMULQDQ (+pclmulqdq)");
    }
    if id != Intrinsic::X86Pclmulqdq128 && (!features.avx || !features.vpclmulqdq) {
        return Err("wide VPCLMULQDQ requires AVX and VPCLMULQDQ");
    }
    if id == Intrinsic::X86Vpclmulqdq512 && !features.avx512f {
        return Err("512-bit VPCLMULQDQ requires AVX512F");
    }
    let types = id.info().params;
    if args.len() != 3
        || args
            .iter()
            .zip(types)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("PCLMULQDQ requires two matching integer vector operands and i32 control");
    }
    if !iconst(func, args[2]).is_some_and(|n| (0..=255).contains(&n)) {
        return Err("PCLMULQDQ control must be iconst.i32 in0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_pclmul(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[0]);
        for (&arg, scratch) in args[..2].iter().zip([FS0, FS1]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(
                    movs(ty),
                    [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
                );
            }
        }
        let control = iconst(self.func, args[2]).expect("validated PCLMUL immediate");
        if id == Intrinsic::X86Pclmulqdq128 {
            self.emit(
                Opcode::Pclmulqdq,
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                    asm_arg(control),
                ],
            );
        } else {
            let opcode = if id == Intrinsic::X86Vpclmulqdq512 {
                Opcode::Evex(
                    keel_asm_ir::x86::evex::EvexOpcode::parse("vpclmulqdq")
                        .expect("registered EVEX VPCLMULQDQ"),
                )
            } else {
                Opcode::Extension(keel_asm_ir::x86::ExtensionOpcode::Vpclmulqdq)
            };
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(FS1, ty)),
                    asm_arg(control),
                ],
            );
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}
