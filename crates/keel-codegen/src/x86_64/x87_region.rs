//! One bounded physical x87 region, distinct from masked SSA value recipes.
//! The region owns temporary tags until its explicit normal-completion reset.
use super::*;
use keel_ir::X87RegionOperand as Kind;
impl FunctionCompiler<'_> {
    fn region_phase(&mut self, phase: i32) {
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(memory(r64(S2), 0), 32)), asm_arg(phase)],
        );
    }
    fn region_snapshot(&mut self, phase: i32, sw: i128, image: i128) {
        self.region_phase(phase);
        self.emit(
            Opcode::parse("fnstsw").unwrap(),
            [asm_arg(sized(memory(r64(S2), sw), 16))],
        );
        self.emit(
            Opcode::parse("fxsave64").unwrap(),
            [asm_arg(memory(r64(S2), image))],
        );
    }
    fn region_address(&mut self, offset: i128) {
        self.emit(
            Opcode::Mov,
            [
                asm_arg(r64(S1)),
                asm_arg(sized(memory(r64(S3), offset), 64)),
            ],
        );
    }
    fn region_wait(&mut self, phase: i32) {
        self.region_phase(phase);
        self.emit(Opcode::parse("fwait").unwrap(), []);
    }
    pub(super) fn native_x87_region(&mut self, inst: Inst, args: &[Value]) {
        let control = iconst(self.func, args[1]).unwrap();
        let (op, input, output, discard) = keel_ir::x87_region_control(control).unwrap();
        let r = self.use_reg(args[0], S0);
        self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(r64(r))]);
        self.emit(
            Opcode::Mov,
            [asm_arg(r64(S2)), asm_arg(sized(memory(r64(S3), 32), 64))],
        );
        self.region_phase(1);
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(memory(r64(S2), 4), 32)), asm_arg(0)],
        );
        self.emit(
            Opcode::parse("fnstcw").unwrap(),
            [asm_arg(sized(memory(r64(S2), 8), 16))],
        );
        self.emit(
            Opcode::parse("fnstsw").unwrap(),
            [asm_arg(sized(memory(r64(S2), 10), 16))],
        );
        for index in (0..op.loads).rev() {
            self.region_phase(if index == 1 { 10 } else { 11 });
            self.region_address(i128::from(index) * 8);
            self.emit(Opcode::Fld, [asm_arg(sized(memory(r64(S1), 0), input))]);
            // Capture a pending input exception before the next waiting load.
            self.region_snapshot(12, 12, 64);
            self.region_wait(13);
        }
        if op.loads == 0 {
            self.region_snapshot(12, 12, 64);
            self.region_wait(13);
        }
        self.region_phase(20);
        let st = |number| Register {
            class: RegisterClass::X87,
            number,
            width: 80,
            high_byte: false,
        };
        let opcode = Opcode::parse(op.opcode).unwrap();
        match op.operand {
            Kind::None => self.emit(opcode, []),
            Kind::St01 | Kind::Flags => self.emit(opcode, [asm_arg(st(0)), asm_arg(st(1))]),
            Kind::St10 | Kind::St10Pop => self.emit(opcode, [asm_arg(st(1)), asm_arg(st(0))]),
            Kind::St1 => self.emit(opcode, [asm_arg(st(1))]),
            Kind::Input | Kind::Output | Kind::Second => {
                self.region_address(match op.operand {
                    Kind::Input => 0,
                    Kind::Second => 8,
                    _ => 16,
                });
                self.emit(opcode, [asm_arg(sized(memory(r64(S1), 0), op.bits))]);
            }
        }
        if op.operand == Kind::Flags {
            self.emit(Opcode::parse("pushfq").unwrap(), []);
            self.emit(Opcode::Pop, [asm_arg(r64(S0))]);
            self.emit(Opcode::And, [asm_arg(r32(S0)), asm_arg(0x45)]);
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(memory(r64(S2), 24), 32)), asm_arg(r32(S0))],
            );
        }
        self.region_snapshot(21, 14, 576);
        self.region_wait(22);
        if op.operand == Kind::St10 {
            self.emit(Opcode::parse("fxch").unwrap(), [asm_arg(st(1))]);
        }
        for index in 0..op.results {
            let skip = format!(".L{}_{}_region_no_second", self.func.name, inst);
            if index == 1 {
                // Range failures of FSINCOS/FPTAN do not push. TOP, not undefined
                // condition bits, decides whether the second physical value exists.
                self.emit(
                    Opcode::Movzx,
                    [asm_arg(r32(S0)), asm_arg(sized(memory(r64(S2), 10), 16))],
                );
                self.emit(Opcode::Sub, [asm_arg(r32(S0)), asm_arg(0x1000)]);
                self.emit(Opcode::And, [asm_arg(r32(S0)), asm_arg(0x3800)]);
                self.emit(
                    Opcode::Movzx,
                    [asm_arg(r32(S1)), asm_arg(sized(memory(r64(S2), 14), 16))],
                );
                self.emit(Opcode::And, [asm_arg(r32(S1)), asm_arg(0x3800)]);
                self.emit(Opcode::Cmp, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
                self.emit(Opcode::Jcc(5), [asm_arg(&skip)]);
            }
            self.region_phase(if index == 0 { 30 } else { 40 });
            if discard {
                self.emit(Opcode::Fstp, [asm_arg(st(0))]);
            } else {
                self.region_address(16 + i128::from(index) * 8);
                self.emit(Opcode::Fstp, [asm_arg(sized(memory(r64(S1), 0), output))]);
            }
            self.region_snapshot(
                if index == 0 { 31 } else { 41 },
                16 + i128::from(index) * 2,
                1088 + i128::from(index) * 512,
            );
            self.region_wait(if index == 0 { 32 } else { 42 });
            if index == 1 {
                self.out.label(skip);
            }
        }
        // Normal completion explicitly resets exceptions/tags, then restores
        // entry CW and empty entry TOP. Terminal faults never reach this stage.
        self.region_phase(50);
        self.emit(Opcode::parse("fninit").unwrap(), []);
        self.emit(Opcode::Fldcw, [asm_arg(sized(memory(r64(S2), 8), 16))]);
        self.emit(
            Opcode::Movzx,
            [asm_arg(r32(S0)), asm_arg(sized(memory(r64(S2), 10), 16))],
        );
        self.emit(Opcode::Shr, [asm_arg(r32(S0)), asm_arg(11)]);
        self.emit(Opcode::And, [asm_arg(r32(S0)), asm_arg(7)]);
        let loop_label = format!(".L{}_{}_region_top", self.func.name, inst);
        let done = format!(".L{}_{}_region_reset", self.func.name, inst);
        self.emit(Opcode::Cmp, [asm_arg(r32(S0)), asm_arg(0)]);
        self.emit(Opcode::Jcc(4), [asm_arg(&done)]);
        self.region_phase(51);
        self.out.label(loop_label.clone());
        self.emit(Opcode::parse("fincstp").unwrap(), []);
        self.emit(Opcode::Sub, [asm_arg(r32(S0)), asm_arg(1)]);
        self.emit(Opcode::Jcc(5), [asm_arg(&loop_label)]);
        self.out.label(done);
        self.region_snapshot(52, 20, 2112);
        self.region_phase(60);
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(memory(r64(S2), 4), 32)), asm_arg(1)],
        );
        let result = Some(self.func.inst_results(inst)[0]);
        let d = self.def_reg(result);
        self.emit(
            Opcode::Movzx,
            [asm_arg(r32(d)), asm_arg(sized(memory(r64(S2), 14), 16))],
        );
        self.def_done(result, d);
    }
}
