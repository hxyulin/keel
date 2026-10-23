//! Sliding byte SAD windows and unsigned minimum position, with no implicit XMM0.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_packed_search(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (opcode, tier, sad) = id.x86_packed_search().unwrap();
        let input = self.ty(args[0]);
        // Capture every source before defining a possibly coalesced destination.
        // FS0/FS1 are reserved by the target, including for spill materialization.
        for (&arg, scratch) in args.iter().take(if sad { 2 } else { 1 }).zip([FS0, FS1]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(
                    movs(input),
                    [asm_arg(simd(scratch, input)), asm_arg(simd(source, input))],
                );
            }
        }
        let opcode = Opcode::parse(opcode).unwrap();
        if sad {
            let control = iconst(self.func, args[2]).unwrap();
            if tier == 2 {
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, input)),
                        asm_arg(simd(FS1, input)),
                        asm_arg(control),
                    ],
                );
            } else {
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, input)),
                        asm_arg(simd(FS0, input)),
                        asm_arg(simd(FS1, input)),
                        asm_arg(control),
                    ],
                );
            }
        } else {
            self.emit(
                opcode,
                [asm_arg(simd(FS0, input)), asm_arg(simd(FS0, input))],
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
