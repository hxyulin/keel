//! Pinned raw compare operations; no changes to ordinary F80 predicates.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_compare(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        use Intrinsic::*;
        let (name, pops, flags) = match id {
            X86X87Fcom => ("fcom", 0, false),
            X86X87Fcomp => ("fcomp", 1, false),
            X86X87Fcompp => ("fcompp", 2, false),
            X86X87Fucom => ("fucom", 0, false),
            X86X87Fucomp => ("fucomp", 1, false),
            X86X87Fucompp => ("fucompp", 2, false),
            X86X87Fcomi => ("fcomi", 0, true),
            X86X87Fcomip => ("fcomip", 1, true),
            X86X87Fucomi => ("fucomi", 0, true),
            X86X87Fucomip => ("fucomip", 1, true),
            _ => unreachable!(),
        };
        // Read both homes before publishing outputs, including shared homes.
        self.f80_push(args[1]);
        self.f80_push(args[0]);
        let st = |number| Register {
            class: RegisterClass::X87,
            number,
            width: 80,
            high_byte: false,
        };
        let opcode = Opcode::parse(name).unwrap();
        if pops == 2 {
            self.emit(opcode, []);
        } else if flags {
            self.emit(opcode, [asm_arg(st(0)), asm_arg(st(1))]);
        } else {
            self.emit(opcode, [asm_arg(st(1))]);
        }
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        if flags {
            // FNSTSW preserves EFLAGS. Reserved RCX holds the snapshot across
            // cleanup and status publication. PUSHFQ/POP is legal in v1;
            // LAHF in long mode would accidentally require the v2 feature.
            self.emit(Opcode::parse("pushfq").unwrap(), []);
            self.emit(Opcode::Pop, [asm_arg(r64(S1))]);
        }
        for _ in pops..2 {
            self.f80_pop_result(None);
        }
        let results = self.func.inst_results(inst);
        let status = results[usize::from(flags)];
        let d = self.def_reg(Some(status));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(status), d);
        if flags {
            let result = results[0];
            let d = self.def_reg(Some(result));
            self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S1))]);
            self.emit(Opcode::And, [asm_arg(r32(d)), asm_arg(0x45u32)]);
            self.def_done(Some(result), d);
        }
    }
}
