//! Explicit architectural SIMD intrinsics. Never fused, floated or scalarized.
use super::*;
use keel_ir::NeonIntrinsic as N;

impl FunctionCompiler<'_> {
    pub(super) fn neon(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        if op.is_integer_misc() {
            self.neon_integer_misc(op, ty, args, result);
            return;
        }
        if op.is_status() {
            let fpsr = matches!(op, N::ReadFpsr | N::SwapFpsr);
            let name = if fpsr { "fpsr" } else { "fpcr" };
            if let Some(&arg) = args.first() {
                let source = self.use_reg(arg, S1);
                self.move_gp(S1, source, true);
            }
            let d = self.def_reg(result);
            let fields = [
                (FieldId::CRm, 4),
                (FieldId::CRn, 4),
                (FieldId::o0, 1),
                (FieldId::op1, 3),
                (FieldId::op2, u32::from(fpsr)),
                (FieldId::Rt, d.into()),
            ];
            self.native(Opcode::MRS_RS_systemmove, fields, |out| {
                write!(out, "mrs\tx{d}, {name}")
            });
            if !args.is_empty() {
                let mut fields = fields;
                fields[5] = (FieldId::Rt, S1.into());
                self.native(Opcode::MSR_SR_systemmove, fields, |out| {
                    write!(out, "msr\t{name}, x{S1}")
                });
            }
            self.def_done(result, d);
            return;
        }
        if self.neon_float_zero(op, ty, args, result) {
            return;
        }
        let scalar_ty = ty.vector().map_or(ty, |v| v.lane_type());
        if op.accepts_scalar(scalar_ty) && (!ty.is_vector() || ty.vector().unwrap().lanes() == 1) {
            self.neon_scalar(op, ty, args, result);
            return;
        }
        if op.is_table() {
            self.neon_table(op, args, result);
            return;
        }
        if op.is_conversion() {
            self.neon_convert(op, ty, args, result);
            return;
        }
        if op.is_reduction() {
            self.neon_reduce(op, ty, args, result);
            return;
        }
        if op.immediate_range(ty).is_some() {
            self.neon_immediate(op, ty, args, result);
            return;
        }
        if op.changes_width() {
            self.neon_width(op, ty, args, result);
            return;
        }
        let shape = ty.vector().unwrap();
        let arrangement = full_arrangement(ty);
        let float = shape.lane_type().is_float() || op.is_float_comparison();
        let field = if matches!(op, N::Urecpe | N::Ursqrte) {
            (FieldId::sz, 0)
        } else if float {
            (FieldId::sz, u32::from(shape.lane_type().bytes() == Some(8)))
        } else {
            (
                FieldId::size,
                shape.lane_type().bytes().unwrap().trailing_zeros(),
            )
        };
        let (opcode, name) = match op {
            N::Sshl => (Opcode::SSHL_asimdsame_only, "sshl"),
            N::Ushl => (Opcode::USHL_asimdsame_only, "ushl"),
            N::Scvtf
            | N::Ucvtf
            | N::Fcvtas
            | N::Fcvtau
            | N::Fcvtms
            | N::Fcvtmu
            | N::Fcvtns
            | N::Fcvtnu
            | N::Fcvtps
            | N::Fcvtpu
            | N::Fcvtzs
            | N::Fcvtzu
            | N::ScvtfFixed
            | N::UcvtfFixed
            | N::FcvtzsFixed
            | N::FcvtzuFixed
            | N::Fcvtl
            | N::Fcvtn
            | N::Fcvtxn
            | N::Fcvthl
            | N::Fcvthn
            | N::Fminv
            | N::Fmaxv
            | N::Fminnmv
            | N::Fmaxnmv
            | N::Saddlv
            | N::Uaddlv => unreachable!("conversion/reduction dispatched separately"),
            N::Fabs => (Opcode::FABS_asimdmisc_R, "fabs"),
            N::Fneg => (Opcode::FNEG_asimdmisc_R, "fneg"),
            N::Fsqrt => (Opcode::FSQRT_asimdmisc_R, "fsqrt"),
            N::Frintn => (Opcode::FRINTN_asimdmisc_R, "frintn"),
            N::Frintp => (Opcode::FRINTP_asimdmisc_R, "frintp"),
            N::Frintm => (Opcode::FRINTM_asimdmisc_R, "frintm"),
            N::Frintz => (Opcode::FRINTZ_asimdmisc_R, "frintz"),
            N::Fadd => (Opcode::FADD_asimdsame_only, "fadd"),
            N::Fsub => (Opcode::FSUB_asimdsame_only, "fsub"),
            N::Fmul => (Opcode::FMUL_asimdsame_only, "fmul"),
            N::Fdiv => (Opcode::FDIV_asimdsame_only, "fdiv"),
            N::Fmin => (Opcode::FMIN_asimdsame_only, "fmin"),
            N::Fmax => (Opcode::FMAX_asimdsame_only, "fmax"),
            N::Faddp => (Opcode::FADDP_asimdsame_only, "faddp"),
            N::Fminp => (Opcode::FMINP_asimdsame_only, "fminp"),
            N::Fmaxp => (Opcode::FMAXP_asimdsame_only, "fmaxp"),
            N::Fminnmp => (Opcode::FMINNMP_asimdsame_only, "fminnmp"),
            N::Fmaxnmp => (Opcode::FMAXNMP_asimdsame_only, "fmaxnmp"),
            N::Fcmeq => (Opcode::FCMEQ_asimdsame_only, "fcmeq"),
            N::Fcmge => (Opcode::FCMGE_asimdsame_only, "fcmge"),
            N::Fcmgt => (Opcode::FCMGT_asimdsame_only, "fcmgt"),
            N::Facge => (Opcode::FACGE_asimdsame_only, "facge"),
            N::Facgt => (Opcode::FACGT_asimdsame_only, "facgt"),
            N::Tbl1 | N::Tbl2 | N::Tbl3 | N::Tbl4 | N::Tbx1 | N::Tbx2 | N::Tbx3 | N::Tbx4 => {
                unreachable!("table dispatched separately")
            }
            N::Sqadd => (Opcode::SQADD_asimdsame_only, "sqadd"),
            N::Uqadd => (Opcode::UQADD_asimdsame_only, "uqadd"),
            N::Sqsub => (Opcode::SQSUB_asimdsame_only, "sqsub"),
            N::Uqsub => (Opcode::UQSUB_asimdsame_only, "uqsub"),
            N::Suqadd => (Opcode::SUQADD_asimdmisc_R, "suqadd"),
            N::Usqadd => (Opcode::USQADD_asimdmisc_R, "usqadd"),
            N::Sqabs => (Opcode::SQABS_asimdmisc_R, "sqabs"),
            N::Sqneg => (Opcode::SQNEG_asimdmisc_R, "sqneg"),
            N::Sqshl => (Opcode::SQSHL_asimdsame_only, "sqshl"),
            N::Uqshl => (Opcode::UQSHL_asimdsame_only, "uqshl"),
            N::Sqrshl => (Opcode::SQRSHL_asimdsame_only, "sqrshl"),
            N::Uqrshl => (Opcode::UQRSHL_asimdsame_only, "uqrshl"),
            N::Srshl => (Opcode::SRSHL_asimdsame_only, "srshl"),
            N::Urshl => (Opcode::URSHL_asimdsame_only, "urshl"),
            N::Sqdmulh => (Opcode::SQDMULH_asimdsame_only, "sqdmulh"),
            N::Sqrdmulh => (Opcode::SQRDMULH_asimdsame_only, "sqrdmulh"),
            N::Fmla => (Opcode::FMLA_asimdsame_only, "fmla"),
            N::Fmls => (Opcode::FMLS_asimdsame_only, "fmls"),
            N::Fmulx => (Opcode::FMULX_asimdsame_only, "fmulx"),
            N::Fabd => (Opcode::FABD_asimdsame_only, "fabd"),
            N::Frecpe => (Opcode::FRECPE_asimdmisc_R, "frecpe"),
            N::Frsqrte => (Opcode::FRSQRTE_asimdmisc_R, "frsqrte"),
            N::Frecps => (Opcode::FRECPS_asimdsame_only, "frecps"),
            N::Frsqrts => (Opcode::FRSQRTS_asimdsame_only, "frsqrts"),
            N::Urecpe => (Opcode::URECPE_asimdmisc_R, "urecpe"),
            N::Ursqrte => (Opcode::URSQRTE_asimdmisc_R, "ursqrte"),
            N::Frinta => (Opcode::FRINTA_asimdmisc_R, "frinta"),
            N::Frintx => (Opcode::FRINTX_asimdmisc_R, "frintx"),
            N::Frinti => (Opcode::FRINTI_asimdmisc_R, "frinti"),
            N::Fminnm => (Opcode::FMINNM_asimdsame_only, "fminnm"),
            N::Fmaxnm => (Opcode::FMAXNM_asimdsame_only, "fmaxnm"),
            N::Pmul => (Opcode::PMUL_asimdsame_only, "pmul"),
            N::Shrn
            | N::Rshrn
            | N::Sqshrn
            | N::Uqshrn
            | N::Sqrshrn
            | N::Uqrshrn
            | N::Sqshrun
            | N::Sqrshrun
            | N::SshrImm
            | N::UshrImm
            | N::SrshrImm
            | N::UrshrImm
            | N::Sri
            | N::ShlImm
            | N::SqshlImm
            | N::UqshlImm
            | N::Sqshlu
            | N::Sli
            | N::Ssra
            | N::Usra
            | N::Srsra
            | N::Ursra => unreachable!("immediate intrinsic dispatched separately"),
            N::Saddl
            | N::Uaddl
            | N::Ssubl
            | N::Usubl
            | N::Smull
            | N::Umull
            | N::Sabdl
            | N::Uabdl
            | N::Sqdmull
            | N::Pmull
            | N::Smlal
            | N::Umlal
            | N::Smlsl
            | N::Umlsl
            | N::Sabal
            | N::Uabal
            | N::Sqdmlal
            | N::Sqdmlsl
            | N::Saddw
            | N::Uaddw
            | N::Ssubw
            | N::Usubw
            | N::Sqxtn
            | N::Uqxtn
            | N::Sqxtun
            | N::Addhn
            | N::Raddhn
            | N::Subhn
            | N::Rsubhn => unreachable!("width operation dispatched separately"),
            _ => unreachable!("status and structured memory handled separately"),
        };
        // Explicit accumulator forms always compute in reserved FS0. Capture
        // every input first; none of these instructions may overwrite a live
        // allocated input merely because an allocator reuses the destination.
        let a = self.use_freg(args[0], FS0);
        let b = args.get(1).map(|&v| self.use_freg(v, FS1));
        let c = args.get(2).map(|&v| self.use_freg(v, FS2));
        let d = self.def_freg(result);
        let accumulating = matches!(op, N::Fmla | N::Fmls | N::Suqadd | N::Usqadd);
        let work = if accumulating {
            self.move_vector(FS0, a);
            FS0
        } else {
            d
        };
        if let Some(c) = c {
            let b = b.unwrap();
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(shape.bytes() == 16)),
                    field,
                    (FieldId::Rd, work.into()),
                    (FieldId::Rn, b.into()),
                    (FieldId::Rm, c.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{work}.{arrangement}, v{b}.{arrangement}, v{c}.{arrangement}"
                    )
                },
            );
        } else if let Some(b) = b.filter(|_| !accumulating) {
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(shape.bytes() == 16)),
                    field,
                    (FieldId::Rd, work.into()),
                    (FieldId::Rn, a.into()),
                    (FieldId::Rm, b.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{work}.{arrangement}, v{a}.{arrangement}, v{b}.{arrangement}"
                    )
                },
            );
        } else {
            let source = if accumulating { b.unwrap() } else { a };
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(shape.bytes() == 16)),
                    field,
                    (FieldId::Rd, work.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{work}.{arrangement}, v{source}.{arrangement}"
                    )
                },
            );
        }
        self.move_vector(d, work);
        self.def_done(result, d);
    }
}

