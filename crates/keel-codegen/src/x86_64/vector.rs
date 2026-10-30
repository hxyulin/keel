//! Packed x86 instruction selection and shared lane legalization.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};
use std::borrow::Cow;

// Require fresh canonical definitions, while accepting resolved operand aliases.
pub(super) fn literal_i64x2(func: &Function, value: Value) -> Option<i64> {
    let value = func.resolve(value);
    let ValueDef::Result(splat, _) = func.value_def(value) else {
        return None;
    };
    if func.first_result(splat) != value {
        return None;
    }
    let InstData::VectorSplat {
        ty: Type::Vector(VectorType::I64x2),
        arg,
    } = *func.inst_data(splat)
    else {
        return None;
    };
    let arg = func.resolve(arg);
    let ValueDef::Result(constant, _) = func.value_def(arg) else {
        return None;
    };
    if func.first_result(constant) != arg {
        return None;
    }
    match *func.inst_data(constant) {
        InstData::Iconst { ty: Type::I64, imm } => Some(imm),
        _ => None,
    }
}

fn sign_comparison(func: &Function, cond: IntCC, args: [Value; 2]) -> Option<(Value, bool)> {
    // Only these six predicates and their explicit operand-swapped forms.
    for (cc, x, k) in [
        (cond, args[0], args[1]),
        (
            match cond {
                IntCC::Slt => IntCC::Sgt,
                IntCC::Sgt => IntCC::Slt,
                IntCC::Sge => IntCC::Sle,
                IntCC::Sle => IntCC::Sge,
                IntCC::Ult => IntCC::Ugt,
                IntCC::Ugt => IntCC::Ult,
                IntCC::Uge => IntCC::Ule,
                IntCC::Ule => IntCC::Uge,
                other => other,
            },
            args[1],
            args[0],
        ),
    ] {
        let Some(literal) = literal_i64x2(func, k) else {
            continue;
        };
        let complement = match (cc, literal) {
            (IntCC::Slt, 0) | (IntCC::Uge, i64::MIN) | (IntCC::Ugt, i64::MAX) => false,
            (IntCC::Sge, 0) | (IntCC::Ult, i64::MIN) | (IntCC::Ule, i64::MAX) => true,
            _ => continue,
        };
        if func.value_type(func.resolve(x)) == Type::Vector(VectorType::I64x2) {
            return Some((func.resolve(x), complement));
        }
    }
    None
}

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

