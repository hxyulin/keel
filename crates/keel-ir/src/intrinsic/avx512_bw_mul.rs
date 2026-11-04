//! BW multiply, pairwise arithmetic and rounded unsigned averages.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwMul {
    pub name: &'static str,
    pub input: VectorType,
    pub result: VectorType,
    pub mask: Type,
    pub opcode: EvexMnemonic,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_mul(self) -> Option<X86Avx512BwMul> {
        macro_rules! descriptor {
            ($name:literal,$input:ident,$result:ident,$mask:ident,$opcode:ident) => {{
                const V: Type = Type::Vector(VectorType::$input);
                const R: Type = Type::Vector(VectorType::$result);
                const M: Type = Type::$mask;
                X86Avx512BwMul {
                    name: $name,
                    input: VectorType::$input,
                    result: VectorType::$result,
                    mask: M,
                    opcode: EvexMnemonic::$opcode,
                    params: &[R, M, V, V],
                    returns: &[R],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwMullw128 => {
                descriptor!("x86_avx512bw_mullw128", I16x8, I16x8, I8, Vpmullw)
            }
            Self::X86Avx512BwMulhw128 => {
                descriptor!("x86_avx512bw_mulhw128", I16x8, I16x8, I8, Vpmulhw)
            }
            Self::X86Avx512BwMulhuw128 => {
                descriptor!("x86_avx512bw_mulhuw128", I16x8, I16x8, I8, Vpmulhuw)
            }
            Self::X86Avx512BwMulhrsw128 => {
                descriptor!("x86_avx512bw_mulhrsw128", I16x8, I16x8, I8, Vpmulhrsw)
            }
            Self::X86Avx512BwMaddwd128 => {
                descriptor!("x86_avx512bw_maddwd128", I16x8, I32x4, I8, Vpmaddwd)
            }
            Self::X86Avx512BwMaddubsw128 => {
                descriptor!("x86_avx512bw_maddubsw128", I8x16, I16x8, I8, Vpmaddubsw)
            }
            Self::X86Avx512BwAvgb128 => {
                descriptor!("x86_avx512bw_avgb128", I8x16, I8x16, I16, Vpavgb)
            }
            Self::X86Avx512BwAvgw128 => {
                descriptor!("x86_avx512bw_avgw128", I16x8, I16x8, I8, Vpavgw)
            }
            Self::X86Avx512BwMullw256 => {
                descriptor!("x86_avx512bw_mullw256", I16x16, I16x16, I16, Vpmullw)
            }
            Self::X86Avx512BwMulhw256 => {
                descriptor!("x86_avx512bw_mulhw256", I16x16, I16x16, I16, Vpmulhw)
            }
            Self::X86Avx512BwMulhuw256 => {
                descriptor!("x86_avx512bw_mulhuw256", I16x16, I16x16, I16, Vpmulhuw)
            }
            Self::X86Avx512BwMulhrsw256 => {
                descriptor!("x86_avx512bw_mulhrsw256", I16x16, I16x16, I16, Vpmulhrsw)
            }
            Self::X86Avx512BwMaddwd256 => {
                descriptor!("x86_avx512bw_maddwd256", I16x16, I32x8, I8, Vpmaddwd)
            }
            Self::X86Avx512BwMaddubsw256 => {
                descriptor!("x86_avx512bw_maddubsw256", I8x32, I16x16, I16, Vpmaddubsw)
            }
            Self::X86Avx512BwAvgb256 => {
                descriptor!("x86_avx512bw_avgb256", I8x32, I8x32, I32, Vpavgb)
            }
            Self::X86Avx512BwAvgw256 => {
                descriptor!("x86_avx512bw_avgw256", I16x16, I16x16, I16, Vpavgw)
            }
            Self::X86Avx512BwMullw512 => {
                descriptor!("x86_avx512bw_mullw512", I16x32, I16x32, I32, Vpmullw)
            }
            Self::X86Avx512BwMulhw512 => {
                descriptor!("x86_avx512bw_mulhw512", I16x32, I16x32, I32, Vpmulhw)
            }
            Self::X86Avx512BwMulhuw512 => {
                descriptor!("x86_avx512bw_mulhuw512", I16x32, I16x32, I32, Vpmulhuw)
            }
            Self::X86Avx512BwMulhrsw512 => {
                descriptor!("x86_avx512bw_mulhrsw512", I16x32, I16x32, I32, Vpmulhrsw)
            }
            Self::X86Avx512BwMaddwd512 => {
                descriptor!("x86_avx512bw_maddwd512", I16x32, I32x16, I16, Vpmaddwd)
            }
            Self::X86Avx512BwMaddubsw512 => {
                descriptor!("x86_avx512bw_maddubsw512", I8x64, I16x32, I32, Vpmaddubsw)
            }
            Self::X86Avx512BwAvgb512 => {
                descriptor!("x86_avx512bw_avgb512", I8x64, I8x64, I64, Vpavgb)
            }
            Self::X86Avx512BwAvgw512 => {
                descriptor!("x86_avx512bw_avgw512", I16x32, I16x32, I32, Vpavgw)
            }
            _ => return None,
        })
    }
}
