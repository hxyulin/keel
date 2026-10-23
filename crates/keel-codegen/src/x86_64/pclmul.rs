//! Register-only intrinsic lowering. Stage both live operands before destroying FS0.
use super::*;
pub(super) fn validate(
    func: &Function,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.pclmulqdq {
        return Err("x86_pclmulqdq128 requires PCLMULQDQ (+pclmulqdq)");
    }
    let types = Intrinsic::X86Pclmulqdq128.info().params;
    if args.len() != 3
        || args
            .iter()
            .zip(types)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("PCLMULQDQ requires two i64x2 operands and i32 control");
    }
    if !iconst(func, args[2]).is_some_and(|n| (0..=255).contains(&n)) {
        return Err("PCLMULQDQ control must be iconst.i32 in0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_pclmul(&mut self, args: &[Value], result: Option<Value>) {
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
        self.emit(
            Opcode::Pclmulqdq,
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(simd(FS1, ty)),
                asm_arg(iconst(self.func, args[2]).unwrap()),
            ],
        );
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}
