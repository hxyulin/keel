//! Scalar Advanced SIMD with explicit low-lane transport and state effects.
use super::*;
use keel_ir::NeonIntrinsic as N;
fn encoding(op: N, bytes: u32) -> (Opcode, &'static str, bool) {
    match op {
        N::Fmla => (Opcode::FMLA_asisdelem_R_SD, "fmla", true),
        N::Fmls => (Opcode::FMLS_asisdelem_R_SD, "fmls", true),
        N::Sqdmull => (Opcode::SQDMULL_asisddiff_only, "sqdmull", false),
        N::Sqdmlal => (Opcode::SQDMLAL_asisddiff_only, "sqdmlal", false),
        N::Sqdmlsl => (Opcode::SQDMLSL_asisddiff_only, "sqdmlsl", false),
        N::Fcvtxn => (Opcode::FCVTXN_asisdmisc_N, "fcvtxn", false),
        N::Sshl => (Opcode::SSHL_asisdsame_only, "sshl", true),
        N::Ushl => (Opcode::USHL_asisdsame_only, "ushl", true),
        N::Fcmeq => (Opcode::FCMEQ_asisdsame_only, "fcmeq", false),
        N::Fcmge => (Opcode::FCMGE_asisdsame_only, "fcmge", false),
        N::Fcmgt => (Opcode::FCMGT_asisdsame_only, "fcmgt", false),
        N::Facge => (Opcode::FACGE_asisdsame_only, "facge", false),
        N::Facgt => (Opcode::FACGT_asisdsame_only, "facgt", false),
        N::Sqadd => (Opcode::SQADD_asisdsame_only, "sqadd", false),
        N::Uqadd => (Opcode::UQADD_asisdsame_only, "uqadd", false),
        N::Sqsub => (Opcode::SQSUB_asisdsame_only, "sqsub", false),
        N::Uqsub => (Opcode::UQSUB_asisdsame_only, "uqsub", false),
        N::Suqadd => (Opcode::SUQADD_asisdmisc_R, "suqadd", false),
        N::Usqadd => (Opcode::USQADD_asisdmisc_R, "usqadd", false),
        N::Sqabs => (Opcode::SQABS_asisdmisc_R, "sqabs", false),
        N::Sqneg => (Opcode::SQNEG_asisdmisc_R, "sqneg", false),
        N::Sqshl => (Opcode::SQSHL_asisdsame_only, "sqshl", false),
        N::Uqshl => (Opcode::UQSHL_asisdsame_only, "uqshl", false),
        N::Sqrshl => (Opcode::SQRSHL_asisdsame_only, "sqrshl", false),
        N::Uqrshl => (Opcode::UQRSHL_asisdsame_only, "uqrshl", false),
        N::Srshl => (Opcode::SRSHL_asisdsame_only, "srshl", true),
        N::Urshl => (Opcode::URSHL_asisdsame_only, "urshl", true),
        N::Sqdmulh => (Opcode::SQDMULH_asisdsame_only, "sqdmulh", false),
        N::Sqrdmulh => (Opcode::SQRDMULH_asisdsame_only, "sqrdmulh", false),
        N::Fmulx => (Opcode::FMULX_asisdsame_only, "fmulx", false),
        N::Fabd => (Opcode::FABD_asisdsame_only, "fabd", false),
        N::Frecpx => (Opcode::FRECPX_asisdmisc_R, "frecpx", false),
        N::Frecpe => (Opcode::FRECPE_asisdmisc_R, "frecpe", false),
        N::Frsqrte => (Opcode::FRSQRTE_asisdmisc_R, "frsqrte", false),
        N::Frecps => (Opcode::FRECPS_asisdsame_only, "frecps", false),
        N::Frsqrts => (Opcode::FRSQRTS_asisdsame_only, "frsqrts", false),
        N::Sqxtn => (Opcode::SQXTN_asisdmisc_N, "sqxtn", false),
        N::Uqxtn => (Opcode::UQXTN_asisdmisc_N, "uqxtn", false),
        N::Sqxtun => (Opcode::SQXTUN_asisdmisc_N, "sqxtun", false),
        N::Sqshrn => (Opcode::SQSHRN_asisdshf_N, "sqshrn", false),
        N::Uqshrn => (Opcode::UQSHRN_asisdshf_N, "uqshrn", false),
        N::Sqrshrn => (Opcode::SQRSHRN_asisdshf_N, "sqrshrn", false),
        N::Uqrshrn => (Opcode::UQRSHRN_asisdshf_N, "uqrshrn", false),
        N::Sqshrun => (Opcode::SQSHRUN_asisdshf_N, "sqshrun", false),
        N::Sqrshrun => (Opcode::SQRSHRUN_asisdshf_N, "sqrshrun", false),
        N::SshrImm => (Opcode::SSHR_asisdshf_R, "sshr", false),
        N::UshrImm => (Opcode::USHR_asisdshf_R, "ushr", false),
        N::SrshrImm => (Opcode::SRSHR_asisdshf_R, "srshr", false),
        N::UrshrImm => (Opcode::URSHR_asisdshf_R, "urshr", false),
        N::ShlImm => (Opcode::SHL_asisdshf_R, "shl", false),
        N::SqshlImm => (Opcode::SQSHL_asisdshf_R, "sqshl", false),
        N::UqshlImm => (Opcode::UQSHL_asisdshf_R, "uqshl", false),
        N::Sqshlu => (Opcode::SQSHLU_asisdshf_R, "sqshlu", false),
        N::Sli => (Opcode::SLI_asisdshf_R, "sli", false),
        N::Sri => (Opcode::SRI_asisdshf_R, "sri", false),
        N::Ssra => (Opcode::SSRA_asisdshf_R, "ssra", false),
        N::Usra => (Opcode::USRA_asisdshf_R, "usra", false),
        N::Srsra => (Opcode::SRSRA_asisdshf_R, "srsra", false),
        N::Ursra => (Opcode::URSRA_asisdshf_R, "ursra", false),
        N::ScvtfFixed => (Opcode::SCVTF_asisdshf_C, "scvtf", false),
        N::UcvtfFixed => (Opcode::UCVTF_asisdshf_C, "ucvtf", false),
        N::FcvtzsFixed => (Opcode::FCVTZS_asisdshf_C, "fcvtzs", false),
        N::FcvtzuFixed => (Opcode::FCVTZU_asisdshf_C, "fcvtzu", false),
        N::Scvtf => (Opcode::SCVTF_asisdmisc_R, "scvtf", false),
        N::Ucvtf => (Opcode::UCVTF_asisdmisc_R, "ucvtf", false),
        N::Fcvtas => (Opcode::FCVTAS_asisdmisc_R, "fcvtas", false),
        N::Fcvtau => (Opcode::FCVTAU_asisdmisc_R, "fcvtau", false),
        N::Fcvtms => (Opcode::FCVTMS_asisdmisc_R, "fcvtms", false),
        N::Fcvtmu => (Opcode::FCVTMU_asisdmisc_R, "fcvtmu", false),
        N::Fcvtns => (Opcode::FCVTNS_asisdmisc_R, "fcvtns", false),
        N::Fcvtnu => (Opcode::FCVTNU_asisdmisc_R, "fcvtnu", false),
        N::Fcvtps => (Opcode::FCVTPS_asisdmisc_R, "fcvtps", false),
        N::Fcvtpu => (Opcode::FCVTPU_asisdmisc_R, "fcvtpu", false),
        N::Fcvtzs => (Opcode::FCVTZS_asisdmisc_R, "fcvtzs", false),
        N::Fcvtzu => (Opcode::FCVTZU_asisdmisc_R, "fcvtzu", false),
        N::Faddp => (Opcode::FADDP_asisdpair_only_SD, "faddp", false),
        N::Fminp => (Opcode::FMINP_asisdpair_only_SD, "fminp", false),
        N::Fmaxp => (Opcode::FMAXP_asisdpair_only_SD, "fmaxp", false),
        N::Fminnmp => (Opcode::FMINNMP_asisdpair_only_SD, "fminnmp", false),
        N::Fmaxnmp => (Opcode::FMAXNMP_asisdpair_only_SD, "fmaxnmp", false),
        N::Fabs => (
            if bytes == 4 {
                Opcode::FABS_S_floatdp1
            } else {
                Opcode::FABS_D_floatdp1
            },
            "fabs",
            true,
        ),
        N::Fneg => (
            if bytes == 4 {
                Opcode::FNEG_S_floatdp1
            } else {
                Opcode::FNEG_D_floatdp1
            },
            "fneg",
            true,
        ),
        N::Fsqrt => (
            if bytes == 4 {
                Opcode::FSQRT_S_floatdp1
            } else {
                Opcode::FSQRT_D_floatdp1
            },
            "fsqrt",
            true,
        ),
        N::Frintn => (
            if bytes == 4 {
                Opcode::FRINTN_S_floatdp1
            } else {
                Opcode::FRINTN_D_floatdp1
            },
            "frintn",
            true,
        ),
        N::Frintp => (
            if bytes == 4 {
                Opcode::FRINTP_S_floatdp1
            } else {
                Opcode::FRINTP_D_floatdp1
            },
            "frintp",
            true,
        ),
        N::Frintm => (
            if bytes == 4 {
                Opcode::FRINTM_S_floatdp1
            } else {
                Opcode::FRINTM_D_floatdp1
            },
            "frintm",
            true,
        ),
        N::Frintz => (
            if bytes == 4 {
                Opcode::FRINTZ_S_floatdp1
            } else {
                Opcode::FRINTZ_D_floatdp1
            },
            "frintz",
            true,
        ),
        N::Frinta => (
            if bytes == 4 {
                Opcode::FRINTA_S_floatdp1
            } else {
                Opcode::FRINTA_D_floatdp1
            },
            "frinta",
            true,
        ),
        N::Frintx => (
            if bytes == 4 {
                Opcode::FRINTX_S_floatdp1
            } else {
                Opcode::FRINTX_D_floatdp1
            },
            "frintx",
            true,
        ),
        N::Fadd => (
            if bytes == 4 {
                Opcode::FADD_S_floatdp2
            } else {
                Opcode::FADD_D_floatdp2
            },
            "fadd",
            true,
        ),
        N::Fsub => (
            if bytes == 4 {
                Opcode::FSUB_S_floatdp2
            } else {
                Opcode::FSUB_D_floatdp2
            },
            "fsub",
            true,
        ),
        N::Fmul => (
            if bytes == 4 {
                Opcode::FMUL_S_floatdp2
            } else {
                Opcode::FMUL_D_floatdp2
            },
            "fmul",
            true,
        ),
        N::Fdiv => (
            if bytes == 4 {
                Opcode::FDIV_S_floatdp2
            } else {
                Opcode::FDIV_D_floatdp2
            },
            "fdiv",
            true,
        ),
        N::Fmin => (
            if bytes == 4 {
                Opcode::FMIN_S_floatdp2
            } else {
                Opcode::FMIN_D_floatdp2
            },
            "fmin",
            true,
        ),
        N::Fmax => (
            if bytes == 4 {
                Opcode::FMAX_S_floatdp2
            } else {
                Opcode::FMAX_D_floatdp2
            },
            "fmax",
            true,
        ),
        N::Fminnm => (
            if bytes == 4 {
                Opcode::FMINNM_S_floatdp2
            } else {
                Opcode::FMINNM_D_floatdp2
            },
            "fminnm",
            true,
        ),
        N::Fmaxnm => (
            if bytes == 4 {
                Opcode::FMAXNM_S_floatdp2
            } else {
                Opcode::FMAXNM_D_floatdp2
            },
            "fmaxnm",
            true,
        ),
        _ => unreachable!("verified scalar Advanced SIMD form: {op}"),
    }
}
impl FunctionCompiler<'_> {
    pub(super) fn neon_scalar(&mut self, op: N, ty: Type, args: &[Value], result: Option<Value>) {
        let scalar = ty.vector().map_or(ty, |v| v.lane_type());
        let bytes = scalar.bytes().unwrap();
        let (opcode, name, fixed) = encoding(op, bytes);
        let immediate = op.immediate_range(ty).map(|_| {
            let ValueDef::Result(i, _) = self
                .func
                .value_def(self.func.resolve(*args.last().unwrap()))
            else {
                unreachable!()
            };
            let InstData::Iconst { imm, .. } = *self.func.inst_data(i) else {
                unreachable!()
            };
            imm
        });
        let values = &args[..args.len() - usize::from(immediate.is_some())];
        for (i, &arg) in values.iter().enumerate() {
            let scratch = [FS0, FS1, FS2][i];
            if self.ty(arg).is_int() {
                let r = self.use_reg(arg, S0);
                self.mach("fmov", [Reg::d(scratch).into(), Reg::x(r).into()]);
            } else {
                let r = self.use_freg(arg, scratch);
                self.move_vector(scratch, r);
            }
        }
        let accumulating = matches!(
            op,
            N::Fmla
                | N::Fmls
                | N::Suqadd
                | N::Usqadd
                | N::Sqdmlal
                | N::Sqdmlsl
                | N::Ssra
                | N::Usra
                | N::Srsra
                | N::Ursra
                | N::Sli
                | N::Sri
        );
        let narrow = matches!(
            op,
            N::Sqxtn
                | N::Uqxtn
                | N::Sqxtun
                | N::Sqshrn
                | N::Uqshrn
                | N::Sqrshrn
                | N::Uqrshrn
                | N::Sqshrun
                | N::Sqrshrun
        );
        let long = matches!(op, N::Sqdmull | N::Sqdmlal | N::Sqdmlsl);
        let pair = matches!(op, N::Faddp | N::Fminp | N::Fmaxp | N::Fminnmp | N::Fmaxnmp);
        let float = scalar.is_float() || op.is_float_comparison() || op.is_conversion();
        let mut fields = vec![
            (FieldId::Rd, FS0.into()),
            (FieldId::Rn, if accumulating { FS1 } else { FS0 }.into()),
        ];
        let indexed_fma = matches!(op, N::Fmla | N::Fmls);
        if indexed_fma {
            // Lane zero holds the scalar multiplicand. The by-element scalar
            // opcode performs the exact architectural fused operation and its
            // FPCR/FPSR effects, including FMLS operand/NaN ordering.
            fields.extend([
                (FieldId::sz, u32::from(bytes == 8)),
                (FieldId::L, 0),
                (FieldId::H, 0),
                (FieldId::M, (FS2 >> 4).into()),
                (FieldId::Rm, (FS2 & 15).into()),
            ]);
        } else if values.len() - usize::from(accumulating) == 2 {
            fields.push((FieldId::Rm, if accumulating { FS2 } else { FS1 }.into()));
        }
        if let Some(imm) = immediate {
            let width = bytes * 8;
            let left = matches!(
                op,
                N::ShlImm | N::SqshlImm | N::UqshlImm | N::Sqshlu | N::Sli
            );
            let encoded = if left {
                width + imm as u32
            } else {
                width * 2 - imm as u32
            };
            fields.extend([(FieldId::immh, encoded >> 3), (FieldId::immb, encoded & 7)]);
        } else if !fixed && op != N::Fcvtxn {
            fields.push(if float {
                (FieldId::sz, u32::from(bytes == 8))
            } else {
                (FieldId::size, bytes.trailing_zeros() - u32::from(long))
            });
        }
        let class = |n: u32| match n {
            1 => 'b',
            2 => 'h',
            4 => 's',
            8 => 'd',
            _ => unreachable!(),
        };
        let dc = class(bytes);
        let sc = class(if narrow {
            bytes * 2
        } else if long {
            bytes / 2
        } else if op == N::Fcvtxn {
            8
        } else {
            bytes
        });
        let left = if accumulating { FS1 } else { FS0 };
        let right = if accumulating { FS2 } else { FS1 };
        self.out.native(
            || keel_asm_ir::Instruction::new(opcode, fields),
            |out| {
                write!(out, "{name}\t{dc}{FS0}, ")?;
                if pair {
                    write!(out, "v{left}.{}", if bytes == 4 { "2s" } else { "2d" })?;
                } else {
                    write!(out, "{sc}{left}")?;
                }
                if values.len() - usize::from(accumulating) == 2 {
                    if indexed_fma {
                        write!(out, ", v{right}.{sc}[0]")?;
                    } else {
                        write!(out, ", {sc}{right}")?;
                    }
                }
                if let Some(imm) = immediate {
                    write!(out, ", #{imm}")?;
                }
                Ok(())
            },
        );
        if ty.is_int() {
            let d = self.def_reg(result);
            self.mach("fmov", [Reg::x(d).into(), Reg::d(FS0).into()]);
            self.def_done(result, d);
        } else {
            let d = self.def_freg(result);
            self.move_vector(d, FS0);
            self.def_done(result, d);
        }
    }
}
