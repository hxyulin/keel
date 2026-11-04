//! Lane-local BW byte shuffles and full-vector word permutations are distinct.
use super::{Intrinsic, Type, VectorType};
use keel_asm_ir::x86::EvexMnemonic;
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
    pub opcode: EvexMnemonic,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// Pure BW shuffles require VL only for their 128- and 256-bit forms.
    pub fn x86_avx512_bw_permute(self) -> Option<X86Avx512BwPermute> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$vector:ident,$mask:ident,$opcode:ident) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const M: Type = Type::$mask;
                use X86Avx512BwPermuteKind::*;
                X86Avx512BwPermute {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    mask: M,
                    opcode: EvexMnemonic::$opcode,
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
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb128", I8x16, I16, Vpshufb)
            }
            Self::X86Avx512BwPalignr128 => {
                descriptor!(Align, "x86_avx512bw_palignr128", I8x16, I16, Vpalignr)
            }
            Self::X86Avx512BwPshufhw128 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw128", I16x8, I8, Vpshufhw)
            }
            Self::X86Avx512BwPshuflw128 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw128", I16x8, I8, Vpshuflw)
            }
            Self::X86Avx512BwPermw128 => {
                descriptor!(Permute, "x86_avx512bw_permw128", I16x8, I8, Vpermw)
            }
            Self::X86Avx512BwPermt2w128 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w128", I16x8, I8, Vpermt2w)
            }
            Self::X86Avx512BwPshufb256 => {
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb256", I8x32, I32, Vpshufb)
            }
            Self::X86Avx512BwPalignr256 => {
                descriptor!(Align, "x86_avx512bw_palignr256", I8x32, I32, Vpalignr)
            }
            Self::X86Avx512BwPshufhw256 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw256", I16x16, I16, Vpshufhw)
            }
            Self::X86Avx512BwPshuflw256 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw256", I16x16, I16, Vpshuflw)
            }
            Self::X86Avx512BwPermw256 => {
                descriptor!(Permute, "x86_avx512bw_permw256", I16x16, I16, Vpermw)
            }
            Self::X86Avx512BwPermt2w256 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w256", I16x16, I16, Vpermt2w)
            }
            Self::X86Avx512BwPshufb512 => {
                descriptor!(ByteShuffle, "x86_avx512bw_pshufb512", I8x64, I64, Vpshufb)
            }
            Self::X86Avx512BwPalignr512 => {
                descriptor!(Align, "x86_avx512bw_palignr512", I8x64, I64, Vpalignr)
            }
            Self::X86Avx512BwPshufhw512 => {
                descriptor!(HalfHigh, "x86_avx512bw_pshufhw512", I16x32, I32, Vpshufhw)
            }
            Self::X86Avx512BwPshuflw512 => {
                descriptor!(HalfLow, "x86_avx512bw_pshuflw512", I16x32, I32, Vpshuflw)
            }
            Self::X86Avx512BwPermw512 => {
                descriptor!(Permute, "x86_avx512bw_permw512", I16x32, I32, Vpermw)
            }
            Self::X86Avx512BwPermt2w512 => {
                descriptor!(TwoTable, "x86_avx512bw_permt2w512", I16x32, I32, Vpermt2w)
            }
            Self::X86Avx512BwPslldq512 => {
                descriptor!(ShiftLeft, "x86_avx512bw_pslldq512", I8x64, I64, Vpslldq)
            }
            Self::X86Avx512BwPsrldq512 => {
                descriptor!(ShiftRight, "x86_avx512bw_psrldq512", I8x64, I64, Vpsrldq)
            }
            _ => return None,
        })
    }
}
