//! Explicit raw environment operations. No x87 data-stack temporaries.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_fxstate(&mut self, id: Intrinsic, args: &[Value]) {
        // Only reserved GPR materialization; no SIMD preparation or shadow image.
        let address = self.use_reg(args[0], S0);
        self.emit(
            Opcode::parse(if id == Intrinsic::X86Fxsave64 {
                "fxsave64"
            } else {
                "fxrstor64"
            })
            .unwrap(),
            [asm_arg(memory(r64(address), 0))],
        );
    }

    pub(super) fn native_x87_state(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        // WAIT is separate in the emitted stream, never silently folded into FN forms.
        if matches!(
            id,
            X86X87ReadCwWait | X86X87ReadSwWait | X86X87ClexWait | X86X87InitWait | X86X87Wait
        ) {
            self.emit(Opcode::parse("fwait").unwrap(), []);
        }
        if matches!(
            id,
            X86X87ReadCwNowait
                | X86X87ReadSwNowait
                | X86X87ReadCwWait
                | X86X87ReadSwWait
                | X86X87ExchangeCw
        ) {
            let slot = self.x87_state.expect("x87 environment frame slots");
            // Preserve input before publishing the old CW even under aliasing/spills.
            if id == X86X87ExchangeCw {
                let r = self.use_reg(args[0], S0);
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(self.frame(slot + 2), 16)), asm_arg(r16(r))],
                );
            }
            let status = matches!(id, X86X87ReadSwNowait | X86X87ReadSwWait);
            self.emit(
                Opcode::parse(if status { "fnstsw" } else { "fnstcw" }).unwrap(),
                [asm_arg(sized(self.frame(slot), 16))],
            );
            if id == X86X87ExchangeCw {
                self.emit(Opcode::Fldcw, [asm_arg(sized(self.frame(slot + 2), 16))]);
            }
            let d = self.def_reg(result);
            self.emit(
                Opcode::Mov,
                [asm_arg(r16(d)), asm_arg(sized(self.frame(slot), 16))],
            );
            self.def_done(result, d);
        } else if let Some(op) = match id {
            X86X87ClexNowait | X86X87ClexWait => Some("fnclex"),
            X86X87InitNowait | X86X87InitWait => Some("fninit"),
            X86MmxEmms => Some("emms"),
            X86X87Wait => None,
            _ => unreachable!(),
        } {
            self.emit(Opcode::parse(op).unwrap(), []);
        }
    }
}
