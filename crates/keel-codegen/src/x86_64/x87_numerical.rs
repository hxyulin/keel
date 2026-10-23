//! Single-step x87 numerical instructions. See docs/f80/numerical.md.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_x87_numerical(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        use Intrinsic::*;
        if id.is_x86_x87_constant() {
            self.native_x87_constant(inst, id);
            return;
        }
        if id.is_x86_x87_bcd() {
            self.native_x87_bcd(inst, id, args);
            return;
        }
        if id.is_x86_x87_fi_arithmetic() {
            self.native_x87_fi_arithmetic(inst, id, args);
            return;
        }
        if id.is_x86_x87_integer() {
            self.native_x87_integer(inst, id, args[0]);
            return;
        }
        if id.is_x86_x87_compare() {
            self.native_x87_compare(inst, id, args);
            return;
        }
        if id.is_x86_x87_binary() {
            self.native_x87_binary(inst, id, args);
            return;
        }
        if id.is_x86_x87_basic() {
            self.native_x87_basic(inst, id, args[0]);
            return;
        }
        if matches!(id, X86X87Fsincos | X86X87Fptan | X86X87Fxtract) {
            self.native_x87_multi(inst, id, args[0]);
            return;
        }
        let (name, retains_second) = match id {
            X86X87Fsin => ("fsin", false),
            X86X87Fcos => ("fcos", false),
            X86X87F2xm1 => ("f2xm1", false),
            X86X87Fpatan => ("fpatan", false),
            X86X87Fyl2x => ("fyl2x", false),
            X86X87Fyl2xp1 => ("fyl2xp1", false),
            X86X87Fscale => ("fscale", true),
            X86X87Fprem => ("fprem", true),
            X86X87Fprem1 => ("fprem1", true),
            _ => unreachable!(),
        };
        // Both reads precede all writes, including scalar outputs sharing a home.
        // The API operands specify ST(0), then ST(1).
        if args.len() == 2 {
            self.f80_push(args[1]);
        }
        self.f80_push(args[0]);
        self.emit(Opcode::parse(name).unwrap(), []);
        // AX is reserved scratch, never an allocator-owned live register. Snapshot
        // before FSTP: its rounding/exception effects are not part of this status.
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        let (value, status) = (results[0], results[1]);
        self.f80_pop_result(Some(value));
        if retains_second {
            self.f80_pop_result(None);
        }
        let d = self.def_reg(Some(status));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(status), d);
    }
}

impl FunctionCompiler<'_> {
    fn native_x87_multi(&mut self, inst: Inst, id: Intrinsic, arg: Value) {
        // Record entry TOP while tags are empty. Exceptional C2 bits need not
        // describe a push; the architectural TOP change always does.
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        self.emit(Opcode::Mov, [asm_arg(r16(S1)), asm_arg(r16(S0))]);
        self.emit(Opcode::Sub, [asm_arg(r16(S1)), asm_arg(0x1000)]);
        self.emit(Opcode::And, [asm_arg(r16(S1)), asm_arg(0x3800)]);
        self.f80_push(arg);
        let name = match id {
            Intrinsic::X86X87Fsincos => "fsincos",
            Intrinsic::X86X87Fptan => "fptan",
            Intrinsic::X86X87Fxtract => "fxtract",
            _ => unreachable!(),
        };
        self.emit(Opcode::parse(name).unwrap(), []);
        self.emit(Opcode::parse("fnstsw").unwrap(), [asm_arg(r16(S0))]);
        let results = self.func.inst_results(inst);
        let (first, second, status) = (results[0], results[1], results[2]);
        let two = format!(".L{}_{}_x87_two", self.func.name, inst);
        // If no push occurred, duplicate the sole operand as an explicitly
        // unspecified second payload. This touches no input/output homes and
        // never pops an empty register. AX remains the immediate snapshot.
        self.emit(Opcode::Mov, [asm_arg(r16(S2)), asm_arg(r16(S0))]);
        self.emit(Opcode::And, [asm_arg(r16(S2)), asm_arg(0x3800)]);
        self.emit(Opcode::Cmp, [asm_arg(r16(S2)), asm_arg(r16(S1))]);
        self.emit(Opcode::Jcc(4), [asm_arg(&two)]);
        self.emit(
            Opcode::Fld,
            [asm_arg(Register {
                class: RegisterClass::X87,
                number: 0,
                width: 80,
                high_byte: false,
            })],
        );
        self.out.label(two);
        // Both original inputs/results are resident before any output write.
        self.f80_pop_result(Some(first));
        self.f80_pop_result(Some(second));
        let d = self.def_reg(Some(status));
        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(r16(S0))]);
        self.def_done(Some(status), d);
    }
}
