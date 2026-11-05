//! Lowering of the target-independent intrinsics: libc memory calls,
//! `va_start`, and stack and frame access, plus the AArch64 SHA-256
//! intrinsics. x86 intrinsics are rejected before lowering.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn intrinsic(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        match id {
            // The libc functions take the same arguments in the same
            // order. memset's `int c` only uses its low byte, so the
            // unspecified upper bits of the i8 are harmless.
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp => {
                let params: Vec<AbiParam> = id
                    .info()
                    .params
                    .iter()
                    .map(|&ty| AbiParam::new(ty))
                    .collect();
                self.move_args(args, &params);
                self.out.branch(true, self.symbol(&id.to_string()));
                if id == Intrinsic::Memcmp {
                    self.call_result(result, None);
                }
            }
            // What clang emits for __builtin_debugtrap; a debugger can
            // step past it.
            Intrinsic::Debugtrap => self.mach("brk", [Arg::imm(0xf000)]),
            // Apple's va_list is a pointer to the next argument slot.
            Intrinsic::VaStart if self.linux => self.linux_va_start(args[0]),
            Intrinsic::VaStart => {
                let list = self.use_reg(args[0], S1);
                self.address(Loc::Reg(S0), "x29", self.variadic_offset);
                self.mach(
                    "str",
                    [(Reg::x(S0)).into(), Address::base(Reg::x(list)).into()],
                );
            }
            // Round up to keep `sp` 16-byte aligned; the memory starts
            // above the outgoing arguments, which stay at `sp`.
            Intrinsic::StackAlloc => {
                let size = self.use_reg(args[0], S0);
                self.mach(
                    "add",
                    [(Reg::x(S0)).into(), (Reg::x(size)).into(), Arg::imm(15)],
                );
                self.mach(
                    "and",
                    [(Reg::x(S0)).into(), (Reg::x(S0)).into(), Arg::imm(-16)],
                );
                self.mach(
                    "sub",
                    [(Reg::SP).into(), (Reg::SP).into(), (Reg::x(S0)).into()],
                );
                let d = self.def_reg(result);
                self.address(Loc::Reg(d), "sp", self.copy_base);
                self.def_done(result, d);
            }
            Intrinsic::StackSave => {
                let d = self.def_reg(result);
                self.mach("mov", [(Reg::x(d)).into(), (Reg::SP).into()]);
                self.def_done(result, d);
            }
            // Lifetime markers constrain IR transformations but change no runtime state.
            Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd => {}
            Intrinsic::StackRestore => {
                let mark = self.use_reg(args[0], S0);
                self.mach("mov", [(Reg::SP).into(), (Reg::x(mark)).into()]);
            }
            keel_ir::x86_intrinsics!() => unreachable!("rejected x86 intrinsic"),
            Intrinsic::Aarch64Sha256h
            | Intrinsic::Aarch64Sha256h2
            | Intrinsic::Aarch64Sha256su0
            | Intrinsic::Aarch64Sha256su1 => self.sha256(id, args, result),

            Intrinsic::FrameAddress => {
                let d = self.def_reg(result);
                self.mach("mov", [(Reg::x(d)).into(), (Reg::x(29)).into()]);
                self.def_done(result, d);
            }
        }
    }

    /// The destination register is also the first input. Every input is
    /// captured in scratch before the instruction, so a result that shares a
    /// register with an input cannot clobber it.
    fn sha256(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        for (&arg, scratch) in args.iter().zip([FS0, FS1, FS2]) {
            let source = self.use_freg(arg, scratch);
            self.move_vector(scratch, source);
        }
        let (opcode, name) = match id {
            Intrinsic::Aarch64Sha256h => (Opcode::SHA256H_QQV_cryptosha3, "sha256h"),
            Intrinsic::Aarch64Sha256h2 => (Opcode::SHA256H2_QQV_cryptosha3, "sha256h2"),
            Intrinsic::Aarch64Sha256su0 => (Opcode::SHA256SU0_VV_cryptosha2, "sha256su0"),
            Intrinsic::Aarch64Sha256su1 => (Opcode::SHA256SU1_VVV_cryptosha3, "sha256su1"),
            _ => unreachable!("an AArch64 SHA-256 intrinsic"),
        };
        let mut fields = vec![(FieldId::Rd, FS0.into()), (FieldId::Rn, FS1.into())];
        if args.len() == 3 {
            fields.push((FieldId::Rm, FS2.into()));
        }
        self.out.native(
            || keel_asm_ir::Instruction::new(opcode, fields),
            |out| match id {
                Intrinsic::Aarch64Sha256su0 => write!(out, "{name}\tv{FS0}.4s, v{FS1}.4s"),
                Intrinsic::Aarch64Sha256su1 => {
                    write!(out, "{name}\tv{FS0}.4s, v{FS1}.4s, v{FS2}.4s")
                }
                _ => write!(out, "{name}\tq{FS0}, q{FS1}, v{FS2}.4s"),
            },
        );
        let d = self.def_freg(result);
        self.move_vector(d, FS0);
        self.def_done(result, d);
    }
}
