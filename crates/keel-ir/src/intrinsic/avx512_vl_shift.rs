//! Dword/qword shifts saturate counts; rotations reduce counts modulo lane width.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512VlShiftKind {
    Immediate,
    Shared,
    Variable,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlShift {
    pub name: &'static str,
    pub kind: X86Avx512VlShiftKind,
    pub vector: VectorType,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// Count form and exact lane shape for pure AVX512F plus AVX512VL shifts/rotates.
    pub fn x86_avx512_vl_shift(self) -> Option<X86Avx512VlShift> {
        macro_rules! descriptor {
            ($kind:ident, $name:literal, $vector:ident, $opcode:literal) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                use X86Avx512VlShiftKind::*;
                X86Avx512VlShift {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    opcode: $opcode,
                    params: match $kind {
                        Immediate => &[V, Type::I8, V, Type::I32],
                        Shared => &[V, Type::I8, V, Type::Vector(VectorType::I64x2)],
                        Variable => &[V, Type::I8, V, V],
                    },
                    returns: &[V],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlSllid128 => {
                descriptor!(Immediate, "x86_avx512vl_sllid128", I32x4, "vpslld")
            }
            Self::X86Avx512VlSlliq128 => {
                descriptor!(Immediate, "x86_avx512vl_slliq128", I64x2, "vpsllq")
            }
            Self::X86Avx512VlSrlid128 => {
                descriptor!(Immediate, "x86_avx512vl_srlid128", I32x4, "vpsrld")
            }
            Self::X86Avx512VlSrliq128 => {
                descriptor!(Immediate, "x86_avx512vl_srliq128", I64x2, "vpsrlq")
            }
            Self::X86Avx512VlSraid128 => {
                descriptor!(Immediate, "x86_avx512vl_sraid128", I32x4, "vpsrad")
            }
            Self::X86Avx512VlSraiq128 => {
                descriptor!(Immediate, "x86_avx512vl_sraiq128", I64x2, "vpsraq")
            }
            Self::X86Avx512VlSlld128 => {
                descriptor!(Shared, "x86_avx512vl_slld128", I32x4, "vpslld")
            }
            Self::X86Avx512VlSllq128 => {
                descriptor!(Shared, "x86_avx512vl_sllq128", I64x2, "vpsllq")
            }
            Self::X86Avx512VlSrld128 => {
                descriptor!(Shared, "x86_avx512vl_srld128", I32x4, "vpsrld")
            }
            Self::X86Avx512VlSrlq128 => {
                descriptor!(Shared, "x86_avx512vl_srlq128", I64x2, "vpsrlq")
            }
            Self::X86Avx512VlSrad128 => {
                descriptor!(Shared, "x86_avx512vl_srad128", I32x4, "vpsrad")
            }
            Self::X86Avx512VlSraq128 => {
                descriptor!(Shared, "x86_avx512vl_sraq128", I64x2, "vpsraq")
            }
            Self::X86Avx512VlSllvd128 => {
                descriptor!(Variable, "x86_avx512vl_sllvd128", I32x4, "vpsllvd")
            }
            Self::X86Avx512VlSllvq128 => {
                descriptor!(Variable, "x86_avx512vl_sllvq128", I64x2, "vpsllvq")
            }
            Self::X86Avx512VlSrlvd128 => {
                descriptor!(Variable, "x86_avx512vl_srlvd128", I32x4, "vpsrlvd")
            }
            Self::X86Avx512VlSrlvq128 => {
                descriptor!(Variable, "x86_avx512vl_srlvq128", I64x2, "vpsrlvq")
            }
            Self::X86Avx512VlSravd128 => {
                descriptor!(Variable, "x86_avx512vl_sravd128", I32x4, "vpsravd")
            }
            Self::X86Avx512VlSravq128 => {
                descriptor!(Variable, "x86_avx512vl_sravq128", I64x2, "vpsravq")
            }
            Self::X86Avx512VlRold128 => {
                descriptor!(Immediate, "x86_avx512vl_rold128", I32x4, "vprold")
            }
            Self::X86Avx512VlRolq128 => {
                descriptor!(Immediate, "x86_avx512vl_rolq128", I64x2, "vprolq")
            }
            Self::X86Avx512VlRord128 => {
                descriptor!(Immediate, "x86_avx512vl_rord128", I32x4, "vprord")
            }
            Self::X86Avx512VlRorq128 => {
                descriptor!(Immediate, "x86_avx512vl_rorq128", I64x2, "vprorq")
            }
            Self::X86Avx512VlRolvd128 => {
                descriptor!(Variable, "x86_avx512vl_rolvd128", I32x4, "vprolvd")
            }
            Self::X86Avx512VlRolvq128 => {
                descriptor!(Variable, "x86_avx512vl_rolvq128", I64x2, "vprolvq")
            }
            Self::X86Avx512VlRorvd128 => {
                descriptor!(Variable, "x86_avx512vl_rorvd128", I32x4, "vprorvd")
            }
            Self::X86Avx512VlRorvq128 => {
                descriptor!(Variable, "x86_avx512vl_rorvq128", I64x2, "vprorvq")
            }
            Self::X86Avx512VlSllid256 => {
                descriptor!(Immediate, "x86_avx512vl_sllid256", I32x8, "vpslld")
            }
            Self::X86Avx512VlSlliq256 => {
                descriptor!(Immediate, "x86_avx512vl_slliq256", I64x4, "vpsllq")
            }
            Self::X86Avx512VlSrlid256 => {
                descriptor!(Immediate, "x86_avx512vl_srlid256", I32x8, "vpsrld")
            }
            Self::X86Avx512VlSrliq256 => {
                descriptor!(Immediate, "x86_avx512vl_srliq256", I64x4, "vpsrlq")
            }
            Self::X86Avx512VlSraid256 => {
                descriptor!(Immediate, "x86_avx512vl_sraid256", I32x8, "vpsrad")
            }
            Self::X86Avx512VlSraiq256 => {
                descriptor!(Immediate, "x86_avx512vl_sraiq256", I64x4, "vpsraq")
            }
            Self::X86Avx512VlSlld256 => {
                descriptor!(Shared, "x86_avx512vl_slld256", I32x8, "vpslld")
            }
            Self::X86Avx512VlSllq256 => {
                descriptor!(Shared, "x86_avx512vl_sllq256", I64x4, "vpsllq")
            }
            Self::X86Avx512VlSrld256 => {
                descriptor!(Shared, "x86_avx512vl_srld256", I32x8, "vpsrld")
            }
            Self::X86Avx512VlSrlq256 => {
                descriptor!(Shared, "x86_avx512vl_srlq256", I64x4, "vpsrlq")
            }
            Self::X86Avx512VlSrad256 => {
                descriptor!(Shared, "x86_avx512vl_srad256", I32x8, "vpsrad")
            }
            Self::X86Avx512VlSraq256 => {
                descriptor!(Shared, "x86_avx512vl_sraq256", I64x4, "vpsraq")
            }
            Self::X86Avx512VlSllvd256 => {
                descriptor!(Variable, "x86_avx512vl_sllvd256", I32x8, "vpsllvd")
            }
            Self::X86Avx512VlSllvq256 => {
                descriptor!(Variable, "x86_avx512vl_sllvq256", I64x4, "vpsllvq")
            }
            Self::X86Avx512VlSrlvd256 => {
                descriptor!(Variable, "x86_avx512vl_srlvd256", I32x8, "vpsrlvd")
            }
            Self::X86Avx512VlSrlvq256 => {
                descriptor!(Variable, "x86_avx512vl_srlvq256", I64x4, "vpsrlvq")
            }
            Self::X86Avx512VlSravd256 => {
                descriptor!(Variable, "x86_avx512vl_sravd256", I32x8, "vpsravd")
            }
            Self::X86Avx512VlSravq256 => {
                descriptor!(Variable, "x86_avx512vl_sravq256", I64x4, "vpsravq")
            }
            Self::X86Avx512VlRold256 => {
                descriptor!(Immediate, "x86_avx512vl_rold256", I32x8, "vprold")
            }
            Self::X86Avx512VlRolq256 => {
                descriptor!(Immediate, "x86_avx512vl_rolq256", I64x4, "vprolq")
            }
            Self::X86Avx512VlRord256 => {
                descriptor!(Immediate, "x86_avx512vl_rord256", I32x8, "vprord")
            }
            Self::X86Avx512VlRorq256 => {
                descriptor!(Immediate, "x86_avx512vl_rorq256", I64x4, "vprorq")
            }
            Self::X86Avx512VlRolvd256 => {
                descriptor!(Variable, "x86_avx512vl_rolvd256", I32x8, "vprolvd")
            }
            Self::X86Avx512VlRolvq256 => {
                descriptor!(Variable, "x86_avx512vl_rolvq256", I64x4, "vprolvq")
            }
            Self::X86Avx512VlRorvd256 => {
                descriptor!(Variable, "x86_avx512vl_rorvd256", I32x8, "vprorvd")
            }
            Self::X86Avx512VlRorvq256 => {
                descriptor!(Variable, "x86_avx512vl_rorvq256", I64x4, "vprorvq")
            }
            _ => return None,
        })
    }
}
