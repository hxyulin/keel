//! Explicit baseline ARM Advanced SIMD semantics, including architectural state.
//!
//! These operations execute with the current FPCR and update FPSR as the named
//! ARM instruction specifies. In particular saturating operations accumulate
//! FPSR.QC, and floating operations may accumulate exception flags. They are
//! pinned state barriers, even when the value result is unused. This separates
//! them from portable pure clamping and modulo-width shift operations.
use crate::{Type, VectorType};
use std::{fmt, str::FromStr};

macro_rules! operations {
    ($($variant:ident => $name:literal),* $(,)?) => {
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum NeonIntrinsic { $($variant),* }
        impl NeonIntrinsic {
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];
            pub fn name(self)-> &'static str { match self { $(Self::$variant => $name),* } }
        }
        impl fmt::Display for NeonIntrinsic {
            fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(self.name())}
        }
        impl FromStr for NeonIntrinsic {
            type Err=();
            fn from_str(s:&str)->Result<Self,()>{Ok(match s{$($name => Self::$variant,)*_=>return Err(())})}
        }
    };
}
operations! {
    Cls=>"neon_cls", Rbit=>"neon_rbit",
    Mla=>"neon_mla", Mls=>"neon_mls", Saba=>"neon_saba", Uaba=>"neon_uaba",
    Sadalp=>"neon_sadalp", Uadalp=>"neon_uadalp",
    Ld1=>"neon_ld1",
    Ld1Lane=>"neon_ld1_lane",
    Ld1R=>"neon_ld1r",
    Ld2=>"neon_ld2",
    Ld2Lane=>"neon_ld2_lane",
    Ld2R=>"neon_ld2r",
    Ld3=>"neon_ld3",
    Ld3Lane=>"neon_ld3_lane",
    Ld3R=>"neon_ld3r",
    Ld4=>"neon_ld4",
    Ld4Lane=>"neon_ld4_lane",
    Ld4R=>"neon_ld4r",
    Ld1x2=>"neon_ld1x2",
    Ld1x3=>"neon_ld1x3",
    Ld1x4=>"neon_ld1x4",
    St1=>"neon_st1",
    St1Lane=>"neon_st1_lane",
    St2=>"neon_st2",
    St2Lane=>"neon_st2_lane",
    St3=>"neon_st3",
    St3Lane=>"neon_st3_lane",
    St4=>"neon_st4",
    St4Lane=>"neon_st4_lane",
    St1x2=>"neon_st1x2",
    St1x3=>"neon_st1x3",
    St1x4=>"neon_st1x4",
    Sqadd=>"neon_sqadd", Uqadd=>"neon_uqadd", Sqsub=>"neon_sqsub", Uqsub=>"neon_uqsub",
    Suqadd=>"neon_suqadd", Usqadd=>"neon_usqadd", Sqabs=>"neon_sqabs", Sqneg=>"neon_sqneg",
    Sqshl=>"neon_sqshl", Uqshl=>"neon_uqshl", Sqrshl=>"neon_sqrshl", Uqrshl=>"neon_uqrshl",
    Srshl=>"neon_srshl", Urshl=>"neon_urshl", Sqdmulh=>"neon_sqdmulh", Sqrdmulh=>"neon_sqrdmulh",
    Fmla=>"neon_fmla", Fmls=>"neon_fmls", Fmulx=>"neon_fmulx", Fabd=>"neon_fabd",
    Frecpx=>"neon_frecpx", Frecpe=>"neon_frecpe", Frsqrte=>"neon_frsqrte", Frecps=>"neon_frecps", Frsqrts=>"neon_frsqrts",
    Urecpe=>"neon_urecpe", Ursqrte=>"neon_ursqrte",
    Frinta=>"neon_frinta", Frintx=>"neon_frintx", Frinti=>"neon_frinti",
    Fminnm=>"neon_fminnm", Fmaxnm=>"neon_fmaxnm", Pmul=>"neon_pmul",
    ReadFpsr=>"neon_read_fpsr", SwapFpsr=>"neon_swap_fpsr", ReadFpcr=>"neon_read_fpcr", SwapFpcr=>"neon_swap_fpcr",
    Saddl=>"neon_saddl",
    Uaddl=>"neon_uaddl",
    Ssubl=>"neon_ssubl",
    Usubl=>"neon_usubl",
    Smull=>"neon_smull",
    Umull=>"neon_umull",
    Sabdl=>"neon_sabdl",
    Uabdl=>"neon_uabdl",
    Sqdmull=>"neon_sqdmull",
    Pmull=>"neon_pmull",
    Smlal=>"neon_smlal",
    Umlal=>"neon_umlal",
    Smlsl=>"neon_smlsl",
    Umlsl=>"neon_umlsl",
    Sabal=>"neon_sabal",
    Uabal=>"neon_uabal",
    Sqdmlal=>"neon_sqdmlal",
    Sqdmlsl=>"neon_sqdmlsl",
    Saddw=>"neon_saddw",
    Uaddw=>"neon_uaddw",
    Ssubw=>"neon_ssubw",
    Usubw=>"neon_usubw",
    Sqxtn=>"neon_sqxtn",
    Uqxtn=>"neon_uqxtn",
    Sqxtun=>"neon_sqxtun",
    Addhn=>"neon_addhn",
    Raddhn=>"neon_raddhn",
    Subhn=>"neon_subhn",
    Rsubhn=>"neon_rsubhn",
    Shrn=>"neon_shrn",
    Rshrn=>"neon_rshrn",
    Sqshrn=>"neon_sqshrn",
    Uqshrn=>"neon_uqshrn",
    Sqrshrn=>"neon_sqrshrn",
    Uqrshrn=>"neon_uqrshrn",
    Sqshrun=>"neon_sqshrun",
    Sqrshrun=>"neon_sqrshrun",
    SshrImm=>"neon_sshr_imm",
    UshrImm=>"neon_ushr_imm",
    SrshrImm=>"neon_srshr_imm",
    UrshrImm=>"neon_urshr_imm",
    Sri=>"neon_sri",
    ShlImm=>"neon_shl_imm",
    SqshlImm=>"neon_sqshl_imm",
    UqshlImm=>"neon_uqshl_imm",
    Sqshlu=>"neon_sqshlu",
    Sli=>"neon_sli",
    Ssra=>"neon_ssra",
    Usra=>"neon_usra",
    Srsra=>"neon_srsra",
    Ursra=>"neon_ursra",
    Sshl=>"neon_sshl",
    Ushl=>"neon_ushl",
    Scvtf=>"neon_scvtf",
    Ucvtf=>"neon_ucvtf",
    Fcvtas=>"neon_fcvtas",
    Fcvtau=>"neon_fcvtau",
    Fcvtms=>"neon_fcvtms",
    Fcvtmu=>"neon_fcvtmu",
    Fcvtns=>"neon_fcvtns",
    Fcvtnu=>"neon_fcvtnu",
    Fcvtps=>"neon_fcvtps",
    Fcvtpu=>"neon_fcvtpu",
    Fcvtzs=>"neon_fcvtzs",
    Fcvtzu=>"neon_fcvtzu",
    ScvtfFixed=>"neon_scvtf_fixed",
    UcvtfFixed=>"neon_ucvtf_fixed",
    FcvtzsFixed=>"neon_fcvtzs_fixed",
    FcvtzuFixed=>"neon_fcvtzu_fixed",
    Fcvtl=>"neon_fcvtl",
    Fcvtn=>"neon_fcvtn",
    Fcvtxn=>"neon_fcvtxn",
    Fcvthl=>"neon_fcvthl",
    Fcvthn=>"neon_fcvthn",
    Fminv=>"neon_fminv",
    Fmaxv=>"neon_fmaxv",
    Fminnmv=>"neon_fminnmv",
    Fmaxnmv=>"neon_fmaxnmv",
    Saddlv=>"neon_saddlv",
    Uaddlv=>"neon_uaddlv",
    Fabs=>"neon_fabs",
    Fneg=>"neon_fneg",
    Fsqrt=>"neon_fsqrt",
    Frintn=>"neon_frintn",
    Frintp=>"neon_frintp",
    Frintm=>"neon_frintm",
    Frintz=>"neon_frintz",
    Fadd=>"neon_fadd",
    Fsub=>"neon_fsub",
    Fmul=>"neon_fmul",
    Fdiv=>"neon_fdiv",
    Fmin=>"neon_fmin",
    Fmax=>"neon_fmax",
    Faddp=>"neon_faddp",
    Fminp=>"neon_fminp",
    Fmaxp=>"neon_fmaxp",
    Fminnmp=>"neon_fminnmp",
    Fmaxnmp=>"neon_fmaxnmp",
    Fcmeq=>"neon_fcmeq",
    Fcmge=>"neon_fcmge",
    Fcmgt=>"neon_fcmgt",
    Facge=>"neon_facge",
    Facgt=>"neon_facgt",
    Tbl1=>"neon_tbl1",
    Tbl2=>"neon_tbl2",
    Tbl3=>"neon_tbl3",
    Tbl4=>"neon_tbl4",
    Tbx1=>"neon_tbx1",
    Tbx2=>"neon_tbx2",
    Tbx3=>"neon_tbx3",
    Tbx4=>"neon_tbx4",
}
impl NeonIntrinsic {
    pub fn arity(self) -> usize {
        if self.is_integer_misc() {
            return match self {
                Self::Cls | Self::Rbit => 1,
                Self::Sadalp | Self::Uadalp => 2,
                _ => 3,
            };
        }
        use NeonIntrinsic::*;
        if let Some(m) = self.memory() {
            return 1
                + if m.store || m.lane {
                    m.count as usize
                } else {
                    0
                }
                + usize::from(m.lane);
        }
        match self {
            Shrn | Rshrn | Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun | SshrImm
            | UshrImm | SrshrImm | UrshrImm | ShlImm | SqshlImm | UqshlImm | Sqshlu | Sshl
            | Ushl => 2,
            Ssra | Usra | Srsra | Ursra | Sli | Sri => 3,
            Scvtf | Ucvtf | Fcvtas | Fcvtau | Fcvtms | Fcvtmu | Fcvtns | Fcvtnu | Fcvtps
            | Fcvtpu | Fcvtzs | Fcvtzu | Fcvtl | Fcvtn | Fcvtxn | Fcvthl | Fcvthn | Fminv
            | Fmaxv | Fminnmv | Fmaxnmv | Saddlv | Uaddlv => 1,
            ScvtfFixed | UcvtfFixed | FcvtzsFixed | FcvtzuFixed => 2,
            Fabs | Fneg | Fsqrt | Frintn | Frintp | Frintm | Frintz => 1,
            Fadd | Fsub | Fmul | Fdiv | Fmin | Fmax | Faddp | Fminp | Fmaxp | Fminnmp | Fmaxnmp
            | Fcmeq | Fcmge | Fcmgt | Facge | Facgt | Tbl1 => 2,
            Tbl2 | Tbx1 => 3,
            Tbl3 | Tbx2 => 4,
            Tbl4 | Tbx3 => 5,
            Tbx4 => 6,
            ReadFpsr | ReadFpcr => 0,
            SwapFpsr | SwapFpcr | Sqabs | Sqneg | Frecpe | Frsqrte | Urecpe | Ursqrte | Frinta
            | Frintx | Frinti | Frecpx => 1,
            Sqadd | Uqadd | Sqsub | Uqsub | Suqadd | Usqadd | Sqshl | Uqshl | Sqrshl | Uqrshl
            | Srshl | Urshl | Sqdmulh | Sqrdmulh | Fmulx | Fabd | Frecps | Frsqrts | Fminnm
            | Fmaxnm | Pmul => 2,
            Fmla | Fmls | Smlal | Umlal | Smlsl | Umlsl | Sabal | Uabal | Sqdmlal | Sqdmlsl => 3,
            Saddl | Uaddl | Ssubl | Usubl | Smull | Umull | Sabdl | Uabdl | Sqdmull | Pmull
            | Saddw | Uaddw | Ssubw | Usubw | Addhn | Raddhn | Subhn | Rsubhn => 2,
            Sqxtn | Uqxtn | Sqxtun => 1,
            _ => unreachable!("memory handled above"),
        }
    }
    pub fn is_status(self) -> bool {
        matches!(
            self,
            Self::ReadFpsr | Self::ReadFpcr | Self::SwapFpsr | Self::SwapFpcr
        )
    }
    /// State swaps return
    /// the previous register value, then write the low architecturally defined
    /// bits of the i64 operand. Reserved bits follow the ARM register rules.
    pub fn accepts(self, ty: Type) -> bool {
        use NeonIntrinsic::*;
        if self.is_integer_misc() {
            let Some(v) = ty.vector() else {
                return false;
            };
            if !matches!(v.bytes(), 8 | 16) || !v.lane_type().is_int() {
                return false;
            }
            let bytes = v.lane_type().bytes().unwrap();
            return match self {
                Self::Rbit => bytes == 1,
                Self::Sadalp | Self::Uadalp => bytes >= 2,
                _ => bytes <= 4,
            };
        }
        if self.memory().is_some() {
            return ty.vector().is_some_and(|v| matches!(v.bytes(), 8 | 16));
        }
        if self.is_status() {
            return ty == Type::I64;
        }
        match self {
            Self::Fminv | Self::Fmaxv | Self::Fminnmv | Self::Fmaxnmv => return ty == Type::F32,
            Self::Saddlv | Self::Uaddlv => return matches!(ty, Type::I16 | Type::I32 | Type::I64),
            _ => {}
        }
        let Some(shape) = ty.vector() else {
            return self.accepts_scalar(ty);
        };
        if shape.bytes() == 8 {
            // A one-lane 64-bit value requires scalar encodings, not Q=0.
            if shape.lane_type().bytes() == Some(8) {
                return self.accepts_scalar(shape.lane_type());
            }
            let q = VectorType::ALL_128
                .into_iter()
                .find(|v| v.lane_type() == shape.lane_type())
                .unwrap();
            // Widening always produces Q; narrowing may return the exact D half.
            let narrow = matches!(
                self,
                Sqxtn
                    | Uqxtn
                    | Sqxtun
                    | Addhn
                    | Raddhn
                    | Subhn
                    | Rsubhn
                    | Shrn
                    | Rshrn
                    | Sqshrn
                    | Uqshrn
                    | Sqrshrn
                    | Uqrshrn
                    | Sqshrun
                    | Sqrshrun
                    | Fcvtn
                    | Fcvtxn
                    | Fcvthn
            );
            return !matches!(self, Fcvtl | Fcvthl)
                && (!self.changes_width() || narrow)
                && self.accepts(Type::Vector(q));
        }
        if shape.bytes() != 16 {
            return false;
        }
        match self {
            Frecpx => false,
            Sqadd | Uqadd | Sqsub | Uqsub | Suqadd | Usqadd | Sqabs | Sqneg | Sqshl | Uqshl
            | Sqrshl | Uqrshl | Srshl | Urshl => shape.lane_type().is_int(),
            Sqdmulh | Sqrdmulh => matches!(shape, VectorType::I16x8 | VectorType::I32x4),
            Urecpe | Ursqrte => shape == VectorType::I32x4,
            Pmul => shape == VectorType::I8x16,
            Fmla | Fmls | Fmulx | Fabd | Frecpe | Frsqrte | Frecps | Frsqrts | Frinta | Frintx
            | Frinti | Fminnm | Fmaxnm => shape.lane_type().is_float(),
            Saddl | Uaddl | Ssubl | Usubl | Smull | Umull | Sabdl | Uabdl | Smlal | Umlal
            | Smlsl | Umlsl | Sabal | Uabal | Saddw | Uaddw | Ssubw | Usubw => matches!(
                shape,
                VectorType::I16x8 | VectorType::I32x4 | VectorType::I64x2
            ),
            Sqdmull | Sqdmlal | Sqdmlsl => matches!(shape, VectorType::I32x4 | VectorType::I64x2),
            Pmull => shape == VectorType::I16x8,
            Sqxtn | Uqxtn | Sqxtun | Addhn | Raddhn | Subhn | Rsubhn => matches!(
                shape,
                VectorType::I8x16 | VectorType::I16x8 | VectorType::I32x4
            ),
            Shrn | Rshrn | Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun => matches!(
                shape,
                VectorType::I8x16 | VectorType::I16x8 | VectorType::I32x4
            ),
            SshrImm | UshrImm | SrshrImm | UrshrImm | Sri | ShlImm | SqshlImm | UqshlImm
            | Sqshlu | Sli | Ssra | Usra | Srsra | Ursra | Sshl | Ushl => {
                shape.lane_type().is_int()
            }
            Scvtf | Ucvtf | ScvtfFixed | UcvtfFixed => shape.lane_type().is_float(),
            Fcvtas | Fcvtau | Fcvtms | Fcvtmu | Fcvtns | Fcvtnu | Fcvtps | Fcvtpu | Fcvtzs
            | Fcvtzu | FcvtzsFixed | FcvtzuFixed => {
                matches!(shape, VectorType::I32x4 | VectorType::I64x2)
            }
            Fcvtl => shape == VectorType::F64x2,
            Fcvtn | Fcvtxn | Fcvthl => shape == VectorType::F32x4,
            Fcvthn => shape == VectorType::I16x8,
            Fminv | Fmaxv | Fminnmv | Fmaxnmv | Saddlv | Uaddlv => unreachable!(),
            Fabs | Fneg | Fsqrt | Frintn | Frintp | Frintm | Frintz | Fadd | Fsub | Fmul | Fdiv
            | Fmin | Fmax | Faddp | Fminp | Fmaxp | Fminnmp | Fmaxnmp => {
                shape.lane_type().is_float()
            }
            Fcmeq | Fcmge | Fcmgt | Facge | Facgt => {
                matches!(shape, VectorType::I32x4 | VectorType::I64x2)
            }
            Tbl1 | Tbl2 | Tbl3 | Tbl4 | Tbx1 | Tbx2 | Tbx3 | Tbx4 => shape == VectorType::I8x16,
            _ => unreachable!("status and memory handled above"),
        }
    }
}

