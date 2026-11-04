//! Pinned raw signed conversion. Portable Convert keeps its saturation contract.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_integer(&mut self, inst: Inst, id: Intrinsic, arg: Value) {
        use Intrinsic::*;
        let (name, width, load, pop) = match id {
            X86X87Fist16 => (op::FIST, 16, false, false),
            X86X87Fist32 => (op::FIST, 32, false, false),
            X86X87Fistp16 => (Opcode::Fistp, 16, false, true),
            X86X87Fistp32 => (Opcode::Fistp, 32, false, true),
            X86X87Fistp64 => (Opcode::Fistp, 64, false, true),
            X86X87Fisttp16 => (op::FISTTP, 16, false, true),
            X86X87Fisttp32 => (op::FISTTP, 32, false, true),
            X86X87Fisttp64 => (op::FISTTP, 64, false, true),
            X86X87Fild16 => (Opcode::Fild, 16, true, false),
            X86X87Fild32 => (Opcode::Fild, 32, true, false),
            X86X87Fild64 => (Opcode::Fild, 64, true, false),
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
        self.emit(name, [asm_arg(temporary.clone())]);
        // Snapshot before FSTP and before any result's home can overwrite an input.
        self.emit(op::FNSTSW, [asm_arg(r16(S0))]);
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
