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
    pub opcode: Opcode,
    pub kind: X86Avx512DqMaskAluKind,
    pub ty: Type,
}
impl Intrinsic {
    pub fn x86_avx512_dq_mask_alu(self) -> Option<X86Avx512DqMaskAlu> {
        macro_rules! descriptor {
            ($opcode:expr,$kind:ident,$ty:ident) => {
                X86Avx512DqMaskAlu {
                    opcode: $opcode,
                    kind: X86Avx512DqMaskAluKind::$kind,
                    ty: Type::$ty,
                }
            };
        }
        Some(match self {
            Self::X86Avx512DqKandb => descriptor!(op::KANDB, Binary, I8),
            Self::X86Avx512DqKandnb => descriptor!(op::KANDNB, Binary, I8),
            Self::X86Avx512DqKorb => descriptor!(op::KORB, Binary, I8),
            Self::X86Avx512DqKxorb => descriptor!(op::KXORB, Binary, I8),
            Self::X86Avx512DqKxnorb => descriptor!(op::KXNORB, Binary, I8),
            Self::X86Avx512DqKnotb => descriptor!(op::KNOTB, Unary, I8),
            Self::X86Avx512DqKshiftlb => descriptor!(op::KSHIFTLB, Shift, I8),
            Self::X86Avx512DqKshiftrb => descriptor!(op::KSHIFTRB, Shift, I8),
            Self::X86Avx512DqKtestb => descriptor!(op::KTESTB, Test, I8),
            Self::X86Avx512DqKortestb => descriptor!(op::KORTESTB, Test, I8),
            Self::X86Avx512DqKaddb => descriptor!(op::KADDB, Binary, I8),
            Self::X86Avx512DqKaddw => descriptor!(op::KADDW, Binary, I16),
            _ => return None,
        })
    }
}