impl NeonIntrinsic {
    /// Typed input shapes; long operations consume the low half of their
    /// narrower inputs. Narrow operations write low lanes and zero all higher
    /// lanes. High placement/extraction can be expressed with vshuffle.
    pub fn operand_types(self, result: Type) -> Option<Vec<Type>> {
        use NeonIntrinsic::*;
        if !self.accepts(result) {
            return None;
        }
        if self.is_integer_misc() {
            let v = result.vector().unwrap();
            return Some(if matches!(self, Self::Sadalp | Self::Uadalp) {
                let source = VectorType::ALL_64
                    .into_iter()
                    .chain(VectorType::ALL_128)
                    .find(|s| {
                        s.bytes() == v.bytes()
                            && s.lane_type().is_int()
                            && s.lane_type().bytes().unwrap() * 2 == v.lane_type().bytes().unwrap()
                    })
                    .unwrap();
                vec![result, Type::Vector(source)]
            } else {
                vec![result; self.arity()]
            });
        }
        if !self.is_status() && self.accepts_scalar(result) {
            return Some(self.scalar_operands(result));
        }
        if let Some(shape) = result.vector()
            && shape.lanes() == 1
            && self.accepts_scalar(shape.lane_type())
        {
            return Some(
                self.scalar_operands(shape.lane_type())
                    .into_iter()
                    .map(|t| {
                        if t == Type::I32 && self.immediate_range(result).is_some() {
                            t
                        } else if t.is_vector() {
                            t
                        } else {
                            Type::Vector(
                                VectorType::ALL_64
                                    .into_iter()
                                    .find(|v| v.lane_type() == t)
                                    .unwrap(),
                            )
                        }
                    })
                    .collect(),
            );
        }
        if let Some(m) = self.memory() {
            let mut types = vec![Type::Ptr];
            if m.store || m.lane {
                types.extend(std::iter::repeat_n(result, m.count as usize));
            }
            if m.lane {
                types.push(Type::I32);
            }
            return Some(types);
        }
        if let Some(shape) = result.vector()
            && shape.bytes() == 8
        {
            let q = VectorType::ALL_128
                .into_iter()
                .find(|v| v.lane_type() == shape.lane_type())
                .unwrap();
            let inputs = self.operand_types(Type::Vector(q))?;
            // Narrowing consumes Q. Table registers are always 16 bytes even
            // when indices, fallback and result have only eight lanes.
            return Some(
                inputs
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        if self.changes_width() || matches!(self, Fcvtn | Fcvtxn | Fcvthn) {
                            return t;
                        }
                        let table_start = usize::from(matches!(self, Tbx1 | Tbx2 | Tbx3 | Tbx4));
                        if self.is_table() && i >= table_start && i + 1 < self.arity() {
                            return t;
                        }
                        if let Some(v) = t.vector() {
                            Type::Vector(
                                VectorType::ALL_64
                                    .into_iter()
                                    .find(|d| d.lane_type() == v.lane_type())
                                    .unwrap(),
                            )
                        } else {
                            t
                        }
                    })
                    .collect(),
            );
        }
        let relative = |factor: u32, divide: bool| {
            let bytes = result.vector().unwrap().lane_type().bytes().unwrap();
            let bytes = if divide {
                bytes / factor
            } else {
                bytes * factor
            };
            Type::Vector(
                VectorType::ALL_128
                    .into_iter()
                    .find(|v| v.lane_type().is_int() && v.lane_type().bytes() == Some(bytes))
                    .unwrap(),
            )
        };
        Some(match self {
            Fcmeq | Fcmge | Fcmgt | Facge | Facgt => vec![
                if result == Type::Vector(VectorType::I32x4) {
                    Type::Vector(VectorType::F32x4)
                } else {
                    Type::Vector(VectorType::F64x2)
                };
                2
            ],
            Fabs | Fneg | Fsqrt | Frintn | Frintp | Frintm | Frintz | Fadd | Fsub | Fmul | Fdiv
            | Fmin | Fmax | Faddp | Fminp | Fmaxp | Fminnmp | Fmaxnmp | Tbl1 | Tbl2 | Tbl3
            | Tbl4 | Tbx1 | Tbx2 | Tbx3 | Tbx4 => vec![result; self.arity()],

            Scvtf | Ucvtf => vec![Type::Vector(result.vector().unwrap().mask())],
            ScvtfFixed | UcvtfFixed => {
                vec![Type::Vector(result.vector().unwrap().mask()), Type::I32]
            }
            Fcvtas | Fcvtau | Fcvtms | Fcvtmu | Fcvtns | Fcvtnu | Fcvtps | Fcvtpu | Fcvtzs
            | Fcvtzu => vec![if result == Type::Vector(VectorType::I32x4) {
                Type::Vector(VectorType::F32x4)
            } else {
                Type::Vector(VectorType::F64x2)
            }],
            FcvtzsFixed | FcvtzuFixed => vec![
                if result == Type::Vector(VectorType::I32x4) {
                    Type::Vector(VectorType::F32x4)
                } else {
                    Type::Vector(VectorType::F64x2)
                },
                Type::I32,
            ],
            Fcvtl | Fcvthn => vec![Type::Vector(VectorType::F32x4)],
            Fcvtn | Fcvtxn => vec![Type::Vector(VectorType::F64x2)],
            Fcvthl => vec![Type::Vector(VectorType::I16x8)],
            Fminv | Fmaxv | Fminnmv | Fmaxnmv => vec![Type::Vector(VectorType::F32x4)],
            Saddlv | Uaddlv => vec![Type::Vector(match result {
                Type::I16 => VectorType::I8x16,
                Type::I32 => VectorType::I16x8,
                Type::I64 => VectorType::I32x4,
                _ => unreachable!(),
            })],

            Shrn | Rshrn | Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun => {
                vec![relative(2, false), Type::I32]
            }
            SshrImm | UshrImm | SrshrImm | UrshrImm | ShlImm | SqshlImm | UqshlImm | Sqshlu => {
                vec![result, Type::I32]
            }
            Ssra | Usra | Srsra | Ursra | Sli | Sri => vec![result, result, Type::I32],
            Sshl | Ushl => vec![result; 2],

            Saddl | Uaddl | Ssubl | Usubl | Smull | Umull | Sabdl | Uabdl | Sqdmull | Pmull => {
                vec![relative(2, true); 2]
            }
            Smlal | Umlal | Smlsl | Umlsl | Sabal | Uabal | Sqdmlal | Sqdmlsl => {
                vec![result, relative(2, true), relative(2, true)]
            }
            Saddw | Uaddw | Ssubw | Usubw => vec![result, relative(2, true)],
            Sqxtn | Uqxtn | Sqxtun => vec![relative(2, false)],
            Addhn | Raddhn | Subhn | Rsubhn => vec![relative(2, false); 2],
            Sqadd | Uqadd | Sqsub | Uqsub | Suqadd | Usqadd | Sqabs | Sqneg | Sqshl | Uqshl
            | Sqrshl | Uqrshl | Srshl | Urshl | Sqdmulh | Sqrdmulh | Fmla | Fmls | Fmulx | Fabd
            | Frecpx | Frecpe | Frsqrte | Frecps | Frsqrts | Urecpe | Ursqrte | Frinta | Frintx
            | Frinti | Fminnm | Fmaxnm | Pmul | ReadFpsr | SwapFpsr | ReadFpcr | SwapFpcr => {
                vec![result; self.arity()]
            }
            _ => unreachable!("memory handled above"),
        })
    }
    pub fn changes_width(self) -> bool {
        use NeonIntrinsic::*;
        matches!(
            self,
            Shrn | Rshrn
                | Sqshrn
                | Uqshrn
                | Sqrshrn
                | Uqrshrn
                | Sqshrun
                | Sqrshrun
                | Saddl
                | Uaddl
                | Ssubl
                | Usubl
                | Smull
                | Umull
                | Sabdl
                | Uabdl
                | Sqdmull
                | Pmull
                | Smlal
                | Umlal
                | Smlsl
                | Umlsl
                | Sabal
                | Uabal
                | Sqdmlal
                | Sqdmlsl
                | Saddw
                | Uaddw
                | Ssubw
                | Usubw
                | Sqxtn
                | Uqxtn
                | Sqxtun
                | Addhn
                | Raddhn
                | Subhn
                | Rsubhn
        )
    }
}

