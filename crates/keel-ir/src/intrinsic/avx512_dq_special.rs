//! Raw DQ floating specials observe MXCSR, including DAZ during classification.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512DqSpecialKind {
    Range,
    Reduce,
    Fpclass,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512DqSpecial {
    pub kind: X86Avx512DqSpecialKind,
    pub opcode: EvexMnemonic,
    pub vector: VectorType,
    pub scalar: bool,
    pub immediate: usize,
    pub sae: Option<usize>,
}
impl X86Avx512DqSpecial {
    pub fn valid_control(self, index: usize, value: i64) -> bool {
        if index == self.immediate {
            (0..=if self.kind == X86Avx512DqSpecialKind::Range {
                15
            } else {
                255
            })
                .contains(&value)
        } else {
            self.sae == Some(index) && matches!(value, 4 | 8)
        }
    }
}
impl Intrinsic {
    pub fn x86_avx512_dq_special(self) -> Option<X86Avx512DqSpecial> {
        macro_rules! descriptor {
            ($kind:ident,$opcode:ident,$vector:ident,$scalar:literal,$immediate:literal,$sae:expr) => {
                X86Avx512DqSpecial {
                    kind: X86Avx512DqSpecialKind::$kind,
                    opcode: EvexMnemonic::$opcode,
                    vector: VectorType::$vector,
                    scalar: $scalar,
                    immediate: $immediate,
                    sae: $sae,
                }
            };
        }
        Some(match self {
            Self::X86Avx512DqRangeps128 => {
                descriptor!(Range, Vrangeps, F32x4, false, 4, None)
            }
            Self::X86Avx512DqRangepd128 => {
                descriptor!(Range, Vrangepd, F64x2, false, 4, None)
            }
            Self::X86Avx512DqReduceps128 => {
                descriptor!(Reduce, Vreduceps, F32x4, false, 3, None)
            }
            Self::X86Avx512DqReducepd128 => {
                descriptor!(Reduce, Vreducepd, F64x2, false, 3, None)
            }
            Self::X86Avx512DqFpclassps128 => {
                descriptor!(Fpclass, Vfpclassps, F32x4, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd128 => {
                descriptor!(Fpclass, Vfpclasspd, F64x2, false, 2, None)
            }
            Self::X86Avx512DqRangeps256 => {
                descriptor!(Range, Vrangeps, F32x8, false, 4, None)
            }
            Self::X86Avx512DqRangepd256 => {
                descriptor!(Range, Vrangepd, F64x4, false, 4, None)
            }
            Self::X86Avx512DqReduceps256 => {
                descriptor!(Reduce, Vreduceps, F32x8, false, 3, None)
            }
            Self::X86Avx512DqReducepd256 => {
                descriptor!(Reduce, Vreducepd, F64x4, false, 3, None)
            }
            Self::X86Avx512DqFpclassps256 => {
                descriptor!(Fpclass, Vfpclassps, F32x8, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd256 => {
                descriptor!(Fpclass, Vfpclasspd, F64x4, false, 2, None)
            }
            Self::X86Avx512DqRangeps512 => {
                descriptor!(Range, Vrangeps, F32x16, false, 4, Some(5))
            }
            Self::X86Avx512DqRangepd512 => {
                descriptor!(Range, Vrangepd, F64x8, false, 4, Some(5))
            }
            Self::X86Avx512DqReduceps512 => {
                descriptor!(Reduce, Vreduceps, F32x16, false, 3, Some(4))
            }
            Self::X86Avx512DqReducepd512 => {
                descriptor!(Reduce, Vreducepd, F64x8, false, 3, Some(4))
            }
            Self::X86Avx512DqFpclassps512 => {
                descriptor!(Fpclass, Vfpclassps, F32x16, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd512 => {
                descriptor!(Fpclass, Vfpclasspd, F64x8, false, 2, None)
            }
            Self::X86Avx512DqRangess => {
                descriptor!(Range, Vrangess, F32x4, true, 4, Some(5))
            }
            Self::X86Avx512DqRangesd => {
                descriptor!(Range, Vrangesd, F64x2, true, 4, Some(5))
            }
            Self::X86Avx512DqReducess => {
                descriptor!(Reduce, Vreducess, F32x4, true, 4, Some(5))
            }
            Self::X86Avx512DqReducesd => {
                descriptor!(Reduce, Vreducesd, F64x2, true, 4, Some(5))
            }
            Self::X86Avx512DqFpclassss => {
                descriptor!(Fpclass, Vfpclassss, F32x4, true, 2, None)
            }
            Self::X86Avx512DqFpclasssd => {
                descriptor!(Fpclass, Vfpclasssd, F64x2, true, 2, None)
            }
            _ => return None,
        })
    }
}