fn full_arrangement(ty: Type) -> &'static str {
    match ty.vector().unwrap() {
        keel_ir::VectorType::I8x8 => "8b",
        keel_ir::VectorType::I16x4 => "4h",
        keel_ir::VectorType::I32x2 | keel_ir::VectorType::F32x2 => "2s",
        keel_ir::VectorType::I64x1 | keel_ir::VectorType::F64x1 => "1d",
        keel_ir::VectorType::I8x16 => "16b",
        keel_ir::VectorType::I16x8 => "8h",
        keel_ir::VectorType::I32x4 | keel_ir::VectorType::F32x4 => "4s",
        keel_ir::VectorType::I64x2 | keel_ir::VectorType::F64x2 => "2d",
        _ => unreachable!("SIMD width validated before AArch64 lowering"),
    }
}
fn half_arrangement(ty: Type) -> &'static str {
    match ty.vector().unwrap() {
        keel_ir::VectorType::I8x8 | keel_ir::VectorType::I8x16 => "8b",
        keel_ir::VectorType::I16x4 | keel_ir::VectorType::I16x8 => "4h",
        keel_ir::VectorType::I32x2 | keel_ir::VectorType::I32x4 => "2s",
        _ => unreachable!(),
    }
}
impl FunctionCompiler<'_> {
    fn neon_width(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        let (opcode, name) = match op {
            N::Saddl => (Opcode::SADDL_asimddiff_L, "saddl"),
            N::Uaddl => (Opcode::UADDL_asimddiff_L, "uaddl"),
            N::Ssubl => (Opcode::SSUBL_asimddiff_L, "ssubl"),
            N::Usubl => (Opcode::USUBL_asimddiff_L, "usubl"),
            N::Smull => (Opcode::SMULL_asimddiff_L, "smull"),
            N::Umull => (Opcode::UMULL_asimddiff_L, "umull"),
            N::Sabdl => (Opcode::SABDL_asimddiff_L, "sabdl"),
            N::Uabdl => (Opcode::UABDL_asimddiff_L, "uabdl"),
            N::Smlal => (Opcode::SMLAL_asimddiff_L, "smlal"),
            N::Umlal => (Opcode::UMLAL_asimddiff_L, "umlal"),
            N::Smlsl => (Opcode::SMLSL_asimddiff_L, "smlsl"),
            N::Umlsl => (Opcode::UMLSL_asimddiff_L, "umlsl"),
            N::Sabal => (Opcode::SABAL_asimddiff_L, "sabal"),
            N::Uabal => (Opcode::UABAL_asimddiff_L, "uabal"),
            N::Sqdmull => (Opcode::SQDMULL_asimddiff_L, "sqdmull"),
            N::Sqdmlal => (Opcode::SQDMLAL_asimddiff_L, "sqdmlal"),
            N::Sqdmlsl => (Opcode::SQDMLSL_asimddiff_L, "sqdmlsl"),
            N::Pmull => (Opcode::PMULL_asimddiff_L, "pmull"),
            N::Saddw => (Opcode::SADDW_asimddiff_W, "saddw"),
            N::Uaddw => (Opcode::UADDW_asimddiff_W, "uaddw"),
            N::Ssubw => (Opcode::SSUBW_asimddiff_W, "ssubw"),
            N::Usubw => (Opcode::USUBW_asimddiff_W, "usubw"),
            N::Sqxtn => (Opcode::SQXTN_asimdmisc_N, "sqxtn"),
            N::Uqxtn => (Opcode::UQXTN_asimdmisc_N, "uqxtn"),
            N::Sqxtun => (Opcode::SQXTUN_asimdmisc_N, "sqxtun"),
            N::Addhn => (Opcode::ADDHN_asimddiff_N, "addhn"),
            N::Raddhn => (Opcode::RADDHN_asimddiff_N, "raddhn"),
            N::Subhn => (Opcode::SUBHN_asimddiff_N, "subhn"),
            N::Rsubhn => (Opcode::RSUBHN_asimddiff_N, "rsubhn"),
            _ => unreachable!("verified width intrinsic"),
        };
        let narrowing = matches!(
            op,
            N::Sqxtn | N::Uqxtn | N::Sqxtun | N::Addhn | N::Raddhn | N::Subhn | N::Rsubhn
        );
        let wide = matches!(op, N::Saddw | N::Uaddw | N::Ssubw | N::Usubw);
        let accumulator = args.len() == 3;
        let a = self.use_freg(args[0], FS0);
        let b = args.get(1).map(|&v| self.use_freg(v, FS1));
        let c = args.get(2).map(|&v| self.use_freg(v, FS2));
        let d = self.def_freg(result);
        let work = if accumulator {
            self.move_vector(FS0, a);
            FS0
        } else {
            d
        };
        let size = if narrowing {
            ty.vector()
                .unwrap()
                .lane_type()
                .bytes()
                .unwrap()
                .trailing_zeros()
        } else {
            ty.vector()
                .unwrap()
                .lane_type()
                .bytes()
                .unwrap()
                .trailing_zeros()
                - 1
        };
        let dest_arr = if narrowing {
            half_arrangement(ty)
        } else {
            full_arrangement(ty)
        };
        let source_ty = if accumulator {
            self.ty(args[1])
        } else {
            self.ty(args[0])
        };
        let source_arr = if narrowing || wide {
            full_arrangement(source_ty)
        } else {
            half_arrangement(source_ty)
        };
        let left = if accumulator { b.unwrap() } else { a };
        let right = if accumulator { c } else { b };
        if let Some(right) = right {
            let right_ty = if accumulator {
                self.ty(args[2])
            } else {
                self.ty(args[1])
            };
            let right_arr = if narrowing {
                full_arrangement(right_ty)
            } else {
                half_arrangement(right_ty)
            };
            self.native(
                opcode,
                [
                    (FieldId::Q, 0),
                    (FieldId::size, size),
                    (FieldId::Rd, work.into()),
                    (FieldId::Rn, left.into()),
                    (FieldId::Rm, right.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{work}.{dest_arr}, v{left}.{source_arr}, v{right}.{right_arr}"
                    )
                },
            );
        } else {
            self.native(
                opcode,
                [
                    (FieldId::Q, 0),
                    (FieldId::size, size),
                    (FieldId::Rd, work.into()),
                    (FieldId::Rn, left.into()),
                ],
                |out| write!(out, "{name}\tv{work}.{dest_arr}, v{left}.{source_arr}"),
            );
        }
        self.move_vector(d, work);
        self.def_done(result, d);
    }
}