pub(super) fn packed(
    op: V,
    shape: VectorType,
    features: keel_asm_ir::x86::Features,
) -> Option<Opcode> {
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
        // Low dword products implement modular lanes for either signedness.
        V::Imul if shape == I32x4 && features.sse41 => {
            Opcode::V2(keel_asm_ir::x86::V2Opcode::Pmulld)
        }
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

// These register-only SSE2 recipes also apply to FP bits and to byte/word
// shuffles that preserve aligned dword groups. Other masks retain the shared
// lane legalizer. No loads are folded or narrowed by this classification.
#[derive(Clone, Copy, Debug)]
enum ShuffleRecipe {
    Dwords { source: usize, imm: u8 },
    Halves { first: usize, imm: u8 },
    Interleave { first: usize, imm: [u8; 2] },
}

fn shuffle_recipe(shape: VectorType, lanes: &[u8], same: bool) -> Option<ShuffleRecipe> {
    if shape.bytes() != 16 {
        return None;
    }
    let width = shape.lane_type().bytes().unwrap() as usize;
    let mut dwords = [0u8; 4];
    for (d, index) in dwords.iter_mut().enumerate() {
        let byte_at = |byte: usize| lanes[byte / width] as usize * width + byte % width;
        let start = byte_at(d * 4);
        if start % 4 != 0 || (1..4).any(|b| byte_at(d * 4 + b) != start + b) {
            return None;
        }
        *index = (start / 4) as u8;
        if same {
            *index %= 4;
        }
    }
    let imm = dwords
        .iter()
        .enumerate()
        .fold(0, |imm, (i, d)| imm | ((d % 4) << (2 * i)));
    let source = usize::from(dwords[0] / 4);
    if dwords.iter().all(|d| usize::from(d / 4) == source) {
        return Some(ShuffleRecipe::Dwords { source, imm });
    }
    if dwords[0] / 4 == dwords[1] / 4
        && dwords[2] / 4 == dwords[3] / 4
        && dwords[0] / 4 != dwords[2] / 4
    {
        return Some(ShuffleRecipe::Halves { first: source, imm });
    }
    if dwords[0] / 4 == dwords[2] / 4
        && dwords[1] / 4 == dwords[3] / 4
        && dwords[0] / 4 != dwords[1] / 4
    {
        return Some(ShuffleRecipe::Interleave {
            first: source,
            imm: [
                (dwords[0] % 4) | ((dwords[2] % 4) << 2),
                (dwords[1] % 4) | ((dwords[3] % 4) << 2),
            ],
        });
    }
    None
}

pub(super) fn lower(func: &Function, features: keel_asm_ir::x86::Features) -> Cow<'_, Function> {
    crate::vector::lower(func, |func, data| {
        // D values share the physical XMM bank with Q/Y values, but the packed
        // recipes below assume 128 bits. Scalarize every operation touching D,
        // including D -> Q/Y conversions and FMA, before choosing native ops.
        // Lane buffers are exactly sized and initialized by the shared lowerer;
        // no result depends on the unspecified upper half of an incoming XMM.
        let d = |ty: Type| ty.vector().is_some_and(|s| s.bytes() == 8);
        let mut touches_d = false;
        data.for_each_operand(|v| touches_d |= d(func.value_type(func.resolve(v))));
        if let InstData::VectorSplat { ty, .. } | InstData::Convert { ty, .. } = data {
            touches_d |= d(*ty);
        }
        if touches_d {
            return false;
        }
        if matches!(data, InstData::Fma { .. }) {
            return features.avx && features.fma;
        }
        if avx512_float::keep(func, data, features)
            || avx512::keep(func, data, features)
            || avx256::keep(func, data, features)
        {
            return true;
        }
        let mut wide = false;
        data.for_each_operand(|v| {
            wide |= func
                .value_type(func.resolve(v))
                .vector()
                .is_some_and(|s| s.bytes() != 16)
        });
        // Conversions can widen a 128-bit operand into a 256-bit result.
        // After AVX2 selection declines them, the SSE recipes below must not
        // retain that result: they only initialize 128 bits. Use lane lowering.
        if let InstData::VectorSplat { ty, .. } | InstData::Convert { ty, .. } = data {
            wide |= ty.vector().is_some_and(|s| s.bytes() != 16);
        }
        if wide {
            return false;
        }
        match data {
            InstData::VectorReduce {
                op: keel_ir::VectorReduceOp::Add,
                arg,
            } => matches!(
                func.value_type(func.resolve(*arg)),
                Type::Vector(VectorType::I64x2 | VectorType::I32x4)
            ),
            InstData::VectorShuffle { args, lanes } => shuffle_recipe(
                func.value_type(func.resolve(args[0])).vector().unwrap(),
                lanes,
                func.resolve(args[0]) == func.resolve(args[1]),
            )
            .is_some(),
            InstData::VectorExtract { arg, .. } => {
                func.value_type(*arg) == Type::Vector(VectorType::I64x2)
            }
            InstData::VectorBinary { op, args } => {
                let shape = func.value_type(args[0]).vector().unwrap();
                (*op == V::Imul && shape == VectorType::I32x4)
                    || packed(*op, shape, features).is_some()
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
        }
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

// Exact all-zero bits, not numerical FP equality (negative zero is distinct).
fn zero_splat(func: &Function, value: Value) -> bool {
    let value = func.resolve(value);
    let ValueDef::Result(i, _) = func.value_def(value) else {
        return false;
    };
    if func.first_result(i) != value {
        return false;
    }
    let InstData::VectorSplat { arg, .. } = *func.inst_data(i) else {
        return false;
    };
    let arg = func.resolve(arg);
    let ValueDef::Result(i, _) = func.value_def(arg) else {
        return false;
    };
    func.first_result(i) == arg
        && matches!(
            *func.inst_data(i),
            InstData::Iconst { imm: 0, .. } | InstData::Fconst { bits: 0, .. }
        )
}

impl FunctionCompiler<'_> {
    pub(super) fn vector_multiply_dwords_sse2(&mut self, args: [Value; 2], result: Option<Value>) {
        // PMULUDQ multiplies the even dwords into qwords. A second product
        // covers odd lanes; only the low dword of each product is retained.
        // Modulo 2^32, the result is independent of operand signedness.
        self.vector_stage(args[0], FS1);
        self.vector_stage(args[1], FS2);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(0xf5i64)],
        );
        self.emit(Opcode::Pmuludq, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS2)), asm_arg(xmm(FS2)), asm_arg(0xf5i64)],
        );
        self.emit(Opcode::Pmuludq, [asm_arg(xmm(FS0)), asm_arg(xmm(FS2))]);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS2)), asm_arg(xmm(FS0)), asm_arg(0xe8i64)],
        );
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(0xe8i64)],
        );
        self.emit(Opcode::Punpckldq, [asm_arg(xmm(FS0)), asm_arg(xmm(FS2))]);
        self.vector_publish(result);
    }

    pub(super) fn vector_reduce_add(&mut self, arg: Value, result: Option<Value>) {
        let shape = self.ty(arg).vector().unwrap();
        // Capture the entire input before defining a scalar destination. Only
        // reserved XMM registers are modified, so live inputs and spilled
        // inputs/results cannot overlap the destructive packed operations.
        self.vector_stage(arg, FS1);
        let add = match shape {
            VectorType::I64x2 => Opcode::Paddq,
            VectorType::I32x4 => Opcode::Paddd,
            _ => unreachable!("legalized wrapping add reduction"),
        };
        // Swap 64-bit halves: lane zero now sums opposite halves.
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS2)), asm_arg(xmm(FS1)), asm_arg(0x4e)],
        );
        self.emit(add, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
        if shape == VectorType::I32x4 {
            // Swap adjacent dwords, completing a modulo-2^32 tree sum.
            self.emit(
                Opcode::Pshufd,
                [asm_arg(xmm(FS2)), asm_arg(xmm(FS1)), asm_arg(0xb1)],
            );
            self.emit(add, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
        }
        let d = self.def_reg(result);
        let lane = shape.lane_type();
        self.emit(
            if lane == Type::I64 {
                Opcode::Movq
            } else {
                Opcode::Movd
            },
            [asm_arg(reg(d, lane)), asm_arg(xmm(FS1))],
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_shuffle(&mut self, args: [Value; 2], lanes: &[u8], result: Option<Value>) {
        let recipe = shuffle_recipe(
            self.ty(args[0]).vector().unwrap(),
            lanes,
            self.func.resolve(args[0]) == self.func.resolve(args[1]),
        )
        .expect("native shuffle retained by vector legalization");
        match recipe {
            ShuffleRecipe::Interleave { first, imm } => {
                // Read both inputs before defining a result that may alias
                // either one. Reserved scratch also covers all-spilled cases.
                let a = self.use_freg(args[first], FS1);
                let b = self.use_freg(args[1 - first], FS2);
                self.emit(
                    Opcode::Pshufd,
                    [
                        asm_arg(xmm(FS0)),
                        asm_arg(xmm(a)),
                        asm_arg(i64::from(imm[0])),
                    ],
                );
                self.emit(
                    Opcode::Pshufd,
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(xmm(b)),
                        asm_arg(i64::from(imm[1])),
                    ],
                );
                self.emit(Opcode::Punpckldq, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
                self.vector_publish(result);
            }
            ShuffleRecipe::Dwords { source, imm } => {
                // PSHUFD has a separate source and can safely overwrite either
                // allocated input. Read a spilled source before defining FS0.
                let a = self.use_freg(args[source], FS0);
                let d = self.def_freg(result);
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(i64::from(imm))],
                );
                self.def_done(result, d);
            }
            ShuffleRecipe::Halves { first, imm } => {
                // SHUFPS/SHUFPD destroy the first input. Stage before publishing
                // so either result alias, live sources and spills stay intact.
                self.vector_stage(args[first], FS0);
                self.vector_stage(args[1 - first], FS1);
                // Retain the existing qword window recipe (and generalize its
                // immediate to the other qword pairs).
                let (opcode, imm) =
                    if self.ty(args[0]).vector().unwrap().lane_type().bytes() == Some(8) {
                        (Opcode::Shufpd, ((imm >> 1) & 1) | ((imm >> 4) & 2))
                    } else {
                        (Opcode::Shufps, imm)
                    };
                self.emit(
                    opcode,
                    [
                        asm_arg(xmm(FS0)),
                        asm_arg(xmm(FS1)),
                        asm_arg(i64::from(imm)),
                    ],
                );
                self.vector_publish(result);
            }
        }
    }

    // Always stage before defining the result: all operands may spill, alias,
    // or remain live, and a spilled result itself uses FS0.
    fn vector_stage(&mut self, value: Value, scratch: u8) {
        let r = self.use_freg(value, scratch);
        if r != scratch {
            self.emit(Opcode::Movaps, [asm_arg(xmm(scratch)), asm_arg(xmm(r))]);
        }
    }
    // Each qword sign comes from its high dword. Copy before destructive shifts:
    // x can remain live, alias the result, or occupy a spill slot.
    pub(super) fn vector_absolute(&mut self, value: Value, result: Option<Value>) {
        let a = self.use_freg(value, FS0);
        let d = self.def_freg(result);
        // Capture the sign before overwriting a coalesced input/result. Neither
        // reserved FS1 nor a spilled result's FS0 can hold an allocated input.
        self.emit(Opcode::Movaps, [asm_arg(xmm(FS1)), asm_arg(xmm(a))]);
        if d != a {
            self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(a))]);
        }
        self.emit(Opcode::Psrad, [asm_arg(xmm(FS1)), asm_arg(31i64)]);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(0xf5i64)],
        );
        self.emit(Opcode::Pxor, [asm_arg(xmm(d)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Psubq, [asm_arg(xmm(d)), asm_arg(xmm(FS1))]);
        self.def_done(result, d);
    }
    /// Load before defining the result; only reserved vector scratch is touched.
    pub(super) fn vector_positive32(&mut self, addr: Value, result: Option<Value>) {
        let at = self.mem(addr, S0, Type::I64);
        self.emit(Opcode::Movq, [asm_arg(xmm(FS0)), asm_arg(&at)]);
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Movaps, [asm_arg(xmm(FS2)), asm_arg(xmm(FS0))]);
        self.emit(Opcode::Pcmpgtd, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Andps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS2))]);
        self.emit(Opcode::Punpckldq, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.vector_publish(result);
    }
    pub(super) fn vector_positive64(&mut self, value: Value, result: Option<Value>) {
        // Retain X before any destination write, including spilled result FS0.
        self.vector_stage(value, FS1);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(0xf5i64)],
        );
        self.emit(Opcode::Psrad, [asm_arg(xmm(FS0)), asm_arg(31i64)]);
        self.emit(Opcode::Andnps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.vector_publish(result);
    }
    pub(super) fn vector_sign_select(&mut self, x: Value, otherwise: Value, result: Option<Value>) {
        // Read both retained leaves before any destination write. FS0 also
        // serves spilled results; FS1/FS2 preserve either input alias and X==F.
        self.vector_stage(x, FS1);
        self.vector_stage(otherwise, FS2);
        self.emit(
            Opcode::Pshufd,
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(0xf5i64)],
        );
        self.emit(Opcode::Psrad, [asm_arg(xmm(FS0)), asm_arg(31i64)]);
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
        self.emit(Opcode::Andps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS2))]);
        self.vector_publish(result);
    }
    pub(super) fn vector_ineg(&mut self, value: Value, result: Option<Value>) {
        self.vector_stage(value, FS1);
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]);
        self.emit(Opcode::Psubq, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        self.vector_publish(result);
    }
    fn vector_publish(&mut self, result: Option<Value>) {
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
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
        if op == ConvertOp::SwidenLow
            && self.ty(arg) == Type::Vector(VectorType::I32x4)
            && to == Type::Vector(VectorType::I64x2)
            && self.features.sse41
        {
            // Capture the resolved source before defining a possibly aliased
            // destination. use_freg retains a spilled vector's full 16-byte
            // home; this is deliberately the register form, never an m64 fold.
            let a = self.use_freg(arg, FS0);
            let d = self.def_freg(result);
            self.emit(
                Opcode::V2(keel_asm_ir::x86::V2Opcode::Pmovsxdq),
                [asm_arg(xmm(d)), asm_arg(xmm(a))],
            );
            self.def_done(result, d);
            return;
        }
        // Stage before defining the result. FS0/FS1 are reserved, so no
        // allocated source (including one live after this instruction) changes.
        // A spilled source and result may each use FS0 at different times.
        self.vector_stage(arg, FS0);
        if op != ConvertOp::Bitcast {
            let mut width = self.ty(arg).vector().unwrap().lane_type().bytes().unwrap();
            let target = to.vector().unwrap().lane_type().bytes().unwrap();
            if op == ConvertOp::UwidenLow {
                self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
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
                    self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
                    self.emit(compare, [asm_arg(xmm(FS1)), asm_arg(xmm(FS0))]);
                }
                self.emit(unpack, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
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
                [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(shuffle)],
            );
        } else {
            let a = self.use_reg(arg, S0);
            if shape == VectorType::I64x2 {
                self.emit(Opcode::Movq, [asm_arg(xmm(d)), asm_arg(r64(a))]);
                self.emit(Opcode::Punpcklqdq, [asm_arg(xmm(d)), asm_arg(xmm(d))]);
            } else {
                self.emit(Opcode::Movd, [asm_arg(xmm(d)), asm_arg(r32(a))]);
                if shape == VectorType::I8x16 {
                    self.emit(Opcode::Punpcklbw, [asm_arg(xmm(d)), asm_arg(xmm(d))]);
                }
                if matches!(shape, VectorType::I8x16 | VectorType::I16x8) {
                    self.emit(Opcode::Punpcklwd, [asm_arg(xmm(d)), asm_arg(xmm(d))]);
                }
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(d)), asm_arg(xmm(d)), asm_arg(0i64)],
                );
            }
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_compare(&mut self, cond: IntCC, args: [Value; 2], result: Option<Value>) {
        if result.is_some_and(|v| self.func.resolve(v) == v)
            && let Some((x, complement)) = sign_comparison(self.func, cond, args)
        {
            self.vector_stage(x, FS0);
            self.emit(
                Opcode::Pshufd,
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS0)), asm_arg(0xf5i64)],
            );
            self.emit(Opcode::Psrad, [asm_arg(xmm(FS0)), asm_arg(31i64)]);
            if complement {
                self.emit(Opcode::Pcmpeqd, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
                self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
            }
            self.vector_publish(result);
            return;
        }
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
            self.emit(Opcode::Pcmpeqd, [asm_arg(xmm(FS2)), asm_arg(xmm(FS2))]);
            match shape {
                VectorType::I8x16 => {
                    // All-ones words << 7 are exactly -128 (0xff80).
                    // Signed packing produces 0x80 in EVERY byte, with no
                    // saturation change and no constant-pool memory access.
                    self.emit(Opcode::Psllw, [asm_arg(xmm(FS2)), asm_arg(7i64)]);
                    self.emit(Opcode::Packsswb, [asm_arg(xmm(FS2)), asm_arg(xmm(FS2))]);
                }
                VectorType::I16x8 => {
                    self.emit(Opcode::Psllw, [asm_arg(xmm(FS2)), asm_arg(15i64)]);
                }
                VectorType::I32x4 | VectorType::I64x2 if unsigned => {
                    self.emit(Opcode::Pslld, [asm_arg(xmm(FS2)), asm_arg(31i64)]);
                }
                VectorType::I64x2 => {
                    self.emit(Opcode::Psrlq, [asm_arg(xmm(FS2)), asm_arg(63i64)]);
                    self.emit(Opcode::Psllq, [asm_arg(xmm(FS2)), asm_arg(31i64)]);
                }
                _ => unreachable!(),
            }
            self.emit(Opcode::Xorps, [asm_arg(xmm(dst)), asm_arg(xmm(FS2))]);
            self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
            if shape != VectorType::I64x2 {
                self.emit(
                    packed_compare(cond, shape).unwrap(),
                    [asm_arg(xmm(dst)), asm_arg(xmm(FS1))],
                );
            } else {
                self.emit(Opcode::Movaps, [asm_arg(xmm(FS2)), asm_arg(xmm(dst))]);
                self.emit(Opcode::Pcmpgtd, [asm_arg(xmm(FS2)), asm_arg(xmm(FS1))]);
                self.emit(Opcode::Pcmpeqd, [asm_arg(xmm(dst)), asm_arg(xmm(FS1))]);
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS2)), asm_arg(0xa0i64)],
                );
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(dst)), asm_arg(xmm(dst)), asm_arg(0xf5i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(xmm(dst)), asm_arg(xmm(FS1))]);
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS2)), asm_arg(0xf5i64)],
                );
                self.emit(Opcode::Orps, [asm_arg(xmm(dst)), asm_arg(xmm(FS1))]);
            }
        } else {
            let a = self.use_freg(lhs, FS0);
            let b = self.use_freg(rhs, FS1);
            // A two-address operation must not destroy rhs before reading it.
            // Reserved FS2 is disjoint from allocated registers and spill loads.
            dst = if d == b && a != d { FS2 } else { d };
            if dst != a {
                self.emit(Opcode::Movaps, [asm_arg(xmm(dst)), asm_arg(xmm(a))]);
            }
            self.emit(
                packed_compare(cond, shape).unwrap(),
                [asm_arg(xmm(dst)), asm_arg(xmm(b))],
            );
            if shape == VectorType::I64x2 {
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(dst)), asm_arg(0xb1i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(xmm(dst)), asm_arg(xmm(FS1))]);
            }
        }
        if matches!(
            cond,
            IntCC::Ne | IntCC::Sle | IntCC::Sge | IntCC::Ule | IntCC::Uge
        ) {
            self.emit(Opcode::Pcmpeqd, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
            self.emit(Opcode::Xorps, [asm_arg(xmm(dst)), asm_arg(xmm(FS1))]);
        }
        if dst != d {
            self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(dst))]);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_select(&mut self, args: [Value; 3], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let canonical = crate::vector::canonical_mask(self.func, args[0]);
        if canonical && (zero_splat(self.func, args[1]) || zero_splat(self.func, args[2])) {
            let zero_true = zero_splat(self.func, args[1]);
            // Read both leaves before defining a possibly coalesced or spilled
            // destination. Shared comparisons retain their original mask value.
            let mask = self.use_freg(args[0], FS0);
            let value = self.use_freg(args[if zero_true { 2 } else { 1 }], FS1);
            let d = self.def_freg(result);
            let dst = if d == value && mask != d { FS2 } else { d };
            if dst != mask {
                self.emit(Opcode::Movaps, [asm_arg(xmm(dst)), asm_arg(xmm(mask))]);
            }
            self.emit(
                if zero_true {
                    Opcode::Andnps
                } else {
                    Opcode::Andps
                },
                [asm_arg(xmm(dst)), asm_arg(xmm(value))],
            );
            if dst != d {
                self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(dst))]);
            }
            self.def_done(result, d);
            return;
        }
        self.vector_stage(args[0], FS0);
        if !canonical {
            self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
            self.emit(
                packed_compare(IntCC::Eq, shape).unwrap(),
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))],
            );
            if shape == VectorType::I64x2 {
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS0)), asm_arg(0xb1i64)],
                );
                self.emit(Opcode::Andps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
            }
        }
        self.vector_stage(args[1], FS1);
        self.vector_stage(args[2], FS2);
        // b ^ ((a ^ b) & !is_zero(mask)); masks need not be canonical.
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
        self.emit(
            if canonical {
                Opcode::Andps
            } else {
                Opcode::Andnps
            },
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))],
        );
        self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS2))]);
        self.vector_publish(result);
    }
}

