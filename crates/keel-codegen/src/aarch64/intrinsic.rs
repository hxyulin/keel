//! Lowering of the target-independent intrinsics: libc memory calls,
//! `va_start`, and stack and frame access. x86 intrinsics are rejected
//! before lowering.
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

            Intrinsic::FrameAddress => {
                let d = self.def_reg(result);
                self.mach("mov", [(Reg::x(d)).into(), (Reg::x(29)).into()]);
                self.def_done(result, d);
            }
        }
    }
}