impl FunctionCompiler<'_> {
    fn neon_immediate(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        let immediate = self.func.resolve(*args.last().unwrap());
        let ValueDef::Result(inst, _) = self.func.value_def(immediate) else {
            unreachable!("verified immediate");
        };
        let InstData::Iconst { imm, .. } = *self.func.inst_data(inst) else {
            unreachable!("verified immediate");
        };
        let (opcode, name) = match op {
            N::Shrn => (Opcode::SHRN_asimdshf_N, "shrn"),
            N::Rshrn => (Opcode::RSHRN_asimdshf_N, "rshrn"),
            N::Sqshrn => (Opcode::SQSHRN_asimdshf_N, "sqshrn"),
            N::Uqshrn => (Opcode::UQSHRN_asimdshf_N, "uqshrn"),
            N::Sqrshrn => (Opcode::SQRSHRN_asimdshf_N, "sqrshrn"),
            N::Uqrshrn => (Opcode::UQRSHRN_asimdshf_N, "uqrshrn"),
            N::Sqshrun => (Opcode::SQSHRUN_asimdshf_N, "sqshrun"),
            N::Sqrshrun => (Opcode::SQRSHRUN_asimdshf_N, "sqrshrun"),
            N::SshrImm => (Opcode::SSHR_asimdshf_R, "sshr"),
            N::UshrImm => (Opcode::USHR_asimdshf_R, "ushr"),
            N::SrshrImm => (Opcode::SRSHR_asimdshf_R, "srshr"),
            N::UrshrImm => (Opcode::URSHR_asimdshf_R, "urshr"),
            N::ShlImm => (Opcode::SHL_asimdshf_R, "shl"),
            N::SqshlImm => (Opcode::SQSHL_asimdshf_R, "sqshl"),
            N::UqshlImm => (Opcode::UQSHL_asimdshf_R, "uqshl"),
            N::Sqshlu => (Opcode::SQSHLU_asimdshf_R, "sqshlu"),
            N::Sli => (Opcode::SLI_asimdshf_R, "sli"),
            N::Sri => (Opcode::SRI_asimdshf_R, "sri"),
            N::Ssra => (Opcode::SSRA_asimdshf_R, "ssra"),
            N::Usra => (Opcode::USRA_asimdshf_R, "usra"),
            N::Srsra => (Opcode::SRSRA_asimdshf_R, "srsra"),
            N::Ursra => (Opcode::URSRA_asimdshf_R, "ursra"),
            _ => unreachable!("verified immediate intrinsic"),
        };
        let narrow = op.changes_width();
        let accumulating = matches!(
            op,
            N::Sli | N::Sri | N::Ssra | N::Usra | N::Srsra | N::Ursra
        );
        let first = self.use_freg(args[0], FS0);
        let source = if accumulating {
            self.use_freg(args[1], FS1)
        } else {
            first
        };
        let d = self.def_freg(result);
        let work = if accumulating {
            self.move_vector(FS0, first);
            FS0
        } else {
            d
        };
        let width = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        let left = matches!(
            op,
            N::ShlImm | N::SqshlImm | N::UqshlImm | N::Sqshlu | N::Sli
        );
        let encoded = if left {
            width + imm as u32
        } else {
            width * 2 - imm as u32
        };
        let dest_arr = if narrow {
            half_arrangement(ty)
        } else {
            full_arrangement(ty)
        };
        let source_arr = full_arrangement(self.ty(if accumulating { args[1] } else { args[0] }));
        self.native(
            opcode,
            [
                (FieldId::Q, u32::from(!narrow && ty.bytes() == Some(16))),
                (FieldId::immh, encoded >> 3),
                (FieldId::immb, encoded & 7),
                (FieldId::Rd, work.into()),
                (FieldId::Rn, source.into()),
            ],
            |out| {
                write!(
                    out,
                    "{name}\tv{work}.{dest_arr}, v{source}.{source_arr}, #{imm}"
                )
            },
        );
        self.move_vector(d, work);
        self.def_done(result, d);
    }
}

