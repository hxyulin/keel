//! Explicit CPU query results and hardware ordering operations.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_cpu_control(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        match id {
            Intrinsic::X86Cpuid => {
                let save = sized(
                    self.frame(self.cx16_save.expect("planned fixed-RBX slot")),
                    64,
                );
                self.emit(Opcode::Mov, [asm_arg(save.clone()), asm_arg(r64(3))]);
                let leaf = self.use_reg(args[0], S0);
                self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(r32(leaf))]);
                let subleaf = self.use_reg(args[1], S1);
                self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(subleaf))]);
                self.emit(Opcode::parse("cpuid").unwrap(), []);
                // CPUID zero-extends every 32-bit result in long mode. Pack
                // EAX|EBX<<32 and ECX|EDX<<32 before restoring allocated RBX.
                self.emit(Opcode::Shl, [asm_arg(r64(S2)), asm_arg(32)]);
                self.emit(Opcode::Or, [asm_arg(r64(S2)), asm_arg(r64(S1))]);
                self.emit(Opcode::Shl, [asm_arg(r64(3)), asm_arg(32)]);
                self.emit(Opcode::Or, [asm_arg(r64(S0)), asm_arg(r64(3))]);
                self.emit(Opcode::Mov, [asm_arg(r64(3)), asm_arg(save)]);
                self.pair_out(result);
            }
            Intrinsic::X86Rdtsc | Intrinsic::X86Xgetbv0 | Intrinsic::X86Rdpmc => {
                if id == Intrinsic::X86Xgetbv0 {
                    self.emit(Opcode::Xor, [asm_arg(r32(S1)), asm_arg(r32(S1))]);
                }
                if id == Intrinsic::X86Rdpmc {
                    let index = self.use_reg(args[0], S1);
                    self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(index))]);
                }
                self.emit(
                    Opcode::parse(if id == Intrinsic::X86Rdpmc {
                        "rdpmc"
                    } else if id == Intrinsic::X86Xgetbv0 {
                        "xgetbv"
                    } else {
                        "rdtsc"
                    })
                    .unwrap(),
                    [],
                );
                self.emit(Opcode::Shl, [asm_arg(r64(S2)), asm_arg(32)]);
                self.emit(Opcode::Or, [asm_arg(r64(S0)), asm_arg(r64(S2))]);
                let d = self.def_reg(result);
                if d != S0 {
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
                }
                self.def_done(result, d);
            }
            Intrinsic::X86Lfence => self.emit(Opcode::Lfence, []),
            Intrinsic::X86Sfence => self.emit(Opcode::Sfence, []),
            Intrinsic::X86Mfence => self.emit(Opcode::Mfence, []),
            Intrinsic::X86Pause => self.emit(Opcode::Pause, []),
            _ => unreachable!("CPU control dispatch"),
        }
    }
}