#[cfg(test)]
mod d_tests {
    use super::*;

    #[test]
    fn shuffle_recipes_match_all_dword_masks_and_qword_pairs() {
        for shape in [
            VectorType::I32x4,
            VectorType::F32x4,
            VectorType::I64x2,
            VectorType::F64x2,
        ] {
            let width = shape.lane_type().bytes().unwrap() as usize;
            let count = shape.lanes() as usize;
            let radix = 2 * count;
            for mask in 0..radix.pow(count as u32) {
                let mut m = mask;
                let lanes: Vec<_> = (0..count)
                    .map(|_| {
                        let lane = (m % radix) as u8;
                        m /= radix;
                        lane
                    })
                    .collect();
                for same in [false, true] {
                    let Some(recipe) = shuffle_recipe(shape, &lanes, same) else {
                        continue;
                    };
                    let mut actual = [0; 16];
                    let (imm, sources) = match recipe {
                        ShuffleRecipe::Dwords { source, imm } => (imm, [source; 4]),
                        ShuffleRecipe::Halves { first, imm } => {
                            (imm, [first, first, 1 - first, 1 - first])
                        }
                        ShuffleRecipe::Interleave { first, imm } => {
                            let indices =
                                [imm[0] & 3, imm[1] & 3, (imm[0] >> 2) & 3, (imm[1] >> 2) & 3];
                            (
                                indices
                                    .iter()
                                    .enumerate()
                                    .fold(0, |v, (i, d)| v | (d << (2 * i))),
                                [first, 1 - first, first, 1 - first],
                            )
                        }
                    };
                    for (i, byte) in actual.iter_mut().enumerate() {
                        let source = if same { 0 } else { sources[i / 4] };
                        *byte = source * 16 + usize::from((imm >> (2 * (i / 4))) & 3) * 4 + i % 4;
                    }
                    let expected: Vec<_> = lanes
                        .iter()
                        .flat_map(|&lane| {
                            let lane = if same {
                                usize::from(lane) % count
                            } else {
                                usize::from(lane)
                            };
                            (0..width).map(move |b| lane * width + b)
                        })
                        .collect();
                    assert_eq!(actual.as_slice(), expected, "{shape:?} {lanes:?} {same}");
                }
            }
        }
        // Byte/word masks must preserve every byte in an aligned dword.
        assert!(
            shuffle_recipe(
                VectorType::I8x16,
                &[1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                false
            )
            .is_none()
        );
        assert!(shuffle_recipe(VectorType::I16x8, &[1, 0, 2, 3, 4, 5, 6, 7], false).is_none());
        assert!(shuffle_recipe(VectorType::I32x4, &[0, 4, 1, 5], false).is_some());
        assert!(shuffle_recipe(VectorType::I32x4, &[0, 4, 1, 2], false).is_none());
    }

    #[test]
    fn wrapping_reduction_retention_has_exact_operation_and_result_shapes() {
        use keel_asm_ir::x86::{CpuLevel, Features};
        use keel_ir::VectorReduceOp;
        for shape in [
            VectorType::I8x8,
            VectorType::I16x4,
            VectorType::I32x2,
            VectorType::I64x1,
            VectorType::I8x16,
            VectorType::I16x8,
            VectorType::I32x4,
            VectorType::I64x2,
            VectorType::I32x8,
            VectorType::I64x4,
        ] {
            for op in VectorReduceOp::ALL {
                let source = format!(
                    "func @f({shape}) -> {} {{\nentry(v0:{shape}):\n v1 = {op} v0\n return v1\n}}",
                    shape.lane_type()
                );
                let module = keel_ir::parse(&source).unwrap();
                keel_ir::verify_module(&module).unwrap();
                for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
                    let lowered = lower(&module.functions[0], Features::for_level(level));
                    keel_ir::verify(&lowered).unwrap();
                    let retained: Vec<_> = lowered
                        .layout_insts()
                        .filter_map(|(_, i)| {
                            matches!(lowered.inst_data(i), InstData::VectorReduce { .. })
                                .then_some(i)
                        })
                        .collect();
                    // Existing AVX2 reductions remain retained for every operation.
                    let eligible = (op == VectorReduceOp::Add
                        && matches!(shape, VectorType::I32x4 | VectorType::I64x2))
                        || (shape.bytes() == 32 && level == CpuLevel::V3);
                    assert_eq!(
                        retained.len(),
                        usize::from(eligible),
                        "{shape} {op} {level:?}"
                    );
                    for i in retained {
                        assert_eq!(
                            lowered.value_type(lowered.first_result(i)),
                            shape.lane_type()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dword_multiply_retains_metadata_through_owned_rounds() {
        use keel_asm_ir::x86::{CpuLevel, Features};
        use keel_ir::{EntityRef, SourceFile, SourceLoc};
        let mut module = keel_ir::parse("func @f(i32x4,i32x4)->i32x4 {\nentry(v0:i32x4,v1:i32x4):\n v2 = vimul v0,v1\n v3 = vpadd v2,v0\n return v3\n}\n").unwrap();
        let f = &mut module.functions[0];
        let mul = f
            .layout_insts()
            .find(|(_, i)| matches!(f.inst_data(*i), InstData::VectorBinary { op: V::Imul, .. }))
            .unwrap()
            .1;
        let loc = SourceLoc {
            file: SourceFile::new(0),
            line: 17,
            col: 9,
            scope: None,
        };
        f.set_inst_loc(mul, Some(loc));
        let data = f.inst_data(mul).clone();
        let result = f.first_result(mul);
        for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
            for sse41 in [false, true] {
                let mut features = Features::for_level(level);
                features.sse41 = sse41;
                let lowered = lower(f, features);
                assert!(matches!(lowered, Cow::Owned(_))); // vpadd forces shared rounds.
                keel_ir::verify(&lowered).unwrap();
                let retained = lowered.layout_insts().any(|(_, i)| i == mul);
                assert!(retained);
                if retained {
                    assert_eq!(lowered.inst_data(mul), &data);
                    assert_eq!(lowered.first_result(mul), result);
                    assert_eq!(lowered.inst_loc(mul), Some(loc));
                    assert_eq!(
                        packed(V::Imul, VectorType::I32x4, features),
                        sse41.then_some(Opcode::V2(keel_asm_ir::x86::V2Opcode::Pmulld))
                    );
                }
            }
        }
    }

    #[test]
    fn d_operations_scalarize_before_packed_feature_selection() {
        let source = "func @f(f32x2,f32x2,f32x2)->f32x2 {\nentry(v0:f32x2,v1:f32x2,v2:f32x2):\n v3 = fma v0,v1,v2\n return v3\n}\nfunc @w(i8x8)->i16x8 {\nentry(v0:i8x8):\n v1 = uwiden_low.i16x8 v0\n return v1\n}\nfunc @n(i16x8)->i8x8 {\nentry(v0:i16x8):\n v1 = narrow_low.i8x8 v0\n return v1\n}";
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for features in [
            keel_asm_ir::x86::Features::NONE,
            keel_asm_ir::x86::Features::all_known(),
        ] {
            for f in &m.functions {
                let lowered = lower(f, features);
                keel_ir::verify(&lowered).unwrap();
                for (_, i) in lowered.layout_insts() {
                    let data = lowered.inst_data(i);
                    assert!(!data.is_vector_instruction(), "{data:?}");
                    assert!(
                        !matches!(data, InstData::Fma { args } if lowered.value_type(args[0]).is_vector())
                    );
                    assert!(!matches!(
                        data,
                        InstData::Convert {
                            op: ConvertOp::NarrowLow | ConvertOp::UwidenLow,
                            ..
                        }
                    ));
                }
            }
        }
    }
}