impl FunctionCompiler<'_> {
    fn neon_convert(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        let source = self.use_freg(args[0], FS0);
        let d = self.def_freg(result);
        let source_arr = full_arrangement(self.ty(args[0]));
        if matches!(op, N::Fcvtl | N::Fcvtn | N::Fcvtxn | N::Fcvthl | N::Fcvthn) {
            let (opcode, name, dest_arr, source_arr, sz) = match op {
                N::Fcvtl => (Opcode::FCVTL_asimdmisc_L, "fcvtl", "2d", "2s", Some(1)),
                N::Fcvtn => (Opcode::FCVTN_asimdmisc_N, "fcvtn", "2s", "2d", Some(1)),
                N::Fcvtxn => (Opcode::FCVTXN_asimdmisc_N, "fcvtxn", "2s", "2d", None),
                N::Fcvthl => (Opcode::FCVTL_asimdmisc_L, "fcvtl", "4s", "4h", Some(0)),
                N::Fcvthn => (Opcode::FCVTN_asimdmisc_N, "fcvtn", "4h", "4s", Some(0)),
                _ => unreachable!(),
            };
            let base = [
                (FieldId::Q, 0),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, source.into()),
            ];
            let text = |out: &mut crate::text::Text| {
                write!(out, "{name}\tv{d}.{dest_arr}, v{source}.{source_arr}")
            };
            if let Some(sz) = sz {
                self.native(opcode, [base[0], base[1], base[2], (FieldId::sz, sz)], text);
            } else {
                self.native(opcode, base, text);
            }
        } else if args.len() == 2 {
            let count = self.func.resolve(args[1]);
            let ValueDef::Result(inst, _) = self.func.value_def(count) else {
                unreachable!()
            };
            let InstData::Iconst { imm, .. } = *self.func.inst_data(inst) else {
                unreachable!()
            };
            let (opcode, name) = match op {
                N::ScvtfFixed => (Opcode::SCVTF_asimdshf_C, "scvtf"),
                N::UcvtfFixed => (Opcode::UCVTF_asimdshf_C, "ucvtf"),
                N::FcvtzsFixed => (Opcode::FCVTZS_asimdshf_C, "fcvtzs"),
                N::FcvtzuFixed => (Opcode::FCVTZU_asimdshf_C, "fcvtzu"),
                _ => unreachable!(),
            };
            let width = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
            let encoded = width * 2 - imm as u32;
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(ty.bytes() == Some(16))),
                    (FieldId::immh, encoded >> 3),
                    (FieldId::immb, encoded & 7),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{d}.{}, v{source}.{}, #{imm}",
                        full_arrangement(ty),
                        source_arr
                    )
                },
            );
        } else {
            let (opcode, name) = match op {
                N::Scvtf => (Opcode::SCVTF_asimdmisc_R, "scvtf"),
                N::Ucvtf => (Opcode::UCVTF_asimdmisc_R, "ucvtf"),
                N::Fcvtas => (Opcode::FCVTAS_asimdmisc_R, "fcvtas"),
                N::Fcvtau => (Opcode::FCVTAU_asimdmisc_R, "fcvtau"),
                N::Fcvtms => (Opcode::FCVTMS_asimdmisc_R, "fcvtms"),
                N::Fcvtmu => (Opcode::FCVTMU_asimdmisc_R, "fcvtmu"),
                N::Fcvtns => (Opcode::FCVTNS_asimdmisc_R, "fcvtns"),
                N::Fcvtnu => (Opcode::FCVTNU_asimdmisc_R, "fcvtnu"),
                N::Fcvtps => (Opcode::FCVTPS_asimdmisc_R, "fcvtps"),
                N::Fcvtpu => (Opcode::FCVTPU_asimdmisc_R, "fcvtpu"),
                N::Fcvtzs => (Opcode::FCVTZS_asimdmisc_R, "fcvtzs"),
                N::Fcvtzu => (Opcode::FCVTZU_asimdmisc_R, "fcvtzu"),
                _ => unreachable!(),
            };
            let sz = u32::from(ty.vector().unwrap().lane_type().bytes() == Some(8));
            self.native(
                opcode,
                [
                    (FieldId::Q, u32::from(ty.bytes() == Some(16))),
                    (FieldId::sz, sz),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{d}.{}, v{source}.{}",
                        full_arrangement(ty),
                        source_arr
                    )
                },
            );
        }
        self.def_done(result, d);
    }
    fn neon_reduce(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        let source = self.use_freg(args[0], FS0);
        let source_arr = full_arrangement(self.ty(args[0]));
        if ty == Type::F32 {
            let (opcode, name) = match op {
                N::Fminv => (Opcode::FMINV_asimdall_only_SD, "fminv"),
                N::Fmaxv => (Opcode::FMAXV_asimdall_only_SD, "fmaxv"),
                N::Fminnmv => (Opcode::FMINNMV_asimdall_only_SD, "fminnmv"),
                N::Fmaxnmv => (Opcode::FMAXNMV_asimdall_only_SD, "fmaxnmv"),
                _ => unreachable!(),
            };
            let d = self.def_freg(result);
            self.native(
                opcode,
                [(FieldId::Rd, d.into()), (FieldId::Rn, source.into())],
                |out| write!(out, "{name}\ts{d}, v{source}.4s"),
            );
            self.def_done(result, d);
        } else {
            let (opcode, name) = if op == N::Saddlv {
                (Opcode::SADDLV_asimdall_only, "saddlv")
            } else {
                (Opcode::UADDLV_asimdall_only, "uaddlv")
            };
            let bytes = ty.bytes().unwrap();
            let element = match bytes {
                2 => "h",
                4 => "s",
                8 => "d",
                _ => unreachable!(),
            };
            self.native(
                opcode,
                [
                    (FieldId::Q, 1),
                    (FieldId::size, bytes.trailing_zeros() - 1),
                    (FieldId::Rd, FS2.into()),
                    (FieldId::Rn, source.into()),
                ],
                |out| write!(out, "{name}\t{element}{FS2}, v{source}.{}", source_arr),
            );
            let d = self.def_reg(result);
            let (opcode, gpr) = if bytes == 8 {
                (Opcode::UMOV_asimdins_X_x, "x")
            } else {
                (Opcode::UMOV_asimdins_W_w, "w")
            };
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
    }
}

