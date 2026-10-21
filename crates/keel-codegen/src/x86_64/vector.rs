//! Packed x86 instruction selection and shared lane legalization.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};
use std::borrow::Cow;

// Preserve written FP source order, including add/mul, as in scalar AVX selection.
pub(super) fn packed_avx(op: V, shape: VectorType) -> Option<Opcode> {
    use VectorType::*;
    Some(match (op, shape) {
        (V::Fadd, F32x4) => Opcode::Vaddps,
        (V::Fadd, F64x2) => Opcode::Vaddpd,
        (V::Fsub, F32x4) => Opcode::Vsubps,
        (V::Fsub, F64x2) => Opcode::Vsubpd,
        (V::Fmul, F32x4) => Opcode::Vmulps,
        (V::Fmul, F64x2) => Opcode::Vmulpd,
        (V::Fdiv, F32x4) => Opcode::Vdivps,
        (V::Fdiv, F64x2) => Opcode::Vdivpd,
        _ => return None,
    })
}

pub(super) fn packed(op: V, shape: VectorType) -> Option<Opcode> {
    use VectorType::*;
    Some(match op {
        V::Iadd => match shape {
            I8x16 => Opcode::Paddb,
            I16x8 => Opcode::Paddw,
            I32x4 => Opcode::Paddd,
            I64x2 => Opcode::Paddq,
            _ => return None,
        },
        V::Isub => match shape {
            I8x16 => Opcode::Psubb,
            I16x8 => Opcode::Psubw,
            I32x4 => Opcode::Psubd,
            I64x2 => Opcode::Psubq,
            _ => return None,
        },
        V::Imul if shape == I16x8 => Opcode::Pmullw,
        V::And => Opcode::Andps,
        V::Or => Opcode::Orps,
        V::Xor => Opcode::Xorps,
        V::Fadd => {
            if shape == F32x4 {
                Opcode::Addps
            } else {
                Opcode::Addpd
            }
        }
        V::Fsub => {
            if shape == F32x4 {
                Opcode::Subps
            } else {
                Opcode::Subpd
            }
        }
        V::Fmul => {
            if shape == F32x4 {
                Opcode::Mulps
            } else {
                Opcode::Mulpd
            }
        }
        V::Fdiv => {
            if shape == F32x4 {
                Opcode::Divps
            } else {
                Opcode::Divpd
            }
        }
        _ => return None,
    })
}
pub(super) fn lower(func: &Function) -> Cow<'_, Function> {
    crate::vector::lower(func, |func, data| match data {
        InstData::VectorExtract { arg, .. } => {
            func.value_type(*arg) == Type::Vector(VectorType::I64x2)
        }
        InstData::VectorBinary { op, args } => {
            packed(*op, func.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorSelect { .. } => true,
        InstData::VectorIcmp { cond, args } => {
            packed_compare(*cond, func.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary {
            op: UnaryOp::Fsqrt, ..
        } => true,
        _ => false,
    })
}

// SSE2 handles equality and signed ordering at every integer width.
// i64 ordering uses a dword recipe; unsigned ordering still uses lane legalization.
pub(super) fn packed_compare(cond: IntCC, shape: VectorType) -> Option<Opcode> {
    use VectorType::*;
    Some(match (cond, shape) {
        (IntCC::Eq | IntCC::Ne, I8x16) => Opcode::Pcmpeqb,
        (IntCC::Eq | IntCC::Ne, I16x8) => Opcode::Pcmpeqw,
        (IntCC::Eq | IntCC::Ne, I32x4 | I64x2) => Opcode::Pcmpeqd,
        (IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge, I8x16) => Opcode::Pcmpgtb,
        (IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge, I16x8) => Opcode::Pcmpgtw,
        (IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge, I32x4 | I64x2) => Opcode::Pcmpgtd,
        _ => return None,
    })
}
impl FunctionCompiler<'_> {
    // Always stage before defining the result: all operands may spill, alias,
    // or remain live, and a spilled result itself uses FS0.
    fn vector_stage(&mut self, value: Value, scratch: u8) {
        let r = self.use_freg(value, scratch);
        if r != scratch {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(scratch)), asm_arg(&xmm(r))]);
        }
    }
    fn vector_publish(&mut self, result: Option<Value>) {
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(d)), asm_arg(&xmm(FS0))]);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_compare(&mut self, cond: IntCC, args: [Value; 2], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let swap = matches!(cond, IntCC::Slt | IntCC::Sge);
        self.vector_stage(args[usize::from(swap)], FS0);
        self.vector_stage(args[usize::from(!swap)], FS1);
        if shape == VectorType::I64x2 && !matches!(cond, IntCC::Eq | IntCC::Ne) {
            // Compare signed high dwords, breaking ties by UNSIGNED low dwords.
            // Bias just the low dwords before pcmpgtd. Three reserved registers
            // suffice even when every operand and the result is spilled.
            self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS2))]);
            self.emit(Opcode::Psrlq, [asm_arg(&xmm(FS2)), asm_arg(&63i64)]);
            self.emit(Opcode::Psllq, [asm_arg(&xmm(FS2)), asm_arg(&31i64)]);
            self.emit(Opcode::Xorps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS2))]);
            self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2))]);
            self.emit(Opcode::Movaps, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS0))]);
            self.emit(Opcode::Pcmpgtd, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS1))]);
            self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
            self.emit(
                Opcode::Pshufd,
                [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2)), asm_arg(&0xa0i64)],
            );
            self.emit(
                Opcode::Pshufd,
                [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS0)), asm_arg(&0xf5i64)],
            );
            self.emit(Opcode::Andps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
            self.emit(
                Opcode::Pshufd,
                [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2)), asm_arg(&0xf5i64)],
            );
            self.emit(Opcode::Orps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
        } else {
            self.emit(
                packed_compare(cond, shape).unwrap(),
                [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))],
            );
            if shape == VectorType::I64x2 {
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS0)), asm_arg(&0xb1i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
            }
        }
        if matches!(cond, IntCC::Ne | IntCC::Sle | IntCC::Sge) {
            self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1))]);
            self.emit(Opcode::Xorps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
        }
        self.vector_publish(result);
    }
    pub(super) fn vector_select(&mut self, args: [Value; 3], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        self.vector_stage(args[0], FS0);
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1))]);
        self.emit(
            packed_compare(IntCC::Eq, shape).unwrap(),
            [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))],
        );
        if shape == VectorType::I64x2 {
            self.emit(
                Opcode::Pshufd,
                [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS0)), asm_arg(&0xb1i64)],
            );
            self.emit(Opcode::Andps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
        }
        self.vector_stage(args[1], FS1);
        self.vector_stage(args[2], FS2);
        // b ^ ((a ^ b) & !is_zero(mask)); masks need not be canonical.
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2))]);
        self.emit(Opcode::Andnps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS2))]);
        self.vector_publish(result);
    }
}
