//! Lane-local BW byte shuffles and full-vector word permutations are distinct.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwPermuteKind {
    ByteShuffle,
    Align,
    HalfHigh,
    HalfLow,
    Permute,
    TwoTable,
    ShiftLeft,
    ShiftRight,
}
impl X86Avx512BwPermuteKind {
    pub const fn control_index(self) -> Option<usize> {
        match self {
            Self::Align => Some(4),
            Self::HalfHigh | Self::HalfLow => Some(3),
            Self::ShiftLeft | Self::ShiftRight => Some(1),
            _ => None,
        }
    }
    pub const fn unmasked(self) -> bool {
        matches!(self, Self::ShiftLeft | Self::ShiftRight)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwPermute {
    pub name: &'static str,
    pub kind: X86Avx512BwPermuteKind,
    pub vector: VectorType,
    pub mask: Type,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// Pure BW shuffles require VL only for their 128- and 256-bit forms.
    pub fn x86_avx512_bw_permute(self) -> Option<X86Avx512BwPermute> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$vector:ident,$mask:ident,$opcode:literal) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const M: Type = Type::$mask;
                use X86Avx512BwPermuteKind::*;
                X86Avx512BwPermute {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    mask: M,
                    opcode: $opcode,
                    params: match $kind {
                        ByteShuffle | Permute => &[V, M, V, V],
                        Align => &[V, M, V, V, Type::I32],
                        HalfHigh | HalfLow => &[V, M, V, Type::I32],
                        TwoTable => &[V, M, V, V, V],
                        ShiftLeft | ShiftRight => &[V, Type::I32],
                    },
                    returns: &[V],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwPshufb128 => {
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb128", I8x16, I16, "vpshufb")
            }
            Self::X86Avx512BwPalignr128 => {
                descriptor!(Align, "x86_avx512bw_palignr128", I8x16, I16, "vpalignr")
            }
            Self::X86Avx512BwPshufhw128 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw128", I16x8, I8, "vpshufhw")
            }
            Self::X86Avx512BwPshuflw128 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw128", I16x8, I8, "vpshuflw")
            }
            Self::X86Avx512BwPermw128 => {
                descriptor!(Permute, "x86_avx512bw_permw128", I16x8, I8, "vpermw")
            }
            Self::X86Avx512BwPermt2w128 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w128", I16x8, I8, "vpermt2w")
            }
            Self::X86Avx512BwPshufb256 => {
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb256", I8x32, I32, "vpshufb")
            }
            Self::X86Avx512BwPalignr256 => {
                descriptor!(Align, "x86_avx512bw_palignr256", I8x32, I32, "vpalignr")
            }
            Self::X86Avx512BwPshufhw256 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw256", I16x16, I16, "vpshufhw")
            }
            Self::X86Avx512BwPshuflw256 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw256", I16x16, I16, "vpshuflw")
            }
            Self::X86Avx512BwPermw256 => {
                descriptor!(Permute, "x86_avx512bw_permw256", I16x16, I16, "vpermw")
            }
            Self::X86Avx512BwPermt2w256 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w256", I16x16, I16, "vpermt2w")
            }
            Self::X86Avx512BwPshufb512 => {
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb512", I8x64, I64, "vpshufb")
            }
            Self::X86Avx512BwPalignr512 => {
                descriptor!(Align, "x86_avx512bw_palignr512", I8x64, I64, "vpalignr")
            }
            Self::X86Avx512BwPshufhw512 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw512", I16x32, I32, "vpshufhw")
            }
            Self::X86Avx512BwPshuflw512 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw512", I16x32, I32, "vpshuflw")
            }
            Self::X86Avx512BwPermw512 => {
                descriptor!(Permute, "x86_avx512bw_permw512", I16x32, I32, "vpermw")
            }
            Self::X86Avx512BwPermt2w512 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w512", I16x32, I32, "vpermt2w")
            }
            Self::X86Avx512BwPslldq512 => {
                descriptor!(ShiftLeft, "x86_avx512bw_pslldq512", I8x64, I64, "vpslldq")
            }
            Self::X86Avx512BwPsrldq512 => {
                descriptor!(ShiftRight, "x86_avx512bw_psrldq512", I8x64, I64, "vpsrldq")
            }
            _ => return None,
        })
    }
}
