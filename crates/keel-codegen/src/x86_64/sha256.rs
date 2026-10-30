//! SHA-256 operations preserve every live input, including implicit XMM0.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_sha256(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[0]);
        for (&arg, scratch) in args[..2].iter().zip([FS0, FS1]) {
            let source = self.use_freg(arg, scratch);
            if source != scratch {
                self.emit(movs(ty), [asm_arg(xmm(scratch)), asm_arg(xmm(source))]);
            }
        }
        let rounds = id == Intrinsic::X86Sha256rnds2;
        if rounds {
            // XMM0 can hold either explicit source or an unrelated live value.
            // Stage explicit sources first, then preserve its low 128 bits. Legacy
            // moves leave its upper YMM/ZMM lanes intact.
            self.emit(Opcode::Movdqu, [asm_arg(xmm(FS2)), asm_arg(xmm(0))]);
            let source = self.use_freg(args[2], 0);
            if source != 0 {
                self.emit(Opcode::Movdqu, [asm_arg(xmm(0)), asm_arg(xmm(source))]);
            }
        }
        let op = Opcode::parse(match id {
            Intrinsic::X86Sha256msg1 => "sha256msg1",
            Intrinsic::X86Sha256msg2 => "sha256msg2",
            Intrinsic::X86Sha256rnds2 => "sha256rnds2",
            _ => unreachable!("SHA lowering requires a SHA intrinsic"),
        })
        .expect("SHA opcode is registered");
        self.emit(op, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        if rounds {
            self.emit(Opcode::Movdqu, [asm_arg(xmm(0)), asm_arg(xmm(FS2))]);
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
        }
        self.def_done(result, d);
    }
}
