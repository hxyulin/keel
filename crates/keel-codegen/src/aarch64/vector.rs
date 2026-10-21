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
        I8x16 => "16b",
        I16x8 => "8h",
        I32x4 | F32x4 => "4s",
        I64x2 | F64x2 => "2d",
    }
}
fn binary(op: V, shape: VectorType) -> Option<Packed> {
    let (opcode, mnemonic) = match op {
        V::Iadd => (Opcode::ADD_asimdsame_only, "add"),
        V::Isub => (Opcode::SUB_asimdsame_only, "sub"),
        V::Imul if shape != VectorType::I64x2 => (Opcode::MUL_asimdsame_only, "mul"),
        V::And => (Opcode::AND_asimdsame_only, "and"),
        V::Or => (Opcode::ORR_asimdsame_only, "orr"),
        V::Xor => (Opcode::EOR_asimdsame_only, "eor"),
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
        arrangement: if bitwise { "16b" } else { arrangement(shape) },
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
    let size = shape.lane_type().bytes().unwrap().trailing_zeros();
    let (opcode, mnemonic) = match op {
        UnaryOp::Ineg => (Opcode::NEG_asimdmisc_R, "neg"),
        UnaryOp::Bnot => (Opcode::NOT_asimdmisc_R, "mvn"),
        UnaryOp::Clz if shape != VectorType::I64x2 => (Opcode::CLZ_asimdmisc_R, "clz"),
        UnaryOp::Popcnt if shape == VectorType::I8x16 => (Opcode::CNT_asimdmisc_R, "cnt"),
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
        arrangement: if bytes { "16b" } else { arrangement(shape) },
        field,
    })
}
pub(super) fn lower(func: &Function) -> Cow<'_, Function> {
    crate::vector::lower(func, |f, data| match data {
        InstData::VectorShuffle { args, lanes } => {
            lanes.as_slice() == [1, 2]
                && matches!(
                    f.value_type(args[0]),
                    Type::Vector(VectorType::I64x2 | VectorType::F64x2)
                )
                && f.value_type(args[0]) == f.value_type(args[1])
        }
        InstData::VectorExtract { arg, .. } => {
            f.value_type(*arg) == Type::Vector(VectorType::I64x2)
        }
        InstData::VectorBinary { op, args } => {
            binary(*op, f.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary { op, arg } => {
            unary(*op, f.value_type(*arg).vector().unwrap()).is_some()
        }
        InstData::Convert {
            op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
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
    // EXT reads both operands before writing, including when d aliases either
    // input. Separate reserved spill scratches keep both captured sources live.
    pub(super) fn vector_shuffle_window(&mut self, args: [Value; 2], result: Option<Value>) {
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let d = self.def_freg(result);
        self.native(
            Opcode::EXT_asimdext_only,
            [
                (FieldId::Q, 1),
                (FieldId::imm4, 8),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
                (FieldId::Rm, b.into()),
            ],
            |out| write!(out, "ext\tv{d}.16b, v{a}.16b, v{b}.16b, #8"),
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
            let next = VectorType::ALL
                .into_iter()
                .find(|s| s.lane_type().is_int() && s.lane_type().bytes() == Some(bytes * 2))
                .unwrap();
            let (opcode, name) = if op == ConvertOp::SwidenLow {
                (Opcode::SSHLL_asimdshf_L, "sshll")
            } else {
                (Opcode::USHLL_asimdshf_L, "ushll")
            };
            let source_arrangement = match bytes {
                1 => "8b",
                2 => "4h",
                4 => "2s",
                _ => unreachable!(),
            };
            self.native(
                opcode,
                [
                    (FieldId::Q, 0),
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
    pub(super) fn vector_bitcast(&mut self, arg: Value, result: Option<Value>) {
        let source = self.use_freg(arg, FS0);
        let d = self.def_freg(result);
        self.move_vector(d, source);
        self.def_done(result, d);
    }

    pub(super) fn vector_extract(&mut self, arg: Value, lane: u8, result: Option<Value>) {
        let source = self.use_freg(arg, FS0);
        let d = self.def_reg(result);
        self.native(
            Opcode::UMOV_asimdins_X_x,
            [
                (FieldId::imm5, 8 | (u32::from(lane) << 4)),
                (FieldId::Rn, source.into()),
                (FieldId::Rd, d.into()),
            ],
            |out| write!(out, "umov\tx{d}, v{source}.d[{lane}]"),
        );
        self.def_done(result, d);
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
        let spec = binary(op, self.ty(args[0]).vector().unwrap()).expect("legalized NEON binary");
        let d = self.def_freg(result);
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Rm, u32::from(b)),
            (FieldId::Q, 1),
        ];
        let text = |out: &mut String| {
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
        let spec = unary(op, self.ty(arg).vector().unwrap()).expect("legalized NEON unary");
        let d = self.def_freg(result);
        let a = self.use_freg(arg, FS0);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Q, 1),
        ];
        let text = |out: &mut String| {
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
        let canonical = canonical_mask(self.func, args[0]);
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
    pub(super) fn vector_spill(&mut self, store: bool, register: u8, slot: u32) {
        let offset = self.spill_base + 8 * slot as usize;
        // Unaligned Q spills use unscaled addressing when possible; large
        // offsets need an address in x17. Scalar slots retain their old layout.
        let at = if !offset.is_multiple_of(16) && offset > 255 {
            self.address(Loc::Reg(S1), self.base, offset);
            Address::base(Reg::x(S1))
        } else {
            self.slot(slot)
        };
        self.mach(
            if store { "str" } else { "ldr" },
            [Reg::q(register).into(), at.into()],
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
    !selection
        .constraints()
        .keys()
        .any(|&(inst, _, _)| inst == producer || inst == consumer)
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
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut selected = HashMap::new();
    let mut previous = None;
    for (block, inst) in func.layout_insts() {
        if let Some((producer_block, producer)) = previous
            && producer_block == block
            && !selection.covered(producer)
            && !selection.covered(inst)
            && preserves_fusion_boundaries(func, selection, producer, inst)
            && let InstData::Convert {
                op: op @ (ConvertOp::SwidenLow | ConvertOp::UwidenLow),
                ty,
                arg,
            } = *func.inst_data(producer)
            && let InstData::VectorBinary { op: V::Iadd, args } = *func.inst_data(inst)
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
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut selected = HashMap::new();
    let mut previous = None;
    for (block, inst) in func.layout_insts() {
        if let Some((load_block, load)) = previous
            && load_block == block
            && !selection.covered(load)
            && !selection.covered(inst)
            && preserves_fusion_boundaries(func, selection, load, inst)
            && let InstData::VectorSplat { ty, arg } = *func.inst_data(inst)
            && let InstData::Load {
                ty: scalar,
                addr,
                flags,
            } = *func.inst_data(load)
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
