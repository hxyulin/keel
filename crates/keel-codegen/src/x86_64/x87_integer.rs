//! Pinned raw signed conversion. Portable Convert keeps its saturation contract.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_integer(&mut self, inst: Inst, id: Intrinsic, arg: Value) {
        use Intrinsic::*;
        let (name, width, load, pop) = match id {
            X86X87Fist16 => ("fist", 16, false, false),
            X86X87Fist32 => ("fist", 32, false, false),
            X86X87Fistp16 => ("fistp", 16, false, true),
            X86X87Fistp32 => ("fistp", 32, false, true),
            X86X87Fistp64 => ("fistp", 64, false, true),
            X86X87Fisttp16 => ("fisttp", 16, false, true),
            X86X87Fisttp32 => ("fisttp", 32, false, true),
            X86X87Fisttp64 => ("fisttp", 64, false, true),
            X86X87Fild16 => ("fild", 16, true, false),
            X86X87Fild32 => ("fild", 32, true, false),
            X86X87Fild64 => ("fild", 64, true, false),
            _ => unreachable!(),
        };
        let temporary = sized(
            self.slot(self.copy_temp.expect("raw integer staging")),
            width,
        );
        // Capture the complete input before publishing either result. The staging
        // slot is reserved independently of every allocator-provided home.
        if load {
            let r = self.use_reg(arg, S0);
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(temporary.clone()),
                    asm_arg(Register { width, ..r64(r) }),
                ],
            );
        } else {
            self.f80_push(arg);
        }
        self.emit(Opcode::parse(name).unwrap(), [asm_arg(temporary.clone())]);
        // Snapshot before FSTP and before any result's home can overwrite an input.
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        self.emit(Opcode::Movzx, [asm_arg(r32(S2)), asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        if load {
            self.f80_pop_result(Some(results[0]));
        } else {
            if !pop {
                self.f80_pop_result(None);
            }
            let d = self.def_reg(Some(results[0]));
            self.emit(
                if width == 16 {
                    Opcode::Movzx
                } else {
                    Opcode::Mov
                },
                [
                    asm_arg(if width == 64 { r64(d) } else { r32(d) }),
                    asm_arg(temporary),
                ],
            );
            self.def_done(Some(results[0]), d);
        }
        let d = self.def_reg(Some(results[1]));
        self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S2))]);
        self.def_done(Some(results[1]), d);
    }
}
