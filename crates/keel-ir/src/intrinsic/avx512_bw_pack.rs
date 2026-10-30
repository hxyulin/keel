//! BW saturation and interleaves operate within independent 128-bit blocks.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwPack {
    pub name: &'static str,
    pub input: VectorType,
    pub result: VectorType,
    pub mask: Type,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_pack(self) -> Option<X86Avx512BwPack> {
        macro_rules! descriptor {
            ($name:literal,$input:ident,$result:ident,$mask:ident,$opcode:literal) => {{
                const V: Type = Type::Vector(VectorType::$input);
                const R: Type = Type::Vector(VectorType::$result);
                const M: Type = Type::$mask;
                X86Avx512BwPack {
                    name: $name,
                    input: VectorType::$input,
                    result: VectorType::$result,
                    mask: M,
                    opcode: $opcode,
                    params: &[R, M, V, V],
                    returns: &[R],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwPacksswb128 => {
                descriptor!("x86_avx512bw_packsswb128", I16x8, I8x16, I16, "vpacksswb")
            }
            Self::X86Avx512BwPackuswb128 => {
                descriptor!("x86_avx512bw_packuswb128", I16x8, I8x16, I16, "vpackuswb")
            }
            Self::X86Avx512BwPackssdw128 => {
                descriptor!("x86_avx512bw_packssdw128", I32x4, I16x8, I8, "vpackssdw")
            }
            Self::X86Avx512BwPackusdw128 => {
                descriptor!("x86_avx512bw_packusdw128", I32x4, I16x8, I8, "vpackusdw")
            }
            Self::X86Avx512BwUnpcklbw128 => {
                descriptor!("x86_avx512bw_unpcklbw128", I8x16, I8x16, I16, "vpunpcklbw")
            }
            Self::X86Avx512BwUnpckhbw128 => {
                descriptor!("x86_avx512bw_unpckhbw128", I8x16, I8x16, I16, "vpunpckhbw")
            }
            Self::X86Avx512BwUnpcklwd128 => {
                descriptor!("x86_avx512bw_unpcklwd128", I16x8, I16x8, I8, "vpunpcklwd")
            }
            Self::X86Avx512BwUnpckhwd128 => {
                descriptor!("x86_avx512bw_unpckhwd128", I16x8, I16x8, I8, "vpunpckhwd")
            }
            Self::X86Avx512BwPacksswb256 => {
                descriptor!("x86_avx512bw_packsswb256", I16x16, I8x32, I32, "vpacksswb")
            }
            Self::X86Avx512BwPackuswb256 => {
                descriptor!("x86_avx512bw_packuswb256", I16x16, I8x32, I32, "vpackuswb")
            }
            Self::X86Avx512BwPackssdw256 => {
                descriptor!("x86_avx512bw_packssdw256", I32x8, I16x16, I16, "vpackssdw")
            }
            Self::X86Avx512BwPackusdw256 => {
                descriptor!("x86_avx512bw_packusdw256", I32x8, I16x16, I16, "vpackusdw")
            }
            Self::X86Avx512BwUnpcklbw256 => {
                descriptor!("x86_avx512bw_unpcklbw256", I8x32, I8x32, I32, "vpunpcklbw")
            }
            Self::X86Avx512BwUnpckhbw256 => {
                descriptor!("x86_avx512bw_unpckhbw256", I8x32, I8x32, I32, "vpunpckhbw")
            }
            Self::X86Avx512BwUnpcklwd256 => descriptor!(
                "x86_avx512bw_unpcklwd256",
                I16x16,
                I16x16,
                I16,
                "vpunpcklwd"
            ),
            Self::X86Avx512BwUnpckhwd256 => descriptor!(
                "x86_avx512bw_unpckhwd256",
                I16x16,
                I16x16,
                I16,
                "vpunpckhwd"
            ),
            Self::X86Avx512BwPacksswb512 => {
                descriptor!("x86_avx512bw_packsswb512", I16x32, I8x64, I64, "vpacksswb")
            }
            Self::X86Avx512BwPackuswb512 => {
                descriptor!("x86_avx512bw_packuswb512", I16x32, I8x64, I64, "vpackuswb")
            }
            Self::X86Avx512BwPackssdw512 => {
                descriptor!("x86_avx512bw_packssdw512", I32x16, I16x32, I32, "vpackssdw")
            }
            Self::X86Avx512BwPackusdw512 => {
                descriptor!("x86_avx512bw_packusdw512", I32x16, I16x32, I32, "vpackusdw")
            }
            Self::X86Avx512BwUnpcklbw512 => {
                descriptor!("x86_avx512bw_unpcklbw512", I8x64, I8x64, I64, "vpunpcklbw")
            }
            Self::X86Avx512BwUnpckhbw512 => {
                descriptor!("x86_avx512bw_unpckhbw512", I8x64, I8x64, I64, "vpunpckhbw")
            }
            Self::X86Avx512BwUnpcklwd512 => descriptor!(
                "x86_avx512bw_unpcklwd512",
                I16x32,
                I16x32,
                I32,
                "vpunpcklwd"
            ),
            Self::X86Avx512BwUnpckhwd512 => descriptor!(
                "x86_avx512bw_unpckhwd512",
                I16x32,
                I16x32,
                I32,
                "vpunpckhwd"
            ),
            _ => return None,
        })
    }
}
