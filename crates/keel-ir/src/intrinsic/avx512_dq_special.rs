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
    pub opcode: &'static str,
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
            ($kind:ident,$opcode:literal,$vector:ident,$scalar:literal,$immediate:literal,$sae:expr) => {
                X86Avx512DqSpecial {
                    kind: X86Avx512DqSpecialKind::$kind,
                    opcode: $opcode,
                    vector: VectorType::$vector,
                    scalar: $scalar,
                    immediate: $immediate,
                    sae: $sae,
                }
            };
        }
        Some(match self {
            Self::X86Avx512DqRangeps128 => descriptor!(Range, "vrangeps", F32x4, false, 4, None),
            Self::X86Avx512DqRangepd128 => descriptor!(Range, "vrangepd", F64x2, false, 4, None),
            Self::X86Avx512DqReduceps128 => descriptor!(Reduce, "vreduceps", F32x4, false, 3, None),
            Self::X86Avx512DqReducepd128 => descriptor!(Reduce, "vreducepd", F64x2, false, 3, None),
            Self::X86Avx512DqFpclassps128 => {
                descriptor!(Fpclass, "vfpclassps", F32x4, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd128 => {
                descriptor!(Fpclass, "vfpclasspd", F64x2, false, 2, None)
            }
            Self::X86Avx512DqRangeps256 => descriptor!(Range, "vrangeps", F32x8, false, 4, None),
            Self::X86Avx512DqRangepd256 => descriptor!(Range, "vrangepd", F64x4, false, 4, None),
            Self::X86Avx512DqReduceps256 => descriptor!(Reduce, "vreduceps", F32x8, false, 3, None),
            Self::X86Avx512DqReducepd256 => descriptor!(Reduce, "vreducepd", F64x4, false, 3, None),
            Self::X86Avx512DqFpclassps256 => {
                descriptor!(Fpclass, "vfpclassps", F32x8, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd256 => {
                descriptor!(Fpclass, "vfpclasspd", F64x4, false, 2, None)
            }
            Self::X86Avx512DqRangeps512 => {
                descriptor!(Range, "vrangeps", F32x16, false, 4, Some(5))
            }
            Self::X86Avx512DqRangepd512 => descriptor!(Range, "vrangepd", F64x8, false, 4, Some(5)),
            Self::X86Avx512DqReduceps512 => {
                descriptor!(Reduce, "vreduceps", F32x16, false, 3, Some(4))
            }
            Self::X86Avx512DqReducepd512 => {
                descriptor!(Reduce, "vreducepd", F64x8, false, 3, Some(4))
            }
            Self::X86Avx512DqFpclassps512 => {
                descriptor!(Fpclass, "vfpclassps", F32x16, false, 2, None)
            }
            Self::X86Avx512DqFpclasspd512 => {
                descriptor!(Fpclass, "vfpclasspd", F64x8, false, 2, None)
            }
            Self::X86Avx512DqRangess => descriptor!(Range, "vrangess", F32x4, true, 4, Some(5)),
            Self::X86Avx512DqRangesd => descriptor!(Range, "vrangesd", F64x2, true, 4, Some(5)),
            Self::X86Avx512DqReducess => descriptor!(Reduce, "vreducess", F32x4, true, 4, Some(5)),
            Self::X86Avx512DqReducesd => descriptor!(Reduce, "vreducesd", F64x2, true, 4, Some(5)),
            Self::X86Avx512DqFpclassss => descriptor!(Fpclass, "vfpclassss", F32x4, true, 2, None),
            Self::X86Avx512DqFpclasssd => descriptor!(Fpclass, "vfpclasssd", F64x2, true, 2, None),
            _ => return None,
        })
    }
}
