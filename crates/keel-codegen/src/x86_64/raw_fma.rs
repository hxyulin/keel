//! Exact FMA3 operations. The destructive destination is always reserved scratch.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_raw_fma(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[0]);
        // Capture ALL inputs before defining/publishing the result. Allocated
        // registers exclude FS0/1/2, so aliases and spilled/live inputs are safe.
        // Scalar upper bits are outside the declared f32/f64 value contract.
        for (&arg, scratch) in args.iter().zip([FS0, FS1, FS2]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        self.emit(
            Opcode::parse(id.x86_raw_fma_opcode().unwrap()).unwrap(),
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(simd(FS1, ty)),
                asm_arg(simd(FS2, ty)),
            ],
        );
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}
