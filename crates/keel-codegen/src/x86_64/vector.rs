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
        InstData::VectorShuffle { args, lanes } => {
            lanes.as_slice() == [1, 2]
                && matches!(
                    func.value_type(args[0]),
                    Type::Vector(VectorType::I64x2 | VectorType::F64x2)
                )
                && func.value_type(args[0]) == func.value_type(args[1])
        }
        InstData::VectorExtract { arg, .. } => {
            func.value_type(*arg) == Type::Vector(VectorType::I64x2)
        }
        InstData::VectorBinary { op, args } => {
            packed(*op, func.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::Convert {
            op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
            ..
        } => true,
        InstData::Convert {
            op: ConvertOp::Bitcast,
            ty,
            arg,
        } => ty.is_vector() && func.value_type(*arg).is_vector(),
        InstData::VectorSelect { .. } | InstData::VectorSplat { .. } => true,
        InstData::VectorIcmp { cond, args } => {
            packed_compare(*cond, func.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary {
            op: UnaryOp::Ineg,
            arg,
        } => func.value_type(*arg) == Type::Vector(VectorType::I64x2),
        InstData::VectorUnary {
            op: UnaryOp::Fsqrt, ..
        } => true,
        _ => false,
    })
}

// SSE2 handles equality and ordering at every integer width. Unsigned ordering
// biases the sign bit; i64 ordering uses a lexicographic dword recipe.
pub(super) fn packed_compare(cond: IntCC, shape: VectorType) -> Option<Opcode> {
    use VectorType::*;
    Some(match (cond, shape) {
        (IntCC::Eq | IntCC::Ne, I8x16) => Opcode::Pcmpeqb,
        (IntCC::Eq | IntCC::Ne, I16x8) => Opcode::Pcmpeqw,
        (IntCC::Eq | IntCC::Ne, I32x4 | I64x2) => Opcode::Pcmpeqd,
        (_, I8x16) => Opcode::Pcmpgtb,
        (_, I16x8) => Opcode::Pcmpgtw,
        (_, I32x4 | I64x2) => Opcode::Pcmpgtd,
        _ => return None,
    })
}
// Signed widening preserves zero/all-ones lanes. Unsigned widening does not:
// e.g. an i32 true mask becomes 0x00000000ffffffff in an i64 lane.
// Do not infer facts through bitcasts or block parameters, even if their
// incoming values happen to be comparisons. Resolve ordinary SSA aliases.
fn canonical_mask(func: &Function, mut value: Value) -> bool {
    // Integer vector lanes can widen at most three times (8 -> 16 -> 32 -> 64).
    for _ in 0..4 {
        value = func.resolve(value);
        let ValueDef::Result(inst, _) = func.value_def(value) else {
            return false;
        };
        match *func.inst_data(inst) {
            InstData::VectorIcmp { .. } => return true,
            InstData::Convert {
                op: ConvertOp::SwidenLow,
                arg,
                ..
            } => value = arg,
            _ => return false,
        }
    }
    false
}

impl FunctionCompiler<'_> {
    pub(super) fn vector_shuffle_window(&mut self, args: [Value; 2], result: Option<Value>) {
        // SHUFPD destroys its first operand. Stage both inputs before defining
        // the destination so live inputs, either destination alias and all
        // spill combinations retain their original bits.
        self.vector_stage(args[0], FS0);
        self.vector_stage(args[1], FS1);
        self.emit(
            Opcode::Shufpd,
            [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1)), asm_arg(&1)],
        );
        self.vector_publish(result);
    }

    // Always stage before defining the result: all operands may spill, alias,
    // or remain live, and a spilled result itself uses FS0.
    fn vector_stage(&mut self, value: Value, scratch: u8) {
        let r = self.use_freg(value, scratch);
        if r != scratch {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(scratch)), asm_arg(&xmm(r))]);
        }
    }
    // Each qword sign comes from its high dword. Copy before destructive shifts:
    // x can remain live, alias the result, or occupy a spill slot.
    pub(super) fn vector_absolute(&mut self, value: Value, result: Option<Value>) {
        let a = self.use_freg(value, FS0);
        let d = self.def_freg(result);
        // Capture the sign before overwriting a coalesced input/result. Neither
        // reserved FS1 nor a spilled result's FS0 can hold an allocated input.
        self.emit(Opcode::Movaps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(a))]);
        if d != a {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(d)), asm_arg(&xmm(a))]);
        }
        self.emit(Opcode::Psrad, [asm_arg(&xmm(FS1)), asm_arg(&31i64)]);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1)), asm_arg(&0xf5i64)],
        );
        self.emit(Opcode::Pxor, [asm_arg(&xmm(d)), asm_arg(&xmm(FS1))]);
        self.emit(Opcode::Psubq, [asm_arg(&xmm(d)), asm_arg(&xmm(FS1))]);
        self.def_done(result, d);
    }
    pub(super) fn vector_ineg(&mut self, value: Value, result: Option<Value>) {
        self.vector_stage(value, FS1);
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS0))]);
        self.emit(Opcode::Psubq, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
        self.vector_publish(result);
    }
    fn vector_publish(&mut self, result: Option<Value>) {
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(d)), asm_arg(&xmm(FS0))]);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_convert(
        &mut self,
        op: ConvertOp,
        to: Type,
        arg: Value,
        result: Option<Value>,
    ) {
        // Stage before defining the result. FS0/FS1 are reserved, so no
        // allocated source (including one live after this instruction) changes.
        // A spilled source and result may each use FS0 at different times.
        self.vector_stage(arg, FS0);
        if op != ConvertOp::Bitcast {
            let mut width = self.ty(arg).vector().unwrap().lane_type().bytes().unwrap();
            let target = to.vector().unwrap().lane_type().bytes().unwrap();
            if op == ConvertOp::UwidenLow {
                self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1))]);
            }
            while width < target {
                let (unpack, compare) = match width {
                    1 => (Opcode::Punpcklbw, Opcode::Pcmpgtb),
                    2 => (Opcode::Punpcklwd, Opcode::Pcmpgtw),
                    4 => (Opcode::Punpckldq, Opcode::Pcmpgtd),
                    _ => unreachable!("invalid integer widening width"),
                };
                if op == ConvertOp::SwidenLow {
                    // 0 > lane produces its sign extension, then low unpack
                    // interleaves each original lane with those high bits.
                    self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1))]);
                    self.emit(compare, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS0))]);
                }
                self.emit(unpack, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))]);
                width *= 2;
            }
        }
        // movaps is a bitwise register move for every vector shape, including
        // FP NaNs and signed zero. Memory traffic remains in the load/store IR.
        self.vector_publish(result);
    }
    pub(super) fn vector_splat(&mut self, ty: Type, arg: Value, result: Option<Value>) {
        let shape = ty.vector().unwrap();
        let d = self.def_freg(result);
        if shape.lane_type().is_float() {
            let a = self.use_freg(arg, FS1);
            // Duplicate the low scalar's bits without FP arithmetic (NaNs and
            // signed zero included). pshufd also permits d to alias its source.
            let shuffle = if shape == VectorType::F32x4 { 0 } else { 0x44 };
            self.emit(
                Opcode::Pshufd,
                [asm_arg(&xmm(d)), asm_arg(&xmm(a)), asm_arg(&shuffle)],
            );
        } else {
            let a = self.use_reg(arg, S0);
            if shape == VectorType::I64x2 {
                self.emit(Opcode::Movq, [asm_arg(&xmm(d)), asm_arg(&r64(a))]);
                self.emit(Opcode::Punpcklqdq, [asm_arg(&xmm(d)), asm_arg(&xmm(d))]);
            } else {
                self.emit(Opcode::Movd, [asm_arg(&xmm(d)), asm_arg(&r32(a))]);
                if shape == VectorType::I8x16 {
                    self.emit(Opcode::Punpcklbw, [asm_arg(&xmm(d)), asm_arg(&xmm(d))]);
                }
                if matches!(shape, VectorType::I8x16 | VectorType::I16x8) {
                    self.emit(Opcode::Punpcklwd, [asm_arg(&xmm(d)), asm_arg(&xmm(d))]);
                }
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(d)), asm_arg(&xmm(d)), asm_arg(&0i64)],
                );
            }
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_compare(&mut self, cond: IntCC, args: [Value; 2], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let unsigned = matches!(cond, IntCC::Ult | IntCC::Ule | IntCC::Ugt | IntCC::Uge);
        let swap = matches!(cond, IntCC::Slt | IntCC::Sge | IntCC::Ult | IntCC::Uge);
        let lhs = args[usize::from(swap)];
        let rhs = args[usize::from(!swap)];
        let d = self.def_freg(result);
        let dst;
        if unsigned || (shape == VectorType::I64x2 && !matches!(cond, IntCC::Eq | IntCC::Ne)) {
            // Capture rhs first: the result may alias it. lhs can then be
            // computed directly in the result register, including a spill's FS0.
            self.vector_stage(rhs, FS1);
            self.vector_stage(lhs, d);
            dst = d;
            // Bias each unsigned lane before signed comparison. For i64 compare
            // high dwords, breaking ties by unsigned low dwords: signed i64
            // biases only low dwords, unsigned i64 biases BOTH dwords.
            // Three reserved registers suffice even with all values spilled.
            self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS2))]);
            match shape {
                VectorType::I8x16 => {
                    self.emit(Opcode::Psllw, [asm_arg(&xmm(FS2)), asm_arg(&15i64)]);
                    self.emit(Opcode::Psrlw, [asm_arg(&xmm(FS2)), asm_arg(&8i64)]);
                    self.emit(Opcode::Punpcklbw, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS2))]);
                    // The first unpack duplicates bytes as 80,80,00,00.
                    // Replicate the low word and then the low dword so every
                    // byte lane receives the unsigned sign-bit bias.
                    self.emit(Opcode::Punpcklwd, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS2))]);
                    self.emit(
                        Opcode::Pshufd,
                        [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS2)), asm_arg(&0i64)],
                    );
                }
                VectorType::I16x8 => {
                    self.emit(Opcode::Psllw, [asm_arg(&xmm(FS2)), asm_arg(&15i64)]);
                }
                VectorType::I32x4 | VectorType::I64x2 if unsigned => {
                    self.emit(Opcode::Pslld, [asm_arg(&xmm(FS2)), asm_arg(&31i64)]);
                }
                VectorType::I64x2 => {
                    self.emit(Opcode::Psrlq, [asm_arg(&xmm(FS2)), asm_arg(&63i64)]);
                    self.emit(Opcode::Psllq, [asm_arg(&xmm(FS2)), asm_arg(&31i64)]);
                }
                _ => unreachable!(),
            }
            self.emit(Opcode::Xorps, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS2))]);
            self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2))]);
            if shape != VectorType::I64x2 {
                self.emit(
                    packed_compare(cond, shape).unwrap(),
                    [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))],
                );
            } else {
                self.emit(Opcode::Movaps, [asm_arg(&xmm(FS2)), asm_arg(&xmm(dst))]);
                self.emit(Opcode::Pcmpgtd, [asm_arg(&xmm(FS2)), asm_arg(&xmm(FS1))]);
                self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))]);
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2)), asm_arg(&0xa0i64)],
                );
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(dst)), asm_arg(&xmm(dst)), asm_arg(&0xf5i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))]);
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2)), asm_arg(&0xf5i64)],
                );
                self.emit(Opcode::Orps, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))]);
            }
        } else {
            let a = self.use_freg(lhs, FS0);
            let b = self.use_freg(rhs, FS1);
            // A two-address operation must not destroy rhs before reading it.
            // Reserved FS2 is disjoint from allocated registers and spill loads.
            dst = if d == b && a != d { FS2 } else { d };
            if dst != a {
                self.emit(Opcode::Movaps, [asm_arg(&xmm(dst)), asm_arg(&xmm(a))]);
            }
            self.emit(
                packed_compare(cond, shape).unwrap(),
                [asm_arg(&xmm(dst)), asm_arg(&xmm(b))],
            );
            if shape == VectorType::I64x2 {
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(&xmm(FS1)), asm_arg(&xmm(dst)), asm_arg(&0xb1i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))]);
            }
        }
        if matches!(
            cond,
            IntCC::Ne | IntCC::Sle | IntCC::Sge | IntCC::Ule | IntCC::Uge
        ) {
            self.emit(Opcode::Pcmpeqd, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS1))]);
            self.emit(Opcode::Xorps, [asm_arg(&xmm(dst)), asm_arg(&xmm(FS1))]);
        }
        if dst != d {
            self.emit(Opcode::Movaps, [asm_arg(&xmm(d)), asm_arg(&xmm(dst))]);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_select(&mut self, args: [Value; 3], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let canonical = canonical_mask(self.func, args[0]);
        self.vector_stage(args[0], FS0);
        if !canonical {
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
        }
        self.vector_stage(args[1], FS1);
        self.vector_stage(args[2], FS2);
        // b ^ ((a ^ b) & !is_zero(mask)); masks need not be canonical.
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS1)), asm_arg(&xmm(FS2))]);
        self.emit(
            if canonical {
                Opcode::Andps
            } else {
                Opcode::Andnps
            },
            [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS1))],
        );
        self.emit(Opcode::Xorps, [asm_arg(&xmm(FS0)), asm_arg(&xmm(FS2))]);
        self.vector_publish(result);
    }
}