impl NeonIntrinsic {
    /// Architectural immediate shifts require a literal i32 SSA constant.
    /// Right shifts include the full lane width; left shifts include zero.
    pub fn immediate_range(self, result: Type) -> Option<std::ops::RangeInclusive<i64>> {
        use NeonIntrinsic::*;
        if !self.accepts(result) {
            return None;
        }
        if self.memory().is_some_and(|m| m.lane) {
            return Some(0..=i64::from(result.vector()?.lanes() - 1));
        }
        let width = i64::from(result.vector().map_or(result, |v| v.lane_type()).bytes()? * 8);
        match self {
            ScvtfFixed | UcvtfFixed | FcvtzsFixed | FcvtzuFixed => Some(1..=width),
            Shrn | Rshrn | Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun | SshrImm
            | UshrImm | SrshrImm | UrshrImm | Sri | Ssra | Usra | Srsra | Ursra => Some(1..=width),
            ShlImm | SqshlImm | UqshlImm | Sqshlu | Sli => Some(0..=width - 1),
            _ => None,
        }
    }
}

impl NeonIntrinsic {
    pub fn is_conversion(self) -> bool {
        matches!(
            self,
            Self::Scvtf
                | Self::Ucvtf
                | Self::Fcvtas
                | Self::Fcvtau
                | Self::Fcvtms
                | Self::Fcvtmu
                | Self::Fcvtns
                | Self::Fcvtnu
                | Self::Fcvtps
                | Self::Fcvtpu
                | Self::Fcvtzs
                | Self::Fcvtzu
                | Self::ScvtfFixed
                | Self::UcvtfFixed
                | Self::FcvtzsFixed
                | Self::FcvtzuFixed
                | Self::Fcvtl
                | Self::Fcvtn
                | Self::Fcvtxn
                | Self::Fcvthl
                | Self::Fcvthn
        )
    }
    pub fn is_reduction(self) -> bool {
        matches!(
            self,
            Self::Fminv | Self::Fmaxv | Self::Fminnmv | Self::Fmaxnmv | Self::Saddlv | Self::Uaddlv
        )
    }
}

