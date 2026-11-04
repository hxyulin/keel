//! BW dword/qword mask algebra uses scalar SSA homes.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwMaskAluKind {
    Unary,
    Binary,
    Shift,
    Test,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwMaskAlu {
    pub name: &'static str,
    pub opcode: Opcode,
    pub kind: X86Avx512BwMaskAluKind,
    pub ty: Type,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_mask_alu(self) -> Option<X86Avx512BwMaskAlu> {
        macro_rules! descriptor {
            ($name:literal, $opcode:ident, $kind:ident, $ty:ident) => {{
                const T: Type = Type::$ty;
                use X86Avx512BwMaskAluKind::*;
                X86Avx512BwMaskAlu {
                    name: $name,
                    opcode: op::$opcode,
                    kind: $kind,
                    ty: T,
                    params: match $kind {
                        Unary => &[T],
                        Shift => &[T, Type::I32],
                        Binary | Test => &[T, T],
                    },
                    returns: match $kind {
                        Test => &[Type::I8],
                        _ => &[T],
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwKandd => descriptor!("x86_avx512bw_kandd", KANDD, Binary, I32),
            Self::X86Avx512BwKandnd => descriptor!("x86_avx512bw_kandnd", KANDND, Binary, I32),
            Self::X86Avx512BwKord => descriptor!("x86_avx512bw_kord", KORD, Binary, I32),
            Self::X86Avx512BwKxord => descriptor!("x86_avx512bw_kxord", KXORD, Binary, I32),
            Self::X86Avx512BwKxnord => descriptor!("x86_avx512bw_kxnord", KXNORD, Binary, I32),
            Self::X86Avx512BwKnotd => descriptor!("x86_avx512bw_knotd", KNOTD, Unary, I32),
            Self::X86Avx512BwKshiftld => descriptor!("x86_avx512bw_kshiftld", KSHIFTLD, Shift, I32),
            Self::X86Avx512BwKshiftrd => descriptor!("x86_avx512bw_kshiftrd", KSHIFTRD, Shift, I32),
            Self::X86Avx512BwKaddd => descriptor!("x86_avx512bw_kaddd", KADDD, Binary, I32),
            Self::X86Avx512BwKtestd => descriptor!("x86_avx512bw_ktestd", KTESTD, Test, I32),
            Self::X86Avx512BwKortestd => descriptor!("x86_avx512bw_kortestd", KORTESTD, Test, I32),
            Self::X86Avx512BwKunpckwd => {
                descriptor!("x86_avx512bw_kunpckwd", KUNPCKWD, Binary, I32)
            }
            Self::X86Avx512BwKandq => descriptor!("x86_avx512bw_kandq", KANDQ, Binary, I64),
            Self::X86Avx512BwKandnq => descriptor!("x86_avx512bw_kandnq", KANDNQ, Binary, I64),
            Self::X86Avx512BwKorq => descriptor!("x86_avx512bw_korq", KORQ, Binary, I64),
            Self::X86Avx512BwKxorq => descriptor!("x86_avx512bw_kxorq", KXORQ, Binary, I64),
            Self::X86Avx512BwKxnorq => descriptor!("x86_avx512bw_kxnorq", KXNORQ, Binary, I64),
            Self::X86Avx512BwKnotq => descriptor!("x86_avx512bw_knotq", KNOTQ, Unary, I64),
            Self::X86Avx512BwKshiftlq => descriptor!("x86_avx512bw_kshiftlq", KSHIFTLQ, Shift, I64),
            Self::X86Avx512BwKshiftrq => descriptor!("x86_avx512bw_kshiftrq", KSHIFTRQ, Shift, I64),
            Self::X86Avx512BwKaddq => descriptor!("x86_avx512bw_kaddq", KADDQ, Binary, I64),
            Self::X86Avx512BwKtestq => descriptor!("x86_avx512bw_ktestq", KTESTQ, Test, I64),
            Self::X86Avx512BwKortestq => descriptor!("x86_avx512bw_kortestq", KORTESTQ, Test, I64),
            Self::X86Avx512BwKunpckdq => {
                descriptor!("x86_avx512bw_kunpckdq", KUNPCKDQ, Binary, I64)
            }
            _ => return None,
        })
    }
}
