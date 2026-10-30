//! Byte/word conversion width, produced lanes and memory footprint are distinct.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwConvertKind {
    Widen,
    WidenLoad,
    Narrow,
    NarrowStore,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwConvert {
    pub name: &'static str,
    pub kind: X86Avx512BwConvertKind,
    pub encoded_bits: u16,
    pub byte: VectorType,
    pub word: VectorType,
    pub mask: Type,
    pub memory_bytes: u16,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_convert(self) -> Option<X86Avx512BwConvert> {
        macro_rules! descriptor {
            ($kind:ident,$name:literal,$width:literal,$byte:ident,$word:ident,$mask:ident,$opcode:literal) => {{
                const B: Type = Type::Vector(VectorType::$byte);
                const W: Type = Type::Vector(VectorType::$word);
                const M: Type = Type::$mask;
                use X86Avx512BwConvertKind::*;
                X86Avx512BwConvert {
                    name: $name,
                    kind: $kind,
                    encoded_bits: $width,
                    byte: VectorType::$byte,
                    word: VectorType::$word,
                    mask: M,
                    memory_bytes: $width / 16,
                    opcode: $opcode,
                    params: match $kind {
                        Widen => &[W, M, B],
                        WidenLoad => &[Type::Ptr, W, M],
                        Narrow => &[B, M, W],
                        NarrowStore => &[Type::Ptr, W, M],
                    },
                    returns: match $kind {
                        Widen | WidenLoad => &[W],
                        Narrow => &[B],
                        NarrowStore => &[],
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwPmovsxbw128 => descriptor!(
                Widen,
                "x86_avx512bw_pmovsxbw128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovsxbwLoad128 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovsxbwload128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovzxbw128 => descriptor!(
                Widen,
                "x86_avx512bw_pmovzxbw128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovzxbwLoad128 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovzxbwload128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovwb128 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovwb128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovwbStore128 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovwbstore128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovswb128 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovswb128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovswbStore128 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovswbstore128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovuswb128 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovuswb128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovuswb"
            ),
            Self::X86Avx512BwPmovuswbStore128 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovuswbstore128",
                128,
                I8x16,
                I16x8,
                I8,
                "vpmovuswb"
            ),
            Self::X86Avx512BwPmovsxbw256 => descriptor!(
                Widen,
                "x86_avx512bw_pmovsxbw256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovsxbwLoad256 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovsxbwload256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovzxbw256 => descriptor!(
                Widen,
                "x86_avx512bw_pmovzxbw256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovzxbwLoad256 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovzxbwload256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovwb256 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovwb256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovwbStore256 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovwbstore256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovswb256 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovswb256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovswbStore256 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovswbstore256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovuswb256 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovuswb256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovuswb"
            ),
            Self::X86Avx512BwPmovuswbStore256 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovuswbstore256",
                256,
                I8x16,
                I16x16,
                I16,
                "vpmovuswb"
            ),
            Self::X86Avx512BwPmovsxbw512 => descriptor!(
                Widen,
                "x86_avx512bw_pmovsxbw512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovsxbwLoad512 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovsxbwload512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovsxbw"
            ),
            Self::X86Avx512BwPmovzxbw512 => descriptor!(
                Widen,
                "x86_avx512bw_pmovzxbw512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovzxbwLoad512 => descriptor!(
                WidenLoad,
                "x86_avx512bw_pmovzxbwload512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovzxbw"
            ),
            Self::X86Avx512BwPmovwb512 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovwb512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovwbStore512 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovwbstore512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovwb"
            ),
            Self::X86Avx512BwPmovswb512 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovswb512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovswbStore512 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovswbstore512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovswb"
            ),
            Self::X86Avx512BwPmovuswb512 => descriptor!(
                Narrow,
                "x86_avx512bw_pmovuswb512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovuswb"
            ),
            Self::X86Avx512BwPmovuswbStore512 => descriptor!(
                NarrowStore,
                "x86_avx512bw_pmovuswbstore512",
                512,
                I8x32,
                I16x32,
                I32,
                "vpmovuswb"
            ),
            _ => return None,
        })
    }
}
