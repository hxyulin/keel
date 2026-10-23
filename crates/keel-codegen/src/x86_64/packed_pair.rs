//! Exact one/two-source packed integer arithmetic.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_packed_pair(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (opcode, tier) = id.x86_packed_pair().unwrap();
        let input = self.ty(args[0]);
        for (&arg, scratch) in args.iter().zip([FS0, FS1]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(
                    movs(input),
                    [asm_arg(simd(scratch, input)), asm_arg(simd(source, input))],
                );
            }
        }
        let opcode = Opcode::parse(opcode).unwrap();
        if args.len() == 1 {
            self.emit(
                opcode,
                [asm_arg(simd(FS0, input)), asm_arg(simd(FS0, input))],
            );
        } else if tier < 3 {
            self.emit(
                opcode,
                [asm_arg(simd(FS0, input)), asm_arg(simd(FS1, input))],
            );
        } else {
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, input)),
                    asm_arg(simd(FS0, input)),
                    asm_arg(simd(FS1, input)),
                ],
            );
        }
        let output = self.func.value_type(result.unwrap());
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                movs(output),
                [asm_arg(simd(d, output)), asm_arg(simd(FS0, output))],
            );
        }
        self.def_done(result, d);
    }
}
