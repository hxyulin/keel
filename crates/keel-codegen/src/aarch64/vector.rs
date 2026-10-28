//! Packed NEON selection and shared scalar-lane legalization.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};
use std::borrow::Cow;

struct Packed {
    opcode: Opcode,
    mnemonic: &'static str,
    arrangement: &'static str,
    field: Option<(FieldId, u32)>,
}
fn arrangement(shape: VectorType) -> &'static str {
    use VectorType::*;
    match shape {
        I8x8 => "8b",
        I8x16 => "16b",
        I16x4 => "4h",
        I16x8 => "8h",
        I32x2 | F32x2 => "2s",
        I32x4 | F32x4 => "4s",
        I64x1 | F64x1 => "1d",
        I64x2 | F64x2 => "2d",
        _ => unreachable!("SIMD width validated before AArch64 lowering"),
    }
}
fn binary(op: V, shape: VectorType) -> Option<Packed> {
    if shape.bytes() == 8
        && shape.lane_type().bytes() == Some(8)
        && !matches!(op, V::And | V::Or | V::Xor)
    {
        return None;
    }
    let (opcode, mnemonic) = match op {
        V::Iadd => (Opcode::ADD_asimdsame_only, "add"),
        V::Isub => (Opcode::SUB_asimdsame_only, "sub"),
        V::Imul if shape != VectorType::I64x2 => (Opcode::MUL_asimdsame_only, "mul"),
        V::And => (Opcode::AND_asimdsame_only, "and"),
        V::Or => (Opcode::ORR_asimdsame_only, "orr"),
        V::Xor => (Opcode::EOR_asimdsame_only, "eor"),
        V::Padd => (Opcode::ADDP_asimdsame_only, "addp"),
        V::Psmin if shape != VectorType::I64x2 => (Opcode::SMINP_asimdsame_only, "sminp"),
        V::Psmax if shape != VectorType::I64x2 => (Opcode::SMAXP_asimdsame_only, "smaxp"),
        V::Pumin if shape != VectorType::I64x2 => (Opcode::UMINP_asimdsame_only, "uminp"),
        V::Pumax if shape != VectorType::I64x2 => (Opcode::UMAXP_asimdsame_only, "umaxp"),
        V::Fpadd => (Opcode::FADDP_asimdsame_only, "faddp"),
        V::Fpmin => (Opcode::FMINP_asimdsame_only, "fminp"),
        V::Fpmax => (Opcode::FMAXP_asimdsame_only, "fmaxp"),
        V::Smin if shape != VectorType::I64x2 => (Opcode::SMIN_asimdsame_only, "smin"),
        V::Smax if shape != VectorType::I64x2 => (Opcode::SMAX_asimdsame_only, "smax"),
        V::Umin if shape != VectorType::I64x2 => (Opcode::UMIN_asimdsame_only, "umin"),
        V::Umax if shape != VectorType::I64x2 => (Opcode::UMAX_asimdsame_only, "umax"),
        V::Sabd if shape != VectorType::I64x2 => (Opcode::SABD_asimdsame_only, "sabd"),
        V::Uabd if shape != VectorType::I64x2 => (Opcode::UABD_asimdsame_only, "uabd"),
        V::Shadd if shape != VectorType::I64x2 => (Opcode::SHADD_asimdsame_only, "shadd"),
        V::Uhadd if shape != VectorType::I64x2 => (Opcode::UHADD_asimdsame_only, "uhadd"),
        V::Srhadd if shape != VectorType::I64x2 => (Opcode::SRHADD_asimdsame_only, "srhadd"),
        V::Urhadd if shape != VectorType::I64x2 => (Opcode::URHADD_asimdsame_only, "urhadd"),
        V::Shsub if shape != VectorType::I64x2 => (Opcode::SHSUB_asimdsame_only, "shsub"),
        V::Uhsub if shape != VectorType::I64x2 => (Opcode::UHSUB_asimdsame_only, "uhsub"),
        V::Fadd => (Opcode::FADD_asimdsame_only, "fadd"),
        V::Fsub => (Opcode::FSUB_asimdsame_only, "fsub"),
        V::Fmul => (Opcode::FMUL_asimdsame_only, "fmul"),
        V::Fdiv => (Opcode::FDIV_asimdsame_only, "fdiv"),
        V::Fmin => (Opcode::FMIN_asimdsame_only, "fmin"),
        V::Fmax => (Opcode::FMAX_asimdsame_only, "fmax"),
        _ => return None,
    };
    let bitwise = matches!(op, V::And | V::Or | V::Xor);
    Some(Packed {
        opcode,
        mnemonic,
        arrangement: if bitwise { if shape.bytes() == 8 { "8b" } else { "16b" } } else { arrangement(shape) },
        field: if bitwise {
            None
        } else if op.is_float() {
            Some((FieldId::sz, u32::from(shape == VectorType::F64x2)))
        } else {
            Some((
                FieldId::size,
                shape.lane_type().bytes().unwrap().trailing_zeros(),
            ))
        },
    })
}
fn unary(op: UnaryOp, shape: VectorType) -> Option<Packed> {
    if shape.bytes() == 8 && shape.lane_type().bytes() == Some(8)
        && !matches!(op, UnaryOp::Bnot | UnaryOp::Bswap)
    {
        return None;
    }
    let size = shape.lane_type().bytes().unwrap().trailing_zeros();
    let (opcode, mnemonic) = match op {
        UnaryOp::Ineg => (Opcode::NEG_asimdmisc_R, "neg"),
        UnaryOp::Bnot => (Opcode::NOT_asimdmisc_R, "mvn"),
        UnaryOp::Clz if shape != VectorType::I64x2 => (Opcode::CLZ_asimdmisc_R, "clz"),
        UnaryOp::Popcnt if matches!(shape, VectorType::I8x8 | VectorType::I8x16) => (Opcode::CNT_asimdmisc_R, "cnt"),
        UnaryOp::Bswap => match size {
            1 => (Opcode::REV16_asimdmisc_R, "rev16"),
            2 => (Opcode::REV32_asimdmisc_R, "rev32"),
            3 => (Opcode::REV64_asimdmisc_R, "rev64"),
            _ => return None,
        },
        UnaryOp::Fneg => (Opcode::FNEG_asimdmisc_R, "fneg"),
        UnaryOp::Fabs => (Opcode::FABS_asimdmisc_R, "fabs"),
        UnaryOp::Fsqrt => (Opcode::FSQRT_asimdmisc_R, "fsqrt"),
        UnaryOp::Fceil => (Opcode::FRINTP_asimdmisc_R, "frintp"),
        UnaryOp::Ffloor => (Opcode::FRINTM_asimdmisc_R, "frintm"),
        UnaryOp::Ftrunc => (Opcode::FRINTZ_asimdmisc_R, "frintz"),
        UnaryOp::Fnearest => (Opcode::FRINTN_asimdmisc_R, "frintn"),
        _ => return None,
    };
    let bytes = matches!(op, UnaryOp::Bnot | UnaryOp::Bswap);
    let field = if op == UnaryOp::Bnot {
        None
    } else if op.is_float() {
        Some((FieldId::sz, u32::from(shape == VectorType::F64x2)))
    } else {
        Some((FieldId::size, if bytes { 0 } else { size }))
    };
    Some(Packed {
        opcode,
        mnemonic,
        arrangement: if bytes { if shape.bytes() == 8 { "8b" } else { "16b" } } else { arrangement(shape) },
        field,
    })
}
pub(super) fn lower(func: &Function) -> Cow<'_, Function> {
    crate::vector::lower(func, |f, data| {
        // Retain D widths only on paths with exact D encodings. Other paths
        // use shared lane legalization without widening a memory access.
        let mut narrow = matches!(
            data,
            InstData::VectorSplat { ty, .. } | InstData::Convert { ty, .. }
                if ty.vector().is_some_and(|v| v.bytes() == 8)
        );
        data.for_each_operand(|v| {
            narrow |= f.value_type(v).vector().is_some_and(|v| v.bytes() == 8);
        });
        if narrow {
            return match data {
                InstData::VectorBinary { op, args } => binary(*op, f.value_type(args[0]).vector().unwrap()).is_some(),
                InstData::VectorUnary { op, arg } => unary(*op, f.value_type(*arg).vector().unwrap()).is_some(),
                InstData::VectorShuffle { .. } => true,
                _ => false,
            };
        }
        match data {
        InstData::VectorReduce { .. }
        | InstData::VectorShuffle { .. }
        | InstData::VectorExtract { .. }
        | InstData::VectorInsert { .. } => true,
        InstData::VectorBinary { op, args } => {
            matches!(op, V::Shl | V::Ushr | V::Sshr)
                || binary(*op, f.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary { op, arg } => {
            *op == UnaryOp::Popcnt || unary(*op, f.value_type(*arg).vector().unwrap()).is_some()
        }
        InstData::Convert {
            op:
                ConvertOp::SwidenLow
                | ConvertOp::UwidenLow
                | ConvertOp::SwidenHigh
                | ConvertOp::UwidenHigh
                | ConvertOp::NarrowLow
                | ConvertOp::SpairwiseAddLong
                | ConvertOp::UpairwiseAddLong,
            ..
        } => true,
        InstData::Convert {
            op: ConvertOp::Bitcast,
            ty,
            arg,
        } => ty.is_vector() && f.value_type(*arg).is_vector(),
        InstData::VectorSplat { .. }
        | InstData::VectorSelect { .. }
        | InstData::VectorIcmp { .. } => true,
        _ => false,
        }
    })
}

impl FunctionCompiler<'_> {
    pub(super) fn vector_absolute_difference(
        &mut self,
        difference: absolute_diff::ExactSabal,
        result: Option<Value>,
    ) {
        // Capture every retained leaf before touching a destructive destination.
        let acc = self.use_freg(difference.accumulator, FS0);
        let left = self.use_freg(difference.left, FS1);
        let right = self.use_freg(difference.right, FS2);
        let d = self.def_freg(result);
        let work = if (d == left || d == right) && d != acc {
            FS0
        } else {
            d
        };
        self.move_vector(work, acc);
        self.native(
            Opcode::SABAL_asimddiff_L,
            [
                (FieldId::Q, 0),
                (FieldId::size, 2),
                (FieldId::Rd, work.into()),
                (FieldId::Rn, left.into()),
                (FieldId::Rm, right.into()),
            ],
            |out| write!(out, "sabal\tv{work}.2d, v{left}.2s, v{right}.2s"),
        );
        self.move_vector(d, work);
        self.def_done(result, d);
    }

    // EXT reads both operands before writing, including when d aliases either
    // input. Separate reserved spill scratches keep both captured sources live.
    pub(super) fn vector_shuffle_window(
        &mut self,
        args: [Value; 2],
        lanes: &[u8],
        result: Option<Value>,
    ) {
        let shape = self.ty(args[0]).vector().unwrap();
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let d = self.def_freg(result);
        // Exact static permutations are single native instructions. Operand
        // producers remain in place; all sources are captured before d writes.
        let n = shape.lanes();
        if n >= 2 {
            for (opcode,name,kind) in [
                (Opcode::ZIP1_asimdperm_only,"zip1",0), (Opcode::ZIP2_asimdperm_only,"zip2",1),
                (Opcode::UZP1_asimdperm_only,"uzp1",2), (Opcode::UZP2_asimdperm_only,"uzp2",3),
                (Opcode::TRN1_asimdperm_only,"trn1",4), (Opcode::TRN2_asimdperm_only,"trn2",5),
            ] {
                if lanes.iter().copied().enumerate().all(|(i,lane)| {
                    let i=i as u8;
                    let expected=match kind {
                        0|1 => (kind as u8)*(n/2)+i/2+(i%2)*n,
                        2|3 => (i/(n/2))*n+2*(i%(n/2))+(kind as u8-2),
                        _ => (i/2)*2+(kind as u8-4)+(i%2)*n,
                    };
                    lane==expected
                }) {
                    let arr=arrangement(shape);
                    self.native(opcode,[(FieldId::Q,u32::from(shape.bytes()==16)),
                        (FieldId::size,shape.lane_type().bytes().unwrap().trailing_zeros()),
                        (FieldId::Rd,d.into()),(FieldId::Rn,a.into()),(FieldId::Rm,b.into())],
                        |out|write!(out,"{name}\tv{d}.{arr}, v{a}.{arr}, v{b}.{arr}"));
                    self.def_done(result,d);
                    return;
                }
            }
        }
        // A contiguous lane window is one EXT for every shape. Otherwise build
        // in reserved FS2, so allocated destinations may alias either input.
        let first = lanes[0];
        if !lanes
            .iter()
            .enumerate()
            .all(|(i, n)| usize::from(*n) == usize::from(first) + i)
            || first >= shape.lanes()
        {
            for (i, lane) in lanes.iter().copied().enumerate() {
                let source = if lane < shape.lanes() { a } else { b };
                self.vector_insert_lane(FS2, i as u8, source, lane % shape.lanes(), shape);
            }
            self.move_vector(d, FS2);
            self.def_done(result, d);
            return;
        }
        let offset = u32::from(first) * shape.lane_type().bytes().unwrap();
        self.native(
            Opcode::EXT_asimdext_only,
            [
                (FieldId::Q, u32::from(shape.bytes()==16)),
                (FieldId::imm4, offset),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, b.into()),
            ],
            |out| {let arr=if shape.bytes()==16 {"16b"} else {"8b"};write!(out, "ext\tv{d}.{arr}, v{a}.{arr}, v{b}.{arr}, #{offset}")},
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_widening_add(&mut self, add: WideningAdd, result: Option<Value>) {
        let source_shape = self.ty(add.source).vector().unwrap();
        let destination = self.ty(add.accumulator).vector().unwrap();
        let accumulator = self.use_freg(add.accumulator, FS0);
        let source = self.use_freg(add.source, FS1);
        let d = self.def_freg(result);
        let bytes = source_shape.lane_type().bytes().unwrap();
        let (opcode, name) = if add.signed {
            (Opcode::SADDW_asimddiff_W, "saddw")
        } else {
            (Opcode::UADDW_asimddiff_W, "uaddw")
        };
        let narrow = match bytes {
            1 => "8b",
            2 => "4h",
            4 => "2s",
            _ => unreachable!(),
        };
        self.native(
            opcode,
            [
                (FieldId::Q, 0),
                (FieldId::size, bytes.trailing_zeros()),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, accumulator.into()),
                (FieldId::Rm, source.into()),
            ],
            |out| {
                write!(
                    out,
                    "{name}\tv{d}.{}, v{accumulator}.{}, v{source}.{narrow}",
                    arrangement(destination),
                    arrangement(destination)
                )
            },
        );
        self.def_done(result, d);
    }
    pub(super) fn vector_replicate_load(&mut self, addr: Value, result: Option<Value>) {
        let shape = self.ty(result.unwrap()).vector().unwrap();
        let base = self.use_reg(addr, S0);
        let d = self.def_freg(result);
        self.native(
            Opcode::LD1R_asisdlso_R1,
            [
                (FieldId::Q, 1),
                (
                    FieldId::size,
                    shape.lane_type().bytes().unwrap().trailing_zeros(),
                ),
                (FieldId::Rn, base.into()),
                (FieldId::Rt, d.into()),
            ],
            |out| write!(out, "ld1r\t{{v{d}.{}}}, [x{base}]", arrangement(shape)),
        );
        self.def_done(result, d);
    }
    pub(super) fn vector_widen(
        &mut self,
        op: ConvertOp,
        ty: Type,
        arg: Value,
        result: Option<Value>,
    ) {
        let mut shape = self.ty(arg).vector().unwrap();
        let destination = ty.vector().unwrap();
        let d = self.def_freg(result);
        let mut source = self.use_freg(arg, FS0);
        while shape != destination {
            let bytes = shape.lane_type().bytes().unwrap();
            let next = VectorType::ALL_128
                .into_iter()
                .find(|s| s.lane_type().is_int() && s.lane_type().bytes() == Some(bytes * 2))
                .unwrap();
            let high = matches!(op, ConvertOp::SwidenHigh | ConvertOp::UwidenHigh);
            let (opcode, name) = match op {
                ConvertOp::SwidenLow => (Opcode::SSHLL_asimdshf_L, "sshll"),
                ConvertOp::SwidenHigh => (Opcode::SSHLL_asimdshf_L, "sshll2"),
                ConvertOp::UwidenLow => (Opcode::USHLL_asimdshf_L, "ushll"),
                ConvertOp::UwidenHigh => (Opcode::USHLL_asimdshf_L, "ushll2"),
                _ => unreachable!(),
            };
            let source_arrangement = if high {
                arrangement(shape)
            } else {
                match bytes {
                    1 => "8b",
                    2 => "4h",
                    4 => "2s",
                    _ => unreachable!(),
                }
            };
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(high)),
                    (FieldId::immh, bytes),
                    (FieldId::immb, 0),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{d}.{}, v{source}.{source_arrangement}, #0",
                        arrangement(next)
                    )
                },
            );
            source = d;
            shape = next;
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_pairwise_long(
        &mut self,
        op: ConvertOp,
        ty: Type,
        arg: Value,
        result: Option<Value>,
    ) {
        let source = self.use_freg(arg, FS0);
        let d = self.def_freg(result);
        let shape = self.ty(arg).vector().unwrap();
        let bytes = shape.lane_type().bytes().unwrap();
        let (opcode, name) = if op == ConvertOp::SpairwiseAddLong {
            (Opcode::SADDLP_asimdmisc_P, "saddlp")
        } else {
            (Opcode::UADDLP_asimdmisc_P, "uaddlp")
        };
        self.native(
            opcode,
            [
                (FieldId::Q, 1),
                (FieldId::size, bytes.trailing_zeros()),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, source.into()),
            ],
            |out| {
                write!(
                    out,
                    "{name}\tv{d}.{}, v{source}.{}",
                    arrangement(ty.vector().unwrap()),
                    arrangement(shape)
                )
            },
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_narrow(&mut self, ty: Type, arg: Value, result: Option<Value>) {
        let mut shape = self.ty(arg).vector().unwrap();
        let dest = ty.vector().unwrap();
        let mut source = self.use_freg(arg, FS0);
        let d = self.def_freg(result);
        while shape != dest {
            let bytes = shape.lane_type().bytes().unwrap() / 2;
            let next = VectorType::ALL_128
                .into_iter()
                .find(|s| s.lane_type().is_int() && s.lane_type().bytes() == Some(bytes))
                .unwrap();
            let narrow = match bytes {
                1 => "8b",
                2 => "4h",
                4 => "2s",
                _ => unreachable!(),
            };
            self.native(
                Opcode::XTN_asimdmisc_N,
                [
                    (FieldId::Q, 0),
                    (FieldId::size, bytes.trailing_zeros()),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| write!(out, "xtn\tv{d}.{narrow}, v{source}.{}", arrangement(shape)),
            );
            source = d;
            shape = next;
        }
        self.def_done(result, d);
    }

    pub(super) fn vector_bitcast(&mut self, arg: Value, result: Option<Value>) {
        let source = self.use_freg(arg, FS0);
        let d = self.def_freg(result);
        self.move_vector(d, source);
        self.def_done(result, d);
    }

    fn vector_insert_lane(&mut self, d: u8, lane: u8, source: u8, index: u8, shape: VectorType) {
        let bytes = shape.lane_type().bytes().unwrap();
        let element = ["b", "h", "s", "d"][bytes.trailing_zeros() as usize];
        self.native(
            Opcode::INS_asimdins_IV_v,
            [
                (FieldId::imm5, bytes | (u32::from(lane) * bytes * 2)),
                (FieldId::imm4, u32::from(index) * bytes),
                (FieldId::Rn, source.into()),
                (FieldId::Rd, d.into()),
            ],
            |out| {
                write!(
                    out,
                    "ins\tv{d}.{element}[{lane}], v{source}.{element}[{index}]"
                )
            },
        );
    }

    pub(super) fn vector_insert(&mut self, args: [Value; 2], lane: u8, result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let source = self.use_freg(args[0], FS0);
        // Preserve the scalar operand too before touching an aliased destination.
        let scalar = if shape.lane_type().is_float() {
            self.use_freg(args[1], FS1)
        } else {
            self.use_reg(args[1], S0)
        };
        self.move_vector(FS2, source);
        if shape.lane_type().is_float() {
            self.vector_insert_lane(FS2, lane, scalar, 0, shape);
        } else {
            let bytes = shape.lane_type().bytes().unwrap();
            let element = ["b", "h", "s", "d"][bytes.trailing_zeros() as usize];
            let gpr = if bytes == 8 { "x" } else { "w" };
            self.native(
                Opcode::INS_asimdins_IR_r,
                [
                    (FieldId::imm5, bytes | (u32::from(lane) * bytes * 2)),
                    (FieldId::Rn, scalar.into()),
                    (FieldId::Rd, FS2.into()),
                ],
                |out| write!(out, "ins\tv{FS2}.{element}[{lane}], {gpr}{scalar}"),
            );
        }
        let d = self.def_freg(result);
        self.move_vector(d, FS2);
        self.def_done(result, d);
    }

    pub(super) fn vector_reduce(
        &mut self,
        op: keel_ir::VectorReduceOp,
        arg: Value,
        result: Option<Value>,
    ) {
        use keel_ir::VectorReduceOp as R;
        let shape = self.ty(arg).vector().unwrap();
        let bytes = shape.lane_type().bytes().unwrap();
        let source = self.use_freg(arg, FS0);
        if bytes == 8 && matches!(op, R::Smin | R::Smax | R::Umin | R::Umax) {
            for (lane, d) in [(0, S0), (1, S1)] {
                self.native(
                    Opcode::UMOV_asimdins_X_x,
                    [
                        (FieldId::imm5, 8 | (lane << 4)),
                        (FieldId::Rn, source.into()),
                        (FieldId::Rd, d.into()),
                    ],
                    |out| write!(out, "umov\tx{d}, v{source}.d[{lane}]"),
                );
            }
            let d = self.def_reg(result);
            let cc = match op {
                R::Smin => "lt",
                R::Smax => "gt",
                R::Umin => "lo",
                R::Umax => "hi",
                _ => unreachable!(),
            };
            self.mach("cmp", [Reg::x(S0).into(), Reg::x(S1).into()]);
            self.mach(
                "csel",
                [
                    Reg::x(d).into(),
                    Reg::x(S0).into(),
                    Reg::x(S1).into(),
                    Arg::from(cc),
                ],
            );
            self.def_done(result, d);
            return;
        }
        if matches!(op, R::And | R::Or | R::Xor) {
            self.move_vector(FS2, source);
            let mut offset = 8;
            let (opcode, name) = match op {
                R::And => (Opcode::AND_asimdsame_only, "and"),
                R::Or => (Opcode::ORR_asimdsame_only, "orr"),
                R::Xor => (Opcode::EOR_asimdsame_only, "eor"),
                _ => unreachable!(),
            };
            while offset >= bytes {
                self.native(
                    Opcode::EXT_asimdext_only,
                    [
                        (FieldId::Q, 1),
                        (FieldId::imm4, offset),
                        (FieldId::Rd, FS1.into()),
                        (FieldId::Rn, FS2.into()),
                        (FieldId::Rm, FS2.into()),
                    ],
                    |out| write!(out, "ext\tv{FS1}.16b, v{FS2}.16b, v{FS2}.16b, #{offset}"),
                );
                self.native(
                    opcode,
                    [
                        (FieldId::Q, 1),
                        (FieldId::Rd, FS2.into()),
                        (FieldId::Rn, FS2.into()),
                        (FieldId::Rm, FS1.into()),
                    ],
                    |out| write!(out, "{name}\tv{FS2}.16b, v{FS2}.16b, v{FS1}.16b"),
                );
                offset /= 2;
            }
        } else if bytes == 8 {
            self.native(
                Opcode::ADDP_asisdpair_only,
                [(FieldId::Rd, FS2.into()), (FieldId::Rn, source.into())],
                |out| write!(out, "addp\td{FS2}, v{source}.2d"),
            );
        } else {
            let (opcode, name) = match op {
                R::Add => (Opcode::ADDV_asimdall_only, "addv"),
                R::Smin => (Opcode::SMINV_asimdall_only, "sminv"),
                R::Smax => (Opcode::SMAXV_asimdall_only, "smaxv"),
                R::Umin => (Opcode::UMINV_asimdall_only, "uminv"),
                R::Umax => (Opcode::UMAXV_asimdall_only, "umaxv"),
                _ => unreachable!(),
            };
            let element = ["b", "h", "s"][bytes.trailing_zeros() as usize];
            self.native(
                opcode,
                [
                    (FieldId::Q, 1),
                    (FieldId::size, bytes.trailing_zeros()),
                    (FieldId::Rd, FS2.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\t{element}{FS2}, v{source}.{}",
                        arrangement(shape)
                    )
                },
            );
        }
        let d = self.def_reg(result);
        let (opcode, gpr) = if bytes == 8 {
            (Opcode::UMOV_asimdins_X_x, "x")
        } else {
            (Opcode::UMOV_asimdins_W_w, "w")
        };
        let element = ["b", "h", "s", "d"][bytes.trailing_zeros() as usize];
        self.native(
            opcode,
            [
                (FieldId::imm5, bytes),
                (FieldId::Rn, FS2.into()),
                (FieldId::Rd, d.into()),
            ],
            |out| write!(out, "umov\t{gpr}{d}, v{FS2}.{element}[0]"),
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_extract(&mut self, arg: Value, lane: u8, result: Option<Value>) {
        let shape = self.ty(arg).vector().unwrap();
        let bytes = shape.lane_type().bytes().unwrap();
        let element = ["b", "h", "s", "d"][bytes.trailing_zeros() as usize];
        let source = self.use_freg(arg, FS0);
        let imm5 = bytes | (u32::from(lane) * bytes * 2);
        if shape.lane_type().is_float() {
            let d = self.def_freg(result);
            self.native(
                Opcode::DUP_asisdone_only,
                [
                    (FieldId::imm5, imm5),
                    (FieldId::Rn, source.into()),
                    (FieldId::Rd, d.into()),
                ],
                |out| write!(out, "dup\t{element}{d}, v{source}.{element}[{lane}]"),
            );
            self.def_done(result, d);
        } else {
            let d = self.def_reg(result);
            let (opcode, gpr) = if bytes == 8 {
                (Opcode::UMOV_asimdins_X_x, "x")
            } else {
                (Opcode::UMOV_asimdins_W_w, "w")
            };
            self.native(
                opcode,
                [
                    (FieldId::imm5, imm5),
                    (FieldId::Rn, source.into()),
                    (FieldId::Rd, d.into()),
                ],
                |out| write!(out, "umov\t{gpr}{d}, v{source}.{element}[{lane}]"),
            );
            self.def_done(result, d);
        }
    }
    pub(super) fn vector_splat(&mut self, ty: Type, arg: Value, result: Option<Value>) {
        let shape = ty.vector().unwrap();
        let lane = shape.lane_type();
        let d = self.def_freg(result);
        let (opcode, source) = if lane.is_float() {
            (Opcode::DUP_asimdins_DV_v, self.use_freg(arg, FS1))
        } else {
            (Opcode::DUP_asimdins_DR_r, self.use_reg(arg, S0))
        };
        self.native(
            opcode,
            [
                (FieldId::Q, 1),
                (FieldId::imm5, lane.bytes().unwrap()),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, source.into()),
            ],
            |out| {
                write!(out, "dup\tv{d}.{}, ", arrangement(shape))?;
                if lane.is_float() {
                    write!(
                        out,
                        "v{source}.{}[0]",
                        if lane == Type::F32 { "s" } else { "d" }
                    )
                } else {
                    write!(out, "{}{source}", if lane == Type::I64 { "x" } else { "w" })
                }
            },
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_binary(&mut self, op: V, args: [Value; 2], result: Option<Value>) {
        if matches!(op, V::Shl | V::Ushr | V::Sshr) {
            self.vector_shift(op, args, result);
            return;
        }
        let shape = self.ty(args[0]).vector().unwrap();
        let spec = binary(op, shape).expect("legalized NEON binary");
        let d = self.def_freg(result);
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Rm, u32::from(b)),
            (FieldId::Q, u32::from(shape.bytes() == 16)),
        ];
        let text = |out: &mut crate::text::Text| {
            write!(
                out,
                "{}\tv{d}.{}, v{a}.{}, v{b}.{}",
                spec.mnemonic, spec.arrangement, spec.arrangement, spec.arrangement
            )
        };
        if let Some(field) = spec.field {
            self.native(
                spec.opcode,
                [base[0], base[1], base[2], base[3], field],
                text,
            );
        } else {
            self.native(spec.opcode, base, text);
        }
        self.def_done(result, d);
    }
    fn vector_shift(&mut self, op: V, args: [Value; 2], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let bytes = shape.lane_type().bytes().unwrap();
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        // SSHL/USHL interpret only the low signed byte of each count. Masking
        // to width-1 first implements the IR's modulo-width count exactly,
        // including counts with their high bit set and arbitrarily large lanes.
        self.mach(
            "mov",
            [Reg::w(S0).into(), Arg::imm(i64::from(bytes * 8 - 1))],
        );
        self.native(
            Opcode::DUP_asimdins_DR_r,
            [
                (FieldId::Q, 1),
                (FieldId::imm5, bytes),
                (FieldId::Rd, FS2.into()),
                (FieldId::Rn, S0.into()),
            ],
            |out| {
                write!(
                    out,
                    "dup\tv{FS2}.{}, {}{S0}",
                    arrangement(shape),
                    if bytes == 8 { "x" } else { "w" }
                )
            },
        );
        self.native(
            Opcode::AND_asimdsame_only,
            [
                (FieldId::Q, 1),
                (FieldId::Rd, FS2.into()),
                (FieldId::Rn, b.into()),
                (FieldId::Rm, FS2.into()),
            ],
            |out| write!(out, "and\tv{FS2}.16b, v{b}.16b, v{FS2}.16b"),
        );
        let size = bytes.trailing_zeros();
        if op != V::Shl {
            self.native(
                Opcode::NEG_asimdmisc_R,
                [
                    (FieldId::Q, 1),
                    (FieldId::size, size),
                    (FieldId::Rd, FS2.into()),
                    (FieldId::Rn, FS2.into()),
                ],
                |out| {
                    write!(
                        out,
                        "neg\tv{FS2}.{}, v{FS2}.{}",
                        arrangement(shape),
                        arrangement(shape)
                    )
                },
            );
        }
        let d = self.def_freg(result);
        let (opcode, name) = if op == V::Sshr {
            (Opcode::SSHL_asimdsame_only, "sshl")
        } else {
            (Opcode::USHL_asimdsame_only, "ushl")
        };
        self.native(
            opcode,
            [
                (FieldId::Q, 1),
                (FieldId::size, size),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, FS2.into()),
            ],
            |out| {
                write!(
                    out,
                    "{name}\tv{d}.{}, v{a}.{}, v{FS2}.{}",
                    arrangement(shape),
                    arrangement(shape),
                    arrangement(shape)
                )
            },
        );
        self.def_done(result, d);
    }

    pub(super) fn vector_minmax(&mut self, minmax: packed_minmax::MinMax, result: Option<Value>) {
        // Stage both reads before any destination write. Three-operand NEON
        // permits the destination to alias either input, including spill scratch.
        let a = self.use_freg(minmax.args[0], FS0);
        let b = self.use_freg(minmax.args[1], FS1);
        let d = self.def_freg(result);
        let (opcode, mnemonic) = minmax.kind.encoding();
        self.native(
            opcode,
            [
                (FieldId::Q, 1),
                (FieldId::size, 2),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, b.into()),
            ],
            |out| write!(out, "{mnemonic}\tv{d}.4s, v{a}.4s, v{b}.4s"),
        );
        self.def_done(result, d);
    }
    // Non-saturating ABS preserves the IR wrapping result for i64::MIN.
    pub(super) fn vector_absolute(&mut self, value: Value, result: Option<Value>) {
        let a = self.use_freg(value, FS0);
        let d = self.def_freg(result);
        self.native(
            Opcode::ABS_asimdmisc_R,
            [
                (FieldId::Q, 1),
                (FieldId::size, 3),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
            ],
            |out| write!(out, "abs\tv{d}.2d, v{a}.2d"),
        );
        self.def_done(result, d);
    }
    pub(super) fn vector_unary(&mut self, op: UnaryOp, arg: Value, result: Option<Value>) {
        let shape = self.ty(arg).vector().unwrap();
        if op == UnaryOp::Popcnt && !matches!(shape, VectorType::I8x8 | VectorType::I8x16) {
            let a = self.use_freg(arg, FS0);
            let d = self.def_freg(result);
            self.native(
                Opcode::CNT_asimdmisc_R,
                [
                    (FieldId::Q, 1),
                    (FieldId::size, 0),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, a.into()),
                ],
                |out| write!(out, "cnt\tv{d}.16b, v{a}.16b"),
            );
            for size in 0..shape.lane_type().bytes().unwrap().trailing_zeros() {
                let source = ["16b", "8h", "4s"][size as usize];
                let dest = ["8h", "4s", "2d"][size as usize];
                self.native(
                    Opcode::UADDLP_asimdmisc_P,
                    [
                        (FieldId::Q, 1),
                        (FieldId::size, size),
                        (FieldId::Rd, d.into()),
                        (FieldId::Rn, d.into()),
                    ],
                    |out| write!(out, "uaddlp\tv{d}.{dest}, v{d}.{source}"),
                );
            }
            self.def_done(result, d);
            return;
        }
        let spec = unary(op, shape).expect("legalized NEON unary");
        let d = self.def_freg(result);
        let a = self.use_freg(arg, FS0);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Q, u32::from(shape.bytes() == 16)),
        ];
        let text = |out: &mut crate::text::Text| {
            write!(
                out,
                "{}\tv{d}.{}, v{a}.{}",
                spec.mnemonic, spec.arrangement, spec.arrangement
            )
        };
        if let Some(field) = spec.field {
            self.native(spec.opcode, [base[0], base[1], base[2], field], text);
        } else {
            self.native(spec.opcode, base, text);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_compare(&mut self, cond: IntCC, args: [Value; 2], result: Option<Value>) {
        if self.vector_compare_zero(cond, args, result) { return; }
        let (op, name, swap) = match cond {
            IntCC::Eq | IntCC::Ne => (Opcode::CMEQ_asimdsame_only, "cmeq", false),
            IntCC::Sgt => (Opcode::CMGT_asimdsame_only, "cmgt", false),
            IntCC::Slt => (Opcode::CMGT_asimdsame_only, "cmgt", true),
            IntCC::Sge => (Opcode::CMGE_asimdsame_only, "cmge", false),
            IntCC::Sle => (Opcode::CMGE_asimdsame_only, "cmge", true),
            IntCC::Ugt => (Opcode::CMHI_asimdsame_only, "cmhi", false),
            IntCC::Ult => (Opcode::CMHI_asimdsame_only, "cmhi", true),
            IntCC::Uge => (Opcode::CMHS_asimdsame_only, "cmhs", false),
            IntCC::Ule => (Opcode::CMHS_asimdsame_only, "cmhs", true),
        };
        let shape = self.ty(args[0]).vector().unwrap();
        let a = self.use_freg(args[usize::from(swap)], FS0);
        let b = self.use_freg(args[usize::from(!swap)], FS1);
        let d = self.def_freg(result);
        self.native(
            op,
            [
                (FieldId::Q, 1),
                (
                    FieldId::size,
                    shape.lane_type().bytes().unwrap().trailing_zeros(),
                ),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, b.into()),
            ],
            |out| {
                write!(
                    out,
                    "{name}\tv{d}.{}, v{a}.{}, v{b}.{}",
                    arrangement(shape),
                    arrangement(shape),
                    arrangement(shape)
                )
            },
        );
        if cond == IntCC::Ne {
            self.native(
                Opcode::NOT_asimdmisc_R,
                [
                    (FieldId::Q, 1),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, d.into()),
                ],
                |out| write!(out, "mvn\tv{d}.16b, v{d}.16b"),
            );
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_select(&mut self, args: [Value; 3], result: Option<Value>) {
        let shape = self.ty(args[0]).vector().unwrap();
        let canonical = crate::vector::canonical_mask(self.func, args[0]);
        let mask = self.use_freg(args[0], FS0);
        self.move_vector(FS0, mask);
        if !canonical {
            self.native(
                Opcode::EOR_asimdsame_only,
                [
                    (FieldId::Q, 1),
                    (FieldId::Rd, FS1.into()),
                    (FieldId::Rn, FS1.into()),
                    (FieldId::Rm, FS1.into()),
                ],
                |out| write!(out, "eor\tv{FS1}.16b, v{FS1}.16b, v{FS1}.16b"),
            );
            self.native(
                Opcode::CMEQ_asimdsame_only,
                [
                    (FieldId::Q, 1),
                    (
                        FieldId::size,
                        shape.lane_type().bytes().unwrap().trailing_zeros(),
                    ),
                    (FieldId::Rd, FS0.into()),
                    (FieldId::Rn, FS0.into()),
                    (FieldId::Rm, FS1.into()),
                ],
                |out| {
                    write!(
                        out,
                        "cmeq\tv{FS0}.{}, v{FS0}.{}, v{FS1}.{}",
                        arrangement(shape),
                        arrangement(shape),
                        arrangement(shape)
                    )
                },
            );
        }
        // The generic path has an inverted mask (mask == 0).
        let a = self.use_freg(args[1], FS1);
        let b = self.use_freg(args[2], FS2);
        let (a, b) = if canonical { (a, b) } else { (b, a) };
        self.native(
            Opcode::BSL_asimdsame_only,
            [
                (FieldId::Q, 1),
                (FieldId::Rd, FS0.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, b.into()),
            ],
            |out| write!(out, "bsl\tv{FS0}.16b, v{a}.16b, v{b}.16b"),
        );
        let d = self.def_freg(result);
        self.move_vector(d, FS0);
        self.def_done(result, d);
    }
    pub(super) fn move_vector(&mut self, d: u8, s: u8) {
        if d == s {
            return;
        }
        self.native(
            Opcode::ORR_asimdsame_only,
            [
                (FieldId::Q, 1),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, s.into()),
                (FieldId::Rm, s.into()),
            ],
            |out| write!(out, "mov\tv{d}.16b, v{s}.16b"),
        );
    }
    pub(super) fn vector_spill(&mut self, store: bool, register: u8, slot: u32, bytes: u32) {
        let offset = self.spill_base + 8 * slot as usize;
        // Unaligned Q spills use unscaled addressing when possible; large
        // offsets need an address in x17. Scalar slots retain their old layout.
        let at = if bytes == 16 && !offset.is_multiple_of(16) && offset > 255 {
            self.address(Loc::Reg(S1), self.base, offset);
            Address::base(Reg::x(S1))
        } else {
            self.slot(slot)
        };
        self.mach(
            if store { "str" } else { "ldr" },
            [if bytes == 16 { Reg::q(register) } else { Reg::d(register) }.into(), at.into()],
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct WideningAdd {
    accumulator: Value,
    source: Value,
    signed: bool,
}

fn preserves_fusion_boundaries(
    func: &Function,
    selection: &crate::select::Selection,
    producer: Inst,
    consumer: Inst,
) -> bool {
    !selection.has_constraint(producer)
        && !selection.has_constraint(consumer)
        && !matches!(
            (func.inst_loc(producer), func.inst_loc(consumer)),
            (Some(a), Some(b)) if a.file != b.file || a.line != b.line
        )
}

/// SADDW/UADDW have exactly the same low-lane extension followed by modular
/// addition as this adjacent pair. Only a single doubling is covered: chained
/// extensions and shared widened values keep their original materialization.
pub(super) fn select_widening_adds(
    func: &Function,
    selection: &mut crate::select::Selection,
) -> HashMap<Inst, WideningAdd> {
    // Every pair is rooted at a low-lane widening.
    if !func.layout_insts().any(|(_, i)| {
        matches!(
            func.inst_data(i),
            InstData::Convert {
                op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
                ..
            }
        )
    }) {
        return HashMap::default();
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut selected = HashMap::default();
    let mut previous = None;
    for (block, inst) in func.layout_insts() {
        if let Some((producer_block, producer)) = previous
            && producer_block == block
            && let InstData::Convert {
                op: op @ (ConvertOp::SwidenLow | ConvertOp::UwidenLow),
                ty,
                arg,
            } = *func.inst_data(producer)
            && let InstData::VectorBinary { op: V::Iadd, args } = *func.inst_data(inst)
            && !selection.covered(producer)
            && !selection.covered(inst)
            && preserves_fusion_boundaries(func, selection, producer, inst)
            && uses[func.first_result(producer).index()] == 1
            && func
                .value_type(func.resolve(arg))
                .vector()
                .unwrap()
                .lane_type()
                .bytes()
                .unwrap()
                * 2
                == ty.vector().unwrap().lane_type().bytes().unwrap()
        {
            let value = func.first_result(producer);
            let args = args.map(|v| func.resolve(v));
            let accumulator = if args[0] == value {
                Some(args[1])
            } else if args[1] == value {
                Some(args[0])
            } else {
                None
            };
            if let Some(accumulator) = accumulator {
                let source = func.resolve(arg);
                selection.cover(producer);
                selection.set_operands(inst, vec![accumulator, source]);
                selected.insert(
                    inst,
                    WideningAdd {
                        accumulator,
                        source,
                        signed: op == ConvertOp::SwidenLow,
                    },
                );
            }
        }
        previous = Some((block, inst));
    }
    selected
}

/// Preserve the complete splat, including lanes unused by widening. Moving an
/// ordinary load across its immediately following pure splat preserves the
/// first fault and memory order. Shared scalar results remain materialized.
pub(super) fn select_replicate_loads(
    func: &Function,
    selection: &mut crate::select::Selection,
) -> HashMap<Inst, Value> {
    // Every pair is rooted at a vector splat.
    if !func
        .layout_insts()
        .any(|(_, i)| matches!(func.inst_data(i), InstData::VectorSplat { .. }))
    {
        return HashMap::default();
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut selected = HashMap::default();
    let mut previous = None;
    for (block, inst) in func.layout_insts() {
        if let Some((load_block, load)) = previous
            && load_block == block
            && let InstData::VectorSplat { ty, arg } = *func.inst_data(inst)
            && let InstData::Load {
                ty: scalar,
                addr,
                flags,
            } = *func.inst_data(load)
            && !selection.covered(load)
            && !selection.covered(inst)
            && preserves_fusion_boundaries(func, selection, load, inst)
            && !flags.volatile
            && scalar.is_int()
            && ty.vector().is_some_and(|s| s.lane_type() == scalar)
            && func.resolve(arg) == func.first_result(load)
            && uses[func.resolve(arg).index()] == 1
            && !selection.addresses.contains_key(&func.resolve(addr))
            && !selection
                .indexed_addresses
                .contains_key(&func.resolve(addr))
            && !matches!(func.value_def(func.resolve(addr)), keel_ir::ValueDef::Result(i, _) if selection.covered(i))
        {
            selection.cover(load);
            selection.set_operands(inst, vec![func.resolve(addr)]);
            selected.insert(inst, func.resolve(addr));
        }
        previous = Some((block, inst));
    }
    selected
}
