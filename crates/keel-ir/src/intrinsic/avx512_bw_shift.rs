//! Raw BW word shifts saturate counts, unlike portable IR modulo shifts.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwShiftKind {
    Immediate,
    Shared,
    Variable,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwShift {
    pub name: &'static str,
    pub kind: X86Avx512BwShiftKind,
    pub vector: VectorType,
    pub mask: Type,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_shift(self) -> Option<X86Avx512BwShift> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$vector:ident,$mask:ident,$opcode:literal) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const M: Type = Type::$mask;
                use X86Avx512BwShiftKind::*;
                X86Avx512BwShift {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    mask: M,
                    opcode: $opcode,
                    params: match $kind {
                        Immediate => &[V, M, V, Type::I32],
                        Shared => &[V, M, V, Type::Vector(VectorType::I64x2)],
                        Variable => &[V, M, V, V],
                    },
                    returns: &[V],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwSlliw128 => {
                descriptor!(Immediate, "x86_avx512bw_slliw128", I16x8, I8, "vpsllw")
            }
            Self::X86Avx512BwSllw128 => {
                descriptor!(Shared, "x86_avx512bw_sllw128", I16x8, I8, "vpsllw")
            }
            Self::X86Avx512BwSllvw128 => {
                descriptor!(Variable, "x86_avx512bw_sllvw128", I16x8, I8, "vpsllvw")
            }
            Self::X86Avx512BwSrliw128 => {
                descriptor!(Immediate, "x86_avx512bw_srliw128", I16x8, I8, "vpsrlw")
            }
            Self::X86Avx512BwSrlw128 => {
                descriptor!(Shared, "x86_avx512bw_srlw128", I16x8, I8, "vpsrlw")
            }
            Self::X86Avx512BwSrlvw128 => {
                descriptor!(Variable, "x86_avx512bw_srlvw128", I16x8, I8, "vpsrlvw")
            }
            Self::X86Avx512BwSraiw128 => {
                descriptor!(Immediate, "x86_avx512bw_sraiw128", I16x8, I8, "vpsraw")
            }
            Self::X86Avx512BwSraw128 => {
                descriptor!(Shared, "x86_avx512bw_sraw128", I16x8, I8, "vpsraw")
            }
            Self::X86Avx512BwSravw128 => {
                descriptor!(Variable, "x86_avx512bw_sravw128", I16x8, I8, "vpsravw")
            }
            Self::X86Avx512BwSlliw256 => {
                descriptor!(Immediate, "x86_avx512bw_slliw256", I16x16, I16, "vpsllw")
            }
            Self::X86Avx512BwSllw256 => {
                descriptor!(Shared, "x86_avx512bw_sllw256", I16x16, I16, "vpsllw")
            }
            Self::X86Avx512BwSllvw256 => {
                descriptor!(Variable, "x86_avx512bw_sllvw256", I16x16, I16, "vpsllvw")
            }
            Self::X86Avx512BwSrliw256 => {
                descriptor!(Immediate, "x86_avx512bw_srliw256", I16x16, I16, "vpsrlw")
            }
            Self::X86Avx512BwSrlw256 => {
                descriptor!(Shared, "x86_avx512bw_srlw256", I16x16, I16, "vpsrlw")
            }
            Self::X86Avx512BwSrlvw256 => {
                descriptor!(Variable, "x86_avx512bw_srlvw256", I16x16, I16, "vpsrlvw")
            }
            Self::X86Avx512BwSraiw256 => {
                descriptor!(Immediate, "x86_avx512bw_sraiw256", I16x16, I16, "vpsraw")
            }
            Self::X86Avx512BwSraw256 => {
                descriptor!(Shared, "x86_avx512bw_sraw256", I16x16, I16, "vpsraw")
            }
            Self::X86Avx512BwSravw256 => {
                descriptor!(Variable, "x86_avx512bw_sravw256", I16x16, I16, "vpsravw")
            }
            Self::X86Avx512BwSlliw512 => {
                descriptor!(Immediate, "x86_avx512bw_slliw512", I16x32, I32, "vpsllw")
            }
            Self::X86Avx512BwSllw512 => {
                descriptor!(Shared, "x86_avx512bw_sllw512", I16x32, I32, "vpsllw")
            }
            Self::X86Avx512BwSllvw512 => {
                descriptor!(Variable, "x86_avx512bw_sllvw512", I16x32, I32, "vpsllvw")
            }
            Self::X86Avx512BwSrliw512 => {
                descriptor!(Immediate, "x86_avx512bw_srliw512", I16x32, I32, "vpsrlw")
            }
            Self::X86Avx512BwSrlw512 => {
                descriptor!(Shared, "x86_avx512bw_srlw512", I16x32, I32, "vpsrlw")
            }
            Self::X86Avx512BwSrlvw512 => {
                descriptor!(Variable, "x86_avx512bw_srlvw512", I16x32, I32, "vpsrlvw")
            }
            Self::X86Avx512BwSraiw512 => {
                descriptor!(Immediate, "x86_avx512bw_sraiw512", I16x32, I32, "vpsraw")
            }
            Self::X86Avx512BwSraw512 => {
                descriptor!(Shared, "x86_avx512bw_sraw512", I16x32, I32, "vpsraw")
            }
            Self::X86Avx512BwSravw512 => {
                descriptor!(Variable, "x86_avx512bw_sravw512", I16x32, I32, "vpsravw")
            }
            _ => return None,
        })
    }
}
