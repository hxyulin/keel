//! BW comparison, predicate-test and sign-mask conversion contracts.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwMaskKind {
    Compare,
    Test,
    ToMask,
    FromMask,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwMask {
    pub name: &'static str,
    pub kind: X86Avx512BwMaskKind,
    pub vector: VectorType,
    pub mask: Type,
    pub opcode: EvexMnemonic,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_mask(self) -> Option<X86Avx512BwMask> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$vector:ident,$mask:ident,$opcode:ident) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                const M: Type = Type::$mask;
                use X86Avx512BwMaskKind::*;
                X86Avx512BwMask {
                    name: $name,
                    kind: $kind,
                    vector: VectorType::$vector,
                    mask: M,
                    opcode: EvexMnemonic::$opcode,
                    params: match $kind {
                        Compare => &[V, V, M, Type::I32],
                        Test => &[V, V, M],
                        ToMask => &[V],
                        FromMask => &[M],
                    },
                    returns: match $kind {
                        FromMask => &[V],
                        _ => &[M],
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwCmpb128 => {
                descriptor!(Compare, "x86_avx512bw_cmpb128", I8x16, I16, Vpcmpb)
            }
            Self::X86Avx512BwCmpub128 => {
                descriptor!(Compare, "x86_avx512bw_cmpub128", I8x16, I16, Vpcmpub)
            }
            Self::X86Avx512BwTestb128 => {
                descriptor!(Test, "x86_avx512bw_testb128", I8x16, I16, Vptestmb)
            }
            Self::X86Avx512BwTestnb128 => {
                descriptor!(Test, "x86_avx512bw_testnb128", I8x16, I16, Vptestnmb)
            }
            Self::X86Avx512BwMovb2m128 => {
                descriptor!(ToMask, "x86_avx512bw_movb2m128", I8x16, I16, Vpmovb2m)
            }
            Self::X86Avx512BwMovm2b128 => {
                descriptor!(FromMask, "x86_avx512bw_movm2b128", I8x16, I16, Vpmovm2b)
            }
            Self::X86Avx512BwCmpw128 => {
                descriptor!(Compare, "x86_avx512bw_cmpw128", I16x8, I8, Vpcmpw)
            }
            Self::X86Avx512BwCmpuw128 => {
                descriptor!(Compare, "x86_avx512bw_cmpuw128", I16x8, I8, Vpcmpuw)
            }
            Self::X86Avx512BwTestw128 => {
                descriptor!(Test, "x86_avx512bw_testw128", I16x8, I8, Vptestmw)
            }
            Self::X86Avx512BwTestnw128 => {
                descriptor!(Test, "x86_avx512bw_testnw128", I16x8, I8, Vptestnmw)
            }
            Self::X86Avx512BwMovw2m128 => {
                descriptor!(ToMask, "x86_avx512bw_movw2m128", I16x8, I8, Vpmovw2m)
            }
            Self::X86Avx512BwMovm2w128 => {
                descriptor!(FromMask, "x86_avx512bw_movm2w128", I16x8, I8, Vpmovm2w)
            }
            Self::X86Avx512BwCmpb256 => {
                descriptor!(Compare, "x86_avx512bw_cmpb256", I8x32, I32, Vpcmpb)
            }
            Self::X86Avx512BwCmpub256 => {
                descriptor!(Compare, "x86_avx512bw_cmpub256", I8x32, I32, Vpcmpub)
            }
            Self::X86Avx512BwTestb256 => {
                descriptor!(Test, "x86_avx512bw_testb256", I8x32, I32, Vptestmb)
            }
            Self::X86Avx512BwTestnb256 => {
                descriptor!(Test, "x86_avx512bw_testnb256", I8x32, I32, Vptestnmb)
            }
            Self::X86Avx512BwMovb2m256 => {
                descriptor!(ToMask, "x86_avx512bw_movb2m256", I8x32, I32, Vpmovb2m)
            }
            Self::X86Avx512BwMovm2b256 => {
                descriptor!(FromMask, "x86_avx512bw_movm2b256", I8x32, I32, Vpmovm2b)
            }
            Self::X86Avx512BwCmpw256 => {
                descriptor!(Compare, "x86_avx512bw_cmpw256", I16x16, I16, Vpcmpw)
            }
            Self::X86Avx512BwCmpuw256 => {
                descriptor!(Compare, "x86_avx512bw_cmpuw256", I16x16, I16, Vpcmpuw)
            }
            Self::X86Avx512BwTestw256 => {
                descriptor!(Test, "x86_avx512bw_testw256", I16x16, I16, Vptestmw)
            }
            Self::X86Avx512BwTestnw256 => {
                descriptor!(Test, "x86_avx512bw_testnw256", I16x16, I16, Vptestnmw)
            }
            Self::X86Avx512BwMovw2m256 => {
                descriptor!(ToMask, "x86_avx512bw_movw2m256", I16x16, I16, Vpmovw2m)
            }
            Self::X86Avx512BwMovm2w256 => {
                descriptor!(FromMask, "x86_avx512bw_movm2w256", I16x16, I16, Vpmovm2w)
            }
            Self::X86Avx512BwCmpb512 => {
                descriptor!(Compare, "x86_avx512bw_cmpb512", I8x64, I64, Vpcmpb)
            }
            Self::X86Avx512BwCmpub512 => {
                descriptor!(Compare, "x86_avx512bw_cmpub512", I8x64, I64, Vpcmpub)
            }
            Self::X86Avx512BwTestb512 => {
                descriptor!(Test, "x86_avx512bw_testb512", I8x64, I64, Vptestmb)
            }
            Self::X86Avx512BwTestnb512 => {
                descriptor!(Test, "x86_avx512bw_testnb512", I8x64, I64, Vptestnmb)
            }
            Self::X86Avx512BwMovb2m512 => {
                descriptor!(ToMask, "x86_avx512bw_movb2m512", I8x64, I64, Vpmovb2m)
            }
            Self::X86Avx512BwMovm2b512 => {
                descriptor!(FromMask, "x86_avx512bw_movm2b512", I8x64, I64, Vpmovm2b)
            }
            Self::X86Avx512BwCmpw512 => {
                descriptor!(Compare, "x86_avx512bw_cmpw512", I16x32, I32, Vpcmpw)
            }
            Self::X86Avx512BwCmpuw512 => {
                descriptor!(Compare, "x86_avx512bw_cmpuw512", I16x32, I32, Vpcmpuw)
            }
            Self::X86Avx512BwTestw512 => {
                descriptor!(Test, "x86_avx512bw_testw512", I16x32, I32, Vptestmw)
            }
            Self::X86Avx512BwTestnw512 => {
                descriptor!(Test, "x86_avx512bw_testnw512", I16x32, I32, Vptestnmw)
            }
            Self::X86Avx512BwMovw2m512 => {
                descriptor!(ToMask, "x86_avx512bw_movw2m512", I16x32, I32, Vpmovw2m)
            }
            Self::X86Avx512BwMovm2w512 => {
                descriptor!(FromMask, "x86_avx512bw_movm2w512", I16x32, I32, Vpmovm2w)
            }
            _ => return None,
        })
    }
}
