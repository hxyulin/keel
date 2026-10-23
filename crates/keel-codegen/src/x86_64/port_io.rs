//! Exact, pinned scalar port I/O. Caller supplies permission and device contract.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_port_io(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let (output, width, immediate) = id.x86_port_io().expect("port I/O dispatch");
        // RAX (S0) and RDX (S2) are allocator-reserved. Stage all SSA inputs
        // before publishing any result; neither scratch can be an SSA home.
        let port = if immediate {
            asm_arg(iconst(self.func, args[0]).expect("verified immediate port"))
        } else {
            let r = self.use_reg(args[0], S2);
            if r != S2 {
                self.emit(Opcode::Mov, [asm_arg(r64(S2)), asm_arg(r64(r))]);
            }
            asm_arg(r16(S2))
        };
        let acc = asm_arg(Register { width, ..r64(S0) });
        if output {
            let r = self.use_reg(args[1], S0);
            if r != S0 {
                self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(r))]);
            }
            self.emit(Opcode::parse("out").expect("baseline OUT"), [port, acc]);
        } else {
            self.emit(Opcode::parse("in").expect("baseline IN"), [acc, port]);
            if width < 32 {
                self.emit(
                    Opcode::Movzx,
                    [asm_arg(r32(S0)), asm_arg(Register { width, ..r64(S0) })],
                );
            }
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
            }
            self.def_done(result, d);
        }
    }
}
