//! DQ byte mask algebra and byte/word addition use scalar SSA homes.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512DqMaskAluKind {
    Unary,
    Binary,
    Shift,
    Test,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512DqMaskAlu {
    pub opcode: &'static str,
    pub kind: X86Avx512DqMaskAluKind,
    pub ty: Type,
}
impl Intrinsic {
    pub fn x86_avx512_dq_mask_alu(self) -> Option<X86Avx512DqMaskAlu> {
        macro_rules! descriptor {
            ($opcode:literal,$kind:ident,$ty:ident) => {
                X86Avx512DqMaskAlu {
                    opcode: $opcode,
                    kind: X86Avx512DqMaskAluKind::$kind,
                    ty: Type::$ty,
                }
            };
        }
        Some(match self {
            Self::X86Avx512DqKandb => descriptor!("kandb", Binary, I8),
            Self::X86Avx512DqKandnb => descriptor!("kandnb", Binary, I8),
            Self::X86Avx512DqKorb => descriptor!("korb", Binary, I8),
            Self::X86Avx512DqKxorb => descriptor!("kxorb", Binary, I8),
            Self::X86Avx512DqKxnorb => descriptor!("kxnorb", Binary, I8),
            Self::X86Avx512DqKnotb => descriptor!("knotb", Unary, I8),
            Self::X86Avx512DqKshiftlb => descriptor!("kshiftlb", Shift, I8),
            Self::X86Avx512DqKshiftrb => descriptor!("kshiftrb", Shift, I8),
            Self::X86Avx512DqKtestb => descriptor!("ktestb", Test, I8),
            Self::X86Avx512DqKortestb => descriptor!("kortestb", Test, I8),
            Self::X86Avx512DqKaddb => descriptor!("kaddb", Binary, I8),
            Self::X86Avx512DqKaddw => descriptor!("kaddw", Binary, I16),
            _ => return None,
        })
    }
}