impl FunctionCompiler<'_> {
    fn neon_table(&mut self, op: N, args: &[Value], result: Option<Value>) {
        let extension = matches!(op, N::Tbx1 | N::Tbx2 | N::Tbx3 | N::Tbx4);
        let start = usize::from(extension);
        let tables = args.len() - start - 1;
        if extension {
            let fallback = self.use_freg(args[0], FS0);
            self.move_vector(FS2, fallback);
        } else {
            self.native(
                Opcode::EOR_asimdsame_only,
                [
                    (FieldId::Q, 1),
                    (FieldId::Rd, FS2.into()),
                    (FieldId::Rn, FS2.into()),
                    (FieldId::Rm, FS2.into()),
                ],
                |out| write!(out, "eor\tv{FS2}.16b, v{FS2}.16b, v{FS2}.16b"),
            );
        }
        // Single-register TBX stages preserve every original table and index,
        // including when all are spilled or the destination aliases any input.
        // Subtraction wraps at 8 bits; indices outside each 16-byte window
        // cannot wrap into that window for the baseline maximum four tables.
        for i in 0..tables {
            let index = self.use_freg(*args.last().unwrap(), FS1);
            let index = if i == 0 {
                index
            } else {
                let offset = i * 16;
                self.mach("mov", [Reg::w(S0).into(), Arg::imm(offset as i64)]);
                self.native(
                    Opcode::DUP_asimdins_DR_r,
                    [
                        (FieldId::Q, 1),
                        (FieldId::imm5, 1),
                        (FieldId::Rd, FS0.into()),
                        (FieldId::Rn, S0.into()),
                    ],
                    |out| write!(out, "dup\tv{FS0}.16b, w{S0}"),
                );
                self.native(
                    Opcode::SUB_asimdsame_only,
                    [
                        (FieldId::Q, 1),
                        (FieldId::size, 0),
                        (FieldId::Rd, FS1.into()),
                        (FieldId::Rn, index.into()),
                        (FieldId::Rm, FS0.into()),
                    ],
                    |out| write!(out, "sub\tv{FS1}.16b, v{index}.16b, v{FS0}.16b"),
                );
                FS1
            };
            let table = self.use_freg(args[start + i], FS0);
            let (opcode, name) = if tables == 1 && !extension {
                (Opcode::TBL_asimdtbl_L1_1, "tbl")
            } else {
                (Opcode::TBX_asimdtbl_L1_1, "tbx")
            };
            let arr = full_arrangement(self.ty(*args.last().unwrap()));
            self.native(
                opcode,
                [
                    (
                        FieldId::Q,
                        u32::from(self.ty(*args.last().unwrap()).bytes() == Some(16)),
                    ),
                    (FieldId::Rd, FS2.into()),
                    (FieldId::Rn, table.into()),
                    (FieldId::Rm, index.into()),
                ],
                |out| {
                    write!(
                        out,
                        "{name}\tv{FS2}.{arr}, {{v{table}.16b}}, v{index}.{arr}"
                    )
                },
            );
        }
        let d = self.def_freg(result);
        self.move_vector(d, FS2);
        self.def_done(result, d);
    }
}