impl NeonIntrinsic {
    pub fn is_table(self) -> bool {
        matches!(
            self,
            Self::Tbl1
                | Self::Tbl2
                | Self::Tbl3
                | Self::Tbl4
                | Self::Tbx1
                | Self::Tbx2
                | Self::Tbx3
                | Self::Tbx4
        )
    }
    pub fn is_float_comparison(self) -> bool {
        matches!(
            self,
            Self::Fcmeq | Self::Fcmge | Self::Fcmgt | Self::Facge | Self::Facgt
        )
    }
}

/// Exact-width structured transfers. Contiguous LD1 lists differ from
/// interleaved LD2/3/4; lane and replicate operations touch one element per
/// structure. Address writeback is explicit ptradd in SSA.
#[derive(Clone, Copy, Debug)]
pub struct NeonMemory {
    pub count: u8,
    pub store: bool,
    pub lane: bool,
    pub replicate: bool,
    pub interleaved: bool,
}
impl NeonIntrinsic {
    pub fn memory(self) -> Option<NeonMemory> {
        use NeonIntrinsic::*;
        let (count, store, lane, replicate, interleaved) = match self {
            Ld1 => (1, false, false, false, true),
            Ld1Lane => (1, false, true, false, true),
            Ld1R => (1, false, false, true, true),
            Ld2 => (2, false, false, false, true),
            Ld2Lane => (2, false, true, false, true),
            Ld2R => (2, false, false, true, true),
            Ld3 => (3, false, false, false, true),
            Ld3Lane => (3, false, true, false, true),
            Ld3R => (3, false, false, true, true),
            Ld4 => (4, false, false, false, true),
            Ld4Lane => (4, false, true, false, true),
            Ld4R => (4, false, false, true, true),
            Ld1x2 => (2, false, false, false, false),
            Ld1x3 => (3, false, false, false, false),
            Ld1x4 => (4, false, false, false, false),
            St1 => (1, true, false, false, true),
            St1Lane => (1, true, true, false, true),
            St2 => (2, true, false, false, true),
            St2Lane => (2, true, true, false, true),
            St3 => (3, true, false, false, true),
            St3Lane => (3, true, true, false, true),
            St4 => (4, true, false, false, true),
            St4Lane => (4, true, true, false, true),
            St1x2 => (2, true, false, false, false),
            St1x3 => (3, true, false, false, false),
            St1x4 => (4, true, false, false, false),
            _ => return None,
        };
        Some(NeonMemory {
            count,
            store,
            lane,
            replicate,
            interleaved,
        })
    }
    pub fn arity_for(self, ty: Type) -> usize {
        if self.accepts_scalar(ty)
            || ty
                .vector()
                .is_some_and(|v| v.lanes() == 1 && self.accepts_scalar(v.lane_type()))
        {
            self.scalar_operands(ty.vector().map_or(ty, |v| v.lane_type()))
                .len()
        } else {
            self.arity()
        }
    }
    pub fn result_count(self) -> usize {
        self.memory()
            .map_or(1, |m| if m.store { 0 } else { m.count as usize })
    }
}

