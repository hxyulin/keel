//! Explicit CPU query results and hardware ordering operations.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_system_register(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, write) = id.x86_system_register().expect("system register dispatch");
        let register = Register::parse(name).expect("known system register");
        if write {
            let input = self.use_reg(args[0], S0);
            self.emit(Opcode::Mov, [asm_arg(register), asm_arg(r64(input))]);
        } else {
            let output = self.def_reg(result);
            self.emit(Opcode::Mov, [asm_arg(r64(output)), asm_arg(register)]);
            self.def_done(result, output);
        }
    }

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
            Intrinsic::X86Rdtsc
            | Intrinsic::X86Xgetbv0
            | Intrinsic::X86Rdpmc
            | Intrinsic::X86Rdmsr => {
                if id == Intrinsic::X86Xgetbv0 {
                    self.emit(Opcode::Xor, [asm_arg(r32(S1)), asm_arg(r32(S1))]);
                }
                if matches!(id, Intrinsic::X86Rdpmc | Intrinsic::X86Rdmsr) {
                    let index = self.use_reg(args[0], S1);
                    self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(index))]);
                }
                self.emit(
                    Opcode::parse(if id == Intrinsic::X86Rdmsr {
                        "rdmsr"
                    } else if id == Intrinsic::X86Rdpmc {
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
            Intrinsic::X86Wrmsr => {
                // RAX, RCX and RDX are reserved scratch registers. Capture the
                // index and full SSA value before splitting EDX:EAX; neither
                // input's allocated home is modified by the hardware access.
                let index = self.use_reg(args[0], S1);
                self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(r32(index))]);
                let value = self.use_reg(args[1], S0);
                if value != S0 {
                    self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(value))]);
                }
                self.emit(Opcode::Mov, [asm_arg(r64(S2)), asm_arg(r64(S0))]);
                self.emit(Opcode::Shr, [asm_arg(r64(S2)), asm_arg(32)]);
                self.emit(Opcode::parse("wrmsr").unwrap(), []);
            }
            Intrinsic::X86Clts => self.emit(Opcode::parse("clts").unwrap(), []),
            Intrinsic::X86Invd => self.emit(Opcode::parse("invd").unwrap(), []),
            Intrinsic::X86Wbinvd => self.emit(Opcode::parse("wbinvd").unwrap(), []),
            Intrinsic::X86Cli => self.emit(Opcode::parse("cli").unwrap(), []),
            Intrinsic::X86Sti => self.emit(Opcode::parse("sti").unwrap(), []),
            Intrinsic::X86Hlt => self.emit(Opcode::parse("hlt").unwrap(), []),
            Intrinsic::X86Swapgs => self.emit(Opcode::parse("swapgs").unwrap(), []),
            Intrinsic::X86StiHlt => {
                // One IR operation: no allocation, spills or labels may appear
                // between STI and HLT. The caller supplies the interrupt context.
                self.emit(Opcode::parse("sti").unwrap(), []);
                self.emit(Opcode::parse("hlt").unwrap(), []);
            }
            Intrinsic::X86Lfence => self.emit(Opcode::Lfence, []),
            Intrinsic::X86Sfence => self.emit(Opcode::Sfence, []),
            Intrinsic::X86Mfence => self.emit(Opcode::Mfence, []),
            Intrinsic::X86Pause => self.emit(Opcode::Pause, []),
            _ => unreachable!("CPU control dispatch"),
        }
    }
}
