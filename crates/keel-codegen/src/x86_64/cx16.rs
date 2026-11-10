//! CMPXCHG16B has fixed inputs RDX:RAX and RCX:RBX, and returns RDX:RAX.
//! The intrinsic and the i128 atomics use it.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_cx16(&mut self, args: &[Value], result: Option<Value>) {
        // Capture the address before RBX or any fixed input can change.
        let base = self.use_reg(args[0], S3);
        if base != S3 {
            self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(r64(base))]);
        }
        let save = sized(self.frame(self.cx16_save.expect("planned CX16 slot")), 64);
        self.emit(Opcode::Mov, [asm_arg(save.clone()), asm_arg(r64(3))]);
        let [expected_lo, expected_hi] = self.halves(args[1]);
        let [desired_lo, desired_hi] = self.halves(args[2]);
        self.emit(Opcode::Mov, [asm_arg(r64(S0)), expected_lo]);
        self.emit(Opcode::Mov, [asm_arg(r64(S2)), expected_hi]);
        // Desired high may itself be allocated in RBX: consume it first.
        self.emit(Opcode::Mov, [asm_arg(r64(S1)), desired_hi]);
        self.emit(Opcode::Mov, [asm_arg(r64(3)), desired_lo]);
        self.emit_locked(op::CMPXCHG16B, [asm_arg(sized(memory(r64(S3), 0), 128))]);
        self.emit(Opcode::Mov, [asm_arg(r64(3)), asm_arg(save)]);
        self.pair_out(result);
    }

    /// An i128 atomic. With AVX, an aligned 16-byte `vmovdqa` is atomic, so
    /// loads and stores use it and never write memory they only read.
    /// Everything else is `lock cmpxchg16b`, a full barrier, so the
    /// orderings lower alike except for an AVX seq_cst store's `mfence`.
    /// The address goes to r11 and RBX is saved around the sequence, as for
    /// the intrinsic. The preflight requires CX16.
    pub(super) fn atomic_i128(&mut self, inst: Inst, data: &InstData, result: Option<Value>) {
        let (addr, ordering) = match *data {
            InstData::AtomicLoad { addr, ordering, .. }
            | InstData::AtomicStore { addr, ordering, .. } => (addr, ordering),
            InstData::AtomicRmw { args, ordering, .. } => (args[0], ordering),
            InstData::AtomicCas { args, success, .. } => (args[0], success),
            _ => unreachable!("not an i128 atomic"),
        };
        let vector = |base: u8| sized(memory(r64(base), 0), 128);
        if self.features.avx {
            match *data {
                InstData::AtomicLoad { .. } => {
                    let base = self.use_reg(addr, S3);
                    self.emit(op::VMOVDQA, [asm_arg(xmm(FS0)), asm_arg(vector(base))]);
                    self.emit(op::VMOVQ, [asm_arg(r64(S0)), asm_arg(xmm(FS0))]);
                    self.emit(
                        op::VPEXTRQ,
                        [asm_arg(r64(S2)), asm_arg(xmm(FS0)), asm_arg(1u8)],
                    );
                    self.pair_out(result);
                    return;
                }
                InstData::AtomicStore { value, .. } => {
                    let [lo, hi] = self.halves(value);
                    self.emit(op::VMOVQ, [asm_arg(xmm(FS0)), lo]);
                    self.emit(
                        op::VPINSRQ,
                        [asm_arg(xmm(FS0)), asm_arg(xmm(FS0)), hi, asm_arg(1u8)],
                    );
                    let base = self.use_reg(addr, S3);
                    self.emit(op::VMOVDQA, [asm_arg(vector(base)), asm_arg(xmm(FS0))]);
                    if ordering == Ordering::SeqCst {
                        self.emit(Opcode::Mfence, []);
                    }
                    return;
                }
                _ => {}
            }
        }
        let base = self.use_reg(addr, S3);
        if base != S3 {
            self.emit(Opcode::Mov, [asm_arg(r64(S3)), asm_arg(r64(base))]);
        }
        let at = sized(memory(r64(S3), 0), 128);
        let save = sized(self.frame(self.cx16_save.expect("planned CX16 slot")), 64);
        let rbx = || asm_arg(r64(3));
        let mov =
            |this: &mut Self, d: u8, s: AsmOperand| this.emit(Opcode::Mov, [asm_arg(r64(d)), s]);
        // The current value as a first guess; a failed exchange refreshes it.
        let guess = |this: &mut Self| {
            mov(this, S0, asm_arg(sized(memory(r64(S3), 0), 64)));
            mov(this, S2, asm_arg(sized(memory(r64(S3), 8), 64)));
        };
        let retry = format!(
            ".L{}_{inst_name}_retry",
            self.func.name,
            inst_name = self.func.inst_name(inst)
        );
        let mut flag = None;
        match *data {
            // Comparing with zero and storing zero back leaves memory as it
            // was, but the access is a write.
            InstData::AtomicLoad { .. } => {
                self.emit(Opcode::Mov, [asm_arg(save.clone()), rbx()]);
                for r in [S0, S2, S1, 3] {
                    self.emit(Opcode::Xor, [asm_arg(r32(r)), asm_arg(r32(r))]);
                }
                self.emit_locked(op::CMPXCHG16B, [asm_arg(at)]);
            }
            InstData::AtomicStore { value, .. } => {
                let [lo, hi] = self.halves(value);
                self.emit(Opcode::Mov, [asm_arg(save.clone()), rbx()]);
                // The high half may be in RBX: read it first.
                mov(self, S1, hi);
                mov(self, 3, lo);
                guess(self);
                self.out.label(retry.clone());
                self.emit_locked(op::CMPXCHG16B, [asm_arg(at)]);
                self.emit(Opcode::Jcc(5), [asm_arg(retry)]);
            }
            InstData::AtomicCas { args, .. } => {
                let [expected_lo, expected_hi] = self.halves(args[1]);
                let [new_lo, new_hi] = self.halves(args[2]);
                self.emit(Opcode::Mov, [asm_arg(save.clone()), rbx()]);
                mov(self, S0, expected_lo);
                mov(self, S2, expected_hi);
                // The new value's high half may be in RBX: read it first.
                mov(self, S1, new_hi);
                mov(self, 3, new_lo);
                self.emit_locked(op::CMPXCHG16B, [asm_arg(at)]);
                // ZF is set when it stored. A fused flag leaves it for the
                // next `brif`; restoring RBX and moving results keep it.
                let value = self.func.inst_results(inst)[1];
                if !self.fused_compares.contains(&value)
                    && let Some(loc) = self.defined(value)
                {
                    self.emit(Opcode::Setcc(condition("e")), [asm_arg(r8(S1))]);
                    flag = Some(loc);
                }
            }
            InstData::AtomicRmw { op, args, .. } => {
                let [mut lo, mut hi] = self.halves(args[1]);
                // The loop rewrites RBX, so an operand half there moves to
                // R10, which only indirect calls use.
                for (half, loc) in [(&mut lo, self.loc(args[1])), (&mut hi, self.hi(args[1]))] {
                    if loc == Loc::Reg(3) {
                        mov(self, 10, rbx());
                        *half = asm_arg(r64(10));
                    }
                }
                self.emit(Opcode::Mov, [asm_arg(save.clone()), rbx()]);
                guess(self);
                if op == RmwOp::Xchg {
                    mov(self, S1, hi.clone());
                    mov(self, 3, lo.clone());
                }
                self.out.label(retry.clone());
                let pair = |this: &mut Self, low: Opcode, high: Opcode| {
                    mov(this, 3, asm_arg(r64(S0)));
                    mov(this, S1, asm_arg(r64(S2)));
                    this.emit(low, [rbx(), lo.clone()]);
                    this.emit(high, [asm_arg(r64(S1)), hi.clone()]);
                };
                match op {
                    RmwOp::Add => pair(self, Opcode::Add, Opcode::Adc),
                    RmwOp::Sub => pair(self, Opcode::Sub, Opcode::Sbb),
                    RmwOp::And => pair(self, Opcode::And, Opcode::And),
                    RmwOp::Or => pair(self, Opcode::Or, Opcode::Or),
                    RmwOp::Xor => pair(self, Opcode::Xor, Opcode::Xor),
                    RmwOp::Xchg => {}
                    RmwOp::Umin | RmwOp::Umax | RmwOp::Smin | RmwOp::Smax => {
                        // The flags of old minus operand pick the old value
                        // where it is the minimum or maximum.
                        let keep = match op {
                            RmwOp::Umin => "b",
                            RmwOp::Umax => "ae",
                            RmwOp::Smin => "l",
                            _ => "ge",
                        };
                        self.emit(Opcode::Cmp, [asm_arg(r64(S0)), lo.clone()]);
                        mov(self, S1, asm_arg(r64(S2)));
                        self.emit(Opcode::Sbb, [asm_arg(r64(S1)), hi.clone()]);
                        mov(self, 3, lo.clone());
                        mov(self, S1, hi.clone());
                        let cmov = Opcode::Cmovcc(condition(keep));
                        self.emit(cmov, [rbx(), asm_arg(r64(S0))]);
                        self.emit(cmov, [asm_arg(r64(S1)), asm_arg(r64(S2))]);
                    }
                }
                self.emit_locked(op::CMPXCHG16B, [asm_arg(at)]);
                self.emit(Opcode::Jcc(5), [asm_arg(retry)]);
            }
            _ => unreachable!("not an i128 atomic"),
        }
        self.emit(Opcode::Mov, [rbx(), asm_arg(save)]);
        if !matches!(data, InstData::AtomicStore { .. }) {
            self.pair_out(result);
        }
        if let Some(loc) = flag {
            self.emit_move(loc, Loc::Reg(S1));
        }
    }
}
