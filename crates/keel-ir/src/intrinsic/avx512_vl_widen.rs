//! Masked widening uses low source elements in explicit 128-bit containers.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlWiden {
    pub name: &'static str,
    pub opcode: &'static str,
    pub destination: VectorType,
    pub source: VectorType,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_vl_widen(self) -> Option<X86Avx512VlWiden> {
        macro_rules! descriptor {
            ($name:literal,$opcode:literal,$dest:ident,$src:ident) => {{
                const D: Type = Type::Vector(VectorType::$dest);
                const S: Type = Type::Vector(VectorType::$src);
                X86Avx512VlWiden {
                    name: $name,
                    opcode: $opcode,
                    destination: VectorType::$dest,
                    source: VectorType::$src,
                    params: &[D, Type::I8, S],
                    returns: &[D],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlPmovsxbd128 => {
                descriptor!("x86_avx512vl_pmovsxbd128", "vpmovsxbd", I32x4, I8x16)
            }
            Self::X86Avx512VlPmovsxbq128 => {
                descriptor!("x86_avx512vl_pmovsxbq128", "vpmovsxbq", I64x2, I8x16)
            }
            Self::X86Avx512VlPmovsxwd128 => {
                descriptor!("x86_avx512vl_pmovsxwd128", "vpmovsxwd", I32x4, I16x8)
            }
            Self::X86Avx512VlPmovsxwq128 => {
                descriptor!("x86_avx512vl_pmovsxwq128", "vpmovsxwq", I64x2, I16x8)
            }
            Self::X86Avx512VlPmovsxdq128 => {
                descriptor!("x86_avx512vl_pmovsxdq128", "vpmovsxdq", I64x2, I32x4)
            }
            Self::X86Avx512VlPmovzxbd128 => {
                descriptor!("x86_avx512vl_pmovzxbd128", "vpmovzxbd", I32x4, I8x16)
            }
            Self::X86Avx512VlPmovzxbq128 => {
                descriptor!("x86_avx512vl_pmovzxbq128", "vpmovzxbq", I64x2, I8x16)
            }
            Self::X86Avx512VlPmovzxwd128 => {
                descriptor!("x86_avx512vl_pmovzxwd128", "vpmovzxwd", I32x4, I16x8)
            }
            Self::X86Avx512VlPmovzxwq128 => {
                descriptor!("x86_avx512vl_pmovzxwq128", "vpmovzxwq", I64x2, I16x8)
            }
            Self::X86Avx512VlPmovzxdq128 => {
                descriptor!("x86_avx512vl_pmovzxdq128", "vpmovzxdq", I64x2, I32x4)
            }
            Self::X86Avx512VlPmovsxbd256 => {
                descriptor!("x86_avx512vl_pmovsxbd256", "vpmovsxbd", I32x8, I8x16)
            }
            Self::X86Avx512VlPmovsxbq256 => {
                descriptor!("x86_avx512vl_pmovsxbq256", "vpmovsxbq", I64x4, I8x16)
            }
            Self::X86Avx512VlPmovsxwd256 => {
                descriptor!("x86_avx512vl_pmovsxwd256", "vpmovsxwd", I32x8, I16x8)
            }
            Self::X86Avx512VlPmovsxwq256 => {
                descriptor!("x86_avx512vl_pmovsxwq256", "vpmovsxwq", I64x4, I16x8)
            }
            Self::X86Avx512VlPmovsxdq256 => {
                descriptor!("x86_avx512vl_pmovsxdq256", "vpmovsxdq", I64x4, I32x4)
            }
            Self::X86Avx512VlPmovzxbd256 => {
                descriptor!("x86_avx512vl_pmovzxbd256", "vpmovzxbd", I32x8, I8x16)
            }
            Self::X86Avx512VlPmovzxbq256 => {
                descriptor!("x86_avx512vl_pmovzxbq256", "vpmovzxbq", I64x4, I8x16)
            }
            Self::X86Avx512VlPmovzxwd256 => {
                descriptor!("x86_avx512vl_pmovzxwd256", "vpmovzxwd", I32x8, I16x8)
            }
            Self::X86Avx512VlPmovzxwq256 => {
                descriptor!("x86_avx512vl_pmovzxwq256", "vpmovzxwq", I64x4, I16x8)
            }
            Self::X86Avx512VlPmovzxdq256 => {
                descriptor!("x86_avx512vl_pmovzxdq256", "vpmovzxdq", I64x4, I32x4)
            }
            _ => return None,
        })
    }
}