impl NeonIntrinsic {
    /// Scalar Advanced SIMD legal widths; integer SSA values travel through
    /// the low B/H/S/D register lane at the instruction boundary.
    pub fn accepts_scalar(self, ty: Type) -> bool {
        use NeonIntrinsic::*;
        let int = matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64);
        let fp = matches!(ty, Type::F32 | Type::F64);
        match self {
            Sqadd | Uqadd | Sqsub | Uqsub | Suqadd | Usqadd | Sqabs | Sqneg | Sqshl | Uqshl
            | Sqrshl | Uqrshl | SqshlImm | UqshlImm | Sqshlu => int,
            Sshl | Ushl | Srshl | Urshl | SshrImm | UshrImm | SrshrImm | UrshrImm | ShlImm
            | Sli | Sri | Ssra | Usra | Srsra | Ursra => ty == Type::I64,
            Sqdmulh | Sqrdmulh => matches!(ty, Type::I16 | Type::I32),
            Sqdmull | Sqdmlal | Sqdmlsl => matches!(ty, Type::I32 | Type::I64),
            Sqxtn | Uqxtn | Sqxtun | Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun => {
                matches!(ty, Type::I8 | Type::I16 | Type::I32)
            }
            Fmla | Fmls | Fmulx | Fabd | Frecpx | Frecpe | Frsqrte | Frecps | Frsqrts | Fabs
            | Fneg | Fsqrt | Frintn | Frintp | Frintm | Frintz | Frinta | Frintx | Fadd | Fsub
            | Fmul | Fdiv | Fmin | Fmax | Fminnm | Fmaxnm | Faddp | Fminp | Fmaxp | Fminnmp
            | Fmaxnmp | Scvtf | Ucvtf | ScvtfFixed | UcvtfFixed => fp,
            Fcmeq | Fcmge | Fcmgt | Facge | Facgt | Fcvtas | Fcvtau | Fcvtms | Fcvtmu | Fcvtns
            | Fcvtnu | Fcvtps | Fcvtpu | Fcvtzs | Fcvtzu | FcvtzsFixed | FcvtzuFixed => {
                matches!(ty, Type::I32 | Type::I64)
            }
            Fcvtxn => ty == Type::F32,
            _ => false,
        }
    }
    pub fn scalar_operands(self, ty: Type) -> Vec<Type> {
        use NeonIntrinsic::*;
        let int = if ty == Type::F32 {
            Type::I32
        } else {
            Type::I64
        };
        let float = if ty == Type::I32 {
            Type::F32
        } else {
            Type::F64
        };
        let wide = match ty {
            Type::I8 => Type::I16,
            Type::I16 => Type::I32,
            _ => Type::I64,
        };
        let narrow = if ty == Type::I32 {
            Type::I16
        } else {
            Type::I32
        };
        match self {
            Scvtf | Ucvtf => vec![int],
            ScvtfFixed | UcvtfFixed => vec![int, Type::I32],
            Fcvtas | Fcvtau | Fcvtms | Fcvtmu | Fcvtns | Fcvtnu | Fcvtps | Fcvtpu | Fcvtzs
            | Fcvtzu => vec![float],
            FcvtzsFixed | FcvtzuFixed => vec![float, Type::I32],
            Fcmeq | Fcmge | Fcmgt | Facge | Facgt => vec![float; 2],
            Fcvtxn => vec![Type::F64],
            Faddp | Fminp | Fmaxp | Fminnmp | Fmaxnmp => vec![Type::Vector(if ty == Type::F32 {
                VectorType::F32x2
            } else {
                VectorType::F64x2
            })],
            Sqxtn | Uqxtn | Sqxtun => vec![wide],
            Sqshrn | Uqshrn | Sqrshrn | Uqrshrn | Sqshrun | Sqrshrun => vec![wide, Type::I32],
            Sqdmull => vec![narrow; 2],
            Sqdmlal | Sqdmlsl => vec![ty, narrow, narrow],
            Ssra | Usra | Srsra | Ursra | Sli | Sri => vec![ty, ty, Type::I32],
            SshrImm | UshrImm | SrshrImm | UrshrImm | ShlImm | SqshlImm | UqshlImm | Sqshlu => {
                vec![ty, Type::I32]
            }
            _ => vec![ty; self.arity()],
        }
    }
}

impl NeonIntrinsic {
    /// Additional baseline integer lane and accumulator families.
    pub fn is_integer_misc(self) -> bool {
        matches!(
            self,
            Self::Cls
                | Self::Rbit
                | Self::Mla
                | Self::Mls
                | Self::Saba
                | Self::Uaba
                | Self::Sadalp
                | Self::Uadalp
        )
    }
}
