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
    pub opcode: &'static str,
    pub kind: X86Avx512BwMaskAluKind,
    pub ty: Type,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_mask_alu(self) -> Option<X86Avx512BwMaskAlu> {
        macro_rules! descriptor {
            ($opcode:literal,$kind:ident,$ty:ident) => {{
                const T: Type = Type::$ty;
                use X86Avx512BwMaskAluKind::*;
                X86Avx512BwMaskAlu {
                    name: concat!("x86_avx512bw_", $opcode),
                    opcode: $opcode,
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
            Self::X86Avx512BwKandd => descriptor!("kandd", Binary, I32),
            Self::X86Avx512BwKandnd => descriptor!("kandnd", Binary, I32),
            Self::X86Avx512BwKord => descriptor!("kord", Binary, I32),
            Self::X86Avx512BwKxord => descriptor!("kxord", Binary, I32),
            Self::X86Avx512BwKxnord => descriptor!("kxnord", Binary, I32),
            Self::X86Avx512BwKnotd => descriptor!("knotd", Unary, I32),
            Self::X86Avx512BwKshiftld => descriptor!("kshiftld", Shift, I32),
            Self::X86Avx512BwKshiftrd => descriptor!("kshiftrd", Shift, I32),
            Self::X86Avx512BwKaddd => descriptor!("kaddd", Binary, I32),
            Self::X86Avx512BwKtestd => descriptor!("ktestd", Test, I32),
            Self::X86Avx512BwKortestd => descriptor!("kortestd", Test, I32),
            Self::X86Avx512BwKunpckwd => descriptor!("kunpckwd", Binary, I32),
            Self::X86Avx512BwKandq => descriptor!("kandq", Binary, I64),
            Self::X86Avx512BwKandnq => descriptor!("kandnq", Binary, I64),
            Self::X86Avx512BwKorq => descriptor!("korq", Binary, I64),
            Self::X86Avx512BwKxorq => descriptor!("kxorq", Binary, I64),
            Self::X86Avx512BwKxnorq => descriptor!("kxnorq", Binary, I64),
            Self::X86Avx512BwKnotq => descriptor!("knotq", Unary, I64),
            Self::X86Avx512BwKshiftlq => descriptor!("kshiftlq", Shift, I64),
            Self::X86Avx512BwKshiftrq => descriptor!("kshiftrq", Shift, I64),
            Self::X86Avx512BwKaddq => descriptor!("kaddq", Binary, I64),
            Self::X86Avx512BwKtestq => descriptor!("ktestq", Test, I64),
            Self::X86Avx512BwKortestq => descriptor!("kortestq", Test, I64),
            Self::X86Avx512BwKunpckdq => descriptor!("kunpckdq", Binary, I64),
            _ => return None,
        })
    }
}
