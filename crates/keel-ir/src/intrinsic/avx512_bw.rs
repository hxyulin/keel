//! Explicit byte/word lane and scalar-mask contracts for AVX512BW.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwKind {
    Binary,
    Move,
    Load,
    Store,
}

#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwCore {
    pub name: &'static str,
    pub kind: X86Avx512BwKind,
    pub vector: VectorType,
    pub mask: Type,
    pub opcode: EvexMnemonic,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_core(self) -> Option<X86Avx512BwCore> {
        macro_rules! core {
            ($kind:ident, $name:literal, $vector:ident, $mask:ident, $opcode:ident) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const M: Type = Type::$mask;
                use X86Avx512BwKind::*;
                X86Avx512BwCore {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    mask: M,
                    opcode: EvexMnemonic::$opcode,
                    params: match $kind {
                        Binary => &[V, M, V, V],
                        Move => &[V, M, V],
                        Load | Store => &[Type::Ptr, V, M],
                    },
                    returns: match $kind {
                        Store => &[],
                        _ => &[V],
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwAddb128 => core!(Binary, "x86_avx512bw_addb128", I8x16, I16, Vpaddb),
            Self::X86Avx512BwSubb128 => core!(Binary, "x86_avx512bw_subb128", I8x16, I16, Vpsubb),
            Self::X86Avx512BwAddsb128 => {
                core!(Binary, "x86_avx512bw_addsb128", I8x16, I16, Vpaddsb)
            }
            Self::X86Avx512BwSubsb128 => {
                core!(Binary, "x86_avx512bw_subsb128", I8x16, I16, Vpsubsb)
            }
            Self::X86Avx512BwAddusb128 => {
                core!(Binary, "x86_avx512bw_addusb128", I8x16, I16, Vpaddusb)
            }
            Self::X86Avx512BwSubusb128 => {
                core!(Binary, "x86_avx512bw_subusb128", I8x16, I16, Vpsubusb)
            }
            Self::X86Avx512BwMinsb128 => {
                core!(Binary, "x86_avx512bw_minsb128", I8x16, I16, Vpminsb)
            }
            Self::X86Avx512BwMinub128 => {
                core!(Binary, "x86_avx512bw_minub128", I8x16, I16, Vpminub)
            }
            Self::X86Avx512BwMaxsb128 => {
                core!(Binary, "x86_avx512bw_maxsb128", I8x16, I16, Vpmaxsb)
            }
            Self::X86Avx512BwMaxub128 => {
                core!(Binary, "x86_avx512bw_maxub128", I8x16, I16, Vpmaxub)
            }
            Self::X86Avx512BwMoveb128 => {
                core!(Move, "x86_avx512bw_moveb128", I8x16, I16, Vmovdqu8)
            }
            Self::X86Avx512BwLoadb128 => {
                core!(Load, "x86_avx512bw_loadb128", I8x16, I16, Vmovdqu8)
            }
            Self::X86Avx512BwStoreb128 => {
                core!(Store, "x86_avx512bw_storeb128", I8x16, I16, Vmovdqu8)
            }
            Self::X86Avx512BwAddw128 => core!(Binary, "x86_avx512bw_addw128", I16x8, I8, Vpaddw),
            Self::X86Avx512BwSubw128 => core!(Binary, "x86_avx512bw_subw128", I16x8, I8, Vpsubw),
            Self::X86Avx512BwAddsw128 => {
                core!(Binary, "x86_avx512bw_addsw128", I16x8, I8, Vpaddsw)
            }
            Self::X86Avx512BwSubsw128 => {
                core!(Binary, "x86_avx512bw_subsw128", I16x8, I8, Vpsubsw)
            }
            Self::X86Avx512BwAddusw128 => {
                core!(Binary, "x86_avx512bw_addusw128", I16x8, I8, Vpaddusw)
            }
            Self::X86Avx512BwSubusw128 => {
                core!(Binary, "x86_avx512bw_subusw128", I16x8, I8, Vpsubusw)
            }
            Self::X86Avx512BwMinsw128 => {
                core!(Binary, "x86_avx512bw_minsw128", I16x8, I8, Vpminsw)
            }
            Self::X86Avx512BwMinuw128 => {
                core!(Binary, "x86_avx512bw_minuw128", I16x8, I8, Vpminuw)
            }
            Self::X86Avx512BwMaxsw128 => {
                core!(Binary, "x86_avx512bw_maxsw128", I16x8, I8, Vpmaxsw)
            }
            Self::X86Avx512BwMaxuw128 => {
                core!(Binary, "x86_avx512bw_maxuw128", I16x8, I8, Vpmaxuw)
            }
            Self::X86Avx512BwMovew128 => {
                core!(Move, "x86_avx512bw_movew128", I16x8, I8, Vmovdqu16)
            }
            Self::X86Avx512BwLoadw128 => {
                core!(Load, "x86_avx512bw_loadw128", I16x8, I8, Vmovdqu16)
            }
            Self::X86Avx512BwStorew128 => {
                core!(Store, "x86_avx512bw_storew128", I16x8, I8, Vmovdqu16)
            }
            Self::X86Avx512BwAddb256 => core!(Binary, "x86_avx512bw_addb256", I8x32, I32, Vpaddb),
            Self::X86Avx512BwSubb256 => core!(Binary, "x86_avx512bw_subb256", I8x32, I32, Vpsubb),
            Self::X86Avx512BwAddsb256 => {
                core!(Binary, "x86_avx512bw_addsb256", I8x32, I32, Vpaddsb)
            }
            Self::X86Avx512BwSubsb256 => {
                core!(Binary, "x86_avx512bw_subsb256", I8x32, I32, Vpsubsb)
            }
            Self::X86Avx512BwAddusb256 => {
                core!(Binary, "x86_avx512bw_addusb256", I8x32, I32, Vpaddusb)
            }
            Self::X86Avx512BwSubusb256 => {
                core!(Binary, "x86_avx512bw_subusb256", I8x32, I32, Vpsubusb)
            }
            Self::X86Avx512BwMinsb256 => {
                core!(Binary, "x86_avx512bw_minsb256", I8x32, I32, Vpminsb)
            }
            Self::X86Avx512BwMinub256 => {
                core!(Binary, "x86_avx512bw_minub256", I8x32, I32, Vpminub)
            }
            Self::X86Avx512BwMaxsb256 => {
                core!(Binary, "x86_avx512bw_maxsb256", I8x32, I32, Vpmaxsb)
            }
            Self::X86Avx512BwMaxub256 => {
                core!(Binary, "x86_avx512bw_maxub256", I8x32, I32, Vpmaxub)
            }
            Self::X86Avx512BwMoveb256 => {
                core!(Move, "x86_avx512bw_moveb256", I8x32, I32, Vmovdqu8)
            }
            Self::X86Avx512BwLoadb256 => {
                core!(Load, "x86_avx512bw_loadb256", I8x32, I32, Vmovdqu8)
            }
            Self::X86Avx512BwStoreb256 => {
                core!(Store, "x86_avx512bw_storeb256", I8x32, I32, Vmovdqu8)
            }
            Self::X86Avx512BwAddw256 => {
                core!(Binary, "x86_avx512bw_addw256", I16x16, I16, Vpaddw)
            }
            Self::X86Avx512BwSubw256 => {
                core!(Binary, "x86_avx512bw_subw256", I16x16, I16, Vpsubw)
            }
            Self::X86Avx512BwAddsw256 => {
                core!(Binary, "x86_avx512bw_addsw256", I16x16, I16, Vpaddsw)
            }
            Self::X86Avx512BwSubsw256 => {
                core!(Binary, "x86_avx512bw_subsw256", I16x16, I16, Vpsubsw)
            }
            Self::X86Avx512BwAddusw256 => {
                core!(Binary, "x86_avx512bw_addusw256", I16x16, I16, Vpaddusw)
            }
            Self::X86Avx512BwSubusw256 => {
                core!(Binary, "x86_avx512bw_subusw256", I16x16, I16, Vpsubusw)
            }
            Self::X86Avx512BwMinsw256 => {
                core!(Binary, "x86_avx512bw_minsw256", I16x16, I16, Vpminsw)
            }
            Self::X86Avx512BwMinuw256 => {
                core!(Binary, "x86_avx512bw_minuw256", I16x16, I16, Vpminuw)
            }
            Self::X86Avx512BwMaxsw256 => {
                core!(Binary, "x86_avx512bw_maxsw256", I16x16, I16, Vpmaxsw)
            }
            Self::X86Avx512BwMaxuw256 => {
                core!(Binary, "x86_avx512bw_maxuw256", I16x16, I16, Vpmaxuw)
            }
            Self::X86Avx512BwMovew256 => {
                core!(Move, "x86_avx512bw_movew256", I16x16, I16, Vmovdqu16)
            }
            Self::X86Avx512BwLoadw256 => {
                core!(Load, "x86_avx512bw_loadw256", I16x16, I16, Vmovdqu16)
            }
            Self::X86Avx512BwStorew256 => {
                core!(Store, "x86_avx512bw_storew256", I16x16, I16, Vmovdqu16)
            }
            Self::X86Avx512BwAddb512 => core!(Binary, "x86_avx512bw_addb512", I8x64, I64, Vpaddb),
            Self::X86Avx512BwSubb512 => core!(Binary, "x86_avx512bw_subb512", I8x64, I64, Vpsubb),
            Self::X86Avx512BwAddsb512 => {
                core!(Binary, "x86_avx512bw_addsb512", I8x64, I64, Vpaddsb)
            }
            Self::X86Avx512BwSubsb512 => {
                core!(Binary, "x86_avx512bw_subsb512", I8x64, I64, Vpsubsb)
            }
            Self::X86Avx512BwAddusb512 => {
                core!(Binary, "x86_avx512bw_addusb512", I8x64, I64, Vpaddusb)
            }
            Self::X86Avx512BwSubusb512 => {
                core!(Binary, "x86_avx512bw_subusb512", I8x64, I64, Vpsubusb)
            }
            Self::X86Avx512BwMinsb512 => {
                core!(Binary, "x86_avx512bw_minsb512", I8x64, I64, Vpminsb)
            }
            Self::X86Avx512BwMinub512 => {
                core!(Binary, "x86_avx512bw_minub512", I8x64, I64, Vpminub)
            }
            Self::X86Avx512BwMaxsb512 => {
                core!(Binary, "x86_avx512bw_maxsb512", I8x64, I64, Vpmaxsb)
            }
            Self::X86Avx512BwMaxub512 => {
                core!(Binary, "x86_avx512bw_maxub512", I8x64, I64, Vpmaxub)
            }
            Self::X86Avx512BwMoveb512 => {
                core!(Move, "x86_avx512bw_moveb512", I8x64, I64, Vmovdqu8)
            }
            Self::X86Avx512BwLoadb512 => {
                core!(Load, "x86_avx512bw_loadb512", I8x64, I64, Vmovdqu8)
            }
            Self::X86Avx512BwStoreb512 => {
                core!(Store, "x86_avx512bw_storeb512", I8x64, I64, Vmovdqu8)
            }
            Self::X86Avx512BwAddw512 => {
                core!(Binary, "x86_avx512bw_addw512", I16x32, I32, Vpaddw)
            }
            Self::X86Avx512BwSubw512 => {
                core!(Binary, "x86_avx512bw_subw512", I16x32, I32, Vpsubw)
            }
            Self::X86Avx512BwAddsw512 => {
                core!(Binary, "x86_avx512bw_addsw512", I16x32, I32, Vpaddsw)
            }
            Self::X86Avx512BwSubsw512 => {
                core!(Binary, "x86_avx512bw_subsw512", I16x32, I32, Vpsubsw)
            }
            Self::X86Avx512BwAddusw512 => {
                core!(Binary, "x86_avx512bw_addusw512", I16x32, I32, Vpaddusw)
            }
            Self::X86Avx512BwSubusw512 => {
                core!(Binary, "x86_avx512bw_subusw512", I16x32, I32, Vpsubusw)
            }
            Self::X86Avx512BwMinsw512 => {
                core!(Binary, "x86_avx512bw_minsw512", I16x32, I32, Vpminsw)
            }
            Self::X86Avx512BwMinuw512 => {
                core!(Binary, "x86_avx512bw_minuw512", I16x32, I32, Vpminuw)
            }
            Self::X86Avx512BwMaxsw512 => {
                core!(Binary, "x86_avx512bw_maxsw512", I16x32, I32, Vpmaxsw)
            }
            Self::X86Avx512BwMaxuw512 => {
                core!(Binary, "x86_avx512bw_maxuw512", I16x32, I32, Vpmaxuw)
            }
            Self::X86Avx512BwMovew512 => {
                core!(Move, "x86_avx512bw_movew512", I16x32, I32, Vmovdqu16)
            }
            Self::X86Avx512BwLoadw512 => {
                core!(Load, "x86_avx512bw_loadw512", I16x32, I32, Vmovdqu16)
            }
            Self::X86Avx512BwStorew512 => {
                core!(Store, "x86_avx512bw_storew512", I16x32, I32, Vmovdqu16)
            }
            _ => return None,
        })
    }
}
