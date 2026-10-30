//! Pure AVX512DQ lane transfers. Encoded width controls VL legality.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512DqLaneKind {
    Register,
    Memory,
    Extract,
    Insert,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512DqLane {
    pub opcode: &'static str,
    pub width: u16,
    pub kind: X86Avx512DqLaneKind,
    pub input: VectorType,
    pub output: VectorType,
    pub control: Option<(usize, i64)>,
}
impl Intrinsic {
    pub fn x86_avx512_dq_lane(self) -> Option<X86Avx512DqLane> {
        macro_rules! descriptor {
            ($opcode:literal,$width:literal,$kind:ident,$input:ident,$output:ident,$max:literal) => {
                X86Avx512DqLane {
                    opcode: $opcode,
                    width: $width,
                    kind: X86Avx512DqLaneKind::$kind,
                    input: VectorType::$input,
                    output: VectorType::$output,
                    control: match X86Avx512DqLaneKind::$kind {
                        X86Avx512DqLaneKind::Extract => Some((3, $max)),
                        X86Avx512DqLaneKind::Insert => Some((4, $max)),
                        _ => None,
                    },
                }
            };
        }
        Some(match self {
            Self::X86Avx512DqBroadcasti32x2128 => {
                descriptor!("vbroadcasti32x2", 128, Register, I32x4, I32x4, 0)
            }
            Self::X86Avx512DqBroadcastf32x2256 => {
                descriptor!("vbroadcastf32x2", 256, Register, F32x4, F32x8, 0)
            }
            Self::X86Avx512DqBroadcastf64x2256 => {
                descriptor!("vbroadcastf64x2", 256, Memory, F64x2, F64x4, 0)
            }
            Self::X86Avx512DqBroadcasti32x2256 => {
                descriptor!("vbroadcasti32x2", 256, Register, I32x4, I32x8, 0)
            }
            Self::X86Avx512DqBroadcasti64x2256 => {
                descriptor!("vbroadcasti64x2", 256, Memory, I64x2, I64x4, 0)
            }
            Self::X86Avx512DqExtractf64x2256 => {
                descriptor!("vextractf64x2", 256, Extract, F64x4, F64x2, 1)
            }
            Self::X86Avx512DqExtracti64x2256 => {
                descriptor!("vextracti64x2", 256, Extract, I64x4, I64x2, 1)
            }
            Self::X86Avx512DqInsertf64x2256 => {
                descriptor!("vinsertf64x2", 256, Insert, F64x2, F64x4, 1)
            }
            Self::X86Avx512DqInserti64x2256 => {
                descriptor!("vinserti64x2", 256, Insert, I64x2, I64x4, 1)
            }
            Self::X86Avx512DqBroadcastf32x2512 => {
                descriptor!("vbroadcastf32x2", 512, Register, F32x4, F32x16, 0)
            }
            Self::X86Avx512DqBroadcastf32x8512 => {
                descriptor!("vbroadcastf32x8", 512, Memory, F32x8, F32x16, 0)
            }
            Self::X86Avx512DqBroadcastf64x2512 => {
                descriptor!("vbroadcastf64x2", 512, Memory, F64x2, F64x8, 0)
            }
            Self::X86Avx512DqBroadcasti32x2512 => {
                descriptor!("vbroadcasti32x2", 512, Register, I32x4, I32x16, 0)
            }
            Self::X86Avx512DqBroadcasti32x8512 => {
                descriptor!("vbroadcasti32x8", 512, Memory, I32x8, I32x16, 0)
            }
            Self::X86Avx512DqBroadcasti64x2512 => {
                descriptor!("vbroadcasti64x2", 512, Memory, I64x2, I64x8, 0)
            }
            Self::X86Avx512DqExtractf32x8512 => {
                descriptor!("vextractf32x8", 512, Extract, F32x16, F32x8, 1)
            }
            Self::X86Avx512DqExtractf64x2512 => {
                descriptor!("vextractf64x2", 512, Extract, F64x8, F64x2, 3)
            }
            Self::X86Avx512DqExtracti32x8512 => {
                descriptor!("vextracti32x8", 512, Extract, I32x16, I32x8, 1)
            }
            Self::X86Avx512DqExtracti64x2512 => {
                descriptor!("vextracti64x2", 512, Extract, I64x8, I64x2, 3)
            }
            Self::X86Avx512DqInsertf32x8512 => {
                descriptor!("vinsertf32x8", 512, Insert, F32x8, F32x16, 1)
            }
            Self::X86Avx512DqInsertf64x2512 => {
                descriptor!("vinsertf64x2", 512, Insert, F64x2, F64x8, 3)
            }
            Self::X86Avx512DqInserti32x8512 => {
                descriptor!("vinserti32x8", 512, Insert, I32x8, I32x16, 1)
            }
            Self::X86Avx512DqInserti64x2512 => {
                descriptor!("vinserti64x2", 512, Insert, I64x2, I64x8, 3)
            }
            _ => return None,
        })
    }
}
